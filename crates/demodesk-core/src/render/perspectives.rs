//! Enemy replays follow completed exchanges; brief continuations stay before the replay.
use super::*;
use demo_data::{enemies, gun, Event};
use editing::{pacing, timeline::shot};
use utility::Camera;

/// Explicit replay groups only rearrange recorded POVs; they never repeat main-player footage.
pub(super) fn compose_groups(
    recording: Vec<Highlight>,
    cameras: Vec<Option<Camera>>,
    originals: &[Highlight],
    demo: &DemoInfo,
    prepared: &Prepared,
    groups: &[Vec<&editing::Candidate>],
) -> Result<(Vec<Highlight>, Vec<Option<Camera>>)> {
    use anyhow::{ensure, Context};
    let rate = demo.tick_rate.round() as i32;
    let mut insertions = vec![];
    for group in groups {
        let last = group.last().context("Empty replay group")?;
        let original = originals
            .iter()
            .find(|h| h.id == last.highlight_id)
            .context("Unknown replay highlight")?;
        let exchanges = prepared
            .players
            .iter()
            .find(|p| p.main == original.player.steamid)
            .context("Missing prepared exchanges")?;
        let source = recording
            .iter()
            .zip(&cameras)
            .rposition(|(h, camera)| {
                h.id == original.id
                    && h.player.steamid == original.player.steamid
                    && camera.is_none()
                    && h.start_tick <= last.tick
                    && last.tick < h.end_tick
            })
            .context("Replay has no main-player footage")?;
        for candidate in group {
            ensure!(
                exchanges.kills.iter().any(|kill| kill.eligible
                    && kill.tick == candidate.tick
                    && kill.victim == candidate.target_id),
                "Invalid selected exchange"
            );
            ensure!(
                recording[..=source].iter().any(|h| h.id == original.id
                    && h.player.steamid == original.player.steamid
                    && h.start_tick <= candidate.tick
                    && candidate.tick < h.end_tick),
                "Replay cannot precede its main-player exchange"
            );
        }
        let h = &recording[source];
        let next = groups
            .iter()
            .filter_map(|g| g.first())
            .filter(|c| c.highlight_id == last.highlight_id && c.tick > last.tick)
            .map(|c| c.tick)
            .min();
        let mut settled = last.tick;
        for &tick in exchanges.activity.iter().filter(|&&tick| {
            tick > last.tick && tick < h.end_tick && next.is_none_or(|next| tick < next)
        }) {
            if tick - settled > rate {
                break;
            }
            settled = tick;
        }
        let mut cut = (settled + rate / 2).max(last.tick + pacing::event_tail(rate));
        // A final linked replay follows the complete main-player ending, including its existing hold.
        if group.len() > 1
            && !exchanges
                .kills
                .iter()
                .any(|k| k.tick > last.tick && k.tick < h.end_tick)
        {
            cut = h.end_tick;
        }
        insertions.push((source, cut, group));
    }
    insertions.sort_by_key(|(source, _, group)| (*source, group.last().unwrap().tick));
    let (mut out, mut views) = (vec![], vec![]);
    for (index, (h, camera)) in recording.into_iter().zip(cameras).enumerate() {
        let pending: Vec<_> = insertions
            .iter()
            .filter(|(source, _, _)| *source == index)
            .collect();
        if pending.is_empty() {
            out.push(h);
            views.push(camera);
            continue;
        }
        let mut cursor = h.start_tick;
        for (_, requested, group) in pending {
            let cut = pacing::replay_cut(*requested, cursor, h.end_tick, rate)
                .context("Separate replays leave less than 1.5 seconds of main-player footage; link these exchanges")?;
            let mut main = shot(&h, &h.player.steamid, cursor, cut, demo);
            if cut == h.end_tick {
                main.round_result = h.round_result.clone();
            }
            out.push(main);
            views.push(None);
            for candidate in *group {
                out.push(shot(
                    &h,
                    &candidate.target_id,
                    candidate.tick - rate * 3 / 2,
                    candidate.tick + pacing::event_tail(rate),
                    demo,
                ));
                views.push(None);
            }
            cursor = cut;
        }
        if cursor < h.end_tick {
            let mut tail = shot(&h, &h.player.steamid, cursor, h.end_tick, demo);
            tail.round_result = h.round_result;
            out.push(tail);
            views.push(None);
        }
    }
    Ok((out, views))
}

#[cfg(test)]
mod tests {
    use super::*;
    pub(super) fn fixture() -> (DemoInfo, Highlight, Vec<Event>) {
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
    fn minimum_view_duration_absorbs_short_return_before_replay() {
        let (demo, mut h, events) = fixture();
        h.end_tick = 1100;
        let (shots, _) = plan(vec![h.clone()], vec![None], &[h], &demo, &events);
        assert_eq!(shots.len(), 2);
        assert_eq!(shots[0].end_tick, 1100);
        assert_eq!(shots[1].player.steamid, "enemy");
        assert!(shots.iter().all(|h| h.end_tick - h.start_tick >= 96));
    }

    #[test]
    fn explicit_selection_can_replay_every_eligible_kill() {
        let (demo, h, events) = fixture();
        let selected = [1000, 1600, 1750]
            .into_iter()
            .map(|tick| (h.id.clone(), tick, "enemy".into()))
            .collect();
        let (shots, _) = plan_selected(
            vec![h.clone()],
            vec![None],
            &[h.clone()],
            &demo,
            &events,
            Some(&selected),
        );
        assert_eq!(
            shots.iter().filter(|s| s.player.steamid == "enemy").count(),
            3
        );
        let main: Vec<_> = shots
            .iter()
            .filter(|s| s.player.steamid == "main")
            .collect();
        assert_eq!(main.first().unwrap().start_tick, h.start_tick);
        assert_eq!(main.last().unwrap().end_tick, h.end_tick);
        assert!(main.windows(2).all(|w| w[0].end_tick == w[1].start_tick));
        let (plain, _) = plan_selected(
            vec![h.clone()],
            vec![None],
            &[h.clone()],
            &demo,
            &events,
            Some(&Default::default()),
        );
        assert_eq!(plain.len(), 1);
    }
    #[test]
    fn multiple_replays_preserve_main_action_and_original_tail() {
        let (demo, h, events) = fixture();
        let (shots, cameras) = plan(vec![h.clone()], vec![None], &[h.clone()], &demo, &events);
        assert_eq!(shots.len(), 5);
        assert_eq!(cameras.len(), shots.len());
        assert_eq!((shots[0].start_tick, shots[0].end_tick), (800, 1053));
        assert_eq!((shots[1].start_tick, shots[1].end_tick), (904, 1053));
        assert_eq!((shots[2].start_tick, shots[2].end_tick), (1053, 1803));
        assert_eq!((shots[3].start_tick, shots[3].end_tick), (1654, 1803));
        assert_eq!((shots[4].start_tick, shots[4].end_tick), (1803, 2000));
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

#[derive(Serialize, Deserialize)]
pub(super) struct Prepared {
    players: Vec<PlayerExchanges>,
}
#[derive(Serialize, Deserialize)]
struct PlayerExchanges {
    main: String,
    kills: Vec<Kill>,
    activity: Vec<i32>,
}
#[derive(Serialize, Deserialize)]
struct Kill {
    tick: i32,
    victim: String,
    eligible: bool,
    score: i32,
}

pub(super) fn prepare(originals: &[Highlight], demo: &DemoInfo, events: &[Event]) -> Prepared {
    let rate = demo.tick_rate.round() as i32;
    let mains: std::collections::BTreeSet<_> = originals
        .iter()
        .map(|h| h.player.steamid.as_str())
        .collect();
    Prepared {
        players: mains
            .into_iter()
            .map(|main| PlayerExchanges {
                main: main.into(),
                activity: events
                    .iter()
                    .filter(|e| involved(e, main))
                    .map(|e| e.tick)
                    .collect(),
                kills: events
                    .iter()
                    .filter(|e| {
                        e.event_name == "player_death"
                            && e.attacker_steamid.as_deref() == Some(main)
                    })
                    .map(|kill| {
                        let victim = kill.user_steamid.clone().unwrap_or_default();
                        let start = kill.tick - rate * 3 / 2;
                        let end = kill.tick + pacing::event_tail(rate);
                        Kill {
                            tick: kill.tick,
                            score: value(kill, main, events, rate),
                            eligible: rate > 0
                                && enemies(kill)
                                && start > seek::MIN_TICK + 8
                                && !events.iter().any(|e| {
                                    e.event_name == "round_start" && e.tick > start && e.tick <= end
                                })
                                && demo
                                    .players
                                    .iter()
                                    .any(|p| p.steamid == victim && p.user_id.is_some()),
                            victim,
                        }
                    })
                    .collect(),
            })
            .collect(),
    }
}

impl Prepared {
    pub(super) fn recommended(&self, main: &str, tick: i32, victim: &str) -> bool {
        self.players
            .iter()
            .filter(|p| p.main == main)
            .flat_map(|p| &p.kills)
            .any(|kill| kill.tick == tick && kill.victim == victim && kill.score >= 20)
    }

    pub(super) fn all(
        &self,
        originals: &[Highlight],
    ) -> std::collections::HashSet<(String, i32, String)> {
        originals
            .iter()
            .flat_map(|h| {
                self.players
                    .iter()
                    .filter(|p| p.main == h.player.steamid)
                    .flat_map(move |p| {
                        p.kills
                            .iter()
                            .map(move |k| (h.id.clone(), k.tick, k.victim.clone()))
                    })
            })
            .collect()
    }
}

#[cfg(test)]
pub(super) fn plan(
    recording: Vec<Highlight>,
    cameras: Vec<Option<Camera>>,
    originals: &[Highlight],
    demo: &DemoInfo,
    events: &[Event],
) -> (Vec<Highlight>, Vec<Option<Camera>>) {
    plan_selected(recording, cameras, originals, demo, events, None)
}

#[cfg(test)]
pub(super) fn plan_selected(
    recording: Vec<Highlight>,
    cameras: Vec<Option<Camera>>,
    originals: &[Highlight],
    demo: &DemoInfo,
    events: &[Event],
    selected: Option<&std::collections::HashSet<(String, i32, String)>>,
) -> (Vec<Highlight>, Vec<Option<Camera>>) {
    let prepared = prepare(originals, demo, events);
    compose(recording, cameras, originals, demo, &prepared, selected)
}

pub(super) fn compose(
    recording: Vec<Highlight>,
    cameras: Vec<Option<Camera>>,
    originals: &[Highlight],
    demo: &DemoInfo,
    prepared: &Prepared,
    selected: Option<&std::collections::HashSet<(String, i32, String)>>,
) -> (Vec<Highlight>, Vec<Option<Camera>>) {
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
            || selected.is_some_and(|s| !s.iter().any(|(id, _, _)| id == &h.id))
            || !originals
                .iter()
                .any(|o| o.id == h.id && o.player.steamid == main)
        {
            out.push(h);
            views.push(camera);
            continue;
        }
        let Some(exchanges) = prepared.players.iter().find(|p| p.main == main) else {
            out.push(h);
            views.push(camera);
            continue;
        };
        let kills = exchanges
            .kills
            .iter()
            .filter(|k| k.tick >= h.start_tick && k.tick < h.end_tick);
        let mut groups: Vec<Vec<&Kill>> = vec![];
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
            for &tick in exchanges
                .activity
                .iter()
                .filter(|&&t| t > last && t < h.end_tick)
            {
                if tick - settled > rate {
                    break;
                }
                settled = tick;
            }
            let requested = (settled + rate / 2).max(last + pacing::event_tail(rate));
            let Some(cut) = pacing::replay_cut(requested, cursor, h.end_tick, rate) else {
                continue;
            };
            if cut <= cursor || h.round_result.as_ref().is_some_and(|r| cut >= r.from_tick) {
                continue;
            }
            let mut candidates: Vec<_> = group
                .into_iter()
                .filter_map(|kill| {
                    let enemy = kill.victim.as_str();
                    let score = kill.score;
                    let start = kill.tick - rate * 3 / 2;
                    let end = kill.tick + pacing::event_tail(rate);
                    let wanted = selected.map_or(score >= 20, |s| {
                        s.contains(&(h.id.clone(), kill.tick, enemy.to_owned()))
                    });
                    if !wanted
                        || !kill.eligible
                        || used.contains(&(h.id.clone(), kill.tick, enemy.to_owned()))
                        || represented.iter().any(|(id, player, a, b)| {
                            id == &h.id && player == enemy && *a <= end && *b >= start
                        })
                    {
                        return None;
                    }
                    Some((score, kill))
                })
                .collect();
            if selected.is_none() {
                candidates = candidates
                    .into_iter()
                    .max_by_key(|(score, kill)| (*score, kill.tick))
                    .into_iter()
                    .collect();
            }
            if candidates.is_empty() {
                continue;
            }
            out.push(shot(&h, main, cursor, cut, demo));
            views.push(None);
            for (_, kill) in candidates {
                out.push(shot(
                    &h,
                    &kill.victim,
                    kill.tick - rate * 3 / 2,
                    kill.tick + pacing::event_tail(rate),
                    demo,
                ));
                views.push(None);
                used.insert((h.id.clone(), kill.tick, kill.victim.clone()));
            }
            cursor = cut;
        }
        if cursor == h.start_tick {
            out.push(h);
            views.push(None);
        } else if cursor < h.end_tick {
            let mut tail = shot(&h, main, cursor, h.end_tick, demo);
            tail.round_result = h.round_result;
            out.push(tail);
            views.push(None);
        }
    }
    (out, views)
}
