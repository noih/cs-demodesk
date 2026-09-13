use anyhow::{ensure, Result};
use demodesk_core::analysis::native_body;
use std::{path::Path, time::Instant};
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    ensure!(args.len() == 4, "expected STATE GAME VRF CACHE");
    let start = Instant::now();
    let prepared = native_body::prepare(
        Path::new(&args[0]),
        Path::new(&args[3]),
        Path::new(&args[1]),
        Path::new(&args[2]),
    )?;
    let skeleton = &prepared.skeleton;
    println!(
        "{}",
        serde_json::json!({"preparationSeconds":start.elapsed().as_secs_f64(),"assetBytes":prepared.assets.total_bytes})
    );
    let start = Instant::now();
    let mut eyes = 0u64;
    let mut players = std::collections::BTreeSet::new();
    let coverage = native_body::visit(Path::new(&args[0]), &prepared, skeleton, |_, frame| {
        for p in frame {
            players.insert(p.player_id.clone());
            eyes += u64::from(p.eye.is_some() && p.view.is_some());
        }
        Ok(())
    })?;
    println!(
        "{}",
        serde_json::json!({"analysisSeconds":start.elapsed().as_secs_f64(),"players":players.len(),"eyeFrames":eyes,"coverage":coverage})
    );
    ensure!(coverage.measured_frames > 0, "no native measured frames");
    Ok(())
}
