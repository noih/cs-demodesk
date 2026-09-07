//! Position stream for the 2D / 3D replay: every `STEP` ticks a frame with all
//! players (position, view angle, health, armor, flags, weapon) and every live
//! grenade projectile, plus the point events the replay draws (shots, grenade
//! detonations, bomb, deaths). Built on demand from the demo, stored next to
//! the parse result as `parsed/<id>.replay.json` (disposable, see store.rs).
//!
//! The JSON is deliberately compact — a match is ~40k frames × 10 players — so
//! rows are positional arrays, documented on [`Frame`].

use crate::model::{DemoInfo, RoundInfo};
use crate::parser::{DemoParser, Fields};
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Bump when the layout below changes; older files are rebuilt.
pub const REPLAY_SCHEMA_VERSION: u32 = 2;
/// Ticks between frames: 64 tick / 4 = 16 frames per second, interpolated in the UI.
pub const STEP: i32 = 4;

pub const FLAG_ALIVE: i32 = 1;
pub const FLAG_HELMET: i32 = 2;
pub const FLAG_DEFUSER: i32 = 4;
pub const FLAG_BLIND: i32 = 8;
pub const FLAG_BOMB: i32 = 16;
pub const FLAG_SCOPED: i32 = 32;
pub const FLAG_DUCKING: i32 = 64;
/// shift-walking (silent footsteps)
pub const FLAG_WALKING: i32 = 128;

const PLAYER_PROPS: &[&str] = &["X", "Y", "Z", "yaw", "health", "armor_value", "is_alive", "has_helmet", "has_defuser", "flash_duration", "active_weapon_name", "is_scoped", "ducking", "is_walking", "balance", "inventory"];
const EVENTS: &[&str] = &[
    "weapon_fire",
    "smokegrenade_detonate",
    "smokegrenade_expired",
    "flashbang_detonate",
    "hegrenade_detonate",
    "inferno_startburn",
    "inferno_expire",
    "decoy_started",
    "decoy_detonate",
    "bomb_planted",
    "bomb_begindefuse",
    "bomb_abortdefuse",
    "bomb_defused",
    "bomb_exploded",
    "bomb_dropped",
    "bomb_pickup",
    "player_death",
];
const EVENT_PLAYER_EXTRA: &[&str] = &["X", "Y", "Z", "yaw"];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplayPlayer {
    pub steamid: String,
    pub name: String,
}

/// One sampled tick.
/// `p` rows: `[pid, x, y, z, yaw, hp, armor, flags, weapon, money]` — pid indexes
/// [`ReplayData::players`], weapon indexes [`ReplayData::weapons`], flags are
/// the `FLAG_*` bits. `g` rows: `[entityId, kind, x, y, z, pid]` for grenade
/// projectiles in flight (kind indexes [`GRENADE_KINDS`]).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Frame {
    pub t: i32,
    pub p: Vec<[i32; 10]>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub g: Vec<[i32; 6]>,
}

pub const GRENADE_KINDS: &[&str] = &["smoke", "flash", "he", "molotov", "decoy"];

fn grenade_kind(class: &str) -> Option<i32> {
    let c = class.to_ascii_lowercase();
    let k = if c.contains("smoke") {
        0
    } else if c.contains("flash") {
        1
    } else if c.contains("hegrenade") {
        2
    } else if c.contains("molotov") || c.contains("incendiary") {
        3
    } else if c.contains("decoy") {
        4
    } else {
        return None;
    };
    Some(k)
}

/// Point event. `k` is one of: shot, smoke, smokeEnd, flash, he, fire, fireEnd,
/// decoy, decoyEnd, plant, defuseStart, defuseAbort, defuse, explode, bombDrop, bombPickup, death.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayEvent {
    pub t: i32,
    pub k: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub z: Option<i32>,
    /// player index (shooter / planter / victim / carrier)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub p: Option<i32>,
    /// attacker index for `death`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub a: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub yaw: Option<i32>,
    /// grenade / inferno entity id, to pair start and end
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<i32>,
    /// defuseStart: defuser has a kit (5 s instead of 10 s)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kit: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplayData {
    pub schema_version: u32,
    pub tick_rate: f64,
    pub step: i32,
    pub first_tick: i32,
    pub last_tick: i32,
    pub players: Vec<ReplayPlayer>,
    pub weapons: Vec<String>,
    pub frames: Vec<Frame>,
    pub events: Vec<ReplayEvent>,
}

/// Grows a string table; `get` returns the index of `s`, adding it when new.
struct Interner {
    index: HashMap<String, i32>,
    names: Vec<String>,
}
impl Interner {
    fn new(first: &str) -> Self {
        Self { index: HashMap::from([(first.to_string(), 0)]), names: vec![first.to_string()] }
    }
    fn get(&mut self, s: &str) -> i32 {
        if let Some(i) = self.index.get(s) {
            return *i;
        }
        let i = self.names.len() as i32;
        self.names.push(s.to_string());
        self.index.insert(s.to_string(), i);
        i
    }
}

/// Player list with steamid → index; players missing from the parse result
/// (joined late) are added when first seen.
struct Players {
    list: Vec<ReplayPlayer>,
    index: HashMap<String, i32>,
}
impl Players {
    fn new(info: &DemoInfo) -> Self {
        let list: Vec<ReplayPlayer> = info.players.iter().map(|p| ReplayPlayer { steamid: p.steamid.clone(), name: p.name.clone() }).collect();
        let index = list.iter().enumerate().map(|(i, p)| (p.steamid.clone(), i as i32)).collect();
        Self { list, index }
    }
    /// `name` is only read when the steamid is new.
    fn pid(&mut self, steamid: &str, name: impl FnOnce() -> Option<String>) -> i32 {
        if let Some(i) = self.index.get(steamid) {
            return *i;
        }
        let i = self.list.len() as i32;
        self.list.push(ReplayPlayer { steamid: steamid.to_string(), name: name().unwrap_or_else(|| steamid.to_string()) });
        self.index.insert(steamid.to_string(), i);
        i
    }
}

fn round(v: Option<f64>) -> i32 {
    v.map(|v| v.round() as i32).unwrap_or(0)
}

/// Build the replay stream from the .dem `bytes`; `info` / `rounds` come from
/// the parse result (player list, round range). Three passes over the demo:
/// player props per sampled tick, grenade projectiles, then point events.
pub fn build_replay(parser: &DemoParser, info: &DemoInfo, rounds: &[RoundInfo], bytes: &[u8]) -> Result<ReplayData> {
    let header = parser.header(bytes)?;
    let playback_ticks: i32 = header.get("playback_ticks").and_then(|v| v.parse().ok()).unwrap_or(0);
    let first_tick = rounds.first().map(|r| r.start_tick).unwrap_or(0).max(0);
    let last_tick = rounds.last().map(|r| r.officially_ended_tick).filter(|t| *t > first_tick).unwrap_or(playback_ticks);
    if last_tick <= first_tick {
        return Err(anyhow!("demo has no rounds"));
    }
    let wanted: Vec<i32> = (first_tick..=last_tick).step_by(STEP as usize).collect();

    let mut players = Players::new(info);
    let mut weapons = Interner::new("");
    let mut frames = player_frames(parser, bytes, wanted.clone(), &mut players, &mut weapons)?;
    add_grenades(parser, bytes, wanted, &mut players, &mut frames)?;
    let events = point_events(parser, bytes, first_tick, last_tick, &mut players)?;
    drop_detonated(&mut frames, &events);

    Ok(ReplayData { schema_version: REPLAY_SCHEMA_VERSION, tick_rate: info.tick_rate, step: STEP, first_tick, last_tick, players: players.list, weapons: weapons.names, frames, events })
}

/// Pass 1: one row per (sampled tick, player) → frames with `p` rows.
fn player_frames(parser: &DemoParser, bytes: &[u8], wanted: Vec<i32>, players: &mut Players, weapons: &mut Interner) -> Result<Vec<Frame>> {
    let props: Vec<String> = PLAYER_PROPS.iter().map(|s| s.to_string()).collect();
    let rows = parser.ticks(bytes, &props, wanted)?;
    let mut frames: Vec<Frame> = Vec::with_capacity(rows.len() / 10 + 1);
    for row in rows.iter() {
        let Some(tick) = row.tick() else { continue };
        let Some(sid) = row.steamid().filter(|s| s != "0") else { continue };
        let p = players.pid(&sid, || row.str("name").map(str::to_string));
        let bits = [
            (row.flag("is_alive"), FLAG_ALIVE),
            (row.flag("has_helmet"), FLAG_HELMET),
            (row.flag("has_defuser"), FLAG_DEFUSER),
            (row.num("flash_duration").unwrap_or(0.0) > 0.0, FLAG_BLIND),
            (row.strs("inventory").is_some_and(|inv| inv.iter().any(|w| w.starts_with("C4"))), FLAG_BOMB),
            (row.flag("is_scoped"), FLAG_SCOPED),
            (row.flag("ducking"), FLAG_DUCKING),
            (row.flag("is_walking"), FLAG_WALKING),
        ];
        let flags = bits.iter().filter(|(on, _)| *on).fold(0, |acc, (_, bit)| acc | bit);
        let weapon = row.str("active_weapon_name").map(|w| weapons.get(w)).unwrap_or(0);
        let entry = [p, round(row.num("X")), round(row.num("Y")), round(row.num("Z")), round(row.num("yaw")), round(row.num("health")), round(row.num("armor_value")), flags, weapon, round(row.num("balance"))];
        match frames.last_mut() {
            Some(f) if f.t == tick => f.p.push(entry),
            _ => frames.push(Frame { t: tick, p: vec![entry], g: vec![] }),
        }
    }
    // rows come tick-sorted from the parser; frames therefore are too
    Ok(frames)
}

/// Pass 2: grenade projectiles in flight, attached to the frame of their tick.
fn add_grenades(parser: &DemoParser, bytes: &[u8], wanted: Vec<i32>, players: &mut Players, frames: &mut Vec<Frame>) -> Result<()> {
    let mut by_tick: HashMap<i32, usize> = frames.iter().enumerate().map(|(i, f)| (f.t, i)).collect();
    for row in parser.projectiles(bytes, wanted)?.iter() {
        let Some(tick) = row.tick() else { continue };
        let Some(kind) = row.str("grenade_type").and_then(grenade_kind) else { continue };
        let (Some(x), Some(y), Some(z)) = (row.num("x"), row.num("y"), row.num("z")) else { continue };
        let thrower = row.steamid().map(|s| players.pid(&s, || row.str("name").map(str::to_string))).unwrap_or(-1);
        let id = round(row.num("grenade_entity_id"));
        let idx = *by_tick.entry(tick).or_insert_with(|| {
            frames.push(Frame { t: tick, p: vec![], g: vec![] });
            frames.len() - 1
        });
        frames[idx].g.push([id, kind, x.round() as i32, y.round() as i32, z.round() as i32, thrower]);
    }
    frames.sort_by_key(|f| f.t);
    Ok(())
}

/// Pass 3: the point events the replay draws, inside the round range.
fn point_events(parser: &DemoParser, bytes: &[u8], first_tick: i32, last_tick: i32, players: &mut Players) -> Result<Vec<ReplayEvent>> {
    let names: Vec<String> = EVENTS.iter().map(|s| s.to_string()).collect();
    let extra: Vec<String> = EVENT_PLAYER_EXTRA.iter().map(|s| s.to_string()).collect();
    let out = parser.events(bytes, &names, &extra, &[])?;
    let mut events: Vec<ReplayEvent> = vec![];
    for ev in &out.game_events {
        let tick = ev.tick;
        if tick < first_tick || tick > last_tick {
            continue;
        }
        let f = Fields(ev);
        let coord = |key: &str| f.opt_num(key).map(|v| v.round() as i32);
        let user = f.opt_str("user_steamid").filter(|s| s != "0").map(|s| players.pid(&s, || f.opt_str("user_name")));
        let user_pos = || (coord("user_X"), coord("user_Y"), coord("user_Z"));
        let world_pos = || (coord("x"), coord("y"), coord("z"));
        let base = |k: &str, pos: (Option<i32>, Option<i32>, Option<i32>)| ReplayEvent { t: tick, k: k.to_string(), x: pos.0, y: pos.1, z: pos.2, p: user, a: None, yaw: None, id: f.opt_num("entityid").map(|v| v as i32), kit: None };
        let event = match ev.name.as_str() {
            "weapon_fire" => {
                let w = f.str("weapon");
                if w.contains("knife") || w.ends_with("grenade") || w.contains("molotov") || w.contains("incgrenade") || w.contains("decoy") || w.contains("flashbang") || w == "weapon_c4" {
                    continue;
                }
                ReplayEvent { yaw: coord("user_yaw"), ..base("shot", user_pos()) }
            }
            "smokegrenade_detonate" => base("smoke", world_pos()),
            "smokegrenade_expired" => base("smokeEnd", world_pos()),
            "flashbang_detonate" => base("flash", world_pos()),
            "hegrenade_detonate" => base("he", world_pos()),
            "inferno_startburn" => base("fire", world_pos()),
            "inferno_expire" => base("fireEnd", world_pos()),
            "decoy_started" => base("decoy", world_pos()),
            "decoy_detonate" => base("decoyEnd", world_pos()),
            "bomb_planted" => base("plant", user_pos()),
            "bomb_begindefuse" => ReplayEvent { kit: f.opt_bool("haskit"), ..base("defuseStart", user_pos()) },
            "bomb_abortdefuse" => base("defuseAbort", user_pos()),
            "bomb_defused" => base("defuse", user_pos()),
            "bomb_exploded" => base("explode", user_pos()),
            "bomb_dropped" => base("bombDrop", user_pos()),
            "bomb_pickup" => base("bombPickup", user_pos()),
            "player_death" => {
                let attacker = f.opt_str("attacker_steamid").filter(|s| s != "0").map(|s| players.pid(&s, || f.opt_str("attacker_name")));
                ReplayEvent { a: attacker, ..base("death", user_pos()) }
            }
            _ => continue,
        };
        events.push(event);
    }
    events.sort_by_key(|e| e.t);
    Ok(events)
}

/// A grenade entity lingers after it went off; drop its in-flight marker once
/// the detonate event happened nearby (the effect is drawn from the event).
fn drop_detonated(frames: &mut [Frame], events: &[ReplayEvent]) {
    let kind_of = |k: &str| match k {
        "smoke" => Some(0),
        "flash" => Some(1),
        "he" => Some(2),
        "fire" => Some(3),
        _ => None,
    };
    let detonations: Vec<(i32, i32, i32, i32)> = events.iter().filter_map(|e| Some((e.t, kind_of(&e.k)?, e.x?, e.y?))).collect();
    for f in frames.iter_mut() {
        let t = f.t;
        f.g.retain(|g| !detonations.iter().any(|(et, kind, x, y)| *kind == g[1] && *et <= t && *et > t - 64 * 30 && ((x - g[2]).pow(2) + (y - g[3]).pow(2)) < 250 * 250));
    }
}
