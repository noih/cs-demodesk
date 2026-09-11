//! Thin wrapper over the vendored `parser` crate (LaihoE/demoparser, MIT) that
//! turns a .dem file into the normalized [`DemoData`] contract. One pass reads
//! every game event we need; a second pass samples player team/user_id at the
//! freeze-end tick of every round (events and tick collection cannot share a
//! pass in demoparser).

use crate::model::*;
use ahash::AHashMap;
use anyhow::{anyhow, Context, Result};
use parser::first_pass::parser_settings::FirstPassParser;
use parser::first_pass::parser_settings::{rm_user_friendly_names, ParserInputs};
use parser::parse_demo::{DemoOutput, Parser, ParsingMode};
use parser::second_pass::game_events::GameEvent;
use parser::second_pass::parser_settings::create_huffman_lookup_table;
use parser::second_pass::variants::{VarVec, Variant};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;

const PLAYER_EXTRA: &[&str] = &["health", "team_num", "X", "Y", "Z", "pitch", "yaw", "is_scoped", "team_name", "active_weapon_name", "fl_recoil_idx"];
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

/// Reusable parser context: the huffman table is expensive to build, so build it once per process.
pub struct DemoParser {
    huffman: Vec<(u8, u8)>,
}

impl Default for DemoParser {
    fn default() -> Self {
        Self::new()
    }
}

fn strings(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

impl DemoParser {
    pub fn new() -> Self {
        Self { huffman: create_huffman_lookup_table() }
    }

    fn inputs<'a>(&'a self, player_props: &[String], other_props: &[String], events: &[String], ticks: Vec<i32>) -> Result<ParserInputs<'a>> {
        self.inputs_ex(player_props, other_props, events, ticks, false)
    }

    fn inputs_ex<'a>(&'a self, player_props: &[String], other_props: &[String], events: &[String], ticks: Vec<i32>, projectiles: bool) -> Result<ParserInputs<'a>> {
        let real_player = rm_user_friendly_names(&player_props.to_vec()).map_err(|e| anyhow!("{e:?}"))?;
        let real_other = rm_user_friendly_names(&other_props.to_vec()).map_err(|e| anyhow!("{e:?}"))?;
        let mut real_name_to_og_name = AHashMap::default();
        for (real, friendly) in real_player.iter().zip(player_props) {
            real_name_to_og_name.insert(real.clone(), friendly.clone());
        }
        for (real, friendly) in real_other.iter().zip(other_props) {
            real_name_to_og_name.insert(real.clone(), friendly.clone());
        }
        Ok(ParserInputs {
            real_name_to_og_name,
            wanted_players: vec![],
            wanted_player_props: real_player,
            wanted_other_props: real_other,
            wanted_prop_states: AHashMap::default(),
            wanted_ticks: ticks,
            wanted_events: events.to_vec(),
            parse_ents: true,
            parse_projectiles: projectiles,
            parse_grenades: projectiles,
            only_header: false,
            only_convars: false,
            huffman_lookup_table: &self.huffman,
            order_by_steamid: false,
            list_props: false,
            fallback_bytes: None,
        })
    }

    pub fn header(&self, bytes: &[u8]) -> Result<HashMap<String, String>> {
        let inputs = self.inputs(&[], &[], &[], vec![])?;
        let mut first = FirstPassParser::new(&inputs);
        let header = first.parse_header_only(bytes).map_err(|e| anyhow!("header: {e:?}"))?;
        Ok(header.into_iter().collect())
    }

    pub fn events(&self, bytes: &[u8], names: &[String], player_extra: &[String], other_extra: &[String]) -> Result<DemoOutput> {
        let inputs = self.inputs(player_extra, other_extra, names, vec![])?;
        let mut parser = Parser::new(inputs, ParsingMode::Normal);
        parser.parse_demo(bytes).map_err(|e| anyhow!("events: {e:?}"))
    }

    /// One row per (tick, player) with the requested props plus `tick`, `steamid`, `name`.
    pub fn ticks(&self, bytes: &[u8], props: &[String], wanted_ticks: Vec<i32>) -> Result<Rows> {
        let inputs = self.inputs(props, &[], &[], wanted_ticks)?;
        let mut parser = Parser::new(inputs, ParsingMode::Normal);
        let out = parser.parse_demo(bytes).map_err(|e| anyhow!("ticks: {e:?}"))?;
        Ok(Rows::from_output(out))
    }

    /// One row per (tick, grenade projectile) with `grenade_type`, `grenade_entity_id`,
    /// `x`, `y`, `z`, `tick`, `steamid` (thrower), `name`.
    pub fn projectiles(&self, bytes: &[u8], wanted_ticks: Vec<i32>) -> Result<Rows> {
        let inputs = self.inputs_ex(&["m_firePositions".into(), "m_fireCount".into(), "m_nInfernoType".into(), "m_nVoxelFrameDataSize".into()], &[], &[], wanted_ticks, true)?;
        let mut parser = Parser::new(inputs, ParsingMode::Normal);
        let out = parser.parse_demo(bytes).map_err(|e| anyhow!("projectiles: {e:?}"))?;
        Ok(Rows::from_output(out))
    }

    /// Full [`DemoData`] for a demo file.
    pub fn load_demo(&self, path: &Path) -> Result<DemoData> {
        let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
        self.load_demo_bytes(path, &bytes)
    }

    pub fn load_demo_bytes(&self, path: &Path, bytes: &[u8]) -> Result<DemoData> {
        let header = self.header(bytes)?;
        let out = self.events(bytes, &strings(EVENTS), &strings(PLAYER_EXTRA), &strings(OTHER_EXTRA))?;

        let mut players: Vec<PlayerInfo> = if out.player_md.is_empty() { &out.roster } else { &out.player_md }
            .iter()
            .filter_map(|p| {
                Some(PlayerInfo { name: p.name.clone()?, steamid: p.steamid?.to_string(), team_number: p.team_number.unwrap_or(0), user_id: None })
            })
            .collect();

        let mut groups: HashMap<&str, Vec<&GameEvent>> = HashMap::new();
        for ev in &out.game_events {
            groups.entry(ev.name.as_str()).or_default().push(ev);
        }
        let kills = kills_from_events(groups.get("player_death").map(|v| v.as_slice()).unwrap_or(&[]));
        let (mut rounds, defusers) = rounds_from_events(&groups);

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
                for row in by_tick.get(&(r.freeze_end_tick + 1)).map(|v| v.as_slice()).unwrap_or(&[]) {
                    let team = match row.num("team_num").map(|n| n as i32) {
                        Some(2) => Team::T,
                        Some(3) => Team::Ct,
                        _ => continue,
                    };
                    if let Some(sid) = row.steamid() {
                        if let Some(value) = row.num("balance") { cash.insert((r.round, sid.clone()), value.max(0.0) as u32); }
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

        let damage = damage_from_events(groups.get("player_hurt").map(|v| v.as_slice()).unwrap_or(&[]), &rounds);

        let recoil = crate::aim::recoil(&out.game_events, &rounds, 64.0);
        let aim = crate::aim::compute(&out.game_events, &rounds, 64.0);
        let activity = activity_from_events(&out.game_events, &rounds);
        let round_metrics = rounds.iter().map(|r| {
            let window = std::slice::from_ref(r);
            let damage = damage_from_events(groups.get("player_hurt").map(|v| v.as_slice()).unwrap_or(&[]), window);
            let activity = activity_from_events(&out.game_events, window);
            let metrics = players.iter().map(|p| (p.steamid.clone(), RoundMetrics {
                cash: cash.get(&(r.round, p.steamid.clone())).copied(),
                damage: damage.get(&p.steamid).map_or(0, |d| d.total),
                flashed: activity.get(&p.steamid).map_or(0, |a| a.enemies_flashed),
                ..Default::default()
            })).collect();
            (r.round, metrics)
        }).collect();
        Ok(DemoData {
            round_metrics,
            info: DemoInfo {
                path: path.to_string_lossy().to_string(),
                map_name: header.get("map_name").cloned().unwrap_or_default(),
                server_name: header.get("server_name").cloned().unwrap_or_default(),
                tick_rate: 64.0,
                players,
            },
            kills,
            rounds,
            damage,
            activity,
            aim,
            recoil,
            recoil_reference: Default::default(), // Legacy cache field; the chart uses a bundled calibration.
        })
    }
}

/// Column-major result of a tick / projectile pass, taken straight out of the
/// parser's data frame (no per-row allocation). Rows are visited in tick order.
pub struct Rows {
    cols: Vec<VarVec>,
    index: HashMap<String, usize>,
    /// row indices sorted by tick
    order: Vec<usize>,
}

/// One row of [`Rows`]; accessors tolerate the parser's variant choices.
#[derive(Clone, Copy)]
pub struct Row<'a> {
    rows: &'a Rows,
    i: usize,
}

impl Rows {
    fn from_output(mut out: DemoOutput) -> Self {
        let mut cols = vec![];
        let mut index = HashMap::new();
        let mut len = 0;
        for info in &out.prop_controller.prop_infos {
            let Some(col) = out.df.remove(&info.id) else { continue };
            len = len.max(col.data.as_ref().map(col_len).unwrap_or(col.num_nones));
            if let Some(data) = col.data {
                index.insert(info.prop_friendly_name.clone(), cols.len());
                cols.push(data);
            }
        }
        let mut rows = Self { cols, index, order: (0..len).collect() };
        let ticks: Vec<i32> = rows.iter().map(|r| r.tick().unwrap_or(0)).collect();
        rows.order.sort_by_key(|i| ticks[*i]);
        rows
    }

    pub fn len(&self) -> usize {
        self.order.len()
    }

    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = Row<'_>> {
        (0..self.order.len()).map(move |i| Row { rows: self, i })
    }
}

fn col_len(v: &VarVec) -> usize {
    match v {
        VarVec::Bool(v) => v.len(),
        VarVec::U32(v) => v.len(),
        VarVec::I32(v) => v.len(),
        VarVec::F32(v) => v.len(),
        VarVec::U64(v) => v.len(),
        VarVec::String(v) => v.len(),
        VarVec::StringVec(v) => v.len(),
        _ => 0,
    }
}

impl<'a> Row<'a> {
    fn col(&self, key: &str) -> Option<(&'a VarVec, usize)> {
        let col = &self.rows.cols[*self.rows.index.get(key)?];
        Some((col, self.rows.order[self.i]))
    }

    pub fn num(&self, key: &str) -> Option<f64> {
        let (col, i) = self.col(key)?;
        match col {
            VarVec::F32(v) => v.get(i).copied().flatten().map(f64::from),
            VarVec::I32(v) => v.get(i).copied().flatten().map(f64::from),
            VarVec::U32(v) => v.get(i).copied().flatten().map(f64::from),
            VarVec::U64(v) => v.get(i).copied().flatten().map(|v| v as f64),
            VarVec::Bool(v) => v.get(i).copied().flatten().map(|b| b as i32 as f64),
            _ => None,
        }
    }

    pub fn flag(&self, key: &str) -> bool {
        self.num(key).map(|v| v != 0.0).unwrap_or(false)
    }

    pub fn str(&self, key: &str) -> Option<&'a str> {
        match self.col(key)? {
            (VarVec::String(v), i) => v.get(i)?.as_deref(),
            _ => None,
        }
    }

    pub fn strs(&self, key: &str) -> Option<&'a [String]> {
        match self.col(key)? {
            (VarVec::StringVec(v), i) => v.get(i).map(Vec::as_slice),
            _ => None,
        }
    }

    pub fn tick(&self) -> Option<i32> {
        match self.col("tick")? {
            (VarVec::I32(v), i) => v.get(i).copied().flatten(),
            _ => None,
        }
    }

    /// Steamid as the parser stored it (u64 or string); bots carry `0`.
    pub fn steamid(&self) -> Option<String> {
        match self.col("steamid")? {
            (VarVec::U64(v), i) => v.get(i).copied().flatten().map(|v| v.to_string()),
            (VarVec::String(v), i) => v.get(i)?.clone(),
            _ => None,
        }
    }
}

/// Game-event field accessors tolerant to the parser's variant choices; the
/// non-`opt_` variants substitute a zero / empty default.
pub(crate) struct Fields<'a>(pub &'a GameEvent);

impl<'a> Fields<'a> {
    fn get(&self, name: &str) -> Option<&'a Variant> {
        self.0.fields.iter().find(|f| f.name == name).and_then(|f| f.data.as_ref())
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

const UTILITY_WEAPONS: &[&str] = &["hegrenade", "inferno", "molotov", "incgrenade", "decoy", "flashbang", "smokegrenade"];

/// Count actual enemy HP lost within the same round windows used for kills.
fn damage_from_events(events: &[&GameEvent], rounds: &[RoundInfo]) -> BTreeMap<String, DamageTotals> {
    let mut out: BTreeMap<String, DamageTotals> = BTreeMap::new();
    let mut health: HashMap<(usize, String), u32> = HashMap::new();
    let mut events = events.to_vec();
    events.sort_by_key(|ev| Fields(ev).tick());
    for ev in events {
        let f = Fields(ev);
        let Some((index, round)) = rounds.iter().enumerate()
            .find(|(_, r)| f.tick() >= r.start_tick && f.tick() <= r.officially_ended_tick) else { continue };
        let victim = f.str("user_steamid");
        if victim.is_empty() { continue; }
        let raw = f.int("dmg_health").max(0) as u32;
        let remaining = health.entry((index, victim.clone()))
            .or_insert_with(|| f.opt_num("user_health").unwrap_or(100.0).max(0.0) as u32);
        let after = f.opt_num("health").map(|n| n.max(0.0) as u32)
            .unwrap_or_else(|| remaining.saturating_sub(raw));
        // dmg_health includes overkill. Entity health can lag multiple hits in one tick,
        // so carry forward the previous hurt event's remaining health instead.
        let damage = raw.min(remaining.saturating_sub(after));
        *remaining = after;

        // Even excluded damage reduces the HP available to subsequent enemy hits.
        let attacker = f.str("attacker_steamid");
        if attacker.is_empty() || attacker == victim || f.bool("is_freeze_period") || damage == 0 { continue; }
        let team = |field, player: &str| match f.int(field) {
            2 => Some(Team::T),
            3 => Some(Team::Ct),
            _ => round.roster.get(player).copied(),
        };
        let (Some(at), Some(vt)) = (team("attacker_team_num", &attacker), team("user_team_num", &victim)) else { continue };
        let entry = out.entry(attacker).or_default();
        if at == vt {
            entry.friendly += damage;
            continue;
        }
        entry.total += damage;
        match f.str("weapon").as_str() {
            "hegrenade" => entry.he += damage,
            "inferno" | "molotov" | "incgrenade" => entry.fire += damage,
            _ => {}
        }
        if UTILITY_WEAPONS.contains(&f.str("weapon").as_str()) { entry.utility += damage; }
    }
    out
}

fn ticks_of(groups: &HashMap<&str, Vec<&GameEvent>>, name: &str) -> Vec<i32> {
    let mut v: Vec<i32> = groups.get(name).map(|g| g.iter().map(|e| Fields(e).tick()).collect()).unwrap_or_default();
    v.sort_unstable();
    v
}

fn rounds_from_events(groups: &HashMap<&str, Vec<&GameEvent>>) -> (Vec<RoundInfo>, HashMap<i32, String>) {
    let starts = ticks_of(groups, "round_start");
    let freeze_ends = ticks_of(groups, "round_freeze_end");
    let officially = ticks_of(groups, "round_officially_ended");
    let planted = ticks_of(groups, "bomb_planted");
    let exploded = ticks_of(groups, "bomb_exploded");
    let mut defusers: HashMap<i32, String> = HashMap::new();
    let mut defused: Vec<i32> = vec![];
    for ev in groups.get("bomb_defused").map(|v| v.as_slice()).unwrap_or(&[]) {
        let f = Fields(ev);
        defused.push(f.tick());
        defusers.insert(f.tick(), f.str("user_steamid"));
    }
    defused.sort_unstable();

    let mut ends: Vec<(i32, i32, String, String)> = groups
        .get("round_end")
        .map(|v| v.iter().map(|e| (Fields(e).tick(), Fields(e).int("round"), Fields(e).str("winner"), Fields(e).str("reason"))).collect())
        .unwrap_or_default();
    ends.sort_by_key(|e| e.0);

    let last_before = |arr: &[i32], tick: i32| arr.iter().copied().take_while(|t| *t <= tick).last();
    let first_after = |arr: &[i32], tick: i32| arr.iter().copied().find(|t| *t >= tick);
    let between = |arr: &[i32], from: i32, to: i32| arr.iter().copied().find(|t| *t >= from && *t <= to);

    let rounds = ends
        .into_iter()
        .map(|(end_tick, round, winner, reason)| {
            let start_tick = last_before(&starts, end_tick).unwrap_or(0);
            let freeze_end_tick = between(&freeze_ends, start_tick, end_tick).unwrap_or(start_tick);
            RoundInfo {
                round,
                start_tick,
                freeze_end_tick,
                end_tick,
                officially_ended_tick: first_after(&officially, end_tick).unwrap_or(end_tick),
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
mod damage_tests {
    use super::*;
    use parser::second_pass::game_events::EventField;

    fn round(start: i32) -> RoundInfo {
        RoundInfo {
            round: start, start_tick: start, freeze_end_tick: start, end_tick: start + 80,
            officially_ended_tick: start + 90, winner: Some(Team::T), reason: String::new(),
            roster: BTreeMap::from([("enemy".into(), Team::T), ("friend".into(), Team::Ct), ("victim".into(), Team::Ct)]),
            bomb_planted_tick: None, bomb_defused_tick: None, bomb_defuser: None, bomb_exploded_tick: None,
        }
    }

    fn hurt(tick: i32, attacker: &str, raw: i32, after: i32, weapon: &str) -> GameEvent {
        let fields = vec![
            ("attacker_steamid", Variant::String(attacker.into())),
            ("user_steamid", Variant::String("victim".into())),
            ("attacker_team_num", Variant::I32(if attacker == "enemy" { 2 } else { 3 })),
            ("user_team_num", Variant::I32(3)),
            ("user_health", Variant::I32(100)), // Entity sample may lag the same tick's hits.
            ("health", Variant::I32(after)),
            ("dmg_health", Variant::I32(raw)),
            ("weapon", Variant::String(weapon.into())),
        ];
        GameEvent { name: "player_hurt".into(), tick,
            fields: fields.into_iter().map(|(name, value)| EventField { name: name.into(), data: Some(value) }).collect() }
    }

    #[test]
    fn lethal_overkill_and_same_tick_hits_count_only_remaining_hp() {
        let events = [hurt(10, "enemy", 21, 79, "hkp2000"), hurt(10, "enemy", 134, 0, "hkp2000"), hurt(10, "enemy", 134, 0, "hkp2000")];
        let damage = damage_from_events(&events.iter().collect::<Vec<_>>(), &[round(1)]);
        assert_eq!(damage["enemy"].total, 100);
        assert_eq!(damage["enemy"].utility, 0);
    }

    #[test]
    fn excluded_damage_still_reduces_hp_available_to_enemies() {
        let events = [hurt(10, "friend", 20, 80, "ak47"), hurt(11, "", 10, 70, "world"),
            hurt(12, "victim", 5, 65, "inferno"), hurt(13, "enemy", 200, 0, "inferno")];
        let damage = damage_from_events(&events.iter().collect::<Vec<_>>(), &[round(1)]);
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
        let mut events = [hurt(0, "enemy", 100, 0, "ak47"), hurt(10, "enemy", 200, 0, "ak47"), hurt(110, "enemy", 200, 0, "ak47")];
        for event in &mut events {
            event.fields.retain(|f| !f.name.ends_with("team_num"));
        }
        let damage = damage_from_events(&events.iter().collect::<Vec<_>>(), &[round(1), round(101)]);
        assert_eq!(damage["enemy"].total, 200);
    }
}

fn activity_from_events(events: &[GameEvent], rounds: &[RoundInfo]) -> BTreeMap<String, ActivityStats> {
    let mut out: BTreeMap<String, ActivityStats> = BTreeMap::new();
    for event in events {
        if !matches!(event.name.as_str(), "weapon_fire" | "player_blind") { continue; }
        let f = Fields(event);
        if f.bool("is_freeze_period") || !rounds.iter().any(|r| f.tick() >= r.start_tick && f.tick() <= r.officially_ended_tick) { continue; }
        if event.name == "player_blind" {
            let id = f.str("attacker_steamid");
            let duration = f.num("blind_duration");
            // The demo can emit blinds for dead targets; these provide no combat effect.
            if id.is_empty() || duration <= 1.0 || f.opt_num("user_health").is_some_and(|hp| hp <= 0.0) { continue; }
            let (at, vt) = (f.int("attacker_team_num"), f.int("user_team_num"));
            if ![2,3].contains(&at) || ![2,3].contains(&vt) { continue; }
            let s = out.entry(id).or_default();
            if at == vt { s.teammates_flashed += 1; }
            else { s.enemies_flashed += 1; s.enemy_blind_seconds += duration; }
            continue;
        }
        let id = f.str("user_steamid");
        if id.is_empty() { continue; }
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
        } else if !weapon.contains("knife") && !weapon.contains("bayonet") && weapon != "c4" && !UTILITY_WEAPONS.contains(&weapon) {
            s.shots += 1;
        }
    }
    out
}

#[cfg(test)]
mod activity_tests {
    use super::*;
    use parser::second_pass::game_events::EventField;
    #[test]
    fn counts_throws_and_qualifying_blinds_without_counting_grenades_as_shots() {
        let round: RoundInfo = serde_json::from_value(serde_json::json!({"round":1,"startTick":10,"freezeEndTick":10,"endTick":100,"officiallyEndedTick":110,"reason":"","roster":{}})).unwrap();
        let event = |name: &str, weapon: &str, duration: f32, team: i32, tick: i32| GameEvent { name: name.into(), tick, fields: vec![
            EventField { name: "weapon".into(), data: Some(Variant::String(weapon.into())) },
            EventField { name: "user_steamid".into(), data: Some(Variant::String("a".into())) },
            EventField { name: "attacker_steamid".into(), data: Some(Variant::String("a".into())) },
            EventField { name: "attacker_team_num".into(), data: Some(Variant::I32(3)) },
            EventField { name: "user_team_num".into(), data: Some(Variant::I32(team)) },
            EventField { name: "blind_duration".into(), data: Some(Variant::F32(duration)) },
        ] };
        let mut dead = event("player_blind", "", 4.2, 2, 70);
        dead.fields.push(EventField { name: "user_health".into(), data: Some(Variant::I32(0)) });
        let events = vec![event("weapon_fire","weapon_flashbang",0.0,3,20), event("weapon_fire","weapon_ak47",0.0,3,30),
            event("weapon_fire","weapon_knife",0.0,3,40), event("weapon_fire","weapon_hegrenade",0.0,3,50),
            event("player_blind","",3.0,2,60), event("player_blind","",0.5,2,60), event("player_blind","",2.0,3,60),
            event("weapon_fire","weapon_ak47",0.0,3,111), event("player_blind","",1.05,2,80),
            event("player_blind","",1.0,2,80), dead];
        let stats = activity_from_events(&events, &[round]);
        let a = &stats["a"];
        assert_eq!((a.shots,a.flashes,a.hes,a.enemies_flashed,a.teammates_flashed),(1,1,1,2,1));
        assert!((a.enemy_blind_seconds - 4.05).abs() < 0.0001);
    }
}
