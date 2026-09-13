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
}
impl Events {
    pub fn from_raw(raw: Vec<Value>) -> Self {
        let mut shots = BTreeMap::<(i32, String, String), Shot>::new();
        let mut directions = BTreeMap::<(i32, String), (Option<i32>, Option<[f64; 2]>)>::new();
        for event in &raw {
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
        Self { raw, shots: index }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
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
