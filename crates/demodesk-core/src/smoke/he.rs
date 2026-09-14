// SPDX-License-Identifier: GPL-3.0-only
//! HE registration from the recorded explosion and qualified nearest geometry.
use super::effects::{Effects, HeRecord};
use crate::analysis::collision::asset::NearestHit;
use anyhow::{ensure, Result};

#[derive(Clone, Copy, Debug)]
pub struct Explosion {
    pub identity: u64,
    pub position: [f32; 3],
    /// m_nExplodeEffectTickBegin in network clock units, not the demo frame tick.
    pub effect_tick: i32,
}

pub struct RegisteredSmoke<'a> {
    /// Atlas slot shared with the sampled scene. Canonical replay holds this
    /// assignment until removal; client reinitialization is not inferred here.
    pub slot: u8,
    pub bounds: [[f32; 3]; 2],
    /// The first nonempty seed list seen by this scene object's update. These
    /// centres stay fixed even when later density journal records replace seeds.
    pub seed_centres: &'a [[f32; 3]],
}

fn distance_squared(a: [f32; 3], b: [f32; 3]) -> f32 {
    let d = std::array::from_fn::<_, 3, _>(|i| a[i] - b[i]);
    (d[1] * d[1] + d[0] * d[0]) + d[2] * d[2]
}
fn bounds_in_range(point: [f32; 3], bounds: [[f32; 3]; 2]) -> bool {
    let nearest = std::array::from_fn(|i| point[i].clamp(bounds[0][i], bounds[1][i]));
    distance_squared(point, nearest) < 256. * 256.
}

/// The callback must return the nearest collider under SmokeBlast filtering,
/// including qualified dynamic bodies. None means a proved miss; missing scene
/// or filter inputs must return Err. No ring entry is inserted on an error.
/// Apply once on the recorded explosion-state change, in scene update order.
/// A replay seek must reconstruct slots and effect history together; retained
/// masks cannot be carried across a fresh allocation of existing scenes.
pub fn register(
    effects: &mut Effects,
    explosion: Explosion,
    smokes: &[RegisteredSmoke<'_>],
    mut nearest: impl FnMut([f32; 3], [f32; 3]) -> Result<Option<NearestHit>>,
) -> Result<bool> {
    ensure!(
        explosion.effect_tick > 0 && explosion.position.iter().all(|x| x.is_finite()),
        "invalid HE explosion"
    );
    let mut slots = 0_u32;
    for smoke in smokes {
        ensure!(
            smoke.slot < 16 && slots & (1 << smoke.slot) == 0,
            "invalid or duplicate smoke slot"
        );
        slots |= 1 << smoke.slot;
        ensure!(
            smoke
                .bounds
                .iter()
                .flatten()
                .chain(smoke.seed_centres.iter().flatten())
                .all(|x| x.is_finite())
                && (0..3).all(|i| smoke.bounds[0][i] <= smoke.bounds[1][i]),
            "invalid registered smoke geometry"
        );
    }
    let mut mask = 0;
    for smoke in smokes {
        if !bounds_in_range(explosion.position, smoke.bounds) {
            continue;
        }
        for &point in smoke.seed_centres {
            if distance_squared(explosion.position, point) >= 256. * 256. {
                continue;
            }
            // A coincident point still needs the scene's start-solid query.
            // The geometry provider must not turn an unsupported zero ray into a miss.
            let hit = nearest(explosion.position, point)?;
            if let Some(hit) = hit {
                ensure!(hit.fraction.is_finite(), "invalid HE nearest fraction");
            }
            let blocked =
                hit.is_some_and(|h| (h.fraction < 1. || h.start_solid) && h.entity == 0x8000);
            if !blocked {
                mask |= 1 << smoke.slot;
                break;
            }
        }
    }
    effects.push_he(HeRecord {
        identity: explosion.identity,
        position: explosion.position,
        time: explosion.effect_tick as f32 * 0.015625,
        mask,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nearest_identity_and_unknown_scene_control_registration_atomically() {
        let points = [[20., 0., 0.]];
        let smokes = [RegisteredSmoke {
            slot: 3,
            bounds: [[0.; 3], [30.; 3]],
            seed_centres: &points,
        }];
        let explosion = Explosion {
            identity: 7,
            position: [0.; 3],
            effect_tick: 640,
        };
        let hit = |entity, fraction, start_solid| NearestHit {
            entity,
            fraction,
            start_solid,
            part: 0,
            shape: 0,
            attribute: 0,
            surface: 0,
        };
        let mut effects = Effects::default();
        assert!(
            !register(&mut effects, explosion, &smokes, |_, _| Ok(Some(hit(
                0x8000, 0.3, false
            ))))
            .unwrap()
        );
        assert!(
            !register(&mut effects, explosion, &smokes, |_, _| Ok(Some(hit(
                0x8000, 1., true
            ))))
            .unwrap()
        );
        assert!(
            register(&mut effects, explosion, &smokes, |_, _| Ok(Some(hit(
                0xfb016b, 0.3, false
            ))))
            .unwrap()
        );
        let before = effects.pack(10.).unwrap();
        assert_eq!(before.he_count, 1);
        assert_eq!(before.he[0].mask, 8.);
        assert_eq!(before.he[0].position_time[3], 10.);
        let other = [
            RegisteredSmoke {
                slot: 1,
                bounds: [[0.; 3], [30.; 3]],
                seed_centres: &points,
            },
            RegisteredSmoke {
                slot: 3,
                bounds: [[0.; 3], [30.; 3]],
                seed_centres: &points,
            },
        ];
        let mut calls = 0;
        assert!(register(&mut effects, explosion, &other, |_, _| {
            calls += 1;
            if calls == 1 {
                Ok(None)
            } else {
                anyhow::bail!("unqualified dynamic body")
            }
        })
        .is_err());
        assert_eq!(effects.pack(10.).unwrap().he_count, before.he_count);
        let boundary = [[256., 0., 0.]];
        let outside = [RegisteredSmoke {
            slot: 0,
            bounds: [[256., 0., 0.], [300.; 3]],
            seed_centres: &boundary,
        }];
        assert!(!register(&mut effects, explosion, &outside, |_, _| panic!(
            "256 HU boundary must not trace"
        ))
        .unwrap());
    }
}
