//! Native ballistic damage over already-qualified air and solid intervals.
//! Geometry collection, same-team flesh policy, and source material binding remain caller responsibilities.
use anyhow::{ensure, Context, Result};
pub mod materials;
pub mod range;
pub mod rewind;
/// A recorded trigger's raw pellet paths before collision/penetration shortening.
/// Do not use these full-range endpoints as the final smoke-query endpoints.
#[derive(Debug)]
pub struct Fire {
    pub message_tick: i32,
    pub origin: [f32; 3],
    pub deltas: Vec<[f32; 3]>,
    pub weapon: super::smoke::weapons::Weapon,
}
impl Fire {
    pub fn from_event(event: &serde_json::Value, resources: &serde_json::Value) -> Result<Self> {
        use super::smoke::{spread, weapons::Weapon};
        ensure!(
            event["event_name"].as_str() == Some("fire_bullets"),
            "expected FireBullets event"
        );
        let integer = |key: &str| -> Result<u32> {
            Ok(u32::try_from(
                event[key]
                    .as_u64()
                    .with_context(|| format!("missing shot {key}"))?,
            )?)
        };
        let number = |key: &str| -> Result<f32> {
            let value = event[key]
                .as_f64()
                .with_context(|| format!("missing shot {key}"))? as f32;
            ensure!(value.is_finite(), "nonfinite shot {key}");
            Ok(value)
        };
        let flag = |key: &str| {
            event[key]
                .as_bool()
                .with_context(|| format!("unqualified shot policy {key}"))
        };
        let item = integer("item_def_index")?;
        let weapon = Weapon::from_resource(resources, item)?;
        let origin = [
            number("origin_x")?,
            number("origin_y")?,
            number("origin_z")?,
        ];
        let directions = spread::raw_directions(spread::Inputs {
            seed: integer("seed")?,
            item_def_index: item,
            mode: integer("mode")?,
            inaccuracy: number("inaccuracy")?,
            spread: number("spread")?,
            recoil: number("recoil_index")?,
            angles: [
                number("angles_x")?,
                number("angles_y")?,
                number("angles_z")?,
            ],
            weapon: spread::WeaponPattern {
                pellets: weapon.pellets as usize,
                pattern_seed: Some(weapon.pattern_seed as i32),
            },
            policy: spread::Policy {
                patterns_enabled: flag("smoke_patterns_enabled")?,
                only_up: flag("smoke_only_up")?,
                maximum_inaccuracy: flag("smoke_maximum_inaccuracy")?,
            },
        })?;
        let deltas: Vec<_> = directions
            .into_iter()
            .map(|d| d.map(|v| v * weapon.range))
            .collect();
        ensure!(
            deltas.iter().flatten().all(|v| v.is_finite()),
            "pellet path overflow"
        );
        Ok(Self {
            message_tick: i32::try_from(
                event["message_tick"]
                    .as_i64()
                    .context("missing shot clock")?,
            )?,
            origin,
            deltas,
            weapon,
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub struct State {
    pub damage: f32,
    pub penetration: f32,
    pub range_modifier: f32,
    pub path_length: f32,
    pub remaining: i32,
}
#[derive(Clone, Copy, Debug)]
pub struct Segment {
    pub start: f32,
    pub end: f32,
    pub solid: bool,
}
#[derive(Clone, Copy, Debug)]
pub struct Surface {
    pub penetration_distance: f32,
    pub kind: u16,
}
pub struct Solid<'a> {
    pub surfaces: &'a [Surface],
    pub endpoints: [[f32; 3]; 2],
    pub pass_bullets: [bool; 2],
    pub same_team_flesh: bool,
}
#[derive(Debug)]
pub struct Outcome {
    pub state: State,
    pub stopped: bool,
}
pub fn advance(mut state: State, segment: Segment, solid: Option<Solid<'_>>) -> Result<Outcome> {
    ensure!(
        [
            state.damage,
            state.penetration,
            state.range_modifier,
            state.path_length,
            segment.start,
            segment.end
        ]
        .iter()
        .all(|v| v.is_finite()),
        "nonfinite ballistic state"
    );
    ensure!(
        state.path_length >= 0. && (0. ..=1.).contains(&state.range_modifier),
        "invalid ballistic range"
    );
    let from = (state.path_length * segment.start).max(0.);
    let to = (state.path_length * segment.end).max(from);
    if !segment.solid {
        state.damage *= state.range_modifier.powf((to - from) / 500.);
        return Ok(Outcome {
            state,
            stopped: false,
        });
    }
    let solid = solid.ok_or_else(|| anyhow::anyhow!("missing solid inputs"))?;
    ensure!(!solid.surfaces.is_empty(), "missing entry surface");
    ensure!(
        !solid.same_team_flesh,
        "same-team flesh requires native friendly-fire policy"
    );
    ensure!(
        solid.endpoints.iter().flatten().all(|v| v.is_finite())
            && solid
                .surfaces
                .iter()
                .all(|s| s.penetration_distance.is_finite()),
        "nonfinite solid inputs"
    );
    let first = solid.surfaces[0];
    if to > 3000. || first.penetration_distance < 0.1 {
        state.remaining = 0;
    }
    if state.remaining <= 0 {
        return Ok(Outcome {
            state,
            stopped: true,
        });
    }
    ensure!(solid.surfaces.len() >= 2, "missing exit surface");
    ensure!(
        state.penetration > 0.,
        "unsupported nonpositive weapon penetration"
    );
    let last = solid.surfaces[1];
    let mut p = solid.surfaces[2..]
        .iter()
        .fold(first.penetration_distance, |v, m| {
            v.min(m.penetration_distance)
        });
    let a = solid.endpoints[0];
    let b = solid.endpoints[1];
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let dz = a[2] - b[2];
    let thickness = ((dz * dz + dy * dy) + dx * dx).sqrt();
    let mut loss_factor = 0.16_f32;
    if p >= 0.1 && first.kind == last.kind {
        if last.kind == 85 || last.kind == 87 {
            p = 3.;
        } else if last.kind == 76 {
            p = 2.;
        }
        if thickness < 6. {
            if first.kind == 71 || first.kind == 89 {
                p = 3.;
                loss_factor = 0.05;
            }
            if solid.pass_bullets == [true, true] {
                p = 32.;
                loss_factor = 0.00001;
            }
        } else if solid.pass_bullets == [true, true] {
            p = 3.;
        }
    }
    let inv = (1. / p).max(0.);
    let loss = (thickness * thickness * inv) / 24.
        + ((3. / state.penetration * 1.25).max(0.) * (inv * 3.) + loss_factor * state.damage);
    state.damage -= loss.max(0.);
    let stopped = state.damage < 1.;
    if !stopped {
        state.remaining -= 1;
    }
    Ok(Outcome { state, stopped })
}

/// Intervals reference contacts after the native adjacent-boundary exchange.
#[derive(Debug)]
pub struct Interval {
    pub segment: Segment,
    pub entry: usize,
    pub exit: usize,
}

pub fn intervals(
    hits: &mut [super::collision::asset::Hit],
    delta: [f32; 3],
    max_fraction: f32,
) -> Result<Vec<Interval>> {
    ensure!(hits.len() <= 0x7fff, "ballistic contact index overflow");
    ensure!(
        delta.iter().all(|v| v.is_finite()) && max_fraction.is_finite(),
        "nonfinite ballistic path"
    );
    ensure!(
        hits.iter().all(|h| h.contact.fraction.is_finite())
            && hits
                .windows(2)
                .all(|w| w[0].contact.fraction <= w[1].contact.fraction),
        "ballistic contacts are not sorted"
    );
    let length = ((delta[2] * delta[2] + delta[1] * delta[1]) + delta[0] * delta[0]).sqrt();
    ensure!(length.is_finite(), "ballistic path overflow");
    for i in 1..hits.len() {
        if !hits[i].contact.exit
            && hits[i - 1].contact.exit
            && (hits[i].contact.fraction - hits[i - 1].contact.fraction) * length <= 0.001953125
        {
            let previous = hits[i - 1].contact.fraction;
            let current = hits[i].contact.fraction;
            hits.swap(i - 1, i);
            hits[i - 1].contact.fraction = previous;
            hits[i].contact.fraction = current;
        }
    }
    let mut out = Vec::new();
    let push = |out: &mut Vec<Interval>, start, end, entry, exit, solid| {
        out.push(Interval {
            segment: Segment { start, end, solid },
            entry,
            exit,
        })
    };
    let (mut kind, mut previous, mut entry) = (true, 0., 0);
    for (i, hit) in hits.iter().enumerate() {
        if hit.contact.exit == kind {
            continue;
        }
        kind = hit.contact.exit;
        if kind {
            continue;
        }
        if i > 0 {
            let end = hits[i - 1].contact.fraction;
            push(&mut out, previous, end, entry, i - 1, true);
            previous = end;
            entry = i - 1;
        }
        push(&mut out, previous, hit.contact.fraction, entry, i, false);
        previous = hit.contact.fraction;
        entry = i;
    }
    if let Some(last) = out.last() {
        if !last.segment.solid {
            let (start, index, final_index) = (last.segment.end, last.exit, hits.len() - 1);
            if max_fraction < 1. && index < final_index && hits[final_index].contact.exit {
                let end = hits[final_index].contact.fraction;
                push(&mut out, start, end, index, final_index, true);
                push(&mut out, end, 1., final_index, final_index, false);
            } else {
                push(&mut out, start, 1., index, index, true);
            }
        }
    }
    if let Some(first) = out.first() {
        if first.segment.solid {
            let (end, index) = (first.segment.end, first.exit);
            out.clear();
            push(&mut out, end, end, index, index, false);
            push(&mut out, end, 1., index, index, true);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recorded_trigger_preserves_raw_pellets_and_requires_policy() {
        let resources = serde_json::json!({"25":{"m_nNumBullets":6,"m_nSpreadSeed":817955,"m_nDamage":20,"m_flPenetration":1.,"m_flRange":3000.,"m_flRangeModifier":0.7}});
        let mut event = serde_json::json!({"event_name":"fire_bullets","message_tick":100,"item_def_index":25,"seed":3,"mode":0,"inaccuracy":0.1,"spread":0.1,"recoil_index":0.,"angles_x":0.,"angles_y":0.,"angles_z":0.,"origin_x":1.,"origin_y":2.,"origin_z":3.,"smoke_patterns_enabled":true,"smoke_only_up":false,"smoke_maximum_inaccuracy":false});
        let fire = Fire::from_event(&event, &resources).unwrap();
        assert_eq!(fire.message_tick, 100);
        assert_eq!(fire.origin, [1., 2., 3.]);
        assert_eq!(fire.deltas.len(), 6);
        assert!(fire.deltas.iter().all(|d| d[0] == 3000.));
        assert!(fire.deltas.iter().any(|d| d[1] != 0. || d[2] != 0.));
        event
            .as_object_mut()
            .unwrap()
            .remove("smoke_patterns_enabled");
        assert!(Fire::from_event(&event, &resources).is_err());
    }

    #[test]
    fn touching_solids_exchange_identity_without_moving_boundaries() {
        use super::super::collision::{asset::Hit, Contact};
        let hit = |fraction, exit, shape| Hit {
            contact: Contact {
                fraction,
                exit,
                normal: [0.; 3],
            },
            part: 0,
            shape,
            attribute: 0,
            triangle: None,
            surface: 0,
        };
        let mut hits = [
            hit(0.1, false, 0),
            hit(0.2, true, 0),
            hit(0.20001, false, 1),
            hit(0.3, true, 1),
        ];
        let out = intervals(&mut hits, [100., 0., 0.], 0.5).unwrap();
        assert_eq!((hits[1].shape, hits[2].shape), (1, 0));
        assert_eq!(hits[1].contact.fraction.to_bits(), 0.2_f32.to_bits());
        assert_eq!(out.len(), 3);
        assert!(out[1].segment.solid);
        assert_eq!((out[1].segment.start, out[1].segment.end), (0.1, 0.3));
        assert!(!out[2].segment.solid);
        hits.reverse();
        assert!(intervals(&mut hits, [100., 0., 0.], 0.5).is_err());
    }

    #[test]
    fn native_enemy_flesh_interval_and_unknown_policy() {
        let state = State {
            damage: 35.564640045166016,
            penetration: 2.,
            range_modifier: 0.98,
            path_length: 8201.2724609375,
            remaining: 4,
        };
        let segment = Segment {
            start: 0.03671681508421898,
            end: 0.037739098072052,
            solid: true,
        };
        let surfaces = [Surface {
            penetration_distance: 0.9,
            kind: 70,
        }; 4];
        let solid = |same_team_flesh| Solid {
            surfaces: &surfaces,
            endpoints: [[8.38134765625, 0.0325927734375, 0.20947265625], [0.; 3]],
            pass_bullets: [false; 2],
            same_team_flesh,
        };
        let result = advance(state, segment, Some(solid(false))).unwrap();
        assert_eq!(result.state.damage.to_bits(), 1101198808);
        assert_eq!(result.state.remaining, 3);
        assert!(!result.stopped);
        assert!(advance(state, segment, Some(solid(true))).is_err());
        assert!(advance(state, segment, None).is_err());
    }
}
