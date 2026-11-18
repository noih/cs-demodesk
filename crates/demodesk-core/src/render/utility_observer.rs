//! Geometry-qualified observer cameras for a utility arrival and its victim.
use super::*;
use crate::analysis::line_of_sight::{Occlusion, Vec3};

fn sub(a: Vec3, b: Vec3) -> Vec3 {
    std::array::from_fn(|i| a[i] - b[i])
}
fn dot(a: Vec3, b: Vec3) -> f64 {
    a.iter().zip(b).map(|(a, b)| a * b).sum()
}
fn normalize(a: Vec3) -> Vec3 {
    let n = dot(a, a).sqrt();
    a.map(|v| v / n)
}
fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

// Horizontal straight-line coverage, including foreshortening; not screen-width percentage.
fn path_ratio(full_start: Vec3, full_end: Vec3, shown_start: Vec3, right: Vec3) -> f64 {
    let length = (full_end[0] - full_start[0]).hypot(full_end[1] - full_start[1]);
    if length < 1. {
        return 0.;
    }
    ((full_end[0] - shown_start[0]) * right[0] + (full_end[1] - shown_start[1]) * right[1]).abs()
        / length
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Camera {
    pub(super) start_tick: i32,
    pub(super) end_tick: i32,
    pub(super) position: Vec3,
    pub(super) target: Vec3,
    pub(super) fov: f64,
    pub(super) body_height: f64,
    pub(super) trajectory_width: f64,
    pub(super) path_ratio: f64,
    pub(super) protagonist_visibility: f64,
    pub(super) samples: Vec<camera_search::Sample>,
    pub(super) flight: Vec<(i32, Vec3)>,
}

pub(super) fn search(
    flight: &[(i32, Vec3)],
    bodies: &[(i32, Vec3)],
    protagonist: &[(i32, Vec3)],
    death: i32,
    rate: i32,
    ray: &impl Fn(Vec3, Vec3) -> Occlusion,
) -> (Vec<Camera>, [usize; 4]) {
    let mut rejected = [0; 4];
    let views = camera_search::coarse_to_fine(
        |angle| {
            let (views, counts) =
                search_angles(flight, bodies, protagonist, death, rate, &[angle], ray);
            for i in 0..4 {
                rejected[i] += counts[i];
            }
            views
        },
        compare_views,
    );
    (views, rejected)
}

fn compare_views(a: &Camera, b: &Camera) -> std::cmp::Ordering {
    // Quantize below visual precision so roundoff cannot override the tie-breakers.
    ((b.path_ratio * 1_000_000.).round() as u32)
        .cmp(&((a.path_ratio * 1_000_000.).round() as u32))
        .then_with(|| {
            b.protagonist_visibility
                .total_cmp(&a.protagonist_visibility)
        })
        .then_with(|| b.body_height.total_cmp(&a.body_height))
}

pub(super) fn search_angles(
    flight: &[(i32, Vec3)],
    bodies: &[(i32, Vec3)],
    protagonist: &[(i32, Vec3)],
    death: i32,
    rate: i32,
    angles: &[i32],
    ray: &impl Fn(Vec3, Vec3) -> Occlusion,
) -> (Vec<Camera>, [usize; 4]) {
    let victim = bodies.last().unwrap().1;
    let first = flight[0];
    let last = flight.last().unwrap().1;
    let end = death + editing::pacing::event_tail(rate);
    let mut candidates = vec![];
    let mut rejected = [0; 4];
    let mut starts = vec![
        first.0,
        first.0.max(death - rate * 3 / 2),
        first.0.max(death - rate),
    ];
    starts.sort();
    starts.dedup();
    for start in starts {
        let path: Vec<_> = flight.iter().filter(|(tick, _)| *tick >= start).collect();
        if path.len() < 8 || death - start < rate * 3 / 4 {
            continue;
        }
        let body: Vec<_> = bodies.iter().filter(|(tick, _)| *tick >= start).collect();
        let center: Vec3 = std::array::from_fn(|i| {
            path.iter().map(|(_, p)| p[i]).sum::<f64>() / path.len() as f64
        });
        for weight in [0.25, 0.5] {
            let target = std::array::from_fn(|i| {
                (victim[i] + if i == 2 { 36. } else { 0. }) * (1. - weight) + center[i] * weight
            });
            for angle in angles {
                let angle = (*angle as f64).to_radians();
                for radius in [120., 180., 260., 360., 500., 700.] {
                    for height in [48., 80., 120., 180., 260.] {
                        let position = [
                            victim[0] + radius * angle.cos(),
                            victim[1] + radius * angle.sin(),
                            victim[2] + height,
                        ];
                        if !(0..3).all(|axis| {
                            [-12., 12.].iter().all(|d| {
                                let mut p = position;
                                p[axis] += d;
                                ray(position, p) == Occlusion::Clear
                            })
                        }) {
                            rejected[0] += 1;
                            continue;
                        }
                        let forward = normalize(sub(target, position));
                        let right = normalize(cross(forward, [0., 0., 1.]));
                        let up = cross(right, forward);
                        let ratio = path_ratio(first.1, last, path[0].1, right).clamp(0., 1.);
                        if ratio < 0.2 {
                            rejected[1] += 1;
                            continue;
                        }
                        for fov in [60_f64, 75., 90.] {
                            let tangent = (fov.to_radians() / 2.).tan();
                            let project = |p: Vec3| -> Option<[f64; 2]> {
                                let d = sub(p, position);
                                let depth = dot(d, forward);
                                (depth > 16.).then(|| {
                                    [
                                        dot(d, right) / depth / tangent,
                                        dot(d, up) / depth / tangent * 4. / 3.,
                                    ]
                                })
                            };
                            let in_frame = |p: Vec3| {
                                project(p).is_some_and(|p| p[0].abs() < 0.84 && p[1].abs() < 0.84)
                            };
                            let mut min_height = f64::INFINITY;
                            let bodies_fit = body.iter().all(|(_, p)| {
                                let Some(feet) = project(*p) else {
                                    return false;
                                };
                                let Some(head) = project([p[0], p[1], p[2] + 72.]) else {
                                    return false;
                                };
                                min_height = min_height.min((head[1] - feet[1]).abs() / 2.);
                                [-24., 24.].iter().all(|dx| {
                                    [-24., 24.].iter().all(|dy| {
                                        [0., 72.]
                                            .iter()
                                            .all(|dz| in_frame([p[0] + dx, p[1] + dy, p[2] + dz]))
                                    })
                                })
                            });
                            let mut min_x = f64::INFINITY;
                            let mut max_x = f64::NEG_INFINITY;
                            let path_fits = path.iter().all(|(_, p)| {
                                if !in_frame(*p) {
                                    return false;
                                }
                                let x = project(*p).unwrap()[0];
                                min_x = min_x.min(x);
                                max_x = max_x.max(x);
                                true
                            });
                            let width = (max_x - min_x) / 2.;
                            if !bodies_fit || !path_fits || min_height < 0.12 {
                                rejected[2] += 1;
                                continue;
                            }
                            if !body.iter().all(|(_, p)| {
                                [24., 56.].iter().all(|dz| {
                                    ray(position, [p[0], p[1], p[2] + dz]) == Occlusion::Clear
                                })
                            }) || !path
                                .iter()
                                .all(|(_, p)| ray(position, *p) == Occlusion::Clear)
                            {
                                rejected[3] += 1;
                                continue;
                            }
                            let main: Vec<_> = protagonist
                                .iter()
                                .filter(|(t, _)| *t >= start && *t <= death)
                                .collect();
                            let visible = main
                                .iter()
                                .filter(|(_, p)| {
                                    [24., 56.].iter().all(|dz| {
                                        let p = [p[0], p[1], p[2] + dz];
                                        in_frame(p) && ray(position, p) == Occlusion::Clear
                                    })
                                })
                                .count();
                            candidates.push(Camera {
                                start_tick: start,
                                end_tick: end,
                                position,
                                target,
                                fov,
                                body_height: min_height,
                                trajectory_width: width,
                                path_ratio: ratio,
                                protagonist_visibility: visible as f64 / main.len().max(1) as f64,
                                samples: body
                                    .iter()
                                    .map(|(tick, p)| camera_search::Sample {
                                        tick: *tick,
                                        subjects: vec![*p],
                                    })
                                    .collect(),
                                flight: path.iter().map(|p| **p).collect(),
                            });
                        }
                    }
                }
            }
        }
    }
    // A passing view never ends the search: maximize path coverage before protagonist framing.
    candidates.sort_by(compare_views);
    (candidates, rejected)
}

#[test]
fn path_ratio_uses_full_horizontal_chord() {
    let start = [0., 0., 100.];
    let end = [100., 0., 0.];
    assert_eq!(path_ratio(start, end, start, [1., 0., 0.]), 1.);
    assert_eq!(path_ratio(start, end, start, [0., 1., 0.]), 0.);
    assert_eq!(path_ratio(start, end, [80., 0., 500.], [1., 0., 0.]), 0.2);
    assert_eq!(path_ratio(start, start, start, [1., 0., 0.]), 0.);
}

#[test]
fn observer_search_maximizes_path_instead_of_stopping_at_threshold() {
    let flight: Vec<_> = (0..=128)
        .map(|t| (t, [f64::from(t) * 4., 0., 36.]))
        .collect();
    let body: Vec<_> = (0..=184).step_by(4).map(|t| (t, [512., 0., 0.])).collect();
    let (views, _) = search(&flight, &body, &[], 128, 64, &|_, _| Occlusion::Clear);
    assert!(!views.is_empty());
    assert!(
        views[0].path_ratio > 0.5,
        "best ratio: {}",
        views[0].path_ratio
    );
    assert!(views.iter().all(|v| v.path_ratio >= 0.2));
    assert!(views.iter().any(|v| v.path_ratio < 0.5));
    assert!(views
        .windows(2)
        .all(|v| v[0].path_ratio + 0.000001 >= v[1].path_ratio));
    assert!(views.iter().any(|v| v.start_tick > 0));
}
