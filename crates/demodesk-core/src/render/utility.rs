//! Event-driven utility edits. Recording order may differ from the final story order.
use super::demo_data::{enemies, gun, Event};
use super::*;
use editing::{pacing, timeline::shot};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "camera", rename_all = "camelCase")]
pub(super) enum Camera {
    Follow(Chase),
    Observer(Observer),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Observer {
    camera: utility_observer::Camera,
    anchor_slot: i32,
}

#[cfg(test)]
impl Camera {
    fn follow(&self) -> &Chase {
        match self {
            Self::Follow(chase) => chase,
            Self::Observer(_) => panic!("expected follow camera"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Chase {
    pub steam_id: String,
    pub start_tick: i32,
    pub end_tick: i32,
    pub grenade: Option<Grenade>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Grenade {
    pub entity_index: i32,
    pub throw_tick: i32,
    pub class_name: String,
}

fn utility_weapon(weapon: &str) -> bool {
    matches!(
        weapon.trim_start_matches("weapon_"),
        "hegrenade" | "flashbang" | "smokegrenade" | "decoy" | "molotov" | "incgrenade" | "inferno"
    )
}

struct Candidate<'a> {
    effect: &'a Event,
    detonation: Option<&'a Event>,
    throw: &'a Event,
    utility_kill: bool,
    round_start: i32,
}

fn candidate<'a>(
    kill: &'a Event,
    main: &str,
    events: &'a [Event],
    rate: i32,
) -> Option<Candidate<'a>> {
    if !enemies(kill) {
        return None;
    }
    let killer = kill.attacker_steamid.as_deref()?;
    let victim = kill.user_steamid.as_deref()?;
    let utility_kill = utility_weapon(&kill.weapon);
    if !utility_kill && (!gun(&kill.weapon) || killer != main) {
        return None;
    }
    let round_start = events
        .iter()
        .rev()
        .find(|e| e.event_name == "round_start" && e.tick <= kill.tick)
        .map_or(seek::MIN_TICK, |e| e.tick);
    if utility_kill
        && killer != main
        && !events.iter().any(|e| {
            e.event_name == "player_hurt"
                && e.attacker_steamid.as_deref() == Some(main)
                && e.user_steamid.as_deref() == Some(victim)
                && e.attacker_team_num == kill.attacker_team_num
                && e.tick >= (kill.tick - 3 * rate).max(round_start)
                && e.tick <= kill.tick
                && e.dmg_health.unwrap_or(0) > 0
                && gun(&e.weapon)
        })
    {
        return None;
    }
    let effects = events.iter().rev().filter(|e| {
        e.user_steamid.as_deref() == Some(victim)
            && enemies(e)
            && e.attacker_team_num == kill.attacker_team_num
            && e.tick >= (kill.tick - 5 * rate).max(round_start)
            && e.tick <= kill.tick
    });
    for effect in effects {
        let Some(thrower) = effect.attacker_steamid.as_deref() else {
            continue;
        };
        let flash = effect.event_name == "player_blind"
            && effect.blind_duration.unwrap_or(0.0) >= 0.2
            && f64::from(kill.tick - effect.tick) / f64::from(rate)
                <= effect.blind_duration.unwrap_or(0.0) + 0.5;
        let damage = utility_weapon(&effect.weapon)
            && ((effect.event_name == "player_hurt" && effect.dmg_health.unwrap_or(0) > 0)
                || (effect.event_name == "player_death" && effect.tick == kill.tick));
        if utility_kill {
            if !damage || thrower != killer || effect.tick != kill.tick {
                continue;
            }
        } else if (!flash && !damage) || thrower == killer {
            continue;
        }
        let weapon = if flash {
            "flashbang"
        } else {
            effect.weapon.trim_start_matches("weapon_")
        };
        let burning = matches!(weapon, "inferno" | "molotov" | "incgrenade");
        let name = match weapon {
            "flashbang" => "flashbang_detonate",
            "hegrenade" => "hegrenade_detonate",
            "inferno" | "molotov" | "incgrenade" => "inferno_startburn",
            "decoy" => "decoy_detonate",
            "smokegrenade" => "smokegrenade_detonate",
            _ => continue,
        };
        let detonations: Vec<_> = events
            .iter()
            .filter(|d| {
                d.event_name == name
                    && d.user_steamid.as_deref() == Some(thrower)
                    && d.entityid.is_some()
                    && if burning {
                        d.tick <= effect.tick && effect.tick - d.tick <= 8 * rate
                    } else {
                        (d.tick - effect.tick).abs() <= 1
                            && (!flash || d.entityid == effect.entityid)
                    }
            })
            .collect();
        let det = if detonations.len() == 1 {
            Some(detonations[0])
        } else {
            None
        };
        if flash && det.is_none() {
            continue;
        }
        let reference_tick = det.map_or(effect.tick, |d| d.tick);
        let throws: Vec<_> = events
            .iter()
            .filter(|t| {
                let thrown = t.weapon.trim_start_matches("weapon_");
                t.event_name == "weapon_fire"
                    && t.user_steamid.as_deref() == Some(thrower)
                    && (if burning {
                        matches!(thrown, "molotov" | "incgrenade")
                    } else {
                        thrown == weapon
                    })
                    && t.tick >= round_start
                    && t.tick <= reference_tick
                    && reference_tick - t.tick
                        <= if weapon == "decoy" {
                            20 * rate
                        } else if burning {
                            10 * rate
                        } else {
                            3 * rate
                        }
                    && (!flash || reference_tick - t.tick >= rate)
            })
            .collect();
        // Without one identifiable throw, the original clip is safer than an unrelated cutaway.
        if throws.len() != 1 {
            continue;
        }
        let throw = throws[0];
        let shot_end = det
            .map_or(throw.tick + rate, |d| d.tick)
            .min(throw.tick + 3 * rate);
        if events.iter().any(|d| {
            d.event_name == "player_death"
                && d.user_steamid.as_deref() == Some(thrower)
                && d.tick >= (throw.tick - rate).max(round_start)
                && d.tick < shot_end
        }) {
            continue;
        }
        return Some(Candidate {
            effect,
            detonation: det,
            throw,
            utility_kill,
            round_start,
        });
    }
    None
}

#[derive(Serialize, Deserialize)]
pub(super) struct Prepared {
    stories: Vec<Story>,
}

#[derive(Serialize, Deserialize)]
struct Story {
    round_start: i32,
    main: String,
    kill_tick: i32,
    effect_tick: i32,
    chase: Chase,
    shooter: Option<View>,
    victim: View,
    return_start: Option<i32>,
    observer: Option<Observer>,
}

#[derive(Serialize, Deserialize)]
struct View {
    player: String,
    start: i32,
    end: i32,
}

/// Resolve event relationships once; composition only places these prepared stories.
pub(super) fn prepare(originals: &[Highlight], demo: &DemoInfo, events: &[Event]) -> Prepared {
    let rate = demo.tick_rate.round() as i32;
    let mut stories = vec![];
    if rate <= 0 {
        return Prepared { stories };
    }
    let mains: std::collections::BTreeSet<_> = originals
        .iter()
        .map(|h| h.player.steamid.as_str())
        .collect();
    for main in mains {
        for kill in events.iter().filter(|e| {
            e.event_name == "player_death"
                && originals.iter().any(|h| {
                    h.player.steamid == main
                        && e.tick >= h.start_tick - 4 * rate
                        && e.tick < h.end_tick
                })
        }) {
            let Some(c) = candidate(kill, main, events, rate) else {
                continue;
            };
            let thrower = c.throw.user_steamid.as_deref().unwrap_or_default();
            let victim = kill.user_steamid.as_deref().unwrap_or_default();
            if [thrower, victim, main].iter().any(|id| {
                !demo
                    .players
                    .iter()
                    .any(|p| p.steamid == *id && p.user_id.is_some())
            }) {
                continue;
            }
            let proposed_start = (c.throw.tick - rate * 3 / 4)
                .max(c.round_start)
                .max(seek::MIN_TICK + 1);
            let chase_end = c
                .detonation
                .map_or(c.throw.tick + rate / 2, |d| d.tick - (rate / 5).max(1))
                .min(c.throw.tick + 3 * rate)
                .min(c.effect.tick - 1);
            let Some(start) = pacing::lead_start(
                proposed_start,
                chase_end,
                c.round_start.max(seek::MIN_TICK + 1),
                rate,
            ) else {
                continue;
            };
            let round_end = events
                .iter()
                .find(|e| e.event_name == "round_start" && e.tick > kill.tick)
                .map_or(i32::MAX, |e| e.tick);
            let enemy_end = if c.utility_kill {
                kill.tick + pacing::event_tail(rate)
            } else {
                (c.effect.tick + rate + rate / 10).min(kill.tick - 2)
            };
            let Some(enemy_start) = pacing::lead_start(
                c.effect.tick - rate / 5,
                enemy_end,
                c.round_start.max(seek::MIN_TICK + 1),
                rate,
            ) else {
                continue;
            };
            if enemy_end <= c.effect.tick || enemy_end >= round_end {
                continue;
            }
            let shooter = c
                .utility_kill
                .then(|| {
                    events.iter().rev().find(|e| {
                        e.event_name == "player_hurt"
                            && e.user_steamid.as_deref() == Some(victim)
                            && e.attacker_steamid.as_deref() != Some(thrower)
                            && e.attacker_team_num == kill.attacker_team_num
                            && enemies(e)
                            && gun(&e.weapon)
                            && e.dmg_health.unwrap_or(0) > 0
                            && e.tick >= (kill.tick - 3 * rate).max(c.round_start)
                            && e.tick < kill.tick
                    })
                })
                .flatten()
                .and_then(|damage| {
                    let shooter = damage.attacker_steamid.as_deref().filter(|id| {
                        demo.players
                            .iter()
                            .any(|p| p.steamid == *id && p.user_id.is_some())
                    })?;
                    let end = (damage.tick + rate / 3).min(kill.tick - 1);
                    Some(View {
                        player: shooter.into(),
                        start: pacing::lead_start(
                            damage.tick - rate / 2,
                            end,
                            c.round_start,
                            rate,
                        )?,
                        end,
                    })
                });
            stories.push(Story {
                round_start: c.round_start,
                observer: None,
                main: main.into(),
                kill_tick: kill.tick,
                effect_tick: c.effect.tick,
                chase: Chase {
                    steam_id: thrower.into(),
                    start_tick: start,
                    end_tick: chase_end,
                    grenade: c.detonation.and_then(|det| {
                        let class_name = match c.throw.weapon.trim_start_matches("weapon_") {
                            "flashbang" => "flashbang_projectile",
                            "hegrenade" => "hegrenade_projectile",
                            _ => return None,
                        };
                        Some(Grenade {
                            entity_index: det.entityid?,
                            throw_tick: c.throw.tick + 2,
                            class_name: class_name.into(),
                        })
                    }),
                },
                shooter,
                victim: View {
                    player: victim.into(),
                    start: enemy_start,
                    end: enemy_end,
                },
                return_start: (!c.utility_kill).then_some(
                    (c.effect.tick - rate / 2)
                        .min(kill.tick + pacing::event_tail(rate) - pacing::minimum(rate))
                        .max(kill.tick - 3 * rate)
                        .max(c.round_start),
                ),
            });
        }
    }
    Prepared { stories }
}

/// Qualify optional observer shots during analysis; composition and export only use saved data.
pub(super) fn prepare_observers(
    prepared: &mut Prepared,
    tracks: &demo_data::Tracks,
    projectiles: &demo_data::Projectiles,
    demo: &DemoInfo,
    ray: &impl Fn(
        crate::analysis::line_of_sight::Vec3,
        crate::analysis::line_of_sight::Vec3,
    ) -> crate::analysis::line_of_sight::Occlusion,
    cancel: &AtomicBool,
) {
    let rate = demo.tick_rate.round() as i32;
    if rate <= 0 {
        return;
    }
    for story in &mut prepared.stories {
        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            break;
        }
        let Some(grenade) = story
            .chase
            .grenade
            .as_ref()
            .filter(|g| g.class_name == "hegrenade_projectile")
        else {
            continue;
        };
        let Some(path) = projectiles.get(&grenade.entity_index) else {
            continue;
        };
        let flight: Vec<_> = path
            .iter()
            .copied()
            .filter(|(tick, _)| *tick >= grenade.throw_tick - 2 && *tick <= story.effect_tick)
            .collect();
        if flight.len() < 8
            || flight.windows(2).any(|p| p[1].0 - p[0].0 > 2)
            || flight.last().is_none_or(|p| story.effect_tick - p.0 > 2)
        {
            continue;
        }
        let end = story.victim.end;
        let positions = |id: &str| -> Vec<(i32, crate::analysis::line_of_sight::Vec3)> {
            tracks
                .range(flight[0].0 - rate / 4..=end)
                .filter_map(|(&tick, players)| {
                    players
                        .iter()
                        .find(|p| p.id == id && p.alive)
                        .map(|p| (tick, p.position))
                })
                .collect()
        };
        let victim = positions(&story.victim.player);
        if victim.is_empty()
            || victim
                .last()
                .is_none_or(|p| p.0 < story.effect_tick - rate / 4)
        {
            continue;
        }
        let mut body = vec![];
        let mut missing = false;
        for tick in (flight[0].0..=end).step_by((rate / 16).max(1) as usize) {
            let Some(&(at, p)) = victim.iter().rev().find(|p| p.0 <= tick) else {
                missing = true;
                break;
            };
            if tick < story.effect_tick && tick - at > rate / 4 {
                missing = true;
                break;
            }
            body.push((tick, p));
        }
        if missing || body.len() < 2 {
            continue;
        }
        let main = positions(&story.main);
        let (cameras, _) =
            utility_observer::search(&flight, &body, &main, story.effect_tick, rate, ray);
        let Some(mut observer) = cameras.into_iter().next() else {
            continue;
        };
        observer.end_tick = end;
        if observer.end_tick - observer.start_tick < pacing::minimum(rate) {
            continue;
        }
        // Keep spectator lifecycle independent of the victim's death in this world-space shot.
        let anchor_slot = std::iter::once(story.main.as_str())
            .chain(std::iter::once(story.chase.steam_id.as_str()))
            .chain(demo.players.iter().map(|p| p.steamid.as_str()))
            .filter(|id| *id != story.victim.player)
            .find_map(|id| {
                let slot = demo.players.iter().find(|p| p.steamid == id)?.user_id? + 1;
                let after = tracks
                    .range(end..)
                    .next()
                    .filter(|(tick, _)| **tick - end <= rate / 4)?;
                let alive = |players: &Vec<demo_data::Player>| {
                    players.iter().any(|p| p.id == id && p.alive)
                };
                (alive(after.1)
                    && tracks
                        .range(observer.start_tick..=end)
                        .all(|(_, ps)| alive(ps)))
                .then_some(slot)
            });
        let Some(anchor_slot) = anchor_slot else {
            continue;
        };
        // Keep the wind-up and release before the observer; a small rewind may bridge the cut.
        let throw_end = grenade.throw_tick + rate / 4;
        let Some(throw_start) = pacing::lead_start(
            story
                .chase
                .start_tick
                .min(throw_end - pacing::minimum(rate)),
            throw_end,
            story.round_start.max(seek::MIN_TICK + 1),
            rate,
        ) else {
            continue;
        };
        story.chase.start_tick = throw_start;
        story.chase.end_tick = throw_end;
        story.observer = Some(Observer {
            camera: observer,
            anchor_slot,
        });
    }
}

#[cfg(test)]
pub(super) fn plan(
    recording: Vec<Highlight>,
    demo: &DemoInfo,
    events: &[Event],
) -> (Vec<Highlight>, Vec<Option<Camera>>) {
    let prepared = prepare(&recording, demo, events);
    compose(recording, demo, &prepared)
}

pub(super) fn compose(
    recording: Vec<Highlight>,
    demo: &DemoInfo,
    prepared: &Prepared,
) -> (Vec<Highlight>, Vec<Option<Camera>>) {
    let rate = demo.tick_rate.round() as i32;
    let mut shots = vec![];
    let mut cameras = vec![];
    for h in recording {
        let mut cursor = h.start_tick;
        let mut edited = false;
        for story in prepared.stories.iter().filter(|s| {
            s.main == h.player.steamid && s.kill_tick >= h.start_tick && s.kill_tick < h.end_tick
        }) {
            let start = story.chase.start_tick;
            if edited && start < cursor {
                continue;
            }
            if h.kills.iter().any(|k| {
                k.tick >= cursor && k.tick < story.effect_tick && k.tick != story.kill_tick
            }) {
                continue;
            }
            // A throw before the clip is a rewind, not permission to discard the
            // original POV leading up to the story's return to the main player.
            let prefix_end = if start <= cursor {
                story
                    .return_start
                    .unwrap_or(story.effect_tick)
                    .min(h.end_tick)
            } else {
                start
            };
            let prefix_end = if prefix_end > cursor {
                prefix_end.max(cursor + pacing::minimum(rate))
            } else {
                cursor
            };
            // Preserve the initial POV without showing the same main-player kill twice.
            if prefix_end > h.end_tick
                || (story.return_start.is_some() && prefix_end > story.kill_tick)
                || (story.return_start.is_none()
                    && h.end_tick > story.victim.end.max(prefix_end)
                    && h.end_tick - story.victim.end.max(prefix_end) < pacing::minimum(rate))
            {
                continue;
            }
            let return_window = if let Some(main_start) = story.return_start {
                let end = (story.kill_tick + pacing::event_tail(rate)).min(h.end_tick);
                let Some(start) = pacing::lead_start(main_start, end, main_start, rate) else {
                    continue;
                };
                Some((start, end))
            } else {
                None
            };
            if prefix_end > cursor {
                shots.push(shot(&h, &h.player.steamid, cursor, prefix_end, demo));
                cameras.push(None);
            }
            shots.push(shot(
                &h,
                &story.chase.steam_id,
                start,
                story.chase.end_tick,
                demo,
            ));
            cameras.push(Some(Camera::Follow(story.chase.clone())));
            if let Some(view) = &story.shooter {
                shots.push(shot(&h, &view.player, view.start, view.end, demo));
                cameras.push(None);
            }
            if let Some(observer) = &story.observer {
                shots.push(shot(
                    &h,
                    &story.victim.player,
                    observer.camera.start_tick,
                    observer.camera.end_tick,
                    demo,
                ));
                cameras.push(Some(Camera::Observer(observer.clone())));
            } else {
                shots.push(shot(
                    &h,
                    &story.victim.player,
                    story.victim.start,
                    story.victim.end,
                    demo,
                ));
                cameras.push(None);
            }
            if let Some((main_start, main_end)) = return_window {
                shots.push(shot(&h, &h.player.steamid, main_start, main_end, demo));
                cameras.push(None);
                cursor = main_end;
            } else {
                cursor = story.victim.end.max(prefix_end).min(h.end_tick);
            }
            edited = true;
        }
        if !edited {
            shots.push(h);
            cameras.push(None);
        } else if cursor < h.end_tick {
            let mut tail = shot(&h, &h.player.steamid, cursor, h.end_tick, demo);
            tail.round_result = h.round_result;
            shots.push(tail);
            cameras.push(None);
        }
    }
    // Avoid an unnecessary seek/audio boundary between the attack and its continuous aftermath.
    let mut joined: Vec<Highlight> = Vec::new();
    let mut joined_cameras: Vec<Option<Camera>> = Vec::new();
    for (shot, camera) in shots.into_iter().zip(cameras) {
        if camera.is_none() && joined_cameras.last().is_some_and(Option::is_none) {
            if let Some(last) = joined.last_mut().filter(|last| {
                last.id == shot.id
                    && last.player.steamid == shot.player.steamid
                    && last.end_tick == shot.start_tick
            }) {
                last.end_tick = shot.end_tick;
                last.round_result = shot.round_result;
                continue;
            }
        }
        joined.push(shot);
        joined_cameras.push(camera);
    }
    (joined, joined_cameras)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn observer_keeps_throw_and_death_hold_and_blocked_view_keeps_original_story() {
        use crate::analysis::line_of_sight::Occlusion;
        let (demo, mut h, events) = fixture(true, true, 0);
        h.player.steamid = "ally".into();
        let tracks = (700..=1100)
            .step_by(4)
            .map(|tick| {
                (
                    tick,
                    ["ally", "enemy"]
                        .map(|id| demo_data::Player {
                            id: id.into(),
                            team: if id == "ally" { 3 } else { 2 },
                            alive: id == "ally" || tick < 1000,
                            position: if id == "ally" {
                                [0., 0., 0.]
                            } else {
                                [512., 0., 0.]
                            },
                            view: None,
                        })
                        .to_vec(),
                )
            })
            .collect();
        let projectiles = [(
            500,
            (900..=1000)
                .map(|tick| (tick, [f64::from(tick - 900) * 4., 0., 36.]))
                .collect(),
        )]
        .into_iter()
        .collect();
        let mut prepared = prepare(&[h.clone()], &demo, &events);
        let baseline = compose(vec![h.clone()], &demo, &prepared);
        prepare_observers(
            &mut prepared,
            &tracks,
            &projectiles,
            &demo,
            &|_, _| Occlusion::Blocked,
            &AtomicBool::new(false),
        );
        assert_eq!(
            serde_json::to_value(compose(vec![h.clone()], &demo, &prepared)).unwrap(),
            serde_json::to_value(baseline).unwrap()
        );
        prepare_observers(
            &mut prepared,
            &tracks,
            &projectiles,
            &demo,
            &|_, _| Occlusion::Clear,
            &AtomicBool::new(false),
        );
        // The saved analysis includes the chosen camera; composing it does not run the solver.
        let saved: Prepared =
            serde_json::from_slice(&serde_json::to_vec(&prepared).unwrap()).unwrap();
        let (shots, cameras) = compose(vec![h], &demo, &saved);
        let observer = cameras
            .iter()
            .position(|c| matches!(c, Some(Camera::Observer(_))))
            .unwrap();
        let Some(Camera::Observer(view)) = &cameras[observer] else {
            unreachable!()
        };
        assert_eq!(
            view.anchor_slot, 2,
            "anchor must be the living thrower, not the dying victim"
        );
        let dir = tempfile::tempdir().unwrap();
        let mut commands = vec![];
        schedule(&mut commands, &[observer], &cameras, dir.path(), 64., true).unwrap();
        let anchor = commands
            .iter()
            .find(|c| c.cmd.contains("spec_player 2"))
            .unwrap();
        assert!(anchor.tick < f64::from(view.camera.start_tick));
        assert!(commands
            .iter()
            .any(|c| c.cmd == "r_drawviewmodel 1" && c.tick == f64::from(view.camera.end_tick)));
        let Some(Camera::Follow(throw)) = &cameras[observer - 1] else {
            panic!("throw must precede observer")
        };
        assert!(throw.start_tick < 900 && throw.end_tick > 900 && throw.end_tick < 1000);
        assert!(shots
            .iter()
            .all(|s| s.end_tick - s.start_tick >= pacing::minimum(64)));
        assert_eq!(shots[observer].end_tick, 1000 + pacing::event_tail(64));
        assert_eq!(
            cameras
                .iter()
                .filter(|c| matches!(c, Some(Camera::Observer(_))))
                .count(),
            1
        );
    }

    fn fixture(he: bool, direct: bool, delay: i32) -> (DemoInfo, Highlight, Vec<Event>) {
        let demo: DemoInfo = serde_json::from_value(serde_json::json!({
            "path":"test.dem", "mapName":"de_test", "serverName":"test", "tickRate":64.0,
            "players":[{"steamid":"main","name":"Main","teamNumber":3,"userId":0},
                {"steamid":"ally","name":"Ally","teamNumber":3,"userId":1},
                {"steamid":"enemy","name":"Enemy","teamNumber":2,"userId":2}]
        }))
        .unwrap();
        let h = serde_json::from_value(serde_json::json!({"id":"one","player":{"steamid":"main","name":"Main"},
            "round":1,"startTick":800,"endTick":1600,"anchorTick":1200,"score":1,"tags":[],"title":"clip","kills":[],"breakdown":{}})).unwrap();
        let events = vec![
            serde_json::json!({"tick":700,"event_name":"round_start"}),
            serde_json::json!({"tick":900,"event_name":"weapon_fire","weapon":if he {"weapon_hegrenade"} else {"weapon_flashbang"},"user_steamid":"ally"}),
            serde_json::json!({"tick":1000,"event_name":if he {"hegrenade_detonate"} else {"flashbang_detonate"},"user_steamid":"ally","entityid":500}),
            serde_json::json!({"tick":1000,"event_name":if he {"player_hurt"} else {"player_blind"},"weapon":if he {"hegrenade"} else {""},"user_steamid":"enemy","attacker_steamid":"ally","user_team_num":2,"attacker_team_num":3,"entityid":500,"blind_duration":3.0,"dmg_health":40}),
            serde_json::json!({"tick":1000+delay,"event_name":"player_death","weapon":if direct {"hegrenade"} else {"ak47"},"user_steamid":"enemy","attacker_steamid":if direct {"ally"} else {"main"},"user_team_num":2,"attacker_team_num":3}),
        ].into_iter().map(|v| serde_json::from_value(v).unwrap()).collect();
        (demo, h, events)
    }

    #[test]
    fn minimum_view_duration_includes_utility_death_and_shooter() {
        let (demo, mut h, mut events) = fixture(true, true, 0);
        h.player.steamid = "ally".into();
        events.push(serde_json::from_value(serde_json::json!({"tick":980,"event_name":"player_hurt","weapon":"ak47","user_steamid":"enemy","attacker_steamid":"main","user_team_num":2,"attacker_team_num":3,"dmg_health":60})).unwrap());
        events.sort_by_key(|e| e.tick);
        let (shots, cameras) = plan(vec![h], &demo, &events);
        assert_eq!(cameras.iter().flatten().count(), 1);
        assert!(shots.iter().all(|h| h.end_tick - h.start_tick >= 96));
        let victim = shots.iter().find(|h| h.player.steamid == "enemy").unwrap();
        assert!(victim.end_tick >= 1032);
    }

    #[test]
    fn minimum_views_do_not_cross_round_boundaries() {
        let (demo, mut h, mut events) = fixture(true, true, 0);
        h.player.steamid = "ally".into();
        events.push(
            serde_json::from_value(serde_json::json!({"tick":1020,"event_name":"round_start"}))
                .unwrap(),
        );
        events.sort_by_key(|e| e.tick);
        assert!(
            plan(vec![h], &demo, &events).1.iter().all(Option::is_none),
            "death hold must not continue into the next round"
        );

        let (demo, h, mut events) = fixture(false, false, 12);
        events[1].tick = 930;
        events.push(
            serde_json::from_value(serde_json::json!({"tick":920,"event_name":"round_start"}))
                .unwrap(),
        );
        events.sort_by_key(|e| e.tick);
        assert!(
            plan(vec![h], &demo, &events).1.iter().all(Option::is_none),
            "the throw cannot borrow lead-in from the preceding round"
        );
    }

    #[test]
    fn a_preceding_throw_keeps_the_original_pov_before_rewinding() {
        for direct in [false, true] {
            let (demo, mut h, events) = fixture(direct, direct, if direct { 0 } else { 100 });
            h.start_tick = 950;
            if direct {
                h.player.steamid = "ally".into();
            }
            let (shots, cameras) = plan(vec![h.clone()], &demo, &events);
            assert_eq!(shots[0].player.steamid, h.player.steamid);
            assert_eq!(shots[0].start_tick, 950);
            assert_eq!(shots[0].end_tick, 1046);
            assert!(cameras[0].is_none());
            assert!(cameras[1].is_some());
            assert_eq!(shots[1].start_tick, 852);
        }
    }

    #[test]
    fn fast_death_rewinds_without_showing_victim_death_or_losing_kill() {
        let (demo, h, e) = fixture(false, false, 12);
        let (shots, cameras) = plan(vec![h], &demo, &e);
        assert_eq!(cameras.iter().flatten().count(), 1);
        let enemy = shots.iter().find(|s| s.player.steamid == "enemy").unwrap();
        assert!(enemy.end_tick < 1012 && enemy.end_tick > 1000);
        assert!(enemy.end_tick - enemy.start_tick >= 96);
        let main = shots
            .iter()
            .find(|s| s.player.steamid == "main" && s.end_tick > 1012)
            .unwrap();
        assert!(main.start_tick < 1000 && main.start_tick >= 1012 - 192);
        assert!(main.end_tick > 1012);
        assert!(recording_passes(&to_render_clips(&demo, &shots), 64.0).len() > 1);
    }

    #[test]
    fn he_damage_then_gun_kill_and_gun_damage_then_utility_kill() {
        let (demo, h, e) = fixture(true, false, 100);
        let (shots, cameras) = plan(vec![h.clone()], &demo, &e);
        assert_eq!(
            cameras
                .iter()
                .flatten()
                .next()
                .unwrap()
                .follow()
                .grenade
                .as_ref()
                .unwrap()
                .class_name,
            "hegrenade_projectile"
        );
        assert!(shots
            .iter()
            .any(|s| s.player.steamid == "main" && s.start_tick < 1100 && s.end_tick > 1100));
        let (_, _, mut e) = fixture(true, true, 0);
        e.push(serde_json::from_value(serde_json::json!({"tick":980,"event_name":"player_hurt","weapon":"ak47","user_steamid":"enemy","attacker_steamid":"main","user_team_num":2,"attacker_team_num":3,"dmg_health":60})).unwrap());
        e.sort_by_key(|e| e.tick);
        let (shots, cameras) = plan(vec![h], &demo, &e);
        assert_eq!(cameras.iter().flatten().count(), 1);
        let enemy = shots
            .iter()
            .position(|s| s.player.steamid == "enemy")
            .unwrap();
        assert_eq!(shots[enemy - 1].player.steamid, "main");
        assert!(shots[enemy - 1].end_tick < 1000);
        assert!(shots[enemy].end_tick > 1000);
    }

    #[test]
    fn direct_utility_kill_is_a_payoff_without_replaying_a_gun_kill() {
        let (demo, mut h, mut e) = fixture(true, true, 0);
        h.player.steamid = "ally".into();
        let (shots, cameras) = plan(vec![h.clone()], &demo, &e);
        assert_eq!(cameras.iter().flatten().count(), 1);
        let enemy = shots
            .iter()
            .position(|s| s.player.steamid == "enemy")
            .unwrap();
        assert!(shots[enemy + 1].start_tick >= 1000);
        e.retain(|e| e.event_name != "weapon_fire");
        assert_eq!(plan(vec![h], &demo, &e).1.iter().flatten().count(), 0);
    }

    #[test]
    fn no_effect_friendly_fire_expired_flash_and_ambiguous_throw_keep_original() {
        let (demo, h, e) = fixture(false, false, 100);
        for mode in 0..4 {
            let mut e = e.clone();
            match mode {
                0 => e.retain(|e| e.event_name != "player_blind"),
                1 => e[3].attacker_team_num = Some(2),
                2 => e[3].blind_duration = Some(0.2),
                _ => {
                    let mut second = e[1].clone();
                    second.tick = 910;
                    e.push(second);
                }
            }
            let (shots, cameras) = plan(vec![h.clone()], &demo, &e);
            assert_eq!(shots.len(), 1, "mode {mode}");
            assert_eq!(shots[0].start_tick, h.start_tick);
            assert!(cameras[0].is_none());
        }
    }

    #[test]
    fn main_replay_never_exceeds_three_seconds_before_kill() {
        let (demo, h, mut e) = fixture(false, false, 215);
        e[3].blind_duration = Some(3.26);
        let (shots, _) = plan(vec![h], &demo, &e);
        let main = shots
            .iter()
            .find(|s| s.player.steamid == "main" && s.end_tick > 1215)
            .unwrap();
        assert_eq!(main.start_tick, 1215 - 192);
    }

    #[test]
    fn all_utility_damage_and_one_hp_kills_are_supported_without_guessing_projectiles() {
        for weapon in [
            "flashbang",
            "smokegrenade",
            "decoy",
            "molotov",
            "incgrenade",
            "inferno",
        ] {
            for direct in [false, true] {
                let (demo, mut h, mut e) = fixture(true, direct, if direct { 0 } else { 30 });
                e[1].weapon = format!(
                    "weapon_{}",
                    if weapon == "inferno" {
                        "molotov"
                    } else {
                        weapon
                    }
                );
                e[3].weapon = weapon.into();
                e[3].dmg_health = Some(1);
                if direct {
                    e[4].weapon = weapon.into();
                    h.player.steamid = "ally".into();
                }
                e.remove(2); // Damage/kill is authoritative even without a matching detonation entity.
                let (shots, cameras) = plan(vec![h], &demo, &e);
                assert_eq!(
                    cameras.iter().flatten().count(),
                    1,
                    "{weapon} direct={direct}"
                );
                assert!(cameras
                    .iter()
                    .flatten()
                    .next()
                    .unwrap()
                    .follow()
                    .grenade
                    .is_none());
                assert_eq!(
                    shots.iter().filter(|s| s.player.steamid == "enemy").count(),
                    1
                );
            }
        }
    }

    #[test]
    fn direct_utility_death_does_not_require_a_separate_damage_event() {
        let (demo, mut h, mut e) = fixture(true, true, 0);
        h.player.steamid = "ally".into();
        e.retain(|e| e.event_name != "player_hurt");
        assert_eq!(plan(vec![h], &demo, &e).1.iter().flatten().count(), 1);
    }

    #[test]
    fn repeated_fire_damage_creates_one_sequence_and_rounds_do_not_cross() {
        let (demo, h, mut e) = fixture(true, false, 120);
        e[1].weapon = "weapon_molotov".into();
        e[2].event_name = "inferno_startburn".into();
        e[3].weapon = "inferno".into();
        for tick in [1020, 1040, 1060, 1080] {
            let mut damage = e[3].clone();
            damage.tick = tick;
            e.push(damage);
        }
        e.sort_by_key(|e| e.tick);
        assert_eq!(
            plan(vec![h.clone()], &demo, &e).1.iter().flatten().count(),
            1
        );
        e.push(Event {
            event_name: "round_start".into(),
            tick: 1100,
            ..Default::default()
        });
        e.sort_by_key(|e| e.tick);
        assert_eq!(plan(vec![h], &demo, &e).1.iter().flatten().count(), 0);
    }

    #[test]
    fn option_defaults_off_and_survives_saved_jobs() {
        let old: RenderOptions = serde_json::from_str("{}").unwrap();
        assert!(!old.utility_teamwork);
        let enabled: RenderOptions = serde_json::from_str(r#"{"utilityTeamwork":true}"#).unwrap();
        assert!(enabled.utility_teamwork);
        assert_eq!(
            serde_json::to_value(enabled).unwrap()["utilityTeamwork"],
            true
        );
    }
}

pub(super) fn schedule(
    schedule: &mut Vec<actions::Scheduled>,
    indices: &[usize],
    cameras: &[Option<Camera>],
    directory: &Path,
    rate: f64,
    viewmodel: bool,
) -> Result<()> {
    for (local, &global) in indices.iter().enumerate() {
        let Some(camera) = &cameras[global] else {
            continue;
        };
        std::fs::create_dir_all(directory)?;
        let file = directory.join(format!("utility-{local}.mjs"));
        let (start, end, script, observer) = match camera {
            Camera::Follow(camera) => (
                camera.start_tick,
                camera.end_tick,
                format!(
                    "{}\nstartAimFollow({});\n",
                    include_str!("utility-camera.mjs"),
                    serde_json::to_string(camera)?
                ),
                false,
            ),
            Camera::Observer(observer) => {
                let camera = &observer.camera;
                std::fs::write(
                    directory.join("team-push-camera.mjs"),
                    include_str!("team-push-camera.mjs"),
                )?;
                std::fs::write(
                    directory.join("camera-transition.mjs"),
                    include_str!("camera-transition.mjs"),
                )?;
                (
                    camera.start_tick,
                    camera.end_tick,
                    format!(
                        "{}\nstartUtilityObserver({}, {});\n",
                        include_str!("utility-observer-camera.mjs"),
                        serde_json::to_string(camera)?,
                        rate
                    ),
                    true,
                )
            }
        };
        std::fs::write(&file, format!("mirv.events.cViewRenderSetupView.off('demodesk/editing-aim-follow');\nmirv.events.cViewRenderSetupView.off('demodesk/utility-observer');\n{script}"))?;
        let setup = seek::SeekWindow::new(start, end, None, rate.round() as i32).setup_tick;
        // Loading after the standard POV setup preserves the shared seek/audio gate.
        schedule.push(actions::Scheduled {
            tick: f64::from(setup + 1),
            cmd: if observer {
                "spec_mode 3; r_drawviewmodel 0"
            } else {
                "spec_mode 3"
            }
            .into(),
        });
        schedule.push(actions::Scheduled {
            tick: f64::from(setup + 1) + 0.001,
            cmd: format!("mirv_script_load \"{}\"", to_forward_slashes(&file)),
        });
        if observer {
            if let Camera::Observer(view) = camera {
                schedule.push(actions::Scheduled {
                    tick: f64::from((setup + rate.round() as i32 / 4 + 1).min(start - 1)) + 0.01,
                    cmd: format!(
                        "spec_lock_to_accountid 0; spec_player {}; spec_mode 3",
                        view.anchor_slot
                    ),
                });
            }
            schedule.push(actions::Scheduled {
                tick: f64::from(end),
                cmd: format!("r_drawviewmodel {}", viewmodel as u8),
            });
        }
    }
    schedule.sort_by(|a, b| a.tick.total_cmp(&b.tick));
    Ok(())
}
