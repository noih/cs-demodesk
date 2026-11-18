//! Shoulder paths qualified against the whole intro, including the approach to POV.
use crate::analysis::line_of_sight::{Occlusion, Vec3};
use crate::render::demo_data::Tracks;
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Keyframe {
    pub tick: i32,
    pub eye: Vec3,
    pub position: Vec3,
    pub target: Vec3,
}
fn mix(a: Vec3, b: Vec3, t: f64) -> Vec3 {
    std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t)
}
fn distance(a: Vec3, b: Vec3) -> f64 {
    (0..3).map(|i| (a[i] - b[i]).powi(2)).sum::<f64>().sqrt()
}
fn blend(tick: f64, start: i32, end: f64, rate: f64) -> f64 {
    ((tick - f64::from(start) - rate * 1.2) / (end - f64::from(start) - rate * 1.2 - 1.).max(1.))
        .clamp(0., 1.)
        .powi(3)
}

pub(super) fn plan(
    tracks: &Tracks,
    player: &str,
    start: i32,
    end: f64,
    rate: f64,
    ray: &impl Fn(Vec3, Vec3) -> Occlusion,
) -> Option<Vec<Keyframe>> {
    let finish = (end + rate * 0.125).ceil() as i32;
    let before = *tracks.range(..=start).next_back()?.0;
    let after = *tracks.range(finish..).next()?.0;
    let mut samples = vec![];
    for (&tick, players) in tracks.range(before..=after) {
        let p = players.iter().find(|p| p.id == player && p.alive)?;
        let view = p.view.as_ref()?;
        let eye = [
            p.position[0],
            p.position[1],
            p.position[2] + 64. - 18. * view.duck.clamp(0., 1.),
        ];
        samples.push((tick, eye, view.yaw, view.pitch.clamp(-25., 25.)));
    }
    if samples.len() < 2
        || samples
            .windows(2)
            .any(|w| f64::from(w[1].0 - w[0].0) > rate / 4.)
    {
        return None;
    }
    let mut best: Option<(f64, Vec<Keyframe>)> = None;
    for ahead in [128., 64., 32.] {
        for back in [40., 56., 72., 88.] {
            for side in [28., -28., 40., -40., 56., -56.] {
                for lift in [8., 16.] {
                    let path: Vec<_> = samples
                        .iter()
                        .map(|&(tick, eye, yaw, pitch)| {
                            let yaw = yaw.to_radians();
                            let pitch = pitch.to_radians();
                            let target = [
                                eye[0] + ahead * pitch.cos() * yaw.cos(),
                                eye[1] + ahead * pitch.cos() * yaw.sin(),
                                eye[2] - ahead * pitch.sin(),
                            ];
                            Keyframe {
                                tick,
                                eye,
                                target,
                                position: [
                                    eye[0] - back * yaw.cos() + side * yaw.sin(),
                                    eye[1] - back * yaw.sin() - side * yaw.cos(),
                                    eye[2] + lift,
                                ],
                            }
                        })
                        .collect();
                    let mut previous = None;
                    let mut travel = 0.;
                    let mut valid = true;
                    for tick in start..=finish {
                        let index = path
                            .partition_point(|p| p.tick < tick)
                            .min(path.len() - 1)
                            .max(1);
                        let (a, b) = (&path[index - 1], &path[index]);
                        let t =
                            (f64::from(tick - a.tick) / f64::from(b.tick - a.tick)).clamp(0., 1.);
                        let eye = mix(a.eye, b.eye, t);
                        let amount = blend(f64::from(tick), start, end, rate);
                        let position = mix(mix(a.position, b.position, t), eye, amount);
                        let target = mix(a.target, b.target, t);
                        // The lens and its swept envelope must remain on the player's side of walls.
                        let visible =
                            |a, b| distance(a, b) < 0.001 || ray(a, b) == Occlusion::Clear;
                        let clear = visible(eye, position)
                            && (amount > 0.95 || visible(position, target))
                            && (0..3).all(|axis| {
                                [-10., 10.].iter().all(|&delta| {
                                    let mut edge = position;
                                    edge[axis] += delta;
                                    visible(position, edge)
                                        && previous.is_none_or(|p: Vec3| {
                                            let mut from = p;
                                            from[axis] += delta;
                                            visible(from, edge)
                                        })
                                })
                            });
                        // No other player may occupy the lens; the main model is hidden before docking.
                        let bodies =
                            tracks
                                .range(..=tick)
                                .next_back()
                                .is_some_and(|(_, players)| {
                                    players.iter().all(|p| {
                                        !p.alive
                                            || (p.id == player
                                                && f64::from(tick) >= end - rate * 0.15)
                                            || (position[0] - p.position[0])
                                                .hypot(position[1] - p.position[1])
                                                >= 28.
                                            || position[2] < p.position[2] - 10.
                                            || position[2] > p.position[2] + 86.
                                    })
                                });
                        if !clear || !bodies {
                            valid = false;
                            break;
                        }
                        if let Some(p) = previous {
                            travel += distance(p, position);
                        }
                        previous = Some(position);
                    }
                    if valid {
                        let score = ahead * 0.05
                            - travel * 0.1
                            - (back - 72_f64).abs() * 0.3
                            - (side.abs() - 40_f64).abs() * 0.1
                            - lift * 0.02;
                        if best.as_ref().is_none_or(|(s, _)| score > *s) {
                            best = Some((score, path));
                        }
                    }
                }
            }
        }
    }
    best.map(|(_, path)| path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::demo_data::{Player, PlayerView};
    fn tracks() -> Tracks {
        (0..=256)
            .step_by(8)
            .map(|tick| {
                (
                    tick,
                    vec![Player {
                        id: "main".into(),
                        team: 2,
                        alive: true,
                        position: [if tick % 16 == 0 { 0.5 } else { -0.5 }, 0., 0.],
                        view: Some(PlayerView {
                            yaw: 0.,
                            pitch: 0.,
                            duck: 0.,
                        }),
                    }],
                )
            })
            .collect()
    }
    #[test]
    fn stationary_jitter_keeps_a_real_shoulder_distance() {
        let path = plan(&tracks(), "main", 100, 207., 64., &|_, _| Occlusion::Clear).unwrap();
        assert!(path
            .iter()
            .all(|p| (p.position[0] - p.eye[0]).hypot(p.position[1] - p.eye[1]) >= 45.));
    }
    #[test]
    fn clear_endpoints_do_not_allow_a_blocked_middle() {
        let mut tracks = tracks();
        for (&tick, players) in &mut tracks {
            if (136..=176).contains(&tick) {
                players[0].position[0] = 300.;
            }
        }
        assert!(plan(&tracks, "main", 100, 207., 64., &|a, b| {
            if a[0] > 200. || b[0] > 200. {
                Occlusion::Blocked
            } else {
                Occlusion::Clear
            }
        })
        .is_none());
    }

    #[test]
    fn blocked_side_is_rejected_and_unknown_geometry_never_passes() {
        let path = plan(&tracks(), "main", 100, 207., 64., &|a, b| {
            if a[1] < -20. || b[1] < -20. {
                Occlusion::Blocked
            } else {
                Occlusion::Clear
            }
        })
        .unwrap();
        assert!(path.iter().all(|p| p.position[1] > 0.));
        assert!(plan(&tracks(), "main", 100, 207., 64., &|_, _| {
            Occlusion::Unknown
        })
        .is_none());
        let mut missing = tracks();
        missing.get_mut(&152).unwrap().clear();
        assert!(plan(&missing, "main", 100, 207., 64., &|_, _| Occlusion::Clear).is_none());
    }
}
