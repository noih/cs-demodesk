//! Event-driven utility edits. Recording order may differ from the final story order.
use super::*;
use serde_json::Value;

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub(super) struct Event {
    pub(super) tick: i32,
    pub(super) event_name: String,
    pub(super) weapon: String,
    pub(super) user_steamid: Option<String>,
    pub(super) attacker_steamid: Option<String>,
    pub(super) user_team_num: Option<i32>,
    pub(super) attacker_team_num: Option<i32>,
    entityid: Option<i32>,
    pub(super) blind_duration: Option<f64>,
    pub(super) dmg_health: Option<i32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Chase {
    pub steam_id: String,
    pub start_tick: i32,
    pub end_tick: i32,
    pub grenade: Option<Grenade>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Grenade {
    pub entity_index: i32,
    pub throw_tick: i32,
    pub class_name: String,
}

pub(super) fn load(path: &Path, cancel: &AtomicBool) -> Result<Vec<Event>> {
    Control::new(cancel, &mut |_| {}).check_cancelled()?;
    let bytes = std::fs::read(path)?;
    let names = [
        "weapon_fire",
        "player_blind",
        "player_hurt",
        "player_death",
        "flashbang_detonate",
        "hegrenade_detonate",
        "round_start",
        "inferno_startburn",
        "decoy_detonate",
        "smokegrenade_detonate",
    ];
    let data = crate::parser::DemoParser::new().events(
        &bytes,
        &names.map(String::from),
        &["team_num".into()],
        &[],
    )?;
    Control::new(cancel, &mut |_| {}).check_cancelled()?;
    let mut events: Vec<Event> = data
        .game_events
        .iter()
        .map(|e| {
            let value: Value = serde_json::to_value(e)?;
            serde_json::from_value(value)
        })
        .collect::<std::result::Result<_, _>>()?;
    events.sort_by_key(|e| e.tick);
    Ok(events)
}

fn utility_weapon(weapon: &str) -> bool {
    matches!(
        weapon.trim_start_matches("weapon_"),
        "hegrenade" | "flashbang" | "smokegrenade" | "decoy" | "molotov" | "incgrenade" | "inferno"
    )
}

pub(super) fn gun(weapon: &str) -> bool {
    matches!(
        weapon.trim_start_matches("weapon_"),
        "ak47"
            | "m4a1"
            | "m4a1_silencer"
            | "awp"
            | "ssg08"
            | "scar20"
            | "g3sg1"
            | "aug"
            | "sg556"
            | "famas"
            | "galilar"
            | "glock"
            | "hkp2000"
            | "usp_silencer"
            | "p250"
            | "deagle"
            | "revolver"
            | "tec9"
            | "fiveseven"
            | "cz75a"
            | "elite"
            | "mac10"
            | "mp9"
            | "mp7"
            | "mp5sd"
            | "ump45"
            | "p90"
            | "bizon"
            | "nova"
            | "xm1014"
            | "mag7"
            | "sawedoff"
            | "m249"
            | "negev"
    )
}

pub(super) fn enemies(event: &Event) -> bool {
    matches!(
        (event.attacker_team_num, event.user_team_num),
        (Some(2), Some(3)) | (Some(3), Some(2))
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

pub(super) fn shot(
    h: &Highlight,
    player: &str,
    start: i32,
    end: i32,
    demo: &DemoInfo,
) -> Highlight {
    let mut out = h.clone();
    out.start_tick = start;
    out.end_tick = end;
    out.anchor_tick = h.anchor_tick.clamp(start, end);
    out.key_moments.clear();
    out.round_result = None;
    out.player.steamid = player.into();
    out.player.name = demo
        .players
        .iter()
        .find(|p| p.steamid == player)
        .map_or_else(|| h.player.name.clone(), |p| p.name.clone());
    out
}

/// Preserve final story order here. recording_passes separately arranges forward-only capture.
pub(super) fn plan(
    recording: Vec<Highlight>,
    demo: &DemoInfo,
    events: &[Event],
) -> (Vec<Highlight>, Vec<Option<Chase>>) {
    let rate = demo.tick_rate.round() as i32;
    let mut shots = Vec::new();
    let mut cameras = Vec::new();
    if rate <= 0 {
        return (recording.clone(), vec![None; recording.len()]);
    }
    for h in recording {
        let mut cursor = h.start_tick;
        let mut edited = false;
        for kill in events.iter().filter(|e| {
            e.event_name == "player_death" && e.tick >= h.start_tick && e.tick < h.end_tick
        }) {
            let Some(c) = candidate(kill, &h.player.steamid, events, rate) else {
                continue;
            };
            let thrower = c.throw.user_steamid.as_deref().unwrap_or_default();
            let victim = kill.user_steamid.as_deref().unwrap_or_default();
            if [thrower, victim, &h.player.steamid].iter().any(|id| {
                !demo
                    .players
                    .iter()
                    .any(|p| p.steamid == *id && p.user_id.is_some())
            }) {
                continue;
            }
            let start = (c.throw.tick - rate * 3 / 4)
                .max(c.round_start)
                .max(seek::MIN_TICK + 1);
            // Do not replace an earlier kill or replay a utility event already used in this window.
            if edited && start < cursor {
                continue;
            }
            if h.kills
                .iter()
                .any(|k| k.tick >= cursor && k.tick < c.effect.tick && k.tick != kill.tick)
            {
                continue;
            }
            let chase_end = c
                .detonation
                .map_or(c.throw.tick + rate / 2, |d| d.tick - (rate / 5).max(1))
                .min(c.throw.tick + 3 * rate)
                .min(c.effect.tick - 1);
            let enemy_end = if c.utility_kill {
                kill.tick + rate / 3
            } else {
                (c.effect.tick + rate + rate / 10).min(kill.tick - 2)
            };
            let enemy_start = (c.effect.tick - rate / 5)
                .min(enemy_end - rate)
                .max(c.round_start);
            if start >= chase_end || enemy_end <= c.effect.tick || enemy_start >= enemy_end {
                continue;
            }
            if start > cursor && start - cursor >= rate / 3 {
                shots.push(shot(&h, &h.player.steamid, cursor, start, demo));
                cameras.push(None);
            }
            shots.push(shot(&h, thrower, start, chase_end, demo));
            cameras.push(Some(Chase {
                steam_id: thrower.into(),
                start_tick: start,
                end_tick: chase_end,
                grenade: c.detonation.and_then(|det| {
                    let class_name = match c.throw.weapon.trim_start_matches("weapon_") {
                        "flashbang" => "flashbang_projectile",
                        "hegrenade" => "hegrenade_projectile",
                        // Fire entity IDs identify the fire area, not the thrown bottle.
                        _ => return None,
                    };
                    Some(Grenade {
                        entity_index: det.entityid?,
                        throw_tick: c.throw.tick + 2,
                        class_name: class_name.into(),
                    })
                }),
            }));
            if c.utility_kill {
                if let Some(damage) = events.iter().rev().find(|e| {
                    e.event_name == "player_hurt"
                        && e.user_steamid.as_deref() == Some(victim)
                        && e.attacker_steamid.as_deref() != Some(thrower)
                        && e.attacker_team_num == kill.attacker_team_num
                        && enemies(e)
                        && gun(&e.weapon)
                        && e.dmg_health.unwrap_or(0) > 0
                        && e.tick >= (kill.tick - 3 * rate).max(c.round_start)
                        && e.tick < kill.tick
                }) {
                    if let Some(shooter) = damage.attacker_steamid.as_deref().filter(|id| {
                        demo.players
                            .iter()
                            .any(|p| p.steamid == *id && p.user_id.is_some())
                    }) {
                        let end = (damage.tick + rate / 3).min(kill.tick - 1);
                        shots.push(shot(
                            &h,
                            shooter,
                            (damage.tick - rate / 2).max(c.round_start),
                            end,
                            demo,
                        ));
                        cameras.push(None);
                    }
                }
            }
            shots.push(shot(&h, victim, enemy_start, enemy_end, demo));
            cameras.push(None);
            if c.utility_kill {
                // Explosion is the payoff. No invented gun kill after it.
                cursor = enemy_end.min(h.end_tick);
            } else {
                let main_start = (c.effect.tick - rate / 2)
                    .max(kill.tick - 3 * rate)
                    .max(c.round_start);
                let main_end = (kill.tick + rate / 2).min(h.end_tick);
                shots.push(shot(&h, &h.player.steamid, main_start, main_end, demo));
                cameras.push(None);
                cursor = main_end;
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
    let mut joined_cameras: Vec<Option<Chase>> = Vec::new();
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
    fn fast_death_rewinds_without_showing_victim_death_or_losing_kill() {
        let (demo, h, e) = fixture(false, false, 12);
        let (shots, cameras) = plan(vec![h], &demo, &e);
        assert_eq!(cameras.iter().flatten().count(), 1);
        let enemy = shots.iter().find(|s| s.player.steamid == "enemy").unwrap();
        assert!(enemy.end_tick < 1012 && enemy.end_tick > 1000);
        assert!(enemy.end_tick - enemy.start_tick >= 64);
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
                assert!(cameras.iter().flatten().next().unwrap().grenade.is_none());
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
    cameras: &[Option<Chase>],
    directory: &Path,
    rate: f64,
) -> Result<()> {
    for (local, &global) in indices.iter().enumerate() {
        let Some(camera) = &cameras[global] else {
            continue;
        };
        std::fs::create_dir_all(directory)?;
        let file = directory.join(format!("utility-{local}.mjs"));
        std::fs::write(
            &file,
            format!(
                "{}\nstartAimFollow({});\n",
                include_str!("utility-camera.mjs"),
                serde_json::to_string(camera)?
            ),
        )?;
        let setup = seek::SeekWindow::new(
            camera.start_tick,
            camera.end_tick,
            None,
            rate.round() as i32,
        )
        .setup_tick;
        // Loading after the standard POV setup preserves the shared seek/audio gate.
        schedule.push(actions::Scheduled {
            tick: f64::from(setup + 1),
            cmd: "spec_mode 3".into(),
        });
        schedule.push(actions::Scheduled {
            tick: f64::from(setup + 1) + 0.001,
            cmd: format!("mirv_script_load \"{}\"", to_forward_slashes(&file)),
        });
    }
    schedule.sort_by(|a, b| a.tick.total_cmp(&b.tick));
    Ok(())
}
