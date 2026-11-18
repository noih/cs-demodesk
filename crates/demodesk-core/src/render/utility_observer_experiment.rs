//! Opt-in native fixture runner using the production observer solver.
use super::super::utility_observer::{search, search_angles};
use super::*;
use serde_json::json;

#[test]
#[ignore = "offline utility-observer prototype; reads a local demo/map and writes an experiment manifest"]
fn plan_utility_observer_experiment() -> Result<()> {
    let job_path = PathBuf::from(std::env::var("DEMODESK_UTILITY_OBSERVER_JOB")?);
    let output = PathBuf::from(std::env::var("DEMODESK_UTILITY_OBSERVER_OUTPUT")?);
    ensure!(!output.exists(), "output must be a new directory");
    std::fs::create_dir_all(&output)?;
    let job: serde_json::Value = serde_json::from_slice(&std::fs::read(&job_path)?)?;
    let root = job_path
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let parsed = root
        .join("parsed")
        .join(format!("{}.json", job["demoId"].as_str().unwrap()));
    let cached: serde_json::Value = serde_json::from_slice(&std::fs::read(&parsed)?)?;
    let demo: DemoInfo = serde_json::from_value(cached["info"].clone())?;
    let highlights: Vec<Highlight> = serde_json::from_value(cached["highlights"].clone())?;
    let h = highlights
        .iter()
        .find(|h| {
            job["highlightIds"]
                .as_array()
                .unwrap()
                .iter()
                .any(|id| id.as_str() == Some(&h.id))
        })
        .context("highlight missing")?;
    let bytes = std::fs::read(&demo.path)?;
    let events = demo_data::read_events(&bytes, &AtomicBool::new(false))?;
    let kill = events
        .iter()
        .rev()
        .find(|e| {
            e.event_name == "player_death"
                && e.weapon == "hegrenade"
                && e.attacker_steamid.as_deref() == Some(&h.player.steamid)
                && e.tick >= h.start_tick
                && e.tick < h.end_tick
        })
        .context("HE kill missing")?;
    let victim = kill.user_steamid.as_deref().context("victim missing")?;
    let rate = demo.tick_rate.round() as i32;
    let detonation = events
        .iter()
        .find(|e| {
            e.event_name == "hegrenade_detonate"
                && (e.tick - kill.tick).abs() <= 2
                && e.user_steamid == kill.attacker_steamid
        })
        .context("detonation missing")?;
    let entity = detonation.entityid.context("grenade entity missing")?;
    let ticks: Vec<_> = (kill.tick - rate * 3..=kill.tick).collect();
    let parser = crate::parser::DemoParser::new();
    let projectiles = parser.projectiles(&bytes, ticks.clone())?;
    let mut flight: Vec<_> = projectiles
        .iter()
        .filter(|r| r.num("grenade_entity_id") == Some(f64::from(entity)))
        .filter_map(|r| Some((r.tick()?, [r.num("x")?, r.num("y")?, r.num("z")?])))
        .collect();
    flight.sort_by_key(|p| p.0);
    flight.dedup_by_key(|p| p.0);
    ensure!(
        flight.len() > 8,
        "missing projectile path for entity {entity}"
    );
    let rows = parser.ticks(&bytes, &["X", "Y", "Z", "health"].map(String::from), ticks)?;
    let positions: Vec<_> = rows
        .iter()
        .filter(|r| {
            r.steamid().as_deref() == Some(victim) && r.num("health").is_some_and(|h| h > 0.)
        })
        .filter_map(|r| Some((r.tick()?, [r.num("X")?, r.num("Y")?, r.num("Z")?])))
        .collect();
    ensure!(!positions.is_empty(), "missing victim positions");
    let protagonist: Vec<_> = rows
        .iter()
        .filter(|r| r.steamid().as_deref() == Some(&h.player.steamid))
        .filter_map(|r| Some((r.tick()?, [r.num("X")?, r.num("Y")?, r.num("Z")?])))
        .collect();
    let body: Vec<_> = (flight[0].0..=kill.tick + editing::pacing::event_tail(rate))
        .step_by(4)
        .filter_map(|tick| {
            positions
                .iter()
                .rev()
                .find(|(t, _)| *t <= tick)
                .map(|(_, p)| (tick, *p))
        })
        .collect();
    let tools = resolve_tool_paths(&root.join("tools"), &PathOverrides::default());
    let geometry = crate::analysis::visibility_assets::prepare(
        root,
        tools.cs2_dir.as_deref().context("game missing")?,
        tools.vrf_exe.as_deref().context("VRF missing")?,
        &demo.map_name,
    )?;
    let calls = std::cell::Cell::new(0usize);
    let ray = |a, b| {
        calls.set(calls.get() + 1);
        geometry.world.ray(a, b)
    };
    let started = std::time::Instant::now();
    let (candidates, rejected) = search(&flight, &body, &protagonist, kill.tick, rate, &ray);
    let search_ms = started.elapsed().as_secs_f64() * 1000.;
    let search_rays = calls.get();
    let baseline = if std::env::var_os("DEMODESK_UTILITY_OBSERVER_COMPARE").is_some() {
        calls.set(0);
        let started = std::time::Instant::now();
        let (views, _) = search_angles(
            &flight,
            &body,
            &protagonist,
            kill.tick,
            rate,
            &(0..360).step_by(15).collect::<Vec<_>>(),
            &ray,
        );
        Some(json!({"milliseconds":started.elapsed().as_secs_f64()*1000.,
            "rays":calls.get(),"best":views.first()}))
    } else {
        None
    };
    std::fs::write(
        output.join("search.json"),
        serde_json::to_vec_pretty(
            &json!({"deathTick":kill.tick,"entity":entity,"flight":flight,"body":body,
        "candidates":candidates,"rejected":rejected,"geometryFingerprint":geometry.fingerprint,
        "searchMilliseconds":search_ms,"searchRays":search_rays,"baseline":baseline}),
        )?,
    )?;
    for (index, camera) in candidates.iter().take(3).enumerate() {
        let spec = json!({"parsed":parsed,"toolsDir":root.join("tools"),"outputDir":output.join(format!("capture-{}",index+1)),
            "player":h.player.steamid,"teammate":h.player.steamid,"variant":"team-push","hud":false,"viewmodel":false,
            "startTick":camera.start_tick,"switchTick":camera.end_tick+8,"endTick":camera.end_tick+16,
            "encounter":camera});
        std::fs::write(
            output.join(format!("candidate-{}.json", index + 1)),
            serde_json::to_vec_pretty(&spec)?,
        )?;
    }
    println!(
        "UTILITY_OBSERVER: {} flight samples, {} accepted views, rejected {:?}",
        flight.len(),
        candidates.len(),
        rejected
    );
    Ok(())
}
