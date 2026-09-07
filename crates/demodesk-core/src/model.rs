//! Data contracts shared by every stage (parse → detect → render) and with the
//! frontend (serialized as camelCase JSON). Everything time-related is in demo
//! ticks so any render backend can align exactly.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub type SteamId = String;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash, PartialOrd, Ord)]
pub enum Team {
    #[serde(rename = "CT")]
    Ct,
    #[serde(rename = "TERRORIST")]
    T,
}

impl Team {
    pub fn enemy(self) -> Team {
        match self {
            Team::Ct => Team::T,
            Team::T => Team::Ct,
        }
    }
    pub fn from_name(name: &str) -> Team {
        if name == "CT" {
            Team::Ct
        } else {
            Team::T
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerInfo {
    pub name: String,
    pub steamid: SteamId,
    /// 2 = T, 3 = CT as recorded at demo start (teams swap at halftime)
    pub team_number: i32,
    /// In-game user id; the CS2 `spec_player` slot is user_id + 1
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DemoInfo {
    pub path: String,
    pub map_name: String,
    pub server_name: String,
    /// Ticks per second. Valve MM demos are 64.
    pub tick_rate: f64,
    pub players: Vec<PlayerInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Attacker {
    pub steamid: SteamId,
    pub name: String,
    pub team: Team,
    pub health: i32,
    pub weapon_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Victim {
    pub steamid: SteamId,
    pub name: String,
    pub team: Team,
    pub weapon_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Assister {
    pub steamid: SteamId,
    pub name: String,
    pub team: Team,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KillEvent {
    pub tick: i32,
    /// 0-based round index as reported by total_rounds_played at kill time
    pub round: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attacker: Option<Attacker>,
    pub victim: Victim,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assister: Option<Assister>,
    /// weapon class name e.g. "ak47", "usp_silencer", "knife", "taser"
    pub weapon: String,
    pub headshot: bool,
    pub noscope: bool,
    /// number of surfaces penetrated (wallbang when > 0)
    pub penetrated: i32,
    pub thru_smoke: bool,
    pub attacker_blind: bool,
    pub attacker_in_air: bool,
    pub assisted_flash: bool,
    pub distance: f64,
    pub hitgroup: String,
    pub is_freeze_period: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoundInfo {
    /// 1-based round number as reported by the round_end event
    pub round: i32,
    pub start_tick: i32,
    pub freeze_end_tick: i32,
    pub end_tick: i32,
    pub officially_ended_tick: i32,
    /// None = unknown winner
    #[serde(skip_serializing_if = "Option::is_none")]
    pub winner: Option<Team>,
    pub reason: String,
    /// Roster at freeze end: steamid -> team
    pub roster: BTreeMap<SteamId, Team>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bomb_planted_tick: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bomb_defused_tick: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bomb_defuser: Option<SteamId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bomb_exploded_tick: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DemoData {
    pub info: DemoInfo,
    pub kills: Vec<KillEvent>,
    pub rounds: Vec<RoundInfo>,
    /// Damage dealt to enemies, per attacker steamid (from player_hurt)
    #[serde(default)]
    pub damage: BTreeMap<SteamId, DamageTotals>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DamageTotals {
    /// health damage to enemies, all weapons
    pub total: u32,
    /// share of `total` done by grenades / molotov fire
    pub utility: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HighlightPlayer {
    pub steamid: SteamId,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Highlight {
    pub id: String,
    /// Whose POV to render
    pub player: HighlightPlayer,
    pub round: i32,
    /// Inclusive tick window to render (already padded with lead-in / lead-out)
    pub start_tick: i32,
    pub end_tick: i32,
    /// Tick of the first notable action
    pub anchor_tick: i32,
    pub score: f64,
    pub tags: Vec<String>,
    /// Human readable summary e.g. "Player B — 3K (2 HS, clutch) 1v3 won · R13"
    pub title: String,
    pub kills: Vec<KillEvent>,
    /// Per-rule score contributions, for tuning
    pub breakdown: BTreeMap<String, f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DetectOptions {
    /// Restrict to these players (steamids). Empty = everyone.
    pub players: Vec<SteamId>,
    /// Kills further apart than this are split into separate moments
    pub cluster_gap_seconds: f64,
    pub lead_in_seconds: f64,
    pub lead_out_seconds: f64,
    pub min_score: f64,
    pub top_n: usize,
}

impl Default for DetectOptions {
    fn default() -> Self {
        Self { players: vec![], cluster_gap_seconds: 20.0, lead_in_seconds: 5.0, lead_out_seconds: 3.0, min_score: 3.0, top_n: 20 }
    }
}

pub fn seconds_to_ticks(seconds: f64, tick_rate: f64) -> i32 {
    (seconds * tick_rate).round() as i32
}

pub fn format_clock(ticks: i32, tick_rate: f64) -> String {
    let total = (ticks.max(0) as f64 / tick_rate).floor() as i64;
    format!("{}:{:02}", total / 60, total % 60)
}
