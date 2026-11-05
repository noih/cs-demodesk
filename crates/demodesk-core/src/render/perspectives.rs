//! Enemy replays follow completed exchanges; the original main-POV tail survives.
use super::*;
use utility::{enemies, gun, shot, Chase, Event};

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (DemoInfo, Highlight, Vec<Event>) {
        let demo = serde_json::from_value(serde_json::json!({"path":"test.dem","mapName":"de_test","serverName":"test","tickRate":64,
            "players":[{"steamid":"main","name":"Main","teamNumber":3,"userId":0},{"steamid":"enemy","name":"Enemy","teamNumber":2,"userId":1}]})).unwrap();
        let h = serde_json::from_value(serde_json::json!({"id":"one","player":{"steamid":"main","name":"Main"},"round":1,
            "startTick":800,"endTick":2000,"anchorTick":1000,"score":1,"tags":[],"title":"clip","kills":[],"breakdown":{}})).unwrap();
        let mut events = vec![];
        for tick in [1000, 1600, 1750] {
            events.push(serde_json::from_value(serde_json::json!({"tick":tick,"event_name":"player_death","weapon":"ak47","user_steamid":"enemy","attacker_steamid":"main","user_team_num":2,"attacker_team_num":3})).unwrap());
            if tick != 1600 {
                events.push(serde_json::from_value(serde_json::json!({"tick":tick-20,"event_name":"player_hurt","weapon":"m4a1","user_steamid":"main","attacker_steamid":"enemy","user_team_num":3,"attacker_team_num":2,"dmg_health":26})).unwrap());
            }
        }
        events.sort_by_key(|e: &Event| e.tick);
        (demo, h, events)
    }
    #[test]
    fn multiple_replays_preserve_main_action_and_original_tail() {
        let (demo, h, events) = fixture();
        let (shots, cameras) = plan(vec![h.clone()], vec![None], &[h.clone()], &demo, &events);
        assert_eq!(shots.len(), 5);
        assert_eq!(cameras.len(), shots.len());
        assert_eq!((shots[0].start_tick, shots[0].end_tick), (800, 1032));
        assert_eq!((shots[1].start_tick, shots[1].end_tick), (904, 1032));
        assert_eq!((shots[2].start_tick, shots[2].end_tick), (1032, 1782));
        assert_eq!((shots[3].start_tick, shots[3].end_tick), (1654, 1782));
        assert_eq!((shots[4].start_tick, shots[4].end_tick), (1782, 2000));
        assert_eq!(shots[4].player.steamid, "main");
        assert!(shots.iter().all(|s| s.id == h.id));
    }
    #[test]
    fn no_replay_for_ordinary_friendly_utility_blinded_or_missing_camera() {
        for mode in 0..5 {
            let (mut demo, h, mut events) = fixture();
            match mode {
                0 => events.retain(|e| e.event_name != "player_hurt"),
                1 => {
                    for e in &mut events {
                        e.attacker_team_num = e.user_team_num;
                    }
                }
                2 => {
                    for e in &mut events {
                        e.weapon = "hegrenade".into();
                    }
                }
                3 => {
                    events.push(serde_json::from_value(serde_json::json!({"tick":900,"event_name":"player_blind","user_steamid":"main","blind_duration":30})).unwrap());
                    events.sort_by_key(|e| e.tick);
                }
                _ => demo.players[1].user_id = None,
            }
            let (shots, _) = plan(vec![h.clone()], vec![None], &[h.clone()], &demo, &events);
            assert_eq!(shots.len(), 1, "mode {mode}");
            assert_eq!(shots[0].end_tick, h.end_tick);
        }
    }
    #[test]
    fn utility_victim_is_not_replayed_and_active_fire_defers_insertion() {
        let (demo, h, mut events) = fixture();
        let mut victim = h.clone();
        victim.player = demo
            .players
            .iter()
            .find(|p| p.steamid == "enemy")
            .map(|p| crate::model::HighlightPlayer {
                steamid: p.steamid.clone(),
                name: p.name.clone(),
            })
            .unwrap();
        victim.start_tick = 950;
        victim.end_tick = 990;
        let (shots, _) = plan(
            vec![victim, h.clone()],
            vec![None, None],
            &[h.clone()],
            &demo,
            &events,
        );
        assert_eq!(
            shots.iter().filter(|s| s.player.steamid == "enemy").count(),
            2
        );
        for tick in [1040, 1080] {
            events.push(serde_json::from_value(serde_json::json!({"tick":tick,"event_name":"weapon_fire","user_steamid":"main","weapon":"ak47"})).unwrap());
        }
        events.sort_by_key(|e| e.tick);
        let (shots, _) = plan(vec![h.clone()], vec![None], &[h], &demo, &events);
        assert_eq!(shots[0].end_tick, 1112);
    }
    #[test]
    fn option_defaults_off_and_persists() {
        let old: RenderOptions = serde_json::from_value(serde_json::json!({})).unwrap();
        assert!(!old.combat_perspectives);
        let on: RenderOptions =
            serde_json::from_value(serde_json::json!({"combatPerspectives":true})).unwrap();
        assert_eq!(
            serde_json::to_value(on).unwrap()["combatPerspectives"],
            true
        );
    }
}

fn involved(e: &Event, main: &str) -> bool {
    (e.event_name == "weapon_fire" && e.user_steamid.as_deref() == Some(main) && gun(&e.weapon))
        || (matches!(e.event_name.as_str(), "player_hurt" | "player_death")
            && (e.user_steamid.as_deref() == Some(main)
                || e.attacker_steamid.as_deref() == Some(main)))
}

fn value(kill: &Event, main: &str, events: &[Event], rate: i32) -> i32 {
    if !gun(&kill.weapon) || !enemies(kill) {
        return 0;
    }
    let Some(enemy) = kill.user_steamid.as_deref() else {
        return 0;
    };
    // The first release qualifies reciprocal gun exchanges, not unseen aim/peek quality.
    if events.iter().any(|e| {
        e.event_name == "player_blind"
            && [Some(main), Some(enemy)].contains(&e.user_steamid.as_deref())
            && e.tick <= kill.tick
            && f64::from(e.tick) + e.blind_duration.unwrap_or(0.0) * f64::from(rate)
                >= f64::from(kill.tick - rate)
    }) {
        return 0;
    }
    events
        .iter()
        .filter(|e| {
            e.event_name == "player_hurt"
                && gun(&e.weapon)
                && enemies(e)
                && e.attacker_steamid.as_deref() == Some(enemy)
                && e.user_steamid.as_deref() == Some(main)
                && e.tick >= kill.tick - 2 * rate
                && e.tick < kill.tick
        })
        .map(|e| e.dmg_health.unwrap_or(0).max(0))
        .sum::<i32>()
}

pub(super) fn plan(
    recording: Vec<Highlight>,
    cameras: Vec<Option<Chase>>,
    originals: &[Highlight],
    demo: &DemoInfo,
    events: &[Event],
) -> (Vec<Highlight>, Vec<Option<Chase>>) {
    let rate = demo.tick_rate.round() as i32;
    if rate <= 0 {
        return (recording, cameras);
    }
    // Utility edits already contain the victim's perspective; do not replay that kill again.
    let represented: Vec<_> = recording
        .iter()
        .filter(|h| {
            originals
                .iter()
                .any(|o| o.id == h.id && o.player.steamid != h.player.steamid)
        })
        .map(|h| {
            (
                h.id.clone(),
                h.player.steamid.clone(),
                h.start_tick,
                h.end_tick,
            )
        })
        .collect();
    let mut used = std::collections::HashSet::new();
    let (mut out, mut views) = (vec![], vec![]);
    for (h, camera) in recording.into_iter().zip(cameras) {
        let main = h.player.steamid.as_str();
        if camera.is_some()
            || !originals
                .iter()
                .any(|o| o.id == h.id && o.player.steamid == main)
        {
            out.push(h);
            views.push(camera);
            continue;
        }
        let kills: Vec<_> = events
            .iter()
            .filter(|e| {
                e.event_name == "player_death"
                    && e.attacker_steamid.as_deref() == Some(main)
                    && e.tick >= h.start_tick
                    && e.tick < h.end_tick
            })
            .collect();
        let mut groups: Vec<Vec<&Event>> = vec![];
        for kill in kills {
            if let Some(group) = groups
                .last_mut()
                .filter(|g| kill.tick - g.last().unwrap().tick <= 3 * rate)
            {
                group.push(kill);
            } else {
                groups.push(vec![kill]);
            }
        }
        let mut cursor = h.start_tick;
        for group in groups {
            let last = group.last().unwrap().tick;
            let mut settled = last;
            for e in events
                .iter()
                .filter(|e| e.tick > last && e.tick < h.end_tick && involved(e, main))
            {
                if e.tick - settled > rate {
                    break;
                }
                settled = e.tick;
            }
            let cut = settled + rate / 2;
            if cut <= cursor
                || cut >= h.end_tick
                || h.round_result.as_ref().is_some_and(|r| cut >= r.from_tick)
            {
                continue;
            }
            let candidate = group
                .into_iter()
                .filter_map(|kill| {
                    let enemy = kill.user_steamid.as_deref()?;
                    let score = value(kill, main, events, rate);
                    let start = kill.tick - rate * 3 / 2;
                    let end = kill.tick + rate / 2;
                    let boundary = events
                        .iter()
                        .any(|e| e.event_name == "round_start" && e.tick > start && e.tick <= end);
                    if score < 20
                        || start <= seek::MIN_TICK + 8
                        || boundary
                        || used.contains(&(h.id.clone(), kill.tick))
                        || !demo
                            .players
                            .iter()
                            .any(|p| p.steamid == enemy && p.user_id.is_some())
                        || represented.iter().any(|(id, player, a, b)| {
                            id == &h.id && player == enemy && *a <= end && *b >= start
                        })
                    {
                        return None;
                    }
                    Some((score, kill))
                })
                .max_by_key(|(score, kill)| (*score, kill.tick));
            let Some((_, kill)) = candidate else { continue };
            out.push(shot(&h, main, cursor, cut, demo));
            views.push(None);
            out.push(shot(
                &h,
                kill.user_steamid.as_deref().unwrap(),
                kill.tick - rate * 3 / 2,
                kill.tick + rate / 2,
                demo,
            ));
            views.push(None);
            used.insert((h.id.clone(), kill.tick));
            cursor = cut;
        }
        if cursor == h.start_tick {
            out.push(h);
            views.push(None);
        } else {
            let mut tail = shot(&h, main, cursor, h.end_tick, demo);
            tail.round_result = h.round_result;
            out.push(tail);
            views.push(None);
        }
    }
    (out, views)
}
