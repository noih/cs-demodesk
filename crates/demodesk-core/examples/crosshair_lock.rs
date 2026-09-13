//! Run crosshair-lock measurements on explicitly supplied body-point samples.
//! Usage: cargo run -p demodesk-core --example crosshair_lock -- INPUT.json OUTPUT.json [PARAMETERS.json]
//! Synthetic smoke check: ... -- --synthetic OUTPUT.json
use anyhow::{ensure, Context, Result};
use demodesk_core::scoring::crosshair_lock::{
    evaluate, Input, Obstruction, Parameters, Sample, Track,
};
use std::{io::Write, path::Path};

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    ensure!(
        args.len() == 2 || args.len() == 3,
        "expected INPUT.json OUTPUT.json [PARAMETERS.json], or --synthetic OUTPUT.json"
    );
    let parameters = if let Some(path) = args.get(2) {
        serde_json::from_slice(&std::fs::read(path).context("reading parameters")?)
            .context("decoding parameters")?
    } else {
        Parameters::default()
    };
    let input = if args[0] == "--synthetic" {
        Input {
            demo_fingerprint: "synthetic-only".into(),
            player_id: "subject".into(),
            measurement_source: "synthetic exact body point".into(),
            tick_rate: 64.0,
            sample_step_ticks: 1,
            angular_resolution_degrees: 0.01,
            tracks: vec![Track {
                round: 1,
                target_id: "opponent".into(),
                point_id: "arbitrary-local-point".into(),
                enemy: true,
                samples: (0..65)
                    .map(|tick| {
                        let yaw = f64::from(tick) * 0.2;
                        let a = yaw.to_radians();
                        Sample {
                            tick,
                            eye: [0.0; 3],
                            view: [0.0, if tick == 0 { -20.0 } else { yaw }],
                            target: [a.cos() * 100.0, a.sin() * 100.0, 0.0],
                            obstruction: Obstruction::Unknown,
                        }
                    })
                    .collect(),
            }],
        }
    } else {
        serde_json::from_slice(&std::fs::read(&args[0]).context("reading measurements")?)
            .context("decoding measurements")?
    };
    let report = evaluate(&input, &parameters)?;
    let path = Path::new(&args[1]);
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer_pretty(&mut temporary, &report)?;
    temporary.flush()?;
    temporary.as_file().sync_all()?;
    temporary
        .persist_noclobber(path)
        .context("saving report without overwriting earlier evidence")?;
    println!(
        "{} candidates; experimental only, credit score unavailable",
        report.evidence.len()
    );
    Ok(())
}
