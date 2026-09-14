//! Exercise the same scoring path as the desktop app with an isolated data directory.
use anyhow::{ensure, Result};
use demodesk_core::{
    engine::{Engine, Event, Notify},
    store::{Settings, Store},
};
use std::{
    collections::BTreeSet,
    path::Path,
    sync::{Arc, Mutex},
};
#[derive(Default)]
struct Quiet {
    scoring: Mutex<Vec<(String, u8)>>,
}
impl Notify for Quiet {
    fn notify(&self, event: Event) {
        if let Event::ScoringProgress { id, step } = event {
            self.scoring
                .lock()
                .expect("progress mutex")
                .push((id, step));
        }
    }
}
impl Quiet {
    fn assert_steps(&self, id: &str, expected: &[u8]) -> Result<()> {
        let events = std::mem::take(&mut *self.scoring.lock().expect("progress mutex"));
        ensure!(
            events.iter().all(|(demo, _)| demo == id),
            "progress belongs to another demo"
        );
        ensure!(
            events.iter().map(|(_, step)| *step).collect::<Vec<_>>() == expected,
            "unexpected scoring progress: {events:?}"
        );
        Ok(())
    }
}
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    ensure!(args.len() == 2, "expected DEMO ISOLATED_DATA_DIRECTORY");
    let root = Path::new(&args[1]).to_path_buf();
    let store = Store::open(root.clone())?;
    store.save_settings(&Settings {
        scan_game_replays: false,
        ..store.settings()
    })?;
    let notify = Arc::new(Quiet::default());
    let engine = Engine::new(root, notify.clone())?;
    let parse_started = std::time::Instant::now();
    let meta = engine.add_demo(Path::new(&args[0]))?;
    engine.parse_demo(&meta.id)?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
    while engine.is_parsing(&meta.id) {
        ensure!(std::time::Instant::now() < deadline, "parse timeout");
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    let parsed = engine
        .parsed(&meta.id)
        .ok_or_else(|| anyhow::anyhow!("parse did not succeed"))?;
    let parse_seconds = parse_started.elapsed().as_secs_f64();
    notify.assert_steps(&meta.id, &[])?;
    let started = std::time::Instant::now();
    let first = engine.score_match(&meta.id, true)?;
    let first_seconds = started.elapsed().as_secs_f64();
    notify.assert_steps(&meta.id, &[1, 2, 3])?;
    let started = std::time::Instant::now();
    let again = engine.score_match(&meta.id, false)?;
    let reused_seconds = started.elapsed().as_secs_f64();
    notify.assert_steps(&meta.id, &[1])?;
    ensure!(
        again.analysis_seconds == 0.,
        "history reuse recomputed rules"
    );
    let started = std::time::Instant::now();
    let retry = engine.score_match(&meta.id, true)?;
    let retry_seconds = started.elapsed().as_secs_f64();
    notify.assert_steps(&meta.id, &[1, 2, 3])?;
    let roster = parsed
        .info
        .players
        .iter()
        .map(|p| p.steamid.as_str())
        .collect::<BTreeSet<_>>();
    for result in [&first, &again, &retry] {
        ensure!(
            result
                .players
                .keys()
                .map(String::as_str)
                .collect::<BTreeSet<_>>()
                == roster,
            "assessment does not contain the exact match roster"
        );
    }
    let enabled = first.players.values().next().expect("match roster")[0]
        .checks
        .iter()
        .map(|check| check.definition.id.clone())
        .collect::<BTreeSet<_>>();
    for required in [
        "shot-hit-rate",
        "first-shot-hit-rate",
        "unbroken-hit-sequence",
        "rapid-multikill",
        "flashed-hit-rate",
        "shot-synchronous-view-turn",
        "bhop-speed-retention",
        "fixed-view-air-strafe",
        "smoke-hit-rate",
        "penetration-hit-rate",
    ] {
        ensure!(
            enabled.contains(required),
            "missing behavior rule: {required}"
        );
    }
    let retry_runs = retry
        .players
        .values()
        .map(|records| records[0].id.rsplit_once('-').map(|(run, _)| run))
        .collect::<BTreeSet<_>>();
    ensure!(
        retry_runs.len() == 1 && !retry_runs.contains(&None),
        "match results were not published together"
    );
    let mut samples = 0;
    let mut findings = 0;
    for player in &parsed.info.players {
        let first_records = &first.players[&player.steamid];
        let again_records = &again.players[&player.steamid];
        let retry_records = &retry.players[&player.steamid];
        for records in [first_records, again_records, retry_records] {
            let latest = records
                .first()
                .ok_or_else(|| anyhow::anyhow!("missing player assessment"))?;
            ensure!(
                latest.player_id == player.steamid
                    && latest.demo_id == meta.id
                    && latest.demo_fingerprint == first.source_fingerprint
                    && latest.ruleset_version == demodesk_core::scoring::RULESET_VERSION,
                "assessment identity mismatch"
            );
            let checks = latest
                .checks
                .iter()
                .map(|check| check.definition.id.clone())
                .collect::<BTreeSet<_>>();
            ensure!(
                checks == enabled && latest.checks.len() == enabled.len(),
                "player missing or duplicating enabled rules"
            );
            for check in &latest.checks {
                if !check.diagnostics.is_null() {
                    ensure!(
                        check.diagnostics["playerId"].as_str() == Some(player.steamid.as_str())
                            && check.diagnostics["demoFingerprint"].as_str()
                                == Some(first.source_fingerprint.as_str()),
                        "rule diagnostics mixed player identities"
                    );
                }
            }
        }
        ensure!(
            first_records[0].id == again_records[0].id,
            "history reuse created duplicates"
        );
        ensure!(
            retry_records.len() == 1
                && again_records.len() == 1
                && retry_records[0].id != again_records[0].id,
            "retry did not replace latest result"
        );
        let saved = engine.scoring_history(&meta.id, &player.steamid)?;
        ensure!(
            saved.len() == 1 && saved[0].id == retry_records[0].id,
            "stored latest differs from response"
        );
        let encoded = serde_json::to_value(&retry_records[0])?;
        ensure!(
            encoded.get("score").is_none()
                && encoded.get("deductions").is_none()
                && encoded.get("minorCap").is_none(),
            "statistics published score fields"
        );
        ensure!(
            retry_records[0].schema_version == 2,
            "unexpected statistics schema"
        );
        let mut independently_summarized = retry_records[0].checks.clone();
        demodesk_core::scoring::statistics::summarize(&mut independently_summarized);
        ensure!(
            serde_json::to_value(&independently_summarized)?
                == serde_json::to_value(&retry_records[0].checks)?,
            "occurrence counts are not stable"
        );
        ensure!(
            retry_records[0]
                .checks
                .iter()
                .all(|c| c.state != demodesk_core::scoring::State::Failed),
            "evaluation failed"
        );
        samples += retry_records[0]
            .checks
            .iter()
            .map(|c| c.evaluated_samples)
            .sum::<usize>();
        findings += retry_records[0]
            .checks
            .iter()
            .map(|c| c.findings.len())
            .sum::<usize>();
        ensure!(
            serde_json::to_value(&first_records[0].checks)?
                == serde_json::to_value(&retry_records[0].checks)?,
            "warm analysis changed results"
        );
    }
    println!(
        "{}",
        serde_json::json!({"parseSeconds":parse_seconds,"firstAssessmentSeconds":first_seconds,
        "preparationSeconds":first.preparation_seconds,"allRulesAllPlayersSeconds":first.analysis_seconds,
        "sharedAssetBytes":first.players.values().next().map(|r|r[0].input_provenance["native"]["sharedAssetBytes"].clone()),"sharedBytes":first.shared_bytes,"genericBytes":first.generic_bytes,"diagnosticBytes":first.diagnostic_bytes,"historyReuseSeconds":reused_seconds,"warmReassessmentSeconds":retry_seconds,
        "warmPreparationSeconds":retry.preparation_seconds,"warmAnalysisSeconds":retry.analysis_seconds,
        "players":parsed.info.players.len(),"samples":samples,"findings":findings,
        "ttd":retry.players.iter().map(|(id, records)|serde_json::json!({"playerId":id,"samples":records[0].checks.iter().find(|c|c.definition.id=="time-to-damage").map(|c|c.evaluated_samples)})).collect::<Vec<_>>(),
        "mode":if cfg!(debug_assertions) {"debug"} else {"release"}})
    );
    ensure!(
        samples > 0,
        "acceptance failed: no measured player samples reached the rules"
    );
    {
        ensure!(
            first.diagnostic_bytes == 0,
            "acceptance failed: analysis depended on a separately prepared body journal"
        );
        ensure!(
            first.preparation_seconds <= 30.0,
            "acceptance failed: preparation exceeded 30 seconds"
        );
        ensure!(
            first.analysis_seconds <= 30.0,
            "acceptance failed: all-player analysis exceeded 30 seconds"
        );
    }
    Ok(())
}
