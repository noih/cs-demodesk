use anyhow::{ensure, Result};
use demodesk_core::analysis::native_body;
use std::{path::Path, time::Instant};
fn main() -> Result<()> {
    let mut args: Vec<_> = std::env::args().skip(1).collect();
    let diagnostic_output = if let Some(index) = args.iter().position(|arg| arg == "--diagnose") {
        ensure!(index + 1 < args.len(), "--diagnose requires an output path");
        let output = args.remove(index + 1);
        args.remove(index);
        Some(output)
    } else {
        None
    };
    ensure!(
        args.len() == 4 || args.len() == 7,
        "expected STATE GAME VRF CACHE [FIRST_TICK LAST_TICK OUTPUT.json] [--diagnose REPORT.json]"
    );
    let range = if args.len() == 7 {
        let first = args[4].parse::<i32>()?;
        let last = args[5].parse::<i32>()?;
        ensure!(
            first >= 0 && last >= first && last - first < 4096,
            "expected a bounded range of at most 4096 ticks"
        );
        Some(first..=last)
    } else {
        None
    };
    let start = Instant::now();
    let prepared = native_body::prepare(
        Path::new(&args[0]),
        Path::new(&args[3]),
        Path::new(&args[1]),
        Path::new(&args[2]),
    )?;
    let skeleton = &prepared.skeleton;
    ensure!(
        prepared.assets.weapons.is_some(),
        "missing weapon resources for smoke analysis"
    );
    println!(
        "{}",
        serde_json::json!({"preparationSeconds":start.elapsed().as_secs_f64(),"assetBytes":prepared.assets.total_bytes})
    );
    let start = Instant::now();
    let mut eyes = 0u64;
    let mut players = std::collections::BTreeSet::new();
    let mut frames = Vec::new();
    let consume = |tick, frame: &[native_body::PlayerFrame]| {
        if range.as_ref().is_some_and(|range| range.contains(&tick)) {
            frames.push(
                serde_json::json!({"tick":tick,"players":frame.iter().map(|p|serde_json::json!({
                "entity":p.identity_key.0,"serial":p.identity_key.1,"eye":p.eye,"view":p.view,
                "hitboxSet":p.hitbox_set.map(|(model,set)|(model.to_string(),set)),"transforms":p.hitbox_transforms
            })).collect::<Vec<_>>()}),
            );
        }
        for p in frame {
            players.insert(p.player_id.clone());
            eyes += u64::from(p.eye.is_some() && p.view.is_some());
        }
        Ok(())
    };
    let coverage = if diagnostic_output.is_some() {
        native_body::visit_diagnostic(Path::new(&args[0]), &prepared, skeleton, consume)?
    } else {
        native_body::visit(Path::new(&args[0]), &prepared, skeleton, consume)?
    };
    if let Some(path) = diagnostic_output {
        let report = serde_json::to_vec_pretty(&serde_json::json!({
            "stateContract": &prepared.header.contract,
            "source": &prepared.header.source,
            "clientSha256": &prepared.client_sha256,
            "gameContentFingerprint": native_body::game_content_fingerprint(Path::new(&args[1]))?,
            "assetBytes": prepared.assets.total_bytes,
            "coverage": &coverage,
        }))?;
        ensure!(
            report.len() <= 2 * 1024 * 1024,
            "diagnostic report exceeds 2 MiB"
        );
        std::fs::write(path, report)?;
    }
    if range.is_some() {
        let output = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&args[6])?;
        serde_json::to_writer(
            output,
            &serde_json::json!({"frames":frames,"models":prepared.assets.model_hitboxes,
                "source":prepared.header.source,"clientSha256":prepared.client_sha256}),
        )?;
    }
    println!(
        "{}",
        serde_json::json!({"analysisSeconds":start.elapsed().as_secs_f64(),"players":players.len(),"eyeFrames":eyes,"coverage":coverage})
    );
    ensure!(coverage.measured_frames > 0, "no native measured frames");
    Ok(())
}
