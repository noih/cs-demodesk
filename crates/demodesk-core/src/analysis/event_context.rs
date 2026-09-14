//! Shared event ledger and exact-tick indexes, independent of individual checks.
use serde_json::Value;
use std::collections::BTreeMap;
#[derive(Clone, Debug)]
pub struct Shot {
    pub tick: i32,
    pub player_id: String,
    pub weapon: String,
    pub round: Option<i32>,
    pub direction: Option<[f64; 2]>,
}
#[derive(Default)]
pub struct Events {
    pub raw: Vec<Value>,
    pub shots: BTreeMap<i32, Vec<Shot>>,
    bullet_indices: BTreeMap<(i32, String), Option<usize>>,
    rewind: super::ballistics::rewind::Index,
}
impl Events {
    /// Exact trigger match only. Expanded weapon resources supply the concrete item prefab;
    /// missing metadata and ambiguous same-tick triggers remain unclassified.
    pub fn bullet_for_shot<'a>(&'a self, shot: &Shot, weapons: &Value) -> Option<&'a Value> {
        let triggers = self.shots.get(&shot.tick)?;
        let mut same_player = triggers.iter().filter(|s| s.player_id == shot.player_id);
        let recorded = same_player.next()?;
        if same_player.next().is_some() || recorded.weapon != shot.weapon {
            return None;
        }
        let index = (*self
            .bullet_indices
            .get(&(shot.tick, shot.player_id.clone()))?)?;
        let event = self.raw.get(index)?;
        let item = u32::try_from(event["item_def_index"].as_u64()?).ok()?;
        // Concrete item prefab distinguishes shared runtime classes, including
        // revolver/deagle and USP/P2000. Class equality alone is insufficient.
        let prefab = weapons.get(item.to_string())?.get("_base")?.as_str()?;
        let item_weapon = prefab.strip_suffix("_prefab")?.strip_prefix("weapon_")?;
        (item_weapon == shot.weapon.strip_prefix("weapon_").unwrap_or(&shot.weapon))
            .then_some(event)
    }

    pub fn rewind_for_bullet(
        &self,
        bullet: &Value,
    ) -> Option<super::ballistics::rewind::Selected<'_>> {
        self.rewind.for_bullet(bullet)
    }

    pub fn from_raw(raw: Vec<Value>) -> Self {
        let rewind = super::ballistics::rewind::Index::from_raw(&raw);
        let mut shots = BTreeMap::<(i32, String, String), Shot>::new();
        let mut directions = BTreeMap::<(i32, String), (Option<i32>, Option<[f64; 2]>)>::new();
        let mut bullet_indices = BTreeMap::new();
        for (raw_index, event) in raw.iter().enumerate() {
            let (Some(tick), Some(player)) = (
                event["tick"].as_i64().and_then(|v| i32::try_from(v).ok()),
                event["user_steamid"].as_str(),
            ) else {
                continue;
            };
            if tick < 0 || player.is_empty() || player == "0" {
                continue;
            }
            match event["event_name"].as_str() {
                Some("weapon_fire") => {
                    let Some(weapon) = event["weapon"].as_str() else {
                        continue;
                    };
                    shots
                        .entry((tick, player.into(), weapon.into()))
                        .or_insert_with(|| Shot {
                            tick,
                            player_id: player.into(),
                            weapon: weapon.into(),
                            round: None,
                            direction: None,
                        });
                }
                Some("fire_bullets") => {
                    bullet_indices
                        .entry((tick, player.into()))
                        .and_modify(|index| *index = None)
                        .or_insert(Some(raw_index));
                    let round = event["round"]
                        .as_i64()
                        .and_then(|r| i32::try_from(r).ok())
                        .filter(|r| *r > 0);
                    let direction = event["angles_x"]
                        .as_f64()
                        .zip(event["angles_y"].as_f64())
                        .filter(|(p, y)| p.is_finite() && p.abs() <= 90.0 && y.is_finite())
                        .map(|(p, y)| [p, y]);
                    // Multiple bullet-direction messages at one player/tick are ambiguous.
                    directions
                        .entry((tick, player.into()))
                        .and_modify(|v| *v = (None, None))
                        .or_insert((round, direction));
                }
                _ => {}
            }
        }
        let mut index = BTreeMap::<i32, Vec<Shot>>::new();
        for ((tick, player, _), mut shot) in shots {
            if let Some(&(round, direction)) = directions.get(&(tick, player)) {
                shot.round = round;
                shot.direction = direction;
            }
            index.entry(tick).or_default().push(shot);
        }
        Self {
            raw,
            shots: index,
            bullet_indices,
            rewind,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn metadata_matches_unique_trigger_and_installed_weapon_without_tick_fallback() {
        let shot = serde_json::json!({"event_name":"weapon_fire","tick":10,"user_steamid":"a","weapon":"weapon_usp_silencer"});
        let bullet = serde_json::json!({"event_name":"fire_bullets","tick":10,"user_steamid":"a","item_def_index":61,"message_tick":90,"seed":5});
        let weapons = serde_json::json!({"61":{"_base":"weapon_usp_silencer_prefab"},"7":{"_base":"weapon_ak47_prefab"}});
        let base = vec![shot.clone(), shot.clone(), bullet.clone()];
        let events = Events::from_raw(base.clone());
        assert_eq!(
            events.bullet_for_shot(&events.shots[&10][0], &weapons),
            Some(&bullet)
        );
        assert!(events
            .bullet_for_shot(&events.shots[&10][0], &Value::Null)
            .is_none());
        for change in [
            serde_json::json!({"tick":11}),
            serde_json::json!({"user_steamid":"b"}),
            serde_json::json!({"item_def_index":7}),
            serde_json::json!({"item_def_index":null}),
        ] {
            let mut changed = bullet.clone();
            changed
                .as_object_mut()
                .unwrap()
                .extend(change.as_object().unwrap().clone());
            let events = Events::from_raw(vec![shot.clone(), changed]);
            assert!(events
                .bullet_for_shot(&events.shots[&10][0], &weapons)
                .is_none());
        }
        for additional in [
            bullet,
            serde_json::json!({"event_name":"weapon_fire","tick":10,"user_steamid":"a","weapon":"ak47"}),
        ] {
            let mut raw = base.clone();
            raw.push(additional);
            let events = Events::from_raw(raw);
            assert!(events
                .bullet_for_shot(&events.shots[&10][0], &weapons)
                .is_none());
        }
    }
    #[test]
    fn ledger_preserves_all_events_and_indexes_exact_shots_once() {
        let shot = serde_json::json!({"event_name":"weapon_fire","tick":10,"user_steamid":"a","weapon":"weapon_ak47"});
        let bullet = serde_json::json!({"event_name":"fire_bullets","tick":10,"user_steamid":"a","round":2,"angles_x":1.0,"angles_y":2.0});
        let raw = vec![
            shot.clone(),
            shot,
            bullet,
            serde_json::json!({"event_name":"smokegrenade_detonate","tick":10}),
        ];
        let events = Events::from_raw(raw.clone());
        assert_eq!(events.raw, raw);
        assert_eq!(events.shots[&10].len(), 1);
        assert_eq!(events.shots[&10][0].direction, Some([1.0, 2.0]));
        assert_eq!(events.shots[&10][0].round, Some(2));
        assert!(!events.shots.contains_key(&9));
    }
}
