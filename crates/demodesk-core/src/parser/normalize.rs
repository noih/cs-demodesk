#[cfg(test)]
use super::rows::col_len;
#[cfg(test)]
use super::{Column, Rows};
use super::{DemoParser, GameEvent, Row, Variant};
use crate::model::*;
use anyhow::{Context, Result};
use std::{
    collections::{BTreeMap, HashMap},
    path::Path,
};
const PLAYER_EXTRA: &[&str] = &[
    "health",
    "team_num",
    "X",
    "Y",
    "Z",
    "pitch",
    "yaw",
    "is_scoped",
    "team_name",
    "active_weapon_name",
    "fl_recoil_idx",
];
const OTHER_EXTRA: &[&str] = &["total_rounds_played", "is_freeze_period"];
const EVENTS: &[&str] = &[
    "player_death",
    "player_hurt",
    "weapon_fire",
    "fire_bullets",
    "player_blind",
    "round_start",
    "round_freeze_end",
    "round_end",
    "round_officially_ended",
    "bomb_planted",
    "bomb_defused",
    "bomb_exploded",
];

fn strings(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}
impl DemoParser {
    /// Full [`DemoData`] for a demo file.
    pub fn load_demo(&self, path: &Path) -> Result<DemoData> {
        let bytes = std::fs::read(path)
            .with_context(|| format!("reading {}", path.display()))
            .map_err(|e| crate::error::ErrorSource::App.wrap(e))?;
        self.load_demo_bytes(path, &bytes)
    }

    pub fn load_demo_bytes(&self, path: &Path, bytes: &[u8]) -> Result<DemoData> {
        let header = self.header(bytes)?;
        let out = self.events(
            bytes,
            &strings(EVENTS),
            &strings(PLAYER_EXTRA),
            &strings(OTHER_EXTRA),
        )?;

        let tick_rate = crate::analysis::server_tick_rate(&out.server_infos)?;
        let mut players: Vec<PlayerInfo> = out
            .players
            .iter()
            .filter_map(|p| {
                Some(PlayerInfo {
                    name: p.name.clone()?,
                    steamid: p.steamid?.to_string(),
                    team_number: p.team_number.unwrap_or(0),
                    user_id: None,
                })
            })
            .collect();

        let mut groups: HashMap<&str, Vec<&GameEvent>> = HashMap::new();
        for ev in &out.game_events {
            groups.entry(ev.name.as_str()).or_default().push(ev);
        }
        let kills = kills_from_events(
            groups
                .get("player_death")
                .map(|v| v.as_slice())
                .unwrap_or(&[]),
        );
        let demo_end_tick = crate::demo_readiness::end_tick_bytes(bytes)?;
        let (mut rounds, defusers) = rounds_from_events(&groups, demo_end_tick);

        let mut cash = BTreeMap::new();
        // Roster + user ids at every freeze end.
        let wanted: Vec<i32> = rounds.iter().map(|r| r.freeze_end_tick + 1).collect();
        if !wanted.is_empty() {
            let rows = self.ticks(bytes, &strings(&["team_num", "user_id", "balance"]), wanted)?;
            let mut by_tick: HashMap<i32, Vec<Row>> = HashMap::new();
            let mut user_ids: HashMap<String, i32> = HashMap::new();
            for row in rows.iter() {
                let Some(tick) = row.tick() else { continue };
                by_tick.entry(tick).or_default().push(row);
                if let (Some(sid), Some(uid)) = (row.steamid(), row.num("user_id")) {
                    user_ids.insert(sid, uid as i32);
                }
            }
            for r in &mut rounds {
                let mut roster = BTreeMap::new();
                for row in by_tick
                    .get(&(r.freeze_end_tick + 1))
                    .map(|v| v.as_slice())
                    .unwrap_or(&[])
                {
                    let team = match row.num("team_num").map(|n| n as i32) {
                        Some(2) => Team::T,
                        Some(3) => Team::Ct,
                        _ => continue,
                    };
                    if let Some(sid) = row.steamid() {
                        if let Some(value) = row.num("balance") {
                            cash.insert((r.round, sid.clone()), value.max(0.0) as u32);
                        }
                        roster.insert(sid, team);
                    }
                }
                r.roster = roster;
            }
            for p in &mut players {
                p.user_id = user_ids.get(&p.steamid).copied();
            }
        }
        for r in &mut rounds {
            r.bomb_defuser = r.bomb_defused_tick.and_then(|t| defusers.get(&t).cloned());
        }

        let (damage, damage_ticks) = damage_from_events(
            groups
                .get("player_hurt")
                .map(|v| v.as_slice())
                .unwrap_or(&[]),
            &rounds,
        );

        let mut recoil = crate::aim::recoil(&out.game_events, &rounds, tick_rate);
        crate::aim::tracking::attach(self, bytes, &mut recoil, &out.game_events, tick_rate)?;
        let aim = crate::aim::compute(&out.game_events, &rounds, tick_rate);
        let activity = activity_from_events(&out.game_events, &rounds);
        let round_metrics = rounds
            .iter()
            .map(|r| {
                let window = std::slice::from_ref(r);
                let (damage, _) = damage_from_events(
                    groups
                        .get("player_hurt")
                        .map(|v| v.as_slice())
                        .unwrap_or(&[]),
                    window,
                );
                let activity = activity_from_events(&out.game_events, window);
                let metrics = players
                    .iter()
                    .map(|p| {
                        (
                            p.steamid.clone(),
                            RoundMetrics {
                                cash: cash.get(&(r.round, p.steamid.clone())).copied(),
                                damage: damage.get(&p.steamid).map_or(0, |d| d.total),
                                flashed: activity.get(&p.steamid).map_or(0, |a| a.enemies_flashed),
                                ..Default::default()
                            },
                        )
                    })
                    .collect();
                (r.round, metrics)
            })
            .collect();
        let combat_ticks = combat_ticks_from_events(&out.game_events, &kills, &damage_ticks);
        Ok(DemoData {
            combat_ticks,
            round_metrics,
            info: DemoInfo {
                path: path.to_string_lossy().to_string(),
                map_name: header.get("map_name").cloned().unwrap_or_default(),
                server_name: header.get("server_name").cloned().unwrap_or_default(),
                tick_rate,
                players,
            },
            kills,
            rounds,
            damage,
            damage_ticks,
            activity,
            aim,
            recoil,
            recoil_reference: Default::default(), // Legacy cache field; the chart uses a bundled calibration.
        })
    }
}

/// Game-event field accessors tolerant to the parser's variant choices; the
/// non-`opt_` variants substitute a zero / empty default.
pub(crate) struct Fields<'a>(pub &'a GameEvent);

impl<'a> Fields<'a> {
    fn get(&self, name: &str) -> Option<&'a Variant> {
        self.0
            .fields
            .iter()
            .find(|f| f.name == name)
            .and_then(|f| f.data.as_ref())
    }
    pub fn opt_str(&self, name: &str) -> Option<String> {
        match self.get(name)? {
            Variant::String(s) => Some(s.clone()),
            Variant::U64(v) => Some(v.to_string()),
            Variant::I32(v) => Some(v.to_string()),
            Variant::U32(v) => Some(v.to_string()),
            _ => None,
        }
    }
    pub fn str(&self, name: &str) -> String {
        self.opt_str(name).unwrap_or_default()
    }
    pub fn opt_num(&self, name: &str) -> Option<f64> {
        match self.get(name)? {
            Variant::F32(v) => Some(*v as f64),
            Variant::I32(v) => Some(*v as f64),
            Variant::U32(v) => Some(*v as f64),
            Variant::U64(v) => Some(*v as f64),
            Variant::Bool(b) => Some(*b as i32 as f64),
            _ => None,
        }
    }
    pub fn num(&self, name: &str) -> f64 {
        self.opt_num(name).unwrap_or(0.0)
    }
    pub fn int(&self, name: &str) -> i32 {
        self.num(name) as i32
    }
    pub fn opt_bool(&self, name: &str) -> Option<bool> {
        match self.get(name)? {
            Variant::Bool(b) => Some(*b),
            Variant::I32(v) => Some(*v != 0),
            Variant::U32(v) => Some(*v != 0),
            Variant::String(s) => Some(s == "true" || s == "1"),
            _ => None,
        }
    }
    pub fn bool(&self, name: &str) -> bool {
        self.opt_bool(name).unwrap_or(false)
    }
    /// Event tick; falls back to the message tick when the event carries none.
    pub fn tick(&self) -> i32 {
        match self.int("tick") {
            0 => self.0.tick,
            t => t,
        }
    }
}

fn combat_ticks_from_events(
    events: &[GameEvent],
    kills: &[KillEvent],
    damage_ticks: &BTreeMap<SteamId, Vec<i32>>,
) -> BTreeMap<SteamId, Vec<i32>> {
    let mut combat_ticks = damage_ticks.clone();
    for event in events {
        let f = Fields(event);
        if event.name == "weapon_fire"
            && crate::aim::group(f.str("weapon").trim_start_matches("weapon_")).is_some()
        {
            combat_ticks
                .entry(f.str("user_steamid"))
                .or_default()
                .push(f.tick());
        }
    }
    for kill in kills {
        for id in kill
            .attacker
            .iter()
            .map(|a| &a.steamid)
            .chain(std::iter::once(&kill.victim.steamid))
        {
            combat_ticks.entry(id.clone()).or_default().push(kill.tick);
        }
    }
    combat_ticks.retain(|id, ticks| {
        ticks.sort_unstable();
        ticks.dedup();
        !id.is_empty() && id != "0"
    });
    combat_ticks
}

fn kills_from_events(events: &[&GameEvent]) -> Vec<KillEvent> {
    let mut kills: Vec<KillEvent> = events
        .iter()
        .map(|ev| {
            let f = Fields(ev);
            let attacker_id = f.str("attacker_steamid");
            let assister_id = f.str("assister_steamid");
            KillEvent {
                tick: f.tick(),
                round: f.int("total_rounds_played"),
                attacker: (!attacker_id.is_empty()).then(|| Attacker {
                    steamid: attacker_id.clone(),
                    name: f.str("attacker_name"),
                    team: Team::from_name(&f.str("attacker_team_name")),
                    health: f.int("attacker_health"),
                    weapon_name: f.str("attacker_active_weapon_name"),
                }),
                victim: Victim {
                    steamid: f.str("user_steamid"),
                    name: f.str("user_name"),
                    team: Team::from_name(&f.str("user_team_name")),
                    weapon_name: f.str("user_active_weapon_name"),
                },
                assister: (!assister_id.is_empty()).then(|| Assister {
                    steamid: assister_id.clone(),
                    name: f.str("assister_name"),
                    team: Team::from_name(&f.str("assister_team_name")),
                }),
                weapon: f.str("weapon"),
                headshot: f.bool("headshot"),
                noscope: f.bool("noscope"),
                penetrated: f.int("penetrated"),
                thru_smoke: f.bool("thrusmoke"),
                attacker_blind: f.bool("attackerblind"),
                attacker_in_air: f.bool("attackerinair"),
                assisted_flash: f.bool("assistedflash"),
                distance: f.num("distance"),
                hitgroup: f.str("hitgroup"),
                is_freeze_period: f.bool("is_freeze_period"),
            }
        })
        .collect();
    kills.sort_by_key(|k| k.tick);
    kills
}

const UTILITY_WEAPONS: &[&str] = &[
    "hegrenade",
    "inferno",
    "molotov",
    "incgrenade",
    "decoy",
    "flashbang",
    "smokegrenade",
];

/// Count actual enemy HP lost within the same round windows used for kills.
fn damage_from_events(
    events: &[&GameEvent],
    rounds: &[RoundInfo],
) -> (BTreeMap<String, DamageTotals>, BTreeMap<SteamId, Vec<i32>>) {
    let mut out: BTreeMap<String, DamageTotals> = BTreeMap::new();
    let mut ticks: BTreeMap<SteamId, Vec<i32>> = BTreeMap::new();
    let mut health: HashMap<(usize, String), u32> = HashMap::new();
    let mut events = events.to_vec();
    events.sort_by_key(|ev| Fields(ev).tick());
    for ev in events {
        let f = Fields(ev);
        let Some((index, round)) = rounds
            .iter()
            .enumerate()
            .find(|(_, r)| f.tick() >= r.start_tick && f.tick() <= r.officially_ended_tick)
        else {
            continue;
        };
        let victim = f.str("user_steamid");
        if victim.is_empty() {
            continue;
        }
        let raw = f.int("dmg_health").max(0) as u32;
        let remaining = health
            .entry((index, victim.clone()))
            .or_insert_with(|| f.opt_num("user_health").unwrap_or(100.0).max(0.0) as u32);
        let after = f
            .opt_num("health")
            .map(|n| n.max(0.0) as u32)
            .unwrap_or_else(|| remaining.saturating_sub(raw));
        // dmg_health includes overkill. Entity health can lag multiple hits in one tick,
        // so carry forward the previous hurt event's remaining health instead.
        let damage = raw.min(remaining.saturating_sub(after));
        *remaining = after;

        // Even excluded damage reduces the HP available to subsequent enemy hits.
        let attacker = f.str("attacker_steamid");
        if attacker.is_empty() || attacker == victim || f.bool("is_freeze_period") || damage == 0 {
            continue;
        }
        let team = |field, player: &str| match f.int(field) {
            2 => Some(Team::T),
            3 => Some(Team::Ct),
            _ => round.roster.get(player).copied(),
        };
        let (Some(at), Some(vt)) = (
            team("attacker_team_num", &attacker),
            team("user_team_num", &victim),
        ) else {
            continue;
        };
        let entry = out.entry(attacker.clone()).or_default();
        if at == vt {
            entry.friendly += damage;
            continue;
        }
        ticks.entry(attacker).or_default().push(f.tick());
        ticks.entry(victim).or_default().push(f.tick());
        entry.total += damage;
        match f.str("weapon").as_str() {
            "hegrenade" => entry.he += damage,
            "inferno" | "molotov" | "incgrenade" => entry.fire += damage,
            _ => {}
        }
        if UTILITY_WEAPONS.contains(&f.str("weapon").as_str()) {
            entry.utility += damage;
        }
    }
    for player_ticks in ticks.values_mut() {
        player_ticks.dedup();
    }
    (out, ticks)
}

fn ticks_of(groups: &HashMap<&str, Vec<&GameEvent>>, name: &str) -> Vec<i32> {
    let mut v: Vec<i32> = groups
        .get(name)
        .map(|g| g.iter().map(|e| Fields(e).tick()).collect())
        .unwrap_or_default();
    v.sort_unstable();
    v
}

fn rounds_from_events(
    groups: &HashMap<&str, Vec<&GameEvent>>,
    demo_end_tick: Option<i32>,
) -> (Vec<RoundInfo>, HashMap<i32, String>) {
    let starts = ticks_of(groups, "round_start");
    let freeze_ends = ticks_of(groups, "round_freeze_end");
    let officially = ticks_of(groups, "round_officially_ended");
    let planted = ticks_of(groups, "bomb_planted");
    let exploded = ticks_of(groups, "bomb_exploded");
    let mut defusers: HashMap<i32, String> = HashMap::new();
    let mut defused: Vec<i32> = vec![];
    for ev in groups
        .get("bomb_defused")
        .map(|v| v.as_slice())
        .unwrap_or(&[])
    {
        let f = Fields(ev);
        defused.push(f.tick());
        defusers.insert(f.tick(), f.str("user_steamid"));
    }
    defused.sort_unstable();

    let mut ends: Vec<(i32, i32, String, String)> = groups
        .get("round_end")
        .map(|v| {
            v.iter()
                .map(|e| {
                    (
                        Fields(e).tick(),
                        Fields(e).int("round"),
                        Fields(e).str("winner"),
                        Fields(e).str("reason"),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    ends.sort_by_key(|e| e.0);

    let last_before =
        |arr: &[i32], tick: i32| arr.iter().copied().take_while(|t| *t <= tick).last();
    let first_after = |arr: &[i32], tick: i32| arr.iter().copied().find(|t| *t >= tick);
    let between =
        |arr: &[i32], from: i32, to: i32| arr.iter().copied().find(|t| *t >= from && *t <= to);

    let rounds = ends
        .into_iter()
        .map(|(end_tick, round, winner, reason)| {
            let start_tick = last_before(&starts, end_tick).unwrap_or(0);
            let freeze_end_tick = between(&freeze_ends, start_tick, end_tick).unwrap_or(start_tick);
            // The final round can omit round_officially_ended while the demo keeps playing.
            let boundary = starts
                .iter()
                .copied()
                .find(|&t| t > end_tick)
                .into_iter()
                .chain(demo_end_tick)
                .min();
            let officially_ended_tick = first_after(&officially, end_tick)
                .or(boundary)
                .unwrap_or(end_tick)
                .min(boundary.unwrap_or(i32::MAX))
                .max(end_tick);
            RoundInfo {
                round,
                start_tick,
                freeze_end_tick,
                end_tick,
                officially_ended_tick,
                winner: match winner.as_str() {
                    "CT" => Some(Team::Ct),
                    "T" | "TERRORIST" => Some(Team::T),
                    _ => None,
                },
                reason,
                roster: BTreeMap::new(),
                bomb_planted_tick: between(&planted, start_tick, end_tick),
                bomb_defused_tick: between(&defused, start_tick, end_tick),
                bomb_defuser: None,
                bomb_exploded_tick: between(&exploded, start_tick, end_tick),
            }
        })
        .collect();
    (rounds, defusers)
}

#[cfg(test)]
mod round_tests {
    use super::*;
    use crate::parser::EventField;

    #[test]
    fn combat_events_combine_shots_damage_and_both_sides_of_deaths() {
        let shot = |id: &str, weapon: &str, tick| GameEvent {
            name: "weapon_fire".into(),
            tick,
            fields: vec![
                EventField {
                    name: "user_steamid".into(),
                    data: Some(Variant::String(id.into())),
                },
                EventField {
                    name: "weapon".into(),
                    data: Some(Variant::String(weapon.into())),
                },
            ],
        };
        let kill: KillEvent = serde_json::from_value(serde_json::json!({
            "tick":30,"round":1,"attacker":{"steamid":"a","name":"a","team":"TERRORIST","health":100,"weaponName":"ak47"},
            "victim":{"steamid":"b","name":"b","team":"CT","weaponName":"m4a1"},
            "weapon":"ak47","headshot":false,"noscope":false,"penetrated":0,"thruSmoke":false,
            "attackerBlind":false,"attackerInAir":false,"assistedFlash":false,"distance":10,"hitgroup":"chest","isFreezePeriod":false
        })).unwrap();
        let damage = BTreeMap::from([("a".into(), vec![20]), ("b".into(), vec![20])]);
        let ticks = combat_ticks_from_events(
            &[
                shot("a", "weapon_ak47", 10),
                shot("a", "ak47", 10),
                shot("a", "smokegrenade", 15),
                shot("0", "ak47", 25),
            ],
            &[kill],
            &damage,
        );
        assert_eq!(
            ticks,
            BTreeMap::from([("a".into(), vec![10, 20, 30]), ("b".into(), vec![20, 30])])
        );
    }

    fn event(name: &str, tick: i32) -> GameEvent {
        GameEvent {
            name: name.into(),
            tick,
            fields: vec![EventField {
                name: "round".into(),
                data: Some(Variant::I32(1)),
            }],
        }
    }

    #[test]
    fn final_kill_keeps_tail_without_official_round_end() {
        let events = [event("round_start", 100), event("round_end", 1000)];
        let mut groups: HashMap<&str, Vec<&GameEvent>> = HashMap::new();
        for ev in &events {
            groups.entry(&ev.name).or_default().push(ev);
        }
        let (rounds, _) = rounds_from_events(&groups, Some(2280));
        let demo: DemoData = serde_json::from_value(serde_json::json!({
            "info": {"path": "test.dem", "mapName": "de_test", "serverName": "",
                "tickRate": 64.0, "players": []},
            "rounds": rounds,
            "kills": [{"tick": 1000, "round": 0,
                "attacker": {"steamid": "ct1", "name": "Player", "team": "CT",
                    "health": 100, "weaponName": "AK-47"},
                "victim": {"steamid": "t1", "name": "Opponent", "team": "TERRORIST", "weaponName": "AK-47"},
                "weapon": "ak47", "headshot": true, "noscope": false, "penetrated": 0,
                "thruSmoke": false, "attackerBlind": false, "attackerInAir": false,
                "assistedFlash": false, "distance": 10.0, "hitgroup": "head", "isFreezePeriod": false}]
        })).unwrap();
        let highlights = crate::detector::detect(
            &demo,
            &DetectOptions {
                min_score: 0.0,
                ..Default::default()
            },
        );
        assert_eq!(highlights.len(), 1);
        assert_eq!(highlights[0].end_tick, 1192);
        assert_eq!(highlights[0].key_moments, [[680, 1192]]);
    }

    #[test]
    fn round_tail_respects_official_end_next_round_and_demo_end() {
        for (official, next, demo_end, expected) in [
            (None, None, Some(1064), 1064),
            (None, None, Some(1000), 1000),
            (None, None, None, 1000),
            (Some(1200), None, Some(2280), 1200),
            (Some(1200), None, None, 1200),
            (None, Some(1100), Some(2280), 1100),
            (Some(2000), Some(1100), Some(2280), 1100),
            (Some(1200), None, Some(1064), 1064),
        ] {
            let mut events = vec![event("round_start", 100), event("round_end", 1000)];
            if let Some(tick) = official {
                events.push(event("round_officially_ended", tick));
            }
            if let Some(tick) = next {
                events.push(event("round_start", tick));
            }
            let mut groups: HashMap<&str, Vec<&GameEvent>> = HashMap::new();
            for ev in &events {
                groups.entry(&ev.name).or_default().push(ev);
            }
            let (rounds, _) = rounds_from_events(&groups, demo_end);
            assert_eq!(
                rounds[0].officially_ended_tick, expected,
                "official={official:?}, next={next:?}, demo_end={demo_end:?}"
            );
        }
    }
}

#[cfg(test)]
mod damage_tests {
    use super::*;
    use crate::parser::EventField;

    fn round(start: i32) -> RoundInfo {
        RoundInfo {
            round: start,
            start_tick: start,
            freeze_end_tick: start,
            end_tick: start + 80,
            officially_ended_tick: start + 90,
            winner: Some(Team::T),
            reason: String::new(),
            roster: BTreeMap::from([
                ("enemy".into(), Team::T),
                ("friend".into(), Team::Ct),
                ("victim".into(), Team::Ct),
            ]),
            bomb_planted_tick: None,
            bomb_defused_tick: None,
            bomb_defuser: None,
            bomb_exploded_tick: None,
        }
    }

    fn hurt(tick: i32, attacker: &str, raw: i32, after: i32, weapon: &str) -> GameEvent {
        let fields = vec![
            ("attacker_steamid", Variant::String(attacker.into())),
            ("user_steamid", Variant::String("victim".into())),
            (
                "attacker_team_num",
                Variant::I32(if attacker == "enemy" { 2 } else { 3 }),
            ),
            ("user_team_num", Variant::I32(3)),
            ("user_health", Variant::I32(100)), // Entity sample may lag the same tick's hits.
            ("health", Variant::I32(after)),
            ("dmg_health", Variant::I32(raw)),
            ("weapon", Variant::String(weapon.into())),
        ];
        GameEvent {
            name: "player_hurt".into(),
            tick,
            fields: fields
                .into_iter()
                .map(|(name, value)| EventField {
                    name: name.into(),
                    data: Some(value),
                })
                .collect(),
        }
    }

    #[test]
    fn key_damage_ticks_include_every_weapon_and_both_players_without_a_kill() {
        let events: Vec<_> = [
            "ak47",
            "hegrenade",
            "inferno",
            "taser",
            "knife",
            "new_weapon",
        ]
        .iter()
        .enumerate()
        .map(|(i, weapon)| hurt(10 + i as i32, "enemy", 1, 99 - i as i32, weapon))
        .collect();
        let (damage, ticks) = damage_from_events(&events.iter().collect::<Vec<_>>(), &[round(1)]);
        assert_eq!(damage["enemy"].total, 6);
        assert_eq!(ticks["enemy"], vec![10, 11, 12, 13, 14, 15]);
        assert_eq!(ticks["victim"], ticks["enemy"]);
        let excluded = [
            hurt(20, "friend", 10, 90, "taser"),
            hurt(21, "enemy", 0, 90, "inferno"),
        ];
        let (_, ticks) = damage_from_events(&excluded.iter().collect::<Vec<_>>(), &[round(1)]);
        assert!(ticks.is_empty());
    }

    #[test]
    fn lethal_overkill_and_same_tick_hits_count_only_remaining_hp() {
        let events = [
            hurt(10, "enemy", 21, 79, "hkp2000"),
            hurt(10, "enemy", 134, 0, "hkp2000"),
            hurt(10, "enemy", 134, 0, "hkp2000"),
        ];
        let (damage, _) = damage_from_events(&events.iter().collect::<Vec<_>>(), &[round(1)]);
        assert_eq!(damage["enemy"].total, 100);
        assert_eq!(damage["enemy"].utility, 0);
    }

    #[test]
    fn excluded_damage_still_reduces_hp_available_to_enemies() {
        let events = [
            hurt(10, "friend", 20, 80, "ak47"),
            hurt(11, "", 10, 70, "world"),
            hurt(12, "victim", 5, 65, "inferno"),
            hurt(13, "enemy", 200, 0, "inferno"),
        ];
        let (damage, _) = damage_from_events(&events.iter().collect::<Vec<_>>(), &[round(1)]);
        assert_eq!(damage.len(), 2);
        assert_eq!(damage["friend"].friendly, 20);
        assert_eq!(damage["friend"].total, 0);
        assert_eq!(damage["friend"].utility, 0);
        assert_eq!(damage["enemy"].friendly, 0);
        assert_eq!(damage["enemy"].total, 65);
        assert_eq!(damage["enemy"].utility, 65);
    }

    #[test]
    fn health_resets_each_round_and_roster_is_a_team_fallback() {
        let mut events = [
            hurt(0, "enemy", 100, 0, "ak47"),
            hurt(10, "enemy", 200, 0, "ak47"),
            hurt(110, "enemy", 200, 0, "ak47"),
        ];
        for event in &mut events {
            event.fields.retain(|f| !f.name.ends_with("team_num"));
        }
        let (damage, _) =
            damage_from_events(&events.iter().collect::<Vec<_>>(), &[round(1), round(101)]);
        assert_eq!(damage["enemy"].total, 200);
    }
}

fn activity_from_events(
    events: &[GameEvent],
    rounds: &[RoundInfo],
) -> BTreeMap<String, ActivityStats> {
    let mut out: BTreeMap<String, ActivityStats> = BTreeMap::new();
    for event in events {
        if !matches!(event.name.as_str(), "weapon_fire" | "player_blind") {
            continue;
        }
        let f = Fields(event);
        if f.bool("is_freeze_period")
            || !rounds
                .iter()
                .any(|r| f.tick() >= r.start_tick && f.tick() <= r.officially_ended_tick)
        {
            continue;
        }
        if event.name == "player_blind" {
            let id = f.str("attacker_steamid");
            let duration = f.num("blind_duration");
            // The demo can emit blinds for dead targets; these provide no combat effect.
            if id.is_empty()
                || duration <= 1.0
                || f.opt_num("user_health").is_some_and(|hp| hp <= 0.0)
            {
                continue;
            }
            let (at, vt) = (f.int("attacker_team_num"), f.int("user_team_num"));
            if ![2, 3].contains(&at) || ![2, 3].contains(&vt) {
                continue;
            }
            let s = out.entry(id).or_default();
            if at == vt {
                s.teammates_flashed += 1;
            } else {
                s.enemies_flashed += 1;
                s.enemy_blind_seconds += duration;
            }
            continue;
        }
        let id = f.str("user_steamid");
        if id.is_empty() {
            continue;
        }
        let weapon = f.str("weapon");
        let weapon = weapon.strip_prefix("weapon_").unwrap_or(&weapon);
        let s = out.entry(id).or_default();
        if UTILITY_WEAPONS.contains(&weapon) {
            match weapon {
                "flashbang" => s.flashes += 1,
                "smokegrenade" => s.smokes += 1,
                "hegrenade" => s.hes += 1,
                "molotov" | "incgrenade" => s.fires += 1,
                _ => {}
            }
        } else if !weapon.contains("knife")
            && !weapon.contains("bayonet")
            && weapon != "c4"
            && !UTILITY_WEAPONS.contains(&weapon)
        {
            s.shots += 1;
        }
    }
    out
}

#[cfg(test)]
mod activity_tests {
    use super::*;
    use crate::parser::EventField;
    #[test]
    fn counts_throws_and_qualifying_blinds_without_counting_grenades_as_shots() {
        let round: RoundInfo = serde_json::from_value(serde_json::json!({"round":1,"startTick":10,"freezeEndTick":10,"endTick":100,"officiallyEndedTick":110,"reason":"","roster":{}})).unwrap();
        let event = |name: &str, weapon: &str, duration: f32, team: i32, tick: i32| GameEvent {
            name: name.into(),
            tick,
            fields: vec![
                EventField {
                    name: "weapon".into(),
                    data: Some(Variant::String(weapon.into())),
                },
                EventField {
                    name: "user_steamid".into(),
                    data: Some(Variant::String("a".into())),
                },
                EventField {
                    name: "attacker_steamid".into(),
                    data: Some(Variant::String("a".into())),
                },
                EventField {
                    name: "attacker_team_num".into(),
                    data: Some(Variant::I32(3)),
                },
                EventField {
                    name: "user_team_num".into(),
                    data: Some(Variant::I32(team)),
                },
                EventField {
                    name: "blind_duration".into(),
                    data: Some(Variant::F32(duration)),
                },
            ],
        };
        let mut dead = event("player_blind", "", 4.2, 2, 70);
        dead.fields.push(EventField {
            name: "user_health".into(),
            data: Some(Variant::I32(0)),
        });
        let events = vec![
            event("weapon_fire", "weapon_flashbang", 0.0, 3, 20),
            event("weapon_fire", "weapon_ak47", 0.0, 3, 30),
            event("weapon_fire", "weapon_knife", 0.0, 3, 40),
            event("weapon_fire", "weapon_hegrenade", 0.0, 3, 50),
            event("player_blind", "", 3.0, 2, 60),
            event("player_blind", "", 0.5, 2, 60),
            event("player_blind", "", 2.0, 3, 60),
            event("weapon_fire", "weapon_ak47", 0.0, 3, 111),
            event("player_blind", "", 1.05, 2, 80),
            event("player_blind", "", 1.0, 2, 80),
            dead,
        ];
        let stats = activity_from_events(&events, &[round]);
        let a = &stats["a"];
        assert_eq!(
            (
                a.shots,
                a.flashes,
                a.hes,
                a.enemies_flashed,
                a.teammates_flashed
            ),
            (1, 1, 1, 2, 1)
        );
        assert!((a.enemy_blind_seconds - 4.05).abs() < 0.0001);
    }
}

#[cfg(test)]
mod vector_rows_tests {
    use super::*;
    #[test]
    fn position_vectors_preserve_fractional_values_and_missing_samples() {
        let column = Column::XYZVec(vec![Some([1.25, -2.5, 3.75]), None]);
        assert_eq!(col_len(&column), 2);
        let rows = Rows::from_columns(vec![("position".into(), column)], 2);
        let values: Vec<_> = rows.iter().map(|r| r.vec3("position")).collect();
        assert_eq!(values, vec![Some([1.25, -2.5, 3.75]), None]);
        assert_eq!(rows.iter().next().unwrap().vec3("absent"), None);
    }
}

pub(super) fn annotate_smoke_policy(
    events: &mut [GameEvent],
    policies: Vec<crate::analysis::smoke::spread::FirePolicy>,
) {
    use crate::parser::EventField;
    let policies: BTreeMap<_, _> = policies
        .into_iter()
        .map(|p| {
            (
                (p.tick, p.player, p.seed, p.item_def_index),
                (p.policy, p.rewind),
            )
        })
        .collect();
    for event in events.iter_mut().filter(|e| e.name == "fire_bullets") {
        let unsigned = |name: &str| {
            event.fields.iter().find_map(|f| {
                if f.name != name {
                    return None;
                }
                match f.data {
                    Some(Variant::U32(v)) => Some(v),
                    _ => None,
                }
            })
        };
        let (Some(player), Some(seed), Some(item)) = (
            unsigned("player"),
            unsigned("seed"),
            unsigned("item_def_index"),
        ) else {
            continue;
        };
        let Some((policy, rewind)) = policies.get(&(event.tick, player, seed, item)) else {
            continue;
        };
        if let Some(rewind) = rewind {
            event.fields.push(EventField {
                name: "rewind_max_unlag".into(),
                data: Some(Variant::F32(rewind.max_unlag)),
            });
            for (name, value) in [
                ("rewind_use_full_interp", rewind.use_full_interp),
                ("rewind_force_full_interp", rewind.force_full_interp),
                ("rewind_force_target_time", rewind.force_target_time),
                (
                    "rewind_filter_player_simulation_time",
                    rewind.filter_player_simulation_time,
                ),
                ("rewind_command_queue_enabled", rewind.command_queue_enabled),
            ] {
                event.fields.push(EventField {
                    name: name.into(),
                    data: Some(Variant::Bool(value)),
                });
            }
        }
        for (name, value) in [
            ("smoke_patterns_enabled", policy.patterns_enabled),
            ("smoke_only_up", policy.only_up),
            ("smoke_maximum_inaccuracy", policy.maximum_inaccuracy),
        ] {
            event.fields.push(EventField {
                name: name.into(),
                data: Some(Variant::Bool(value)),
            });
        }
    }
}
#[cfg(test)]
mod smoke_policy_tests {
    use super::*;
    use crate::analysis::smoke::spread::{FirePolicy, Policy};
    use crate::parser::EventField;
    #[test]
    fn annotation_requires_the_same_wire_shot() {
        let event = |seed| GameEvent {
            name: "fire_bullets".into(),
            tick: 10,
            fields: [("player", 2), ("seed", seed), ("item_def_index", 7)]
                .into_iter()
                .map(|(name, v)| EventField {
                    name: name.into(),
                    data: Some(Variant::U32(v)),
                })
                .collect(),
        };
        let mut events = [event(3), event(4)];
        annotate_smoke_policy(
            &mut events,
            vec![FirePolicy {
                rewind: Some(crate::analysis::ballistics::rewind::ReplicatedPolicy {
                    max_unlag: 0.5,
                    use_full_interp: true,
                    force_full_interp: false,
                    force_target_time: false,
                    filter_player_simulation_time: true,
                    command_queue_enabled: true,
                }),
                tick: 10,
                player: 2,
                seed: 3,
                item_def_index: 7,
                policy: Policy {
                    patterns_enabled: true,
                    only_up: false,
                    maximum_inaccuracy: false,
                },
            }],
        );
        let first = serde_json::to_value(&events[0]).unwrap();
        assert_eq!(first["rewind_max_unlag"], 0.5);
        assert_eq!(first["rewind_use_full_interp"], true);
        assert_eq!(first["rewind_filter_player_simulation_time"], true);
        assert_eq!(first["rewind_command_queue_enabled"], true);
        assert_eq!(first["smoke_patterns_enabled"], true);
        assert_eq!(first["smoke_only_up"], false);
        assert_eq!(first["smoke_maximum_inaccuracy"], false);
        assert!(serde_json::to_value(&events[1])
            .unwrap()
            .get("smoke_patterns_enabled")
            .is_none());
    }
}
