//! Opt-in local recording experiment. No production options or job formats are changed.
use super::*;
use anyhow::{ensure, Context};
use std::io::Write;

#[path = "utility_observer_experiment.rs"]
mod utility_observer_experiment;

#[test]
#[ignore = "offline characterization of private demo jobs; writes a new output directory"]
fn characterize_editing_pipeline() -> Result<()> {
    let output = PathBuf::from(std::env::var("DEMODESK_PIPELINE_OUTPUT")?);
    ensure!(!output.exists(), "output must be a new directory");
    std::fs::create_dir_all(&output)?;
    for path in std::env::var("DEMODESK_PIPELINE_JOBS")?.split(';') {
        let path = Path::new(path);
        let job: serde_json::Value = serde_json::from_slice(&std::fs::read(path)?)?;
        let root = path.parent().unwrap().parent().unwrap().parent().unwrap();
        let parsed: serde_json::Value = serde_json::from_slice(&std::fs::read(
            root.join("parsed")
                .join(format!("{}.json", job["demoId"].as_str().unwrap())),
        )?)?;
        let demo: DemoInfo = serde_json::from_value(parsed["info"].clone())?;
        let mut clips: Vec<Highlight> = serde_json::from_value(parsed["highlights"].clone())?;
        clips.retain(|h| {
            job["highlightIds"]
                .as_array()
                .unwrap()
                .iter()
                .any(|id| id.as_str() == Some(&h.id))
        });
        let options: RenderOptions = serde_json::from_value(job["options"].clone())?;
        let mut tools = resolve_tool_paths(&root.join("tools"), &PathOverrides::default());
        tools.cs2_exe = None;
        tools.hlae_exe = None;
        tools.hlae_dll = None;
        let analysis =
            editing::Analysis::prepare("comparison".into(), clips, &demo, &options, &tools, root)?;
        let summary = analysis.summary(false);
        ensure!(
            summary.candidates.len() <= 10,
            "too many combinations for this fixture"
        );
        let mut plans = vec![];
        for mask in 0..(1 << summary.candidates.len()) {
            let ids: Vec<_> = summary
                .candidates
                .iter()
                .enumerate()
                .filter(|(i, _)| mask & (1 << i) != 0)
                .map(|(_, c)| c.id.clone())
                .collect();
            let result = match analysis.select(&ids, &demo) {
                Ok(plan) => serde_json::json!({"ids":ids,"plan":plan}),
                Err(_) => serde_json::json!({"ids":ids,"invalid":true}),
            };
            plans.push(result);
        }
        std::fs::write(
            output.join(format!("{}.json", job["id"].as_str().unwrap())),
            serde_json::to_vec_pretty(&serde_json::json!({"summary":summary,"plans":plans}))?,
        )?;
        println!(
            "{}: {} candidates, {} combinations",
            job["id"],
            summary.candidates.len(),
            plans.len()
        );
    }
    Ok(())
}

#[test]
#[ignore = "reproduces a private export job in a new output directory using CS2"]
fn reproduce_editing_job() -> Result<()> {
    let input = PathBuf::from(std::env::var("DEMODESK_REPRO_JOB")?);
    let output = PathBuf::from(std::env::var("DEMODESK_REPRO_OUTPUT")?);
    ensure!(!output.exists(), "output must be a new directory");
    let job: serde_json::Value = serde_json::from_slice(&std::fs::read(&input)?)?;
    let root = input.parent().unwrap().parent().unwrap().parent().unwrap();
    let cached: serde_json::Value = serde_json::from_slice(&std::fs::read(
        root.join("parsed")
            .join(format!("{}.json", job["demoId"].as_str().unwrap())),
    )?)?;
    let demo: DemoInfo = serde_json::from_value(cached["info"].clone())?;
    let highlights: Vec<Highlight> = serde_json::from_value(cached["highlights"].clone())?;
    let highlights: Vec<_> = highlights
        .into_iter()
        .filter(|h| {
            job["highlightIds"]
                .as_array()
                .unwrap()
                .iter()
                .any(|id| id.as_str() == Some(&h.id))
        })
        .collect();
    let mut options: RenderOptions = serde_json::from_value(job["options"].clone())?;
    if std::env::var_os("DEMODESK_REPRO_EXACT").is_none() {
        options.codec = "libx264".into();
        options.width = 1280;
        options.height = 720;
        options.max_size_mb = None;
    }
    options.keep_raw_files = true;
    if std::env::var_os("DEMODESK_REPRO_SKIP_SIZE").is_some() {
        options.max_size_mb = None;
    }
    std::fs::create_dir_all(&output)?;
    let tools = resolve_tool_paths(&root.join("tools"), &PathOverrides::default());
    let prepared_editing = if std::env::var_os("DEMODESK_REPRO_ANALYZE").is_some() {
        let started = std::time::Instant::now();
        let mut offline_tools = tools.clone();
        offline_tools.cs2_exe = None;
        offline_tools.hlae_exe = None;
        offline_tools.hlae_dll = None;
        offline_tools.ffmpeg_exe = None;
        let analysis = editing::Analysis::prepare(
            "experiment".into(),
            highlights.clone(),
            &demo,
            &options,
            &offline_tools,
            &output,
        )?;
        let link_all = std::env::var_os("DEMODESK_REPRO_LINK_ALL").is_some();
        let selected: Vec<_> = if link_all {
            analysis
                .candidates
                .iter()
                .filter(|c| {
                    c.kind == editing::Kind::Combat
                        || (c.kind == editing::Kind::Opening && c.recommended)
                })
                .map(|c| c.id.clone())
                .collect()
        } else if std::env::var_os("DEMODESK_REPRO_SAVED_SELECTION").is_some() {
            options.editing_selections.clone()
        } else {
            analysis
                .candidates
                .iter()
                .filter(|c| {
                    c.recommended
                        || (std::env::var_os("DEMODESK_REPRO_ALL_COMBAT").is_some()
                            && c.kind == editing::Kind::Combat)
                })
                .map(|c| c.id.clone())
                .collect()
        };
        std::fs::write(
            output.join("analysis.json"),
            serde_json::to_vec_pretty(&analysis.summary(false))?,
        )?;
        let plan = if link_all {
            let mut replays: Vec<_> = analysis
                .candidates
                .iter()
                .filter(|c| c.kind == editing::Kind::Combat)
                .collect();
            replays.sort_by_key(|c| (&c.highlight_id, c.tick));
            let links: Vec<_> = replays
                .windows(2)
                .filter(|p| p[0].highlight_id == p[1].highlight_id)
                .map(|p| editing::ReplayLink {
                    from: p[0].id.clone(),
                    to: p[1].id.clone(),
                })
                .collect();
            analysis.select_with_links(&selected, &links, &demo)?
        } else {
            analysis.select(&selected, &demo)?
        };
        std::fs::write(
            output.join("editing-plan.json"),
            serde_json::to_vec_pretty(&plan)?,
        )?;
        println!(
            "Offline analysis: {:.2}s, {} candidates, {} selected",
            started.elapsed().as_secs_f64(),
            analysis.candidates.len(),
            selected.len()
        );
        if std::env::var_os("DEMODESK_REPRO_ANALYSIS_ONLY").is_some() {
            return Ok(());
        }
        Some(plan)
    } else {
        None
    };
    let started = std::time::Instant::now();
    let mut file = std::fs::File::create(output.join("recording.log"))?;
    let result = render_highlights(RenderJobInput {
        prepared_editing,
        combat_ticks: None,
        demo: &demo,
        demo_path: PathBuf::from(&demo.path),
        highlights,
        preserve_merge_order: false,
        output_dir: output.clone(),
        options,
        tools,
        cancel: Arc::new(AtomicBool::new(false)),
        log: &mut |line| {
            println!("{line}");
            let _ = writeln!(file, "{line}");
        },
        stage: &mut |_| {},
        progress: &mut |_| {},
    })?;
    println!("Export elapsed: {:.2}s", started.elapsed().as_secs_f64());
    ensure!(
        result.failed_highlights.is_empty(),
        "reproduction incomplete"
    );
    std::fs::write(
        output.join("result.json"),
        serde_json::to_vec_pretty(&result)?,
    )?;
    Ok(())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Experiment {
    parsed: PathBuf,
    tools_dir: PathBuf,
    output_dir: PathBuf,
    player: String,
    teammate: String,
    start_tick: i32,
    end_tick: i32,
    switch_tick: i32,
    variant: String,
    #[serde(default)]
    camera: Option<Camera>,
    #[serde(default)]
    team_overview: bool,
    #[serde(default)]
    overview_front: Option<String>,
    #[serde(default)]
    encounter: Option<serde_json::Value>,
    #[serde(default)]
    opening_camera: bool,
    #[serde(default)]
    combat_perspectives: bool,
    #[serde(default)]
    team_push: bool,
    #[serde(default)]
    grenade: Option<serde_json::Value>,
    #[serde(default)]
    hud: Option<bool>,
    #[serde(default)]
    viewmodel: Option<bool>,
}

#[test]
#[ignore = "plans team push from a private demo without recording"]
fn plan_team_push_experiment() -> Result<()> {
    let manifest = PathBuf::from(std::env::var("DEMODESK_EDITING_EXPERIMENT")?);
    let spec: Experiment = serde_json::from_slice(&std::fs::read(&manifest)?)?;
    let cached: serde_json::Value = serde_json::from_slice(&std::fs::read(&spec.parsed)?)?;
    let demo: DemoInfo = serde_json::from_value(cached["info"].clone())?;
    let mut h: Highlight = serde_json::from_value(cached["highlights"][0].clone())?;
    h.player.steamid = spec.player;
    h.start_tick = spec.start_tick;
    h.end_tick = spec.end_tick;
    h.round_result = None;
    let tools = resolve_tool_paths(&spec.tools_dir, &PathOverrides::default());
    let cancel = AtomicBool::new(false);
    let path = Path::new(&demo.path);
    let events = demo_data::load(path, &cancel)?;
    let mut clips = vec![h];
    let tracks = demo_data::load_tracks(path, &clips, demo.tick_rate.round() as i32, &cancel)?;
    let geometry = crate::analysis::visibility_assets::prepare(
        spec.tools_dir.parent().context("root missing")?,
        tools.cs2_dir.as_deref().context("game missing")?,
        tools.vrf_exe.as_deref().context("map tools missing")?,
        &demo.map_name,
    )?;
    let plans = team_push::detect(&mut clips, &demo, &events, &tracks, &|a, b| {
        geometry.world.ray(a, b)
    });
    std::fs::write(
        manifest.with_extension("plan.json"),
        serde_json::to_vec_pretty(&serde_json::json!({"clips":clips,"plans":plans}))?,
    )?;
    Ok(())
}

#[test]
#[ignore = "launches CS2/HLAE; requires DEMODESK_EDITING_EXPERIMENT manifest"]
fn record_editing_experiment() -> Result<()> {
    let manifest = PathBuf::from(std::env::var("DEMODESK_EDITING_EXPERIMENT")?);
    let spec: Experiment = serde_json::from_slice(&std::fs::read(&manifest)?)?;
    ensure!(
        matches!(
            spec.variant.as_str(),
            "baseline"
                | "teammate"
                | "third-person"
                | "aim-follow"
                | "grenade-follow"
                | "opening"
                | "team-push"
                | "utility"
        ),
        "unknown variant"
    );
    ensure!(
        spec.start_tick > 300
            && spec.start_tick < spec.switch_tick
            && spec.switch_tick < spec.end_tick,
        "invalid clip window"
    );
    ensure!(!spec.output_dir.exists(), "output must be a new directory");
    let cached: serde_json::Value = serde_json::from_slice(&std::fs::read(&spec.parsed)?)?;
    let demo: DemoInfo = serde_json::from_value(cached["info"].clone())?;
    let player = demo
        .players
        .iter()
        .find(|p| p.steamid == spec.player)
        .context("player missing")?;
    let teammate = demo
        .players
        .iter()
        .find(|p| p.steamid == spec.teammate)
        .context("teammate missing")?;
    let player_slot = player.user_id.context("player slot missing")? + 1;
    let teammate_slot = teammate.user_id.context("teammate slot missing")? + 1;
    let mut highlight: Highlight = serde_json::from_value(cached["highlights"][0].clone())?;
    highlight.player = crate::model::HighlightPlayer {
        steamid: player.steamid.clone(),
        name: player.name.clone(),
    };
    highlight.start_tick = spec.start_tick;
    highlight.end_tick = spec.end_tick;
    highlight.anchor_tick = spec.switch_tick;
    highlight.round_result = None;
    highlight.key_moments.clear();
    let mut clip = RenderClip {
        highlight,
        slot: Some(player_slot),
        round_result_slot: None,
        account_id: steamid_to_account_id(&player.steamid),
    };
    let tools = resolve_tool_paths(&spec.tools_dir, &PathOverrides::default());
    let ffmpeg = tools.ffmpeg_exe.clone().context("ffmpeg missing")?;
    let options = RenderOptions {
        camera: spec.camera.unwrap_or(Camera::Slot),
        width: 1280,
        height: 720,
        fps: 60,
        codec: "libx264".into(),
        crf: 18,
        max_size_mb: None,
        key_moments_only: false,
        show_game: false,
        hud: spec.hud.unwrap_or(true),
        viewmodel: spec.viewmodel.unwrap_or(true),
        ..Default::default()
    };
    let cancel = Arc::new(AtomicBool::new(false));
    std::fs::create_dir_all(&spec.output_dir)?;
    std::fs::copy(&manifest, spec.output_dir.join("experiment.json"))?;
    let mut logfile = std::fs::File::create(spec.output_dir.join("recording.log"))?;
    let mut log = |line: String| {
        println!("{line}");
        let _ = writeln!(logfile, "{line}");
    };
    if matches!(spec.variant.as_str(), "utility" | "opening") {
        if spec.variant == "opening" {
            clip.highlight.start_tick = spec.switch_tick;
        }
        let result = render_highlights(RenderJobInput {
            prepared_editing: None,
            combat_ticks: None,
            demo: &demo,
            demo_path: PathBuf::from(&demo.path),
            highlights: vec![clip.highlight],
            preserve_merge_order: false,
            output_dir: spec.output_dir.clone(),
            options: RenderOptions {
                utility_teamwork: spec.variant == "utility",
                opening_camera: spec.variant == "opening" || spec.opening_camera,
                combat_perspectives: spec.combat_perspectives,
                team_push: spec.team_push,
                ..options
            },
            tools,
            cancel,
            log: &mut log,
            stage: &mut |value| println!("stage: {value}"),
            progress: &mut |_| {},
        })?;
        ensure!(
            result.failed_highlights.is_empty(),
            "editing export incomplete"
        );
        std::fs::write(
            spec.output_dir.join("result.json"),
            serde_json::to_vec_pretty(&result)?,
        )?;
        println!("EDITING_RESULT {}", spec.output_dir.display());
        return Ok(());
    }
    let (preset, _) = encode::checked_record_preset(&ffmpeg, &options, &mut log, &cancel)?;
    let mut schedule = build_schedule(
        &[clip],
        &ActionsOptions {
            render: &options,
            tick_rate: demo.tick_rate,
            output_dir: to_forward_slashes(&spec.output_dir),
            ffmpeg_preset: preset,
        },
    );
    if spec.variant != "baseline" {
        let setup = f64::from(
            seek::SeekWindow::new(
                spec.start_tick,
                spec.end_tick,
                None,
                demo.tick_rate.round() as i32,
            )
            .setup_tick,
        ) + 1.0;
        let mode = if matches!(
            spec.variant.as_str(),
            "third-person" | "aim-follow" | "grenade-follow" | "team-push"
        ) {
            3
        } else {
            1
        };
        if matches!(spec.variant.as_str(), "aim-follow" | "grenade-follow") {
            let script = spec.output_dir.join("aim-follow.mjs");
            let config = serde_json::json!({
                "steamId": spec.teammate,
                "startTick": spec.start_tick,
                "endTick": spec.switch_tick,
                "grenade": if spec.variant == "grenade-follow" { spec.grenade.as_ref() } else { None },
            });
            std::fs::write(
                &script,
                format!(
                    "{}\nstartAimFollow({config});\n",
                    include_str!("utility-camera.mjs")
                ),
            )?;
            schedule.push(actions::Scheduled {
                tick: setup + 0.003,
                cmd: format!("mirv_script_load \"{}\"", to_forward_slashes(&script)),
            });
        }
        let handoff_tick = spec.switch_tick - if spec.variant == "team-push" { 8 } else { 0 };
        if spec.variant == "team-push" {
            let script = spec.output_dir.join("team-push.mjs");
            let source = std::fs::read_to_string(
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../../scripts/editing-team-push.mjs"),
            )?;
            let config = serde_json::json!({"steamId":spec.player,"startTick":spec.start_tick,"endTick":handoff_tick,"overview":spec.team_overview,"frontId":spec.overview_front,"encounter":spec.encounter});
            let source = source.replace(
                "../crates/demodesk-core/src/render/camera-transition.mjs",
                "./camera-transition.mjs",
            );
            let source = source.replace(
                "../crates/demodesk-core/src/render/team-push-camera.mjs",
                "./team-push-camera.mjs",
            );
            std::fs::write(
                spec.output_dir.join("team-push-camera.mjs"),
                include_str!("team-push-camera.mjs"),
            )?;
            std::fs::write(
                spec.output_dir.join("camera-transition.mjs"),
                include_str!("camera-transition.mjs"),
            )?;
            std::fs::write(&script, format!("{source}\nstartTeamPush({config});\n"))?;
            schedule.push(actions::Scheduled {
                tick: setup + 0.003,
                cmd: format!("mirv_script_load \"{}\"", to_forward_slashes(&script)),
            });
        }
        for (tick, command) in [
            (setup, "spec_lock_to_accountid 0".to_string()),
            (setup + 0.001, format!("spec_player {teammate_slot}")),
            (setup + 0.002, format!("spec_mode {mode}")),
            (f64::from(handoff_tick), "spec_lock_to_accountid 0".into()),
            (f64::from(handoff_tick) + 0.001, "spec_mode 1".into()),
            (
                f64::from(handoff_tick) + 0.002,
                format!("spec_player {player_slot}"),
            ),
        ] {
            schedule.push(actions::Scheduled { tick, cmd: command });
        }
        schedule.sort_by(|a, b| a.tick.total_cmp(&b.tick));
    }
    let mut stage = |value: &str| println!("stage: {value}");
    let mut progress = |_: f64| {};
    let mut session = RecordSession {
        demo_path: PathBuf::from(&demo.path),
        cs2_dir: tools.cs2_dir.context("CS2 directory missing")?,
        cs2_exe: tools.cs2_exe.context("CS2 missing")?,
        hlae_exe: tools.hlae_exe.context("HLAE missing")?,
        hlae_dll: tools.hlae_dll.context("HLAE DLL missing")?,
        ffmpeg_exe: ffmpeg.clone(),
        output_dir: spec.output_dir.clone(),
        cfg_dir: spec
            .output_dir
            .parent()
            .context("output parent missing")?
            .join("cfg"),
        show_game: options.show_game,
        width: options.width,
        height: options.height,
        schedule,
        completed_clips: vec![],
        timeout_seconds: 240,
        extra_launch_options: vec![],
        cancel: cancel.clone(),
        log: &mut log,
        stage: &mut stage,
        progress: &mut progress,
    };
    record::run_recording_session(&mut session)?;
    ensure!(session.completed_clips == vec![0], "clip did not complete");
    drop(session);
    let clips = record::collect_clip_outputs(&spec.output_dir, 1, "mp4");
    let mut control = encode::Control::new(&cancel, &mut log);
    encode::mux_clip(
        &ffmpeg,
        &clips[0],
        &spec.output_dir.join("comparison.mp4"),
        192,
        &mut control,
    )?;
    println!("VIDEO {}", spec.output_dir.join("comparison.mp4").display());
    Ok(())
}
