//! Per-player statistics and the cross-halftime team score, plus the
//! [`ParsedDemo`] bundle the app stores per demo.

use crate::detector::{find_clutches, kills_by_round};
use crate::model::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum TeamKey {
    A,
    B,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerStats {
    pub steamid: SteamId,
    pub name: String,
    /// Team identity across halves: A started as CT, B started as T
    pub team: TeamKey,
    pub kills: u32,
    pub deaths: u32,
    pub assists: u32,
    pub headshots: u32,
    pub headshot_pct: u32,
    pub kd: f64,
    /// 2k / 3k / 4k / 5k round counts
    pub multi_kills: BTreeMap<String, u32>,
    pub clutches_won: u32,
    /// health damage dealt to enemies
    pub damage: u32,
    /// part of `damage` done with grenades / molotov
    pub utility_damage: u32,
    /// average damage per round
    pub adr: f64,
    pub highlights: u32,
    pub best_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoundSummary {
    pub round: i32,
    pub winner: Option<TeamKey>,
    pub reason: String,
    pub start_tick: i32,
    pub end_tick: i32,
    /// Kills in this round per team key
    pub kills_a: u32,
    pub kills_b: u32,
    pub bomb_planted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParsedDemo {
    pub info: DemoInfo,
    pub rounds: Vec<RoundInfo>,
    pub kills: Vec<KillEvent>,
    pub highlights: Vec<Highlight>,
    pub stats: Vec<PlayerStats>,
    pub score: BTreeMap<String, u32>,
    pub round_summaries: Vec<RoundSummary>,
    pub parsed_at: String,
}

/// Team identity that survives the halftime swap: A = CT in round 1, B = T in round 1.
pub fn team_of_player(demo: &DemoData) -> impl Fn(&str) -> TeamKey + '_ {
    let first = demo.rounds.first();
    let first_ct: HashSet<String> = first.map(|r| r.roster.iter().filter(|(_, t)| **t == Team::Ct).map(|(id, _)| id.clone()).collect()).unwrap_or_default();
    let first_t: HashSet<String> = first.map(|r| r.roster.iter().filter(|(_, t)| **t == Team::T).map(|(id, _)| id.clone()).collect()).unwrap_or_default();
    move |sid: &str| {
        // not in the first round's roster: fall back to the side recorded at demo start (2 = T)
        if first_ct.contains(sid) {
            TeamKey::A
        } else if first_t.contains(sid) || demo.info.players.iter().any(|p| p.steamid == sid && p.team_number == 2) {
            TeamKey::B
        } else {
            TeamKey::A
        }
    }
}

fn round_winner_key(r: &RoundInfo, team_of: &impl Fn(&str) -> TeamKey) -> Option<TeamKey> {
    let winner = r.winner?;
    let votes: Vec<TeamKey> = r.roster.iter().filter(|(_, t)| **t == winner).map(|(id, _)| team_of(id)).collect();
    let a = votes.iter().filter(|v| **v == TeamKey::A).count();
    let b = votes.len() - a;
    Some(if a >= b { TeamKey::A } else { TeamKey::B })
}

pub fn compute_score(demo: &DemoData) -> BTreeMap<String, u32> {
    let team_of = team_of_player(demo);
    let mut score = BTreeMap::from([("A".to_string(), 0u32), ("B".to_string(), 0u32)]);
    for r in &demo.rounds {
        match round_winner_key(r, &team_of) {
            Some(TeamKey::A) => *score.get_mut("A").unwrap() += 1,
            Some(TeamKey::B) => *score.get_mut("B").unwrap() += 1,
            None => {}
        }
    }
    score
}

pub fn round_summaries(demo: &DemoData) -> Vec<RoundSummary> {
    let team_of = team_of_player(demo);
    let per_round = kills_by_round(demo);
    demo.rounds
        .iter()
        .map(|r| {
            let kills = per_round.get(&r.round).cloned().unwrap_or_default();
            let mut kills_a = 0;
            let mut kills_b = 0;
            for k in &kills {
                let Some(a) = &k.attacker else { continue };
                if r.roster.get(&a.steamid) == r.roster.get(&k.victim.steamid) {
                    continue;
                }
                match team_of(&a.steamid) {
                    TeamKey::A => kills_a += 1,
                    TeamKey::B => kills_b += 1,
                }
            }
            RoundSummary { round: r.round, winner: round_winner_key(r, &team_of), reason: r.reason.clone(), start_tick: r.start_tick, end_tick: r.end_tick, kills_a, kills_b, bomb_planted: r.bomb_planted_tick.is_some() }
        })
        .collect()
}

pub fn compute_stats(demo: &DemoData, highlights: &[Highlight]) -> Vec<PlayerStats> {
    let team_of = team_of_player(demo);
    let mut by_player: HashMap<String, PlayerStats> = HashMap::new();
    for p in &demo.info.players {
        by_player.insert(
            p.steamid.clone(),
            PlayerStats {
                steamid: p.steamid.clone(),
                name: p.name.clone(),
                team: team_of(&p.steamid),
                kills: 0,
                deaths: 0,
                assists: 0,
                headshots: 0,
                headshot_pct: 0,
                kd: 0.0,
                multi_kills: ["2k", "3k", "4k", "5k"].iter().map(|k| (k.to_string(), 0)).collect(),
                clutches_won: 0,
                damage: demo.damage.get(&p.steamid).map(|d| d.total).unwrap_or(0),
                utility_damage: demo.damage.get(&p.steamid).map(|d| d.utility).unwrap_or(0),
                adr: 0.0,
                highlights: 0,
                best_score: 0.0,
            },
        );
    }
    let per_round = kills_by_round(demo);
    for round in &demo.rounds {
        let kills = per_round.get(&round.round).cloned().unwrap_or_default();
        let mut round_kills: HashMap<&str, u32> = HashMap::new();
        for k in &kills {
            if let Some(v) = by_player.get_mut(&k.victim.steamid) {
                v.deaths += 1;
            }
            let Some(a) = &k.attacker else { continue };
            if let (Some(at), Some(vt)) = (round.roster.get(&a.steamid), round.roster.get(&k.victim.steamid)) {
                if at == vt {
                    continue;
                }
            }
            if let Some(s) = by_player.get_mut(&a.steamid) {
                s.kills += 1;
                if k.headshot {
                    s.headshots += 1;
                }
                *round_kills.entry(a.steamid.as_str()).or_default() += 1;
            }
            if let Some(assister) = &k.assister {
                if let Some(s) = by_player.get_mut(&assister.steamid) {
                    s.assists += 1;
                }
            }
        }
        for (sid, n) in round_kills {
            if n >= 2 {
                if let Some(s) = by_player.get_mut(sid) {
                    *s.multi_kills.get_mut(&format!("{}k", n.min(5))).unwrap() += 1;
                }
            }
        }
        for c in find_clutches(round, &kills) {
            if c.won {
                if let Some(s) = by_player.get_mut(&c.player) {
                    s.clutches_won += 1;
                }
            }
        }
    }
    for h in highlights {
        if let Some(s) = by_player.get_mut(&h.player.steamid) {
            s.highlights += 1;
            s.best_score = s.best_score.max(h.score);
        }
    }
    let mut out: Vec<PlayerStats> = by_player
        .into_values()
        .map(|mut s| {
            s.headshot_pct = if s.kills > 0 { ((s.headshots as f64 / s.kills as f64) * 100.0).round() as u32 } else { 0 };
            s.kd = if s.deaths > 0 { ((s.kills as f64 / s.deaths as f64) * 100.0).round() / 100.0 } else { s.kills as f64 };
            s.adr = if demo.rounds.is_empty() { 0.0 } else { (s.damage as f64 / demo.rounds.len() as f64 * 10.0).round() / 10.0 };
            s
        })
        .collect();
    out.sort_by(|a, b| a.team.cmp(&b.team).then(b.kills.cmp(&a.kills)));
    out
}

impl ParsedDemo {
    /// The JSON the UI loads when a demo is opened: everything but the (large) kill list.
    pub fn without_kills(&self) -> serde_json::Value {
        let mut v = serde_json::to_value(self).unwrap_or_default();
        if let Some(obj) = v.as_object_mut() {
            obj.remove("kills");
        }
        v
    }
}

pub fn build_parsed_demo(demo: DemoData) -> ParsedDemo {
    let highlights = crate::detector::detect(&demo, &DetectOptions { min_score: 1.0, top_n: 200, ..Default::default() });
    let stats = compute_stats(&demo, &highlights);
    let score = compute_score(&demo);
    let round_summaries = round_summaries(&demo);
    ParsedDemo { info: demo.info, rounds: demo.rounds, kills: demo.kills, highlights, stats, score, round_summaries, parsed_at: chrono::Utc::now().to_rfc3339() }
}
