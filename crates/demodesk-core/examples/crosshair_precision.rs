//! Compare coordinate precision against the unchanged crosshair rule, without writing rounded datasets.
use anyhow::{ensure, Result};
use demodesk_core::{
    analysis::body_journal,
    scoring::crosshair_lock::{
        self, Input, Obstruction, Parameters, Report, Sample, Stream, Track,
    },
};
use serde_json::{json, Value};
use std::collections::BTreeMap;
const MODES: [(u32, Option<u32>); 5] =
    [(2, None), (3, None), (4, None), (3, Some(3)), (4, Some(4))];
fn rounded(s: &Sample, decimals: u32, angles: Option<u32>) -> Sample {
    let round = |n: f64, d: u32| {
        let scale = 10f64.powi(d as i32);
        (n * scale).round() / scale
    };
    Sample {
        eye: s.eye.map(|v| round(v, decimals)),
        target: s.target.map(|v| round(v, decimals)),
        view: s.view.map(|v| angles.map_or(v, |d| round(v, d))),
        ..s.clone()
    }
}
fn error(a: &Sample, b: &Sample) -> f64 {
    let direction = |s: &Sample| std::array::from_fn::<_, 3, _>(|i| s.target[i] - s.eye[i]);
    let a = direction(a);
    let b = direction(b);
    let cross = [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ];
    cross
        .iter()
        .map(|v| v * v)
        .sum::<f64>()
        .sqrt()
        .atan2(a.iter().zip(b).map(|(x, y)| x * y).sum::<f64>())
        .to_degrees()
}
fn decision(report: &Report) -> Value {
    json!({"samples":report.evaluated_samples,"evidence":report.evidence.iter().map(|e|json!([e.round,e.target_id,e.point_id,e.start_tick,e.lock_tick,e.end_tick,e.rapid_acquisition,e.sustained_follow,e.obstruction])).collect::<Vec<_>>()})
}
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    ensure!(args.len() == 2, "expected BODY_JOURNAL SUMMARY.json");
    let p = Parameters::default();
    let mut players: BTreeMap<String, Vec<Stream>> = BTreeMap::new();
    let mut max_direction_error = [0f64; 5];
    let mut samples = 0usize;
    let file = std::fs::File::open(&args[0])?;
    body_journal::visit(body_journal::reader(&file)?, |header, frame| {
        let mut inputs: BTreeMap<String, Input> = BTreeMap::new();
        for track in frame.tracks {
            let input = inputs
                .entry(track.player_id.clone())
                .or_insert_with(|| Input {
                    demo_fingerprint: header.source.demo_fingerprint.clone().unwrap_or_default(),
                    player_id: track.player_id.clone(),
                    measurement_source: header.data.measurement_source.clone(),
                    tick_rate: header.data.tick_rate,
                    sample_step_ticks: header.data.sample_step_ticks,
                    angular_resolution_degrees: header.data.angular_resolution_degrees,
                    tracks: vec![],
                });
            let camera = &frame.views[&track.player_id];
            input.tracks.push(Track {
                round: track.round,
                target_id: track.target_id.clone(),
                point_id: track.point_id.clone(),
                enemy: track.enemy,
                samples: vec![Sample {
                    tick: frame.tick,
                    eye: camera.eye,
                    view: camera.view,
                    target: frame.points[&track.point_key],
                    obstruction: Obstruction::Unknown,
                }],
            });
        }
        for (player, input) in inputs {
            if !players.contains_key(&player) {
                players.insert(
                    player.clone(),
                    (0..6)
                        .map(|_| Stream::new(&input, &p))
                        .collect::<Result<_>>()?,
                );
            }
            let streams = players.get_mut(&player).expect("inserted above");
            streams[0].push(&input)?;
            samples += input.tracks.len();
            for (index, (digits, angles)) in MODES.iter().enumerate() {
                let mut modified = input.clone();
                for (old, new) in input.tracks.iter().zip(&mut modified.tracks) {
                    new.samples[0] = rounded(&old.samples[0], *digits, *angles);
                    max_direction_error[index] =
                        max_direction_error[index].max(error(&old.samples[0], &new.samples[0]));
                }
                streams[index + 1].push(&modified)?;
            }
        }
        Ok(())
    })?;
    let mut changed = [0usize; 5];
    let mut original_candidates = 0;
    for streams in players.into_values() {
        let mut reports = streams.into_iter().map(Stream::finish);
        let baseline = reports.next().expect("six modes");
        original_candidates += baseline.evidence.len();
        for (index, report) in reports.enumerate() {
            changed[index] += usize::from(decision(&baseline) != decision(&report));
        }
    }
    // Known trajectories on both sides of the lock threshold, at multiple distances and yaw offsets.
    let mut controls = 0usize;
    let mut control_changes = [0usize; 5];
    let mut new_candidates = [0usize; 5];
    for distance in [16.0, 64.0, 512.0, 2048.0] {
        for offset in [0.00004, 0.0004, 0.004, 0.04] {
            for sign in [-1.0, 1.0] {
                for yaw_offset in [0.0, 0.12345678, 89.98765432] {
                    let input = Input {
                        demo_fingerprint: "synthetic".into(),
                        player_id: "observer".into(),
                        measurement_source: "synthetic boundary control".into(),
                        tick_rate: 64.0,
                        sample_step_ticks: 1,
                        angular_resolution_degrees: 0.01,
                        tracks: vec![Track {
                            round: 1,
                            target_id: "target".into(),
                            point_id: "point".into(),
                            enemy: true,
                            samples: (0..65)
                                .map(|tick| {
                                    let yaw = yaw_offset + f64::from(tick) * 0.2;
                                    let a = yaw.to_radians();
                                    Sample {
                                        tick,
                                        eye: [100.12345678, 50.87654321, 64.12345678],
                                        view: [0.0, yaw + p.max_error_degrees + sign * offset],
                                        target: [
                                            100.12345678 + distance * a.cos(),
                                            50.87654321 + distance * a.sin(),
                                            64.12345678,
                                        ],
                                        obstruction: Obstruction::Unknown,
                                    }
                                })
                                .collect(),
                        }],
                    };
                    let baseline = crosshair_lock::evaluate(&input, &p)?;
                    controls += 1;
                    for (index, (digits, angles)) in MODES.iter().enumerate() {
                        let mut modified = input.clone();
                        for track in &mut modified.tracks {
                            for sample in &mut track.samples {
                                *sample = rounded(sample, *digits, *angles);
                            }
                        }
                        let report = crosshair_lock::evaluate(&modified, &p)?;
                        control_changes[index] +=
                            usize::from(decision(&baseline) != decision(&report));
                        new_candidates[index] += usize::from(
                            baseline.evidence.is_empty() && !report.evidence.is_empty(),
                        );
                    }
                }
            }
        }
    }
    let summary = json!({"realSamples":samples,"realBaselineCandidates":original_candidates,"syntheticBoundaryCases":controls,"modes":MODES.iter().enumerate().map(|(i,(position,view))|json!({"positionDecimals":position,"viewDecimals":view,"maxTargetDirectionErrorDegrees":max_direction_error[i],"realPlayersWithChangedDecision":changed[i],"boundaryCasesWithChangedDecision":control_changes[i],"boundaryCasesWithNewCandidate":new_candidates[i]})).collect::<Vec<_>>()});
    let mut output = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args[1])?;
    serde_json::to_writer_pretty(&mut output, &summary)?;
    output.sync_all()?;
    println!("{summary}");
    Ok(())
}
