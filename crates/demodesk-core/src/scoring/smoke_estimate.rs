//! Directional smoke samples; deliberately excludes uncertain and HE-disturbed paths.
use super::combat_stats::SmokeVerdict;
use crate::analysis::{
    ballistics::Fire,
    event_context::{Events, Shot},
    line_of_sight::{Occlusion, World},
    native_body::{PlayerFrame, SceneOcclusion},
};
use serde_json::Value;

pub struct Estimator {
    explosions: Vec<(i32, Option<[f64; 3]>)>,
    rate: f64,
}
impl Estimator {
    pub fn new(events: &[Value], rate: f64) -> Self {
        let mut explosions: Vec<_> = events
            .iter()
            .filter(|e| e["event_name"] == "hegrenade_detonate")
            .filter_map(|e| {
                Some((
                    i32::try_from(e["tick"].as_i64()?).ok()?,
                    (|| Some([e["x"].as_f64()?, e["y"].as_f64()?, e["z"].as_f64()?]))()
                        .filter(|p| p.iter().all(|v| v.is_finite())),
                ))
            })
            .collect();
        explosions.sort_by_key(|e| e.0);
        Self { explosions, rate }
    }
    pub fn classify(
        &self,
        shot: &Shot,
        events: &Events,
        weapons: Option<&Value>,
        players: &[PlayerFrame],
        scene: &SceneOcclusion,
        smoke: &impl crate::smoke::ShotCoverage,
        world: Option<&World>,
    ) -> SmokeVerdict {
        use SmokeVerdict::*;
        if smoke.is_empty_at_fire() {
            return Clear;
        }
        let Some(world) = world else {
            return Unknown;
        };
        let Some(weapons) = weapons else {
            return Unknown;
        };
        let Some(bullet) = events.bullet_for_shot(shot, weapons) else {
            return Unknown;
        };
        let Ok(fire) = Fire::from_event(bullet, weapons) else {
            return Unknown;
        };
        let Some(candidates) = smoke.candidates(fire.message_tick, fire.origin, &fire.deltas)
        else {
            return Unknown;
        };
        self.paths(
            shot.tick,
            &shot.player_id,
            fire.origin,
            &candidates,
            players,
            scene,
            world,
        )
    }
    fn paths(
        &self,
        tick: i32,
        shooter: &str,
        origin: [f32; 3],
        candidates: &[([f32; 3], [f32; 3])],
        players: &[PlayerFrame],
        scene: &SceneOcclusion,
        world: &World,
    ) -> SmokeVerdict {
        use SmokeVerdict::*;
        let first = self
            .explosions
            .partition_point(|(t, _)| f64::from(*t) < f64::from(tick) - self.rate * 5.);
        let last = self.explosions.partition_point(|(t, _)| *t <= tick);
        if candidates.iter().any(|(origin, _)| {
            self.explosions[first..last].iter().any(|(_, p)| {
                p.is_none_or(|p| {
                    (0..3)
                        .map(|i| ((p[i] - f64::from(origin[i])).abs() - 320.).max(0.).powi(2))
                        .sum::<f64>()
                        <= 256_f64.powi(2)
                })
            })
        }) {
            return Unknown;
        }
        let mut uncertain = false;
        for &(_, point) in candidates {
            let start = origin.map(f64::from);
            let point64 = point.map(f64::from);
            if scene.unbounded || scene.uncertain.iter().any(|b| b.intersects(start, point64)) {
                uncertain = true;
                continue;
            }
            if point == origin {
                return Crossing;
            }
            match world.ray(start, point64) {
                Occlusion::Clear => {}
                Occlusion::Blocked | Occlusion::Unknown => {
                    uncertain = true;
                    continue;
                }
            }
            // Current body poses are sufficient for this estimate; uncertain penetration is excluded.
            let ray = std::array::from_fn(|i| point[i] - origin[i]);
            let mut blocked = false;
            for player in players.iter().filter(|p| p.player_id != shooter) {
                for body in &player.capsules {
                    match crate::analysis::collision::capsule::contacts(
                        body.a.map(|v| v as f32),
                        body.b.map(|v| v as f32),
                        body.radius as f32,
                        origin,
                        ray,
                        1.,
                    ) {
                        Ok(hits) if hits.is_empty() => {}
                        _ => {
                            blocked = true;
                            break;
                        }
                    }
                }
                if blocked {
                    break;
                }
            }
            if blocked {
                uncertain = true;
                continue;
            }
            return Crossing;
        }
        if uncertain {
            Unknown
        } else {
            Clear
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::line_of_sight::{Bounds, Triangle};
    #[test]
    fn sample_paths_exclude_he_walls_and_unknown_geometry_without_losing_inside_smoke_shots() {
        let empty = World::new(vec![Triangle {
            vertices: [[500., -100., -100.], [500., 100., -100.], [500., 0., 100.]],
            opaque: true,
        }])
        .unwrap();
        let scene = SceneOcclusion::default();
        let estimate = Estimator::new(&[], 64.);
        let candidates = [([100., 0., 0.], [100., 0., 0.])];
        assert_eq!(
            estimate.paths(640, "a", [0.; 3], &candidates, &[], &scene, &empty),
            SmokeVerdict::Crossing
        );
        assert_eq!(
            estimate.paths(640, "a", [100., 0., 0.], &candidates, &[], &scene, &empty),
            SmokeVerdict::Crossing
        );
        let wall = World::new(vec![Triangle {
            vertices: [[50., -100., -100.], [50., 100., -100.], [50., 0., 100.]],
            opaque: true,
        }])
        .unwrap();
        assert_eq!(
            estimate.paths(640, "a", [0.; 3], &candidates, &[], &scene, &wall),
            SmokeVerdict::Unknown
        );
        let dynamic = SceneOcclusion {
            uncertain: vec![Bounds {
                min: [20., -5., -5.],
                max: [30., 5., 5.],
            }],
            ..Default::default()
        };
        assert_eq!(
            estimate.paths(640, "a", [0.; 3], &candidates, &[], &dynamic, &empty),
            SmokeVerdict::Unknown
        );
        let he = Estimator::new(
            &[
                serde_json::json!({"event_name":"hegrenade_detonate","tick":600,"x":100.,"y":0.,"z":0.}),
            ],
            64.,
        );
        assert_eq!(
            he.paths(640, "a", [0.; 3], &candidates, &[], &scene, &empty),
            SmokeVerdict::Unknown
        );
        assert_eq!(
            he.paths(921, "a", [0.; 3], &candidates, &[], &scene, &empty),
            SmokeVerdict::Crossing
        );
        let mixed = [([2000., 0., 0.], [2000., 0., 0.]), candidates[0]];
        assert_eq!(
            he.paths(640, "a", [0.; 3], &mixed, &[], &scene, &empty),
            SmokeVerdict::Unknown
        );
        let unknown = Estimator::new(
            &[serde_json::json!({"event_name":"hegrenade_detonate","tick":600})],
            64.,
        );
        assert_eq!(
            unknown.paths(640, "a", [0.; 3], &candidates, &[], &scene, &empty),
            SmokeVerdict::Unknown
        );
    }
}
