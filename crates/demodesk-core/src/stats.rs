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
    pub opening_kills: u32,
    pub opening_deaths: u32,
    pub flash_assists: u32,
    pub rounds_played: u32,
    pub rounds_survived: u32,
    pub kast: f64,
    pub trade_kills: u32,
    pub traded_deaths: u32,
    pub he_damage: u32,
    pub fire_damage: u32,
    pub activity: ActivityStats,
    pub aim: BTreeMap<String, crate::aim::AimStats>,
    pub recoil: BTreeMap<String, Vec<crate::aim::RecoilPoint>>,
    pub clutches: Vec<ClutchStats>,
    pub opponents: BTreeMap<String, u32>,
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
    /// Actual health damage to teammates; excluded from damage and ADR.
    #[serde(default)]
    pub friendly_damage: u32,
    /// average damage per round
    pub adr: f64,
    pub highlights: u32,
    pub best_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoundSummary {
    pub players: BTreeMap<SteamId, RoundMetrics>,
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
    #[serde(default)]
    pub recoil_reference: BTreeMap<String, Vec<crate::aim::RecoilPoint>>,
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
            let mut players = demo.round_metrics.get(&r.round).cloned().unwrap_or_default();
            let mut kills_a = 0;
            let mut kills_b = 0;
            for k in &kills {
                players.entry(k.victim.steamid.clone()).or_default().deaths += 1;
                let Some(a) = &k.attacker else { continue };
                if a.steamid == k.victim.steamid || r.roster.get(&a.steamid).copied().unwrap_or(a.team) == r.roster.get(&k.victim.steamid).copied().unwrap_or(k.victim.team) {
                    continue;
                }
                let player = players.entry(a.steamid.clone()).or_default();
                player.kills += 1;
                if k.weapon == "awp" { player.awp += 1; }
                match team_of(&a.steamid) {
                    TeamKey::A => kills_a += 1,
                    TeamKey::B => kills_b += 1,
                }
            }
            RoundSummary { players, round: r.round, winner: round_winner_key(r, &team_of), reason: r.reason.clone(), start_tick: r.start_tick, end_tick: r.end_tick, kills_a, kills_b, bomb_planted: r.bomb_planted_tick.is_some() }
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
                opening_kills: 0,
                opening_deaths: 0,
                flash_assists: 0,
                rounds_played: 0,
                rounds_survived: 0,
                kast: 0.0,
                trade_kills: 0,
                traded_deaths: 0,
                he_damage: demo.damage.get(&p.steamid).map_or(0, |d| d.he),
                fire_damage: demo.damage.get(&p.steamid).map_or(0, |d| d.fire),
                recoil: demo.recoil.get(&p.steamid).cloned().unwrap_or_default(),
                aim: demo.aim.get(&p.steamid).cloned().unwrap_or_default(),
                activity: demo.activity.get(&p.steamid).cloned().unwrap_or_default(),
                clutches: vec![],
                opponents: BTreeMap::new(),
                headshots: 0,
                headshot_pct: 0,
                kd: 0.0,
                multi_kills: ["2k", "3k", "4k", "5k"].iter().map(|k| (k.to_string(), 0)).collect(),
                clutches_won: 0,
                damage: demo.damage.get(&p.steamid).map(|d| d.total).unwrap_or(0),
                utility_damage: demo.damage.get(&p.steamid).map(|d| d.utility).unwrap_or(0),
                friendly_damage: demo.damage.get(&p.steamid).map(|d| d.friendly).unwrap_or(0),
                adr: 0.0,
                highlights: 0,
                best_score: 0.0,
            },
        );
    }
    let per_round = kills_by_round(demo);
    for round in &demo.rounds {
        let mut kills = per_round.get(&round.round).cloned().unwrap_or_default();
        kills.sort_by_key(|k| k.tick);
        let mut opening_recorded = false;
        let mut round_kills: HashMap<&str, u32> = HashMap::new();
        for k in &kills {
            if let Some(v) = by_player.get_mut(&k.victim.steamid) {
                v.deaths += 1;
            }
            if let Some(assister) = enemy_assister(k, round) {
                if let Some(s) = by_player.get_mut(&assister.steamid) {
                    s.assists += 1;
                    s.flash_assists += u32::from(k.assisted_flash);
                }
            }
            let Some(a) = &k.attacker else { continue };
            let attacker_team = round.roster.get(&a.steamid).copied().unwrap_or(a.team);
            let victim_team = round.roster.get(&k.victim.steamid).copied().unwrap_or(k.victim.team);
            if a.steamid == k.victim.steamid || attacker_team == victim_team {
                continue;
            }
            if !opening_recorded {
                if let Some(s) = by_player.get_mut(&a.steamid) {
                    s.opening_kills += 1;
                }
                if let Some(s) = by_player.get_mut(&k.victim.steamid) {
                    s.opening_deaths += 1;
                }
                opening_recorded = true;
            }
            if let Some(s) = by_player.get_mut(&a.steamid) {
                s.kills += 1;
                *s.opponents.entry(k.victim.steamid.clone()).or_default() += 1;
                if k.headshot {
                    s.headshots += 1;
                }
                *round_kills.entry(a.steamid.as_str()).or_default() += 1;
            }
        }
        for (sid, n) in round_kills {
            if n >= 2 {
                if let Some(s) = by_player.get_mut(sid) {
                    *s.multi_kills.get_mut(&format!("{}k", n.min(5))).unwrap() += 1;
                }
            }
        }
        // A traded death must be avenged by a teammate within five seconds in this round.
        let enemy = |k: &&KillEvent| k.attacker.as_ref().is_some_and(|a| a.steamid != k.victim.steamid &&
            round.roster.get(&a.steamid).copied().unwrap_or(a.team) != round.roster.get(&k.victim.steamid).copied().unwrap_or(k.victim.team));
        let mut traded = HashSet::new();
        let mut trade_kill_ticks = HashSet::new();
        for (i, death) in kills.iter().enumerate().filter(|(_, k)| enemy(k)) {
            let killer = death.attacker.as_ref().expect("enemy kill has attacker");
            if let Some((j, revenge)) = kills.iter().enumerate().skip(i + 1).find(|(_, k)| enemy(k) && k.victim.steamid == killer.steamid &&
                (k.tick - death.tick) as f64 <= 5.0 * demo.info.tick_rate &&
                k.attacker.as_ref().is_some_and(|a| round.roster.get(&a.steamid).copied().unwrap_or(a.team) == round.roster.get(&death.victim.steamid).copied().unwrap_or(death.victim.team))) {
                traded.insert(death.victim.steamid.as_str());
                if trade_kill_ticks.insert(j) {
                    if let Some(s) = revenge.attacker.as_ref().and_then(|a| by_player.get_mut(&a.steamid)) { s.trade_kills += 1; }
                }
            }
        }
        for id in round.roster.keys() {
            if let Some(s) = by_player.get_mut(id) {
                s.rounds_played += 1;
                let survived = !kills.iter().any(|k| k.victim.steamid == *id);
                s.rounds_survived += u32::from(survived);
                s.traded_deaths += u32::from(traded.contains(id.as_str()));
                if survived || traded.contains(id.as_str()) || kills.iter().any(|k| (enemy(k) && k.attacker.as_ref().is_some_and(|a| a.steamid == *id)) || enemy_assister(k, round).is_some_and(|a| a.steamid == *id)) { s.kast += 1.0; }
            }
        }
        for c in find_clutches(round, &kills) {
            if let Some(s) = by_player.get_mut(&c.player) {
                s.clutches.push(ClutchStats { round: round.round, side: c.team, versus: c.versus,
                    kills: c.kills.len(), outcome: if c.won { "won" } else if !kills.iter().any(|k| k.tick <= round.end_tick && k.victim.steamid == c.player) { "saved" } else { "lost" }.into() });
            }
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
            s.kast = if s.rounds_played == 0 { 0.0 } else { (s.kast / s.rounds_played as f64 * 1000.0).round() / 10.0 };
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
    ParsedDemo { recoil_reference: demo.recoil_reference, info: demo.info, rounds: demo.rounds, kills: demo.kills, highlights, stats, score, round_summaries, parsed_at: chrono::Utc::now().to_rfc3339() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opening_and_flash_stats_follow_enemy_events_per_round() {
        let mut demo: DemoData = serde_json::from_value(serde_json::json!({
            "info": {"path":"test.dem", "mapName":"de_test", "serverName":"", "tickRate":64,
                "players":[
                    {"steamid":"a","name":"A","teamNumber":3},
                    {"steamid":"b","name":"B","teamNumber":3},
                    {"steamid":"c","name":"C","teamNumber":2}
                ]},
            "kills":[], "rounds":[
                {"round":1,"startTick":0,"freezeEndTick":10,"endTick":90,"officiallyEndedTick":100,"reason":"", "roster":{"a":"CT","b":"CT","c":"TERRORIST"}},
                {"round":2,"startTick":101,"freezeEndTick":110,"endTick":190,"officiallyEndedTick":200,"reason":"", "roster":{"a":"TERRORIST","b":"TERRORIST","c":"CT"}},
                {"round":3,"startTick":201,"freezeEndTick":210,"endTick":290,"officiallyEndedTick":300,"reason":"", "roster":{}}
            ]
        })).unwrap();
        let kill: KillEvent = serde_json::from_value(serde_json::json!({
            "tick":20,"round":0,
            "attacker":{"steamid":"a","name":"A","team":"CT","health":100,"weaponName":"ak47"},
            "victim":{"steamid":"c","name":"C","team":"TERRORIST","weaponName":"ak47"},
            "assister":{"steamid":"b","name":"B","team":"CT"},
            "weapon":"ak47","headshot":false,"noscope":false,"penetrated":0,"thruSmoke":false,
            "attackerBlind":false,"attackerInAir":false,"assistedFlash":true,
            "distance":10,"hitgroup":"chest","isFreezePeriod":false
        })).unwrap();
        let mut later = kill.clone();
        later.tick = 30;
        later.assisted_flash = false;
        let mut friendly = kill.clone();
        friendly.tick = 15;
        friendly.victim.steamid = "b".into();
        let mut world = kill.clone();
        world.tick = 12;
        world.attacker = None;
        world.assister = None;
        let mut frozen = kill.clone();
        frozen.tick = 5;
        frozen.is_freeze_period = true;
        let mut second = kill.clone();
        second.tick = 120;
        second.assister = None;
        let mut outside = kill.clone();
        outside.tick = 301;
        demo.kills = vec![later, friendly, world, frozen, outside, second, kill.clone()];
        let stats = compute_stats(&demo, &[]);
        let a = stats.iter().find(|s| s.steamid == "a").unwrap();
        let b = stats.iter().find(|s| s.steamid == "b").unwrap();
        let c = stats.iter().find(|s| s.steamid == "c").unwrap();
        assert_eq!((a.opening_kills, c.opening_deaths, b.flash_assists), (2, 2, 1));
        assert_eq!((b.opening_kills, b.opening_deaths, a.flash_assists), (0, 0, 0));
        assert_eq!((a.kills, b.assists), (3, 2));
        demo.rounds.truncate(1);
        demo.rounds[0].roster.clear();
        demo.kills = vec![kill.clone()];
        let stats = compute_stats(&demo, &[]);
        assert_eq!(stats.iter().find(|s| s.steamid == "a").unwrap().opening_kills, 1);
        let mut teamkill = kill.clone();
        teamkill.attacker.as_mut().unwrap().steamid = "c".into();
        teamkill.attacker.as_mut().unwrap().team = Team::T;
        demo.kills = vec![teamkill];
        assert_eq!(compute_stats(&demo, &[]).iter().find(|s| s.steamid == "b").unwrap().assists, 1);
        let mut friendly_assist = kill.clone();
        friendly_assist.assister.as_mut().unwrap().steamid = "c".into();
        friendly_assist.assister.as_mut().unwrap().team = Team::T;
        demo.kills = vec![friendly_assist];
        assert_eq!(compute_stats(&demo, &[]).iter().find(|s| s.steamid == "c").unwrap().assists, 0);
        demo.rounds[0].roster = BTreeMap::from([("a".into(),Team::Ct),("b".into(),Team::Ct),("c".into(),Team::T)]);
        demo.rounds[0].end_tick = 1000;
        demo.rounds[0].officially_ended_tick = 1000;
        let mut death = kill.clone();
        death.attacker.as_mut().unwrap().steamid = "c".into();
        death.attacker.as_mut().unwrap().team = Team::T;
        death.victim.steamid = "a".into();
        death.victim.team = Team::Ct;
        death.assister = None;
        let mut revenge = kill;
        revenge.attacker.as_mut().unwrap().steamid = "b".into();
        revenge.assister = None;
        revenge.tick = 340;
        demo.kills = vec![death, revenge];
        let stats = compute_stats(&demo, &[]);
        let a = stats.iter().find(|s| s.steamid == "a").unwrap();
        assert_eq!((a.kast, a.traded_deaths), (100.0, 1));
        assert_eq!(stats.iter().find(|s| s.steamid == "b").unwrap().trade_kills, 1);
        demo.kills[1].tick += 1;
        let stats = compute_stats(&demo, &[]);
        let a = stats.iter().find(|s| s.steamid == "a").unwrap();
        assert_eq!((a.kast, a.traded_deaths), (0.0, 0));
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClutchStats {
    pub round: i32,
    pub side: Team,
    pub versus: usize,
    pub kills: usize,
    pub outcome: String,
}

fn enemy_assister<'a>(kill: &'a KillEvent, round: &RoundInfo) -> Option<&'a Assister> {
    kill.assister.as_ref().filter(|a| a.steamid != kill.victim.steamid &&
        round.roster.get(&a.steamid).copied().unwrap_or(a.team) != round.roster.get(&kill.victim.steamid).copied().unwrap_or(kill.victim.team))
}
