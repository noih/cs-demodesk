//! Experimental fixed-shot search. No map coordinates or player identities in the solver.
use crate::analysis::line_of_sight::{Occlusion, Vec3};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Deserialize, Serialize)]
pub struct Sample {
    pub tick: i32,
    pub subjects: Vec<Vec3>, // Feet positions, with a conservative standing body envelope.
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub position: Vec3,
    pub target: Vec3,
    pub fov: f64,
    pub score: f64,
    pub minimum_body_height: f64,
    pub minimum_separation: f64,
}
#[derive(Serialize)]
pub struct Search {
    pub evaluated: usize,
    pub rejected_clearance: usize,
    pub rejected_framing: usize,
    pub rejected_occlusion: usize,
    pub candidates: Vec<Candidate>,
    pub occlusion_diagnostics: Vec<serde_json::Value>,
}
fn sub(a: Vec3, b: Vec3) -> Vec3 {
    std::array::from_fn(|i| a[i] - b[i])
}
fn dot(a: Vec3, b: Vec3) -> f64 {
    (0..3).map(|i| a[i] * b[i]).sum()
}
fn normalized(v: Vec3) -> Vec3 {
    let length = dot(v, v).sqrt();
    v.map(|x| x / length)
}
fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

pub fn search(samples: &[Sample], ray: impl Fn(Vec3, Vec3) -> Occlusion) -> Result<Search> {
    ensure!(samples.len() >= 2, "camera search needs a time interval");
    let count = samples[0].subjects.len();
    ensure!(
        count >= 2
            && samples
                .iter()
                .all(|s| s.subjects.len() == count
                    && s.subjects.iter().flatten().all(|v| v.is_finite())),
        "invalid camera subjects"
    );
    ensure!(
        samples.windows(2).all(|s| s[0].tick < s[1].tick),
        "camera samples must increase"
    );
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for p in samples.iter().flat_map(|s| &s.subjects) {
        for i in 0..3 {
            lo[i] = lo[i].min(p[i]);
            hi[i] = hi[i].max(p[i]);
        }
    }
    let target = std::array::from_fn(|i| (lo[i] + hi[i]) / 2. + if i == 2 { 36. } else { 0. });
    let span = (hi[0] - lo[0]).hypot(hi[1] - lo[1]);
    let base = (span * 0.7).clamp(200., 600.);
    let mut result = Search {
        evaluated: 0,
        rejected_clearance: 0,
        rejected_framing: 0,
        rejected_occlusion: 0,
        candidates: vec![],
        occlusion_diagnostics: vec![],
    };
    for direction in 0..24 {
        for radius in [base, base * 1.5, base * 2.] {
            for height in [80., 160., 260.] {
                let angle = (direction as f64 * 15.).to_radians();
                let position = [
                    target[0] + radius * angle.cos(),
                    target[1] + radius * angle.sin(),
                    hi[2] + height,
                ];
                let forward = normalized(sub(target, position));
                let right = normalized(cross(forward, [0., 0., 1.]));
                let up = cross(right, forward);
                // A small camera envelope rejects surfaces touching the lens. Subject rays also
                // reject enclosed positions. This is not a general point-in-solid proof.
                let clear = (0..3).all(|axis| {
                    [-12., 12.].iter().all(|offset| {
                        let mut p = position;
                        p[axis] += offset;
                        ray(position, p) == Occlusion::Clear
                    })
                });
                for fov in [60_f64, 75., 90.] {
                    result.evaluated += 1;
                    if !clear {
                        result.rejected_clearance += 1;
                        continue;
                    }
                    // Source's base FOV is horizontal at 4:3; use conservative 4:3 bounds
                    // even when recording 16:9, whose horizontal coverage is wider.
                    let tangent = (fov.to_radians() / 2.).tan();
                    let project = |point: Vec3| -> Option<[f64; 2]> {
                        let d = sub(point, position);
                        let depth = dot(d, forward);
                        (depth > 16.).then(|| {
                            [
                                dot(d, right) / depth / tangent,
                                dot(d, up) / depth / tangent * 4. / 3.,
                            ]
                        })
                    };
                    let mut min_height = f64::INFINITY;
                    let mut min_separation = f64::INFINITY;
                    let mut framed = true;
                    for sample in samples {
                        let mut centers = vec![];
                        for p in &sample.subjects {
                            let mut bottom = f64::INFINITY;
                            let mut top = f64::NEG_INFINITY;
                            for dx in [-16., 16.] {
                                for dy in [-16., 16.] {
                                    for dz in [0., 72.] {
                                        match project([p[0] + dx, p[1] + dy, p[2] + dz]) {
                                            Some([x, y]) if x.abs() <= 0.86 && y.abs() <= 0.86 => {
                                                bottom = bottom.min(y);
                                                top = top.max(y);
                                            }
                                            _ => framed = false,
                                        }
                                    }
                                }
                            }
                            min_height = min_height.min((top - bottom) / 2.);
                            if let Some(center) = project([p[0], p[1], p[2] + 36.]) {
                                centers.push(center);
                            } else {
                                framed = false;
                            }
                        }
                        for i in 0..centers.len() {
                            for j in i + 1..centers.len() {
                                min_separation = min_separation.min(
                                    (centers[i][0] - centers[j][0])
                                        .hypot(centers[i][1] - centers[j][1])
                                        / 2.,
                                );
                            }
                        }
                    }
                    if !framed || min_height < 0.075 || min_separation < 0.06 {
                        result.rejected_framing += 1;
                        continue;
                    }
                    // Unknown material intersections are not treated as visible. Every sampled
                    // subject must expose both chest and head for this first strict prototype.
                    let mut failures = vec![];
                    for s in samples {
                        for (subject, p) in s.subjects.iter().enumerate() {
                            for dz in [36., 64.] {
                                let state = ray(position, [p[0], p[1], p[2] + dz]);
                                if state != Occlusion::Clear {
                                    failures.push(serde_json::json!({"tick":s.tick,"subject":subject,"height":dz,"state":format!("{state:?}")}));
                                }
                            }
                        }
                    }
                    if !failures.is_empty() {
                        result.rejected_occlusion += 1;
                        result.occlusion_diagnostics.push(
                            serde_json::json!({"position":position,"fov":fov,"failures":failures}),
                        );
                        continue;
                    }
                    let score = 100. * min_height + 30. * min_separation.min(0.4) - 0.01 * fov;
                    result.candidates.push(Candidate {
                        position,
                        target,
                        fov,
                        score,
                        minimum_body_height: min_height,
                        minimum_separation: min_separation,
                    });
                }
            }
        }
    }
    result
        .candidates
        .sort_by(|a, b| b.score.total_cmp(&a.score));
    Ok(result)
}

// The lead-in may be hidden by cover before engagement. Shorten only the lead-in,
// never discard any part of the required combat interval to manufacture a shot.
pub(super) fn search_window(
    samples: &[Sample],
    required_tick: i32,
    ray: impl Fn(Vec3, Vec3) -> Occlusion,
) -> Result<(i32, Search)> {
    ensure!(
        samples.first().is_some_and(|s| s.tick <= required_tick)
            && samples.last().is_some_and(|s| s.tick >= required_tick),
        "required combat is outside samples"
    );
    let mut result = search(samples, &ray)?;
    let mut start = samples[0].tick;
    for index in 1..samples.len() - 1 {
        if !result.candidates.is_empty() || samples[index].tick > required_tick {
            break;
        }
        start = samples[index].tick;
        result = search(&samples[index..], &ray)?;
    }
    Ok((start, result))
}

#[test]
fn may_trim_hidden_lead_in_but_never_required_combat() {
    let samples = vec![
        Sample {
            tick: 0,
            subjects: vec![[-100., 0., 0.], [100., 0., 0.]],
        },
        Sample {
            tick: 8,
            subjects: vec![[-120., 0., 0.], [120., 0., 0.]],
        },
        Sample {
            tick: 16,
            subjects: vec![[-130., 0., 0.], [130., 0., 0.]],
        },
    ];
    let ray = |_: Vec3, b: Vec3| {
        if b[0] == 100. && b[1] == 0. && [36., 64.].contains(&b[2]) {
            Occlusion::Blocked
        } else {
            Occlusion::Clear
        }
    };
    let (start, result) = search_window(&samples, 8, ray).unwrap();
    assert_eq!(start, 8);
    assert!(!result.candidates.is_empty());
    assert!(search_window(&samples, 0, ray)
        .unwrap()
        .1
        .candidates
        .is_empty());
}

#[test]
fn whole_interval_framing_and_missing_visibility() {
    let samples = vec![
        Sample {
            tick: 0,
            subjects: vec![[-100., 0., 0.], [100., 0., 0.]],
        },
        Sample {
            tick: 16,
            subjects: vec![[-150., 40., 0.], [130., -30., 0.]],
        },
    ];
    let clear = search(&samples, |_, _| Occlusion::Clear).unwrap();
    assert_eq!(clear.evaluated, 648);
    assert!(!clear.candidates.is_empty());
    assert!(clear
        .candidates
        .iter()
        .all(|c| c.minimum_body_height >= 0.075 && c.minimum_separation >= 0.06));
    for state in [Occlusion::Blocked, Occlusion::Unknown] {
        assert!(search(&samples, |_, _| state)
            .unwrap()
            .candidates
            .is_empty());
    }
    assert!(search(&samples[..1], |_, _| Occlusion::Clear).is_err());
}

#[test]
fn obstruction_late_in_shot_disqualifies_camera() {
    let samples = vec![
        Sample {
            tick: 0,
            subjects: vec![[-100., 0., 0.], [100., 0., 0.]],
        },
        Sample {
            tick: 16,
            subjects: vec![[-150., 40., 0.], [130., -30., 0.]],
        },
    ];
    let result = search(&samples, |_, b| {
        if b[0] == 130. && b[1] == -30. && [36., 64.].contains(&b[2]) {
            Occlusion::Blocked
        } else {
            Occlusion::Clear
        }
    })
    .unwrap();
    assert!(result.candidates.is_empty());
    assert!(result.rejected_occlusion > 0);
}

#[test]
#[ignore = "loads installed map geometry and a private camera fixture"]
fn plan_camera_experiment() -> Result<()> {
    use crate::analysis::visibility_assets;
    use std::{fs, path::PathBuf};
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Input {
        data_root: PathBuf,
        game: PathBuf,
        vrf: PathBuf,
        map: String,
        samples: Vec<Sample>,
        required_tick: i32,
        output: PathBuf,
    }
    let spec: Input = serde_json::from_slice(&fs::read(std::env::var("DEMODESK_CAMERA_SEARCH")?)?)?;
    let prepared = visibility_assets::prepare(&spec.data_root, &spec.game, &spec.vrf, &spec.map)?;
    let (start_tick, result) = search_window(&spec.samples, spec.required_tick, |a, b| {
        prepared.world.ray(a, b)
    })?;
    let report = serde_json::json!({"startTick":start_tick,"requiredTick":spec.required_tick,"search":result,"geometryFingerprint":prepared.fingerprint,"limits":"Static physics approximation; standing body envelopes; no dynamic props/smoke or demo/map version proof."});
    fs::write(&spec.output, serde_json::to_vec_pretty(&report)?)?;
    println!("CAMERA_SEARCH {}", spec.output.display());
    Ok(())
}
