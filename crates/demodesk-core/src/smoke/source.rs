//! CS2 wire adapter. The replay only receives version-independent coverage snapshots.
use super::{
    effects::{EffectFrame, PackedHe},
    projection::{Coverage, Snapshot},
    timeline::Timeline,
};
use crate::{
    analysis::compact::{self, Field},
    parser::DemoParser,
    replay::ReplayEvent,
};
use anyhow::{anyhow, Result};
use parser::{
    first_pass::{
        parser_settings::FirstPassParser,
        prop_controller::{SMOKE_VOXELS_ID, SMOKE_VOXELS_LIMIT},
    },
    second_pass::parser_settings::SecondPassParser,
};
use std::collections::{BTreeMap, HashMap};

pub fn replay(
    parser: &DemoParser,
    bytes: &[u8],
    first_tick: i32,
    last_tick: i32,
    rate: f64,
    events: &[ReplayEvent],
) -> Result<Vec<Snapshot>> {
    anyhow::ensure!(
        rate.is_finite() && rate > 0. && first_tick >= 0 && last_tick >= first_tick,
        "invalid smoke replay interval"
    );
    let inputs = parser.inputs(&[], &[], &[], vec![i32::MIN])?;
    let mut first = FirstPassParser::new(&inputs);
    let parsed = first
        .parse_demo(bytes, true)
        .map_err(|e| anyhow!("smoke schema: {e:?}"))?;
    let mut second = SecondPassParser::new(
        parsed,
        parser::first_pass::parser::HEADER_ENDS_AT_BYTE,
        true,
        None,
    )
    .map_err(|e| anyhow!("smoke parser: {e:?}"))?;
    second.analysis_changes = Some(Default::default());
    let selected: HashMap<u32, String> = second
        .prop_controller
        .id_to_name
        .iter()
        .filter_map(|(id, name)| {
            matches!(
                name.as_str(),
                "m_bDidSmokeEffect"
                    | "m_nSmokeEffectTickBegin"
                    | "m_nVoxelFrameDataSize"
                    | "m_vSmokeDetonationPos"
            )
            .then(|| (*id, name.clone()))
        })
        .collect();
    let name_of = |id: u32| {
        if (SMOKE_VOXELS_ID..SMOKE_VOXELS_ID + SMOKE_VOXELS_LIMIT).contains(&id) {
            Some(format!("smokeVoxel/{}", id - SMOKE_VOXELS_ID))
        } else { selected.get(&id).cloned() }
    };
    let mut fields = Vec::<Field>::new();
    let mut ids = HashMap::new();
    let mut values = BTreeMap::new();
    let mut entity_fields = HashMap::<i32, Vec<u32>>::new();
    let mut timeline = Timeline::default();
    let mut snapshots: Vec<Snapshot> = Vec::new();
    let mut failure = None;
    let mut sampled = i32::MIN;
    let hes: Vec<_> = events.iter().filter(|e| e.k == "he").collect();
    second
        .start_with_observer(bytes, |p| {
            if p.tick > last_tick {
                return false;
            }
            if p.tick < 0 {
                return true;
            }
            let result = (|| -> Result<()> {
                let changes = p.analysis_changes.as_ref().expect("capture enabled");
                let mut changed = Vec::new();
                for entity in &changes.lifecycle {
                    if let Some(indices) = entity_fields.get(entity) {
                        for id in indices {
                            if values.remove(id).is_some() {
                                changed.push(*id);
                            }
                        }
                    }
                }
                let mut updates = Vec::new();
                for entity in &changes.lifecycle {
                    if let Some(e) = p
                        .entities
                        .get(*entity as usize)
                        .and_then(Option::as_ref)
                        .filter(|e| {
                            p.cls_by_id[e.cls_id as usize].name == "CSmokeGrenadeProjectile"
                        })
                    {
                        updates.push((e.entity_id, e.serial, "$present".to_string(), vec![0, 1]));
                        for (id, value) in &e.props {
                            if let Some(name) = name_of(*id) {
                                updates.push((
                                    e.entity_id,
                                    e.serial,
                                    name,
                                    compact::value(value)?,
                                ));
                            }
                        }
                    }
                }
                for (entity, prop) in &changes.properties {
                    if changes.lifecycle.contains(entity) {
                        continue;
                    }
                    let Some(name) = name_of(*prop) else {
                        continue;
                    };
                    let Some(e) = p
                        .entities
                        .get(*entity as usize)
                        .and_then(Option::as_ref)
                        .filter(|e| {
                            p.cls_by_id[e.cls_id as usize].name == "CSmokeGrenadeProjectile"
                        })
                    else {
                        continue;
                    };
                    if let Some(value) = e.props.get(prop) {
                        updates.push((e.entity_id, e.serial, name, compact::value(value)?));
                    }
                }
                for (entity, serial, name, value) in updates {
                    let id = *ids
                        .entry((entity, serial, name.clone()))
                        .or_insert_with(|| {
                            let id = fields.len() as u32;
                            entity_fields.entry(entity).or_default().push(id);
                            fields.push(Field {
                                entity,
                                serial,
                                class: "CSmokeGrenadeProjectile".into(),
                                name,
                            });
                            id
                        });
                    values.insert(id, value);
                    changed.push(id);
                }
                let frame = compact::Frame {
                    tick: p.tick,
                    net_tick: p.net_tick,
                    fields: &fields,
                    values: &values,
                    changed: &changed,
                };
                timeline.update(&frame, |_, _| Ok(()))?;
                if timeline.volumes().next().is_none() {
                    fields.clear();
                    ids.clear();
                    values.clear();
                    entity_fields.clear();
                }
                if p.tick >= first_tick && (sampled == i32::MIN || p.tick - sampled >= 8) {
                    sampled = p.tick;
                    let now = p.net_tick as f32 / 64.;
                    let mut effects = EffectFrame {
                        now,
                        ..Default::default()
                    };
                    // ponytail: proximity-only HE masks approximate top-down disruption; native scene registration is needed for exact masks.
                    let end = hes.partition_point(|e| e.t <= p.tick);
                    for e in hes[..end]
                        .iter()
                        .rev()
                        .take_while(|e| f64::from(p.tick - e.t) < rate * 5.)
                        .take(5)
                    {
                        if let (Some(x), Some(y), Some(z)) = (e.x, e.y, e.z) {
                            let position = [x, y, z];
                            effects.he[effects.he_count] = PackedHe {
                                position_time: [
                                    position[0] as f32,
                                    position[1] as f32,
                                    position[2] as f32,
                                    now - (p.tick - e.t) as f32 / rate as f32,
                                ],
                                mask: 1.,
                            };
                            effects.he_count += 1;
                        }
                    }
                    let cells = timeline.top_down(now, &effects);
                    if snapshots.last().is_none_or(|s| s.cells != cells) {
                        snapshots.push(Snapshot { t: p.tick, cells });
                    }
                }
                Ok(())
            })();
            if let Err(e) = result {
                failure = Some(e);
                return false;
            }
            true
        })
        .map_err(|e| anyhow!("smoke packets: {e:?}"))?;
    if let Some(e) = failure {
        return Err(e);
    }
    Ok(snapshots)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires SMOKE_DEMO local fixture"]
    fn recorded_smoke_prefix_produces_nonempty_coverage() {
        let bytes = std::fs::read(std::env::var("SMOKE_DEMO").unwrap()).unwrap();
        let start = std::time::Instant::now();
        let result = replay(&DemoParser::new(), &bytes, 0, 10000, 64., &[]).unwrap();
        eprintln!("elapsed_us={} hash={}", start.elapsed().as_micros(), sha1_smol::Sha1::from(serde_json::to_vec(&result).unwrap()).digest());
        let covered = result
            .iter()
            .filter(|s| s.cells.as_ref().is_some_and(|c| !c.is_empty()))
            .count();
        eprintln!(
            "smoke snapshots={} covered={covered} bytes={}",
            result.len(),
            serde_json::to_vec(&result).unwrap().len()
        );
        assert!(covered > 0);
    }
}
