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

const PLAYER_EXTRA: &[&str] = &["health", "X", "Y", "Z", "pitch", "yaw", "is_scoped", "team_name", "active_weapon_name"];
const OTHER_EXTRA: &[&str] = &["total_rounds_played", "is_freeze_period"];
const EVENTS: &[&str] = &[
    "player_death",
    "player_hurt",
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
        let inputs = self.inputs_ex(&[], &[], &[], wanted_ticks, true)?;
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
        let damage = damage_from_events(groups.get("player_hurt").map(|v| v.as_slice()).unwrap_or(&[]));
        let (mut rounds, defusers) = rounds_from_events(&groups);

        // Roster + user ids at every freeze end.
        let wanted: Vec<i32> = rounds.iter().map(|r| r.freeze_end_tick + 1).collect();
        if !wanted.is_empty() {
            let rows = self.ticks(bytes, &strings(&["team_num", "user_id"]), wanted)?;
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

        Ok(DemoData {
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

/// Sum player_hurt health damage per attacker. Team damage and self damage are
/// excluded, as is anything during the freeze period.
fn damage_from_events(events: &[&GameEvent]) -> BTreeMap<String, DamageTotals> {
    let mut out: BTreeMap<String, DamageTotals> = BTreeMap::new();
    for ev in events {
        let f = Fields(ev);
        let attacker = f.str("attacker_steamid");
        let victim = f.str("user_steamid");
        if attacker.is_empty() || attacker == victim || f.bool("is_freeze_period") {
            continue;
        }
        let at = f.str("attacker_team_name");
        let vt = f.str("user_team_name");
        if !at.is_empty() && at == vt {
            continue;
        }
        let dmg = f.int("dmg_health").max(0) as u32;
        if dmg == 0 {
            continue;
        }
        let entry = out.entry(attacker).or_default();
        entry.total += dmg;
        if UTILITY_WEAPONS.contains(&f.str("weapon").as_str()) {
            entry.utility += dmg;
        }
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
