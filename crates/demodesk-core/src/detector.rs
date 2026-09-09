//! Highlight detection: turns [`DemoData`] into a ranked list of [`Highlight`]s.
//!
//! Per round: group the round's enemy kills by attacker and split them into
//! "moments" wherever two kills are further apart than `cluster_gap_seconds`;
//! attach round-level situations (clutch, ninja defuse); run every scorer;
//! apply context multipliers; pad the tick window; filter, sort, keep top N.

use crate::model::*;
use std::collections::{BTreeMap, HashMap, HashSet};

/// All tunable weights. Scores are additive; multipliers are applied last.
pub struct Score;
impl Score {
    pub const MULTIKILL: [f64; 6] = [0.0, 0.0, 2.0, 5.0, 9.0, 14.0];
    pub const FAST_BONUS_PER_KILL: f64 = 1.5;
    pub const FAST_WINDOW_SECONDS: f64 = 6.0;
    pub const HEADSHOT: f64 = 0.5;
    pub const NOSCOPE: f64 = 2.0;
    pub const WALLBANG: f64 = 1.5;
    pub const THRU_SMOKE: f64 = 1.5;
    pub const BLIND: f64 = 2.0;
    pub const AIRBORNE: f64 = 1.5;
    pub const KNIFE: f64 = 4.0;
    pub const ZEUS: f64 = 3.0;
    pub const LOW_HP_THRESHOLD: i32 = 20;
    pub const LOW_HP: f64 = 1.0;
    pub const LOW_HP_MAX_KILLS: usize = 2;
    pub const LONG_RANGE_DISTANCE: f64 = 35.0;
    pub const LONG_RANGE: f64 = 1.0;
    pub const CLUTCH_WON: [f64; 6] = [0.0, 2.0, 4.0, 7.0, 10.0, 14.0];
    pub const CLUTCH_ATTEMPT_FACTOR: f64 = 0.4;
    pub const NINJA_DEFUSE: f64 = 4.0;
    pub const PISTOL_ROUND_MULTIPLIER: f64 = 1.15;
    pub const MATCH_POINT_MULTIPLIER: f64 = 1.2;
}

const SCOPED_WEAPONS: &[&str] = &["awp", "ssg08", "scar20", "g3sg1"];
const PISTOL_ROUNDS: &[i32] = &[1, 13];

fn is_knife(weapon: &str) -> bool {
    weapon.starts_with("knife") || weapon.starts_with("bayonet")
}

/// Map every kill onto the round whose [start_tick, officially_ended_tick] window contains it.
pub fn kills_by_round(demo: &DemoData) -> HashMap<i32, Vec<&KillEvent>> {
    let mut out: HashMap<i32, Vec<&KillEvent>> = HashMap::new();
    for kill in &demo.kills {
        if kill.is_freeze_period {
            continue;
        }
        if let Some(round) = demo.rounds.iter().find(|r| kill.tick >= r.start_tick && kill.tick <= r.officially_ended_tick) {
            out.entry(round.round).or_default().push(kill);
        }
    }
    out
}

#[derive(Debug, Clone)]
pub struct ClutchSituation {
    pub player: SteamId,
    pub team: Team,
    /// Opponents alive when the player became the last one standing
    pub versus: usize,
    pub start_tick: i32,
    pub won: bool,
    /// Enemy kills the player made after the clutch started
    pub kills: Vec<KillEvent>,
}

/// Replays the round's deaths against the freeze-end roster to find the
/// moment a player becomes the last one alive on their team (1vN).
pub fn find_clutches(round: &RoundInfo, kills: &[&KillEvent]) -> Vec<ClutchSituation> {
    let mut alive: HashMap<Team, HashSet<&str>> = HashMap::new();
    alive.insert(Team::Ct, HashSet::new());
    alive.insert(Team::T, HashSet::new());
    for (sid, team) in &round.roster {
        alive.get_mut(team).unwrap().insert(sid.as_str());
    }
    if alive[&Team::Ct].is_empty() || alive[&Team::T].is_empty() {
        return vec![];
    }
    let mut sorted: Vec<&KillEvent> = kills.iter().copied().filter(|k| k.tick <= round.end_tick).collect();
    sorted.sort_by_key(|k| k.tick);

    let mut found: Vec<ClutchSituation> = vec![];
    for kill in &sorted {
        if let Some(team) = round.roster.get(&kill.victim.steamid) {
            alive.get_mut(team).unwrap().remove(kill.victim.steamid.as_str());
        }
        for team in [Team::Ct, Team::T] {
            let enemies = alive[&team.enemy()].len();
            if alive[&team].len() == 1 && enemies >= 1 {
                let last = *alive[&team].iter().next().unwrap();
                if !found.iter().any(|s| s.player == last) {
                    found.push(ClutchSituation {
                        player: last.to_string(),
                        team,
                        versus: enemies,
                        start_tick: kill.tick,
                        won: round.winner == Some(team),
                        kills: vec![],
                    });
                }
            }
        }
    }
    for s in &mut found {
        s.kills = sorted
            .iter()
            .filter(|k| {
                k.attacker.as_ref().map(|a| a.steamid == s.player).unwrap_or(false)
                    && k.tick >= s.start_tick
                    && round.roster.get(&k.victim.steamid) == Some(&s.team.enemy())
            })
            .map(|k| (*k).clone())
            .collect();
    }
    found
}

/// Number of opponents of `team` still alive at `tick` (based on deaths only).
pub fn enemies_alive_at(round: &RoundInfo, kills: &[&KillEvent], team: Team, tick: i32) -> usize {
    let enemy = team.enemy();
    let total = round.roster.values().filter(|t| **t == enemy).count();
    let dead = kills.iter().filter(|k| k.tick <= tick && round.roster.get(&k.victim.steamid) == Some(&enemy)).count();
    total.saturating_sub(dead)
}

struct Moment<'a> {
    steamid: SteamId,
    name: String,
    round: &'a RoundInfo,
    kills: Vec<KillEvent>,
    clutch: Option<ClutchSituation>,
    ninja_defuse: bool,
    is_match_point: bool,
}

fn push_moment<'a>(moments: &mut Vec<Moment<'a>>, current: &mut Vec<KillEvent>, attacker: &str, round: &'a RoundInfo, match_point: bool) {
    if current.is_empty() {
        return;
    }
    let a = current[0].attacker.clone().unwrap();
    moments.push(Moment {
        steamid: attacker.to_string(),
        name: a.name,
        round,
        kills: std::mem::take(current),
        clutch: None,
        ninja_defuse: false,
        is_match_point: match_point,
    });
}

fn add_tag(tags: &mut Vec<String>, tag: &str) {
    if !tags.iter().any(|t| t == tag) {
        tags.push(tag.to_string());
    }
}

fn score_multikill(m: &Moment, tick_rate: f64, tags: &mut Vec<String>) -> f64 {
    let n = m.kills.len().min(5);
    if n < 2 {
        return 0.0;
    }
    let tag = if n == 5 { "ace".to_string() } else { format!("{n}k") };
    add_tag(tags, &tag);
    let mut score = Score::MULTIKILL[n];
    let span = (m.kills[m.kills.len() - 1].tick - m.kills[0].tick) as f64 / tick_rate;
    if span <= Score::FAST_WINDOW_SECONDS {
        score += Score::FAST_BONUS_PER_KILL * (n as f64 - 1.0);
        add_tag(tags, "fast");
    }
    score
}

fn score_special_kills(m: &Moment, tags: &mut Vec<String>) -> f64 {
    let mut score = 0.0;
    let mut low_hp_kills = 0;
    for k in &m.kills {
        if k.headshot {
            score += Score::HEADSHOT;
            add_tag(tags, "headshot");
        }
        if k.noscope && SCOPED_WEAPONS.contains(&k.weapon.as_str()) {
            score += Score::NOSCOPE;
            add_tag(tags, "noscope");
        }
        if k.penetrated > 0 {
            score += Score::WALLBANG;
            add_tag(tags, "wallbang");
        }
        if k.thru_smoke {
            score += Score::THRU_SMOKE;
            add_tag(tags, "thrusmoke");
        }
        if k.attacker_blind {
            score += Score::BLIND;
            add_tag(tags, "blind");
        }
        if k.attacker_in_air {
            score += Score::AIRBORNE;
            add_tag(tags, "airborne");
        }
        if is_knife(&k.weapon) {
            score += Score::KNIFE;
            add_tag(tags, "knife");
        }
        if k.weapon == "taser" {
            score += Score::ZEUS;
            add_tag(tags, "zeus");
        }
        if let Some(a) = &k.attacker {
            if a.health <= Score::LOW_HP_THRESHOLD && low_hp_kills < Score::LOW_HP_MAX_KILLS {
                score += Score::LOW_HP;
                low_hp_kills += 1;
                add_tag(tags, "lowhp");
            }
        }
        if k.distance >= Score::LONG_RANGE_DISTANCE {
            score += Score::LONG_RANGE;
        }
    }
    score
}

fn score_clutch(m: &Moment, tags: &mut Vec<String>) -> f64 {
    let Some(c) = &m.clutch else { return 0.0 };
    let base = Score::CLUTCH_WON[c.versus.min(5)];
    if c.won {
        add_tag(tags, "clutch");
        base
    } else if c.kills.len() >= 2 {
        add_tag(tags, "clutch-attempt");
        base * Score::CLUTCH_ATTEMPT_FACTOR
    } else {
        0.0
    }
}

fn score_ninja(m: &Moment, tags: &mut Vec<String>) -> f64 {
    if m.ninja_defuse {
        add_tag(tags, "ninja-defuse");
        Score::NINJA_DEFUSE
    } else {
        0.0
    }
}

fn round2(n: f64) -> f64 {
    (n * 100.0).round() / 100.0
}

fn is_match_point(demo: &DemoData, round: &RoundInfo) -> bool {
    let mut ct = 0;
    let mut t = 0;
    for r in &demo.rounds {
        if r.round < round.round {
            match r.winner {
                Some(Team::Ct) => ct += 1,
                Some(Team::T) => t += 1,
                None => {}
            }
        }
    }
    ct == 12 || t == 12
}

fn describe(m: &Moment, tags: &[String]) -> String {
    let mut parts: Vec<String> = vec![];
    if let Some(kt) = tags.iter().find(|t| *t == "ace" || (t.len() == 2 && t.ends_with('k'))) {
        parts.push(kt.to_uppercase());
    } else if m.kills.len() == 1 {
        parts.push("1k".into());
    }
    let hs = m.kills.iter().filter(|k| k.headshot).count();
    let extras: Vec<&str> = tags
        .iter()
        .map(|s| s.as_str())
        .filter(|t| !((t.len() == 2 && t.ends_with('k')) || matches!(*t, "ace" | "fast" | "headshot" | "pistol-round" | "match-point")))
        .collect();
    let mut detail: Vec<String> = vec![];
    if hs > 0 {
        detail.push(format!("{hs} HS"));
    }
    detail.extend(extras.iter().map(|s| s.to_string()));
    if !detail.is_empty() {
        parts.push(format!("({})", detail.join(", ")));
    }
    if let Some(c) = &m.clutch {
        parts.push(format!("1v{}{}", c.versus, if c.won { " won" } else { " lost" }));
    }
    format!("{} — {} · R{}", m.name, parts.join(" "), m.round.round)
}

pub fn detect(demo: &DemoData, opts: &DetectOptions) -> Vec<Highlight> {
    let tick_rate = demo.info.tick_rate;
    let gap_ticks = seconds_to_ticks(opts.cluster_gap_seconds, tick_rate);
    let lead_in = seconds_to_ticks(opts.lead_in_seconds, tick_rate);
    let lead_out = seconds_to_ticks(opts.lead_out_seconds, tick_rate);
    let player_filter: HashSet<&str> = opts.players.iter().map(|s| s.as_str()).collect();
    let name_of = |sid: &str| demo.info.players.iter().find(|p| p.steamid == sid).map(|p| p.name.clone()).unwrap_or_else(|| sid.to_string());

    let per_round = kills_by_round(demo);
    let mut highlights: Vec<Highlight> = vec![];

    for round in &demo.rounds {
        let kills: Vec<&KillEvent> = per_round.get(&round.round).cloned().unwrap_or_default();
        let match_point = is_match_point(demo, round);

        // 1. Cluster enemy kills per attacker.
        let mut by_attacker: BTreeMap<&str, Vec<&KillEvent>> = BTreeMap::new();
        for k in &kills {
            let Some(a) = &k.attacker else { continue };
            let attacker_team = round.roster.get(&a.steamid).copied().unwrap_or(a.team);
            let victim_team = round.roster.get(&k.victim.steamid).copied().unwrap_or(k.victim.team);
            if attacker_team == victim_team {
                continue;
            }
            by_attacker.entry(a.steamid.as_str()).or_default().push(k);
        }
        let mut moments: Vec<Moment> = vec![];
        for (attacker, list) in by_attacker {
            let mut list = list;
            list.sort_by_key(|k| k.tick);
            let mut current: Vec<KillEvent> = vec![];
            for k in list {
                if let Some(prev) = current.last() {
                    if k.tick - prev.tick > gap_ticks {
                        push_moment(&mut moments, &mut current, attacker, round, match_point);
                    }
                }
                current.push(k.clone());
            }
            push_moment(&mut moments, &mut current, attacker, round, match_point);
        }

        // 2. Clutches: every cluster of the clutcher from the clutch start onwards becomes one moment.
        for situation in find_clutches(round, &kills) {
            let involved: Vec<usize> = moments
                .iter()
                .enumerate()
                .filter(|(_, m)| m.steamid == situation.player && m.kills.iter().any(|k| k.tick >= situation.start_tick))
                .map(|(i, _)| i)
                .collect();
            if let Some(&target) = involved.first() {
                let mut merged: Vec<KillEvent> = involved.iter().flat_map(|&i| moments[i].kills.clone()).collect();
                merged.sort_by_key(|k| k.tick);
                moments[target].kills = merged;
                moments[target].clutch = Some(situation);
                for &i in involved.iter().skip(1).rev() {
                    moments.remove(i);
                }
            } else if situation.won {
                moments.push(Moment {
                    steamid: situation.player.clone(),
                    name: name_of(&situation.player),
                    round,
                    kills: vec![],
                    clutch: Some(situation),
                    ninja_defuse: false,
                    is_match_point: match_point,
                });
            }
        }

        // 3. Ninja defuse: bomb defused while at least one T is still alive.
        if let (Some(defused_tick), Some(defuser)) = (round.bomb_defused_tick, &round.bomb_defuser) {
            if enemies_alive_at(round, &kills, Team::Ct, defused_tick) >= 1 {
                if let Some(m) = moments.iter_mut().rev().find(|m| &m.steamid == defuser) {
                    m.ninja_defuse = true;
                } else {
                    moments.push(Moment {
                        steamid: defuser.clone(),
                        name: name_of(defuser),
                        round,
                        kills: vec![],
                        clutch: None,
                        ninja_defuse: true,
                        is_match_point: match_point,
                    });
                }
            }
        }

        // 4. Score.
        for m in moments {
            if !player_filter.is_empty() && !player_filter.contains(m.steamid.as_str()) {
                continue;
            }
            let mut tags: Vec<String> = vec![];
            let mut breakdown = BTreeMap::new();
            let mut score = 0.0;
            for (name, value) in [
                ("multikill", score_multikill(&m, tick_rate, &mut tags)),
                ("specialKills", score_special_kills(&m, &mut tags)),
                ("clutch", score_clutch(&m, &mut tags)),
                ("ninjaDefuse", score_ninja(&m, &mut tags)),
            ] {
                if value != 0.0 {
                    breakdown.insert(name.to_string(), round2(value));
                }
                score += value;
            }
            let mut factor = 1.0;
            if PISTOL_ROUNDS.contains(&round.round) {
                factor *= Score::PISTOL_ROUND_MULTIPLIER;
                add_tag(&mut tags, "pistol-round");
            }
            if m.is_match_point {
                factor *= Score::MATCH_POINT_MULTIPLIER;
                add_tag(&mut tags, "match-point");
            }
            if factor != 1.0 {
                breakdown.insert("context".into(), round2(score * (factor - 1.0)));
                score *= factor;
            }
            let score = round2(score);
            if score < opts.min_score {
                continue;
            }
            let first_tick = m.kills.first().map(|k| k.tick).or(m.clutch.as_ref().map(|c| c.start_tick)).unwrap_or(round.freeze_end_tick);
            let last_tick = m.kills.last().map(|k| k.tick).unwrap_or(if m.clutch.is_some() { round.end_tick } else { first_tick });
            let start_tick = (first_tick - lead_in).max(round.freeze_end_tick);
            let end_tick = (last_tick + lead_out).min(round.officially_ended_tick);
            highlights.push(Highlight {
                id: format!("r{}-{}-{}", round.round, m.steamid, first_tick),
                player: HighlightPlayer { steamid: m.steamid.clone(), name: m.name.clone() },
                round: round.round,
                start_tick,
                end_tick,
                anchor_tick: first_tick,
                score,
                title: describe(&m, &tags),
                tags,
                kills: m.kills,
                breakdown,
            });
        }
    }

    highlights.sort_by(|a, b| b.score.total_cmp(&a.score).then(a.start_tick.cmp(&b.start_tick)));
    highlights.truncate(opts.top_n);
    highlights
}

#[cfg(test)]
mod tests {
    use super::*;

    const CT: [&str; 5] = ["ct1", "ct2", "ct3", "ct4", "ct5"];
    const TS: [&str; 5] = ["t1", "t2", "t3", "t4", "t5"];
    const TR: f64 = 64.0;

    fn round(n: i32, winner: Team) -> RoundInfo {
        let base = (n - 1) * 10_000;
        let mut roster = BTreeMap::new();
        for id in CT {
            roster.insert(id.to_string(), Team::Ct);
        }
        for id in TS {
            roster.insert(id.to_string(), Team::T);
        }
        RoundInfo {
            round: n,
            start_tick: base,
            freeze_end_tick: base + 1000,
            end_tick: base + 8000,
            officially_ended_tick: base + 8500,
            winner: Some(winner),
            reason: "test".into(),
            roster,
            bomb_planted_tick: None,
            bomb_defused_tick: None,
            bomb_defuser: None,
            bomb_exploded_tick: None,
        }
    }

    fn team(id: &str) -> Team {
        if id.starts_with("ct") {
            Team::Ct
        } else {
            Team::T
        }
    }

    fn kill(tick: i32, attacker: &str, victim: &str) -> KillEvent {
        KillEvent {
            tick,
            round: 0,
            attacker: Some(Attacker { steamid: attacker.into(), name: attacker.into(), team: team(attacker), health: 100, weapon_name: "AK-47".into() }),
            victim: Victim { steamid: victim.into(), name: victim.into(), team: team(victim), weapon_name: "M4A1".into() },
            assister: None,
            weapon: "ak47".into(),
            headshot: false,
            noscope: false,
            penetrated: 0,
            thru_smoke: false,
            attacker_blind: false,
            attacker_in_air: false,
            assisted_flash: false,
            distance: 10.0,
            hitgroup: "chest".into(),
            is_freeze_period: false,
        }
    }

    fn demo(rounds: Vec<RoundInfo>, kills: Vec<KillEvent>) -> DemoData {
        let players = CT.iter().chain(TS.iter()).map(|id| PlayerInfo { name: id.to_string(), steamid: id.to_string(), team_number: if id.starts_with("ct") { 3 } else { 2 }, user_id: None }).collect();
        DemoData { round_metrics: Default::default(), info: DemoInfo { path: "x.dem".into(), map_name: "de_test".into(), server_name: String::new(), tick_rate: TR, players }, kills, rounds, activity: Default::default(), aim: Default::default(), recoil: Default::default(), damage: Default::default() }
    }

    fn opts(min_score: f64) -> DetectOptions {
        DetectOptions { min_score, ..Default::default() }
    }

    #[test]
    fn splits_clusters_by_gap_and_ignores_team_kills() {
        let r = round(2, Team::Ct);
        let f = r.freeze_end_tick;
        let d = demo(vec![r], vec![kill(f + 100, "ct1", "t1"), kill(f + 200, "ct1", "t2"), kill(f + 200 + 30 * 64, "ct1", "t3"), kill(f + 200 + 31 * 64, "ct1", "t4"), kill(f + 300, "ct2", "ct3")]);
        let hl = detect(&d, &opts(0.0));
        let ct1: Vec<_> = hl.iter().filter(|h| h.player.steamid == "ct1").collect();
        assert_eq!(ct1.len(), 2);
        assert!(ct1.iter().all(|h| h.tags.contains(&"2k".to_string())));
        assert!(hl.iter().all(|h| h.player.steamid != "ct2"));
    }

    #[test]
    fn ace_beats_3k_and_fast_tag() {
        let r = round(2, Team::Ct);
        let t0 = r.freeze_end_tick + 500;
        let mut kills: Vec<KillEvent> = TS.iter().enumerate().map(|(i, v)| kill(t0 + i as i32 * 32, "ct1", v)).collect();
        kills.extend(["t1", "t2", "t3"].iter().enumerate().map(|(i, v)| kill(t0 + 3000 + i as i32 * 640, "ct2", v)));
        let hl = detect(&demo(vec![r], kills), &opts(0.0));
        assert_eq!(hl[0].player.steamid, "ct1");
        assert!(hl[0].tags.contains(&"ace".to_string()) && hl[0].tags.contains(&"fast".to_string()));
        assert!(hl[1].tags.contains(&"3k".to_string()) && !hl[1].tags.contains(&"fast".to_string()));
        assert!(hl[0].score > hl[1].score);
    }

    #[test]
    fn noscope_only_for_scoped_weapons_and_pistol_multiplier() {
        let r1 = round(1, Team::Ct);
        let r2 = round(2, Team::Ct);
        let mut awp = kill(r2.freeze_end_tick + 100, "ct1", "t1");
        awp.weapon = "awp".into();
        awp.noscope = true;
        awp.headshot = true;
        let mut ak = kill(r2.freeze_end_tick + 100, "ct2", "t2");
        ak.noscope = true;
        let hl = detect(&demo(vec![r1.clone(), r2.clone()], vec![awp, ak, kill(r1.freeze_end_tick + 100, "ct3", "t3"), kill(r1.freeze_end_tick + 132, "ct3", "t4"), kill(r2.freeze_end_tick + 100, "ct4", "t3"), kill(r2.freeze_end_tick + 132, "ct4", "t4")]), &opts(0.0));
        let find = |id: &str| hl.iter().find(|h| h.player.steamid == id).unwrap();
        assert!(find("ct1").tags.contains(&"noscope".to_string()));
        assert!(!find("ct2").tags.contains(&"noscope".to_string()));
        assert!(find("ct3").tags.contains(&"pistol-round".to_string()));
        assert!(find("ct3").score > find("ct4").score);
    }

    #[test]
    fn clutch_merges_clusters_and_scores_won() {
        let r = round(2, Team::Ct);
        let b = r.freeze_end_tick;
        let kills = vec![
            kill(b + 100, "t1", "ct2"),
            kill(b + 200, "t1", "ct3"),
            kill(b + 300, "t2", "ct4"),
            kill(b + 400, "t3", "ct5"),
            kill(b + 500, "ct1", "t1"),
            kill(b + 600, "ct1", "t2"),
            kill(b + 600 + 40 * 64, "ct1", "t3"),
            kill(b + 700 + 40 * 64, "ct1", "t4"),
            kill(b + 800 + 40 * 64, "ct1", "t5"),
        ];
        let refs: Vec<&KillEvent> = kills.iter().collect();
        let s = find_clutches(&r, &refs).into_iter().find(|s| s.player == "ct1").unwrap();
        assert_eq!((s.versus, s.won, s.kills.len()), (5, true, 5));
        let hl = detect(&demo(vec![r], kills), &opts(0.0));
        let ct1: Vec<_> = hl.iter().filter(|h| h.player.steamid == "ct1").collect();
        assert_eq!(ct1.len(), 1);
        assert!(ct1[0].tags.contains(&"ace".to_string()) && ct1[0].tags.contains(&"clutch".to_string()));
        assert_eq!(ct1[0].kills.len(), 5);
    }

    #[test]
    fn kill_less_clutch_with_ninja_defuse_and_lost_attempt() {
        let mut r = round(2, Team::Ct);
        r.bomb_planted_tick = Some(1500);
        r.bomb_defused_tick = Some(1700);
        r.bomb_defuser = Some("ct1".into());
        let b = r.freeze_end_tick;
        let kills: Vec<KillEvent> = ["ct2", "ct3", "ct4", "ct5"].iter().map(|v| kill(b + 100, "t1", v)).collect();
        let hl = detect(&demo(vec![r], kills), &opts(0.0));
        let ct1 = hl.iter().find(|h| h.player.steamid == "ct1").unwrap();
        assert!(ct1.tags.contains(&"clutch".to_string()) && ct1.tags.contains(&"ninja-defuse".to_string()));
        assert!(ct1.kills.is_empty());

        let r = round(3, Team::T);
        let b = r.freeze_end_tick;
        let mut kills: Vec<KillEvent> = ["ct2", "ct3", "ct4", "ct5"].iter().map(|v| kill(b + 100, "t1", v)).collect();
        kills.extend([kill(b + 200, "ct1", "t1"), kill(b + 300, "ct1", "t2"), kill(b + 400, "t3", "ct1")]);
        let hl = detect(&demo(vec![r], kills), &opts(0.0));
        let ct1 = hl.iter().find(|h| h.player.steamid == "ct1").unwrap();
        assert!(ct1.tags.contains(&"clutch-attempt".to_string()) && !ct1.tags.contains(&"clutch".to_string()));
    }

    #[test]
    fn options_filter_top_and_padding() {
        let r = round(2, Team::Ct);
        let b = r.freeze_end_tick;
        let d = demo(vec![r], vec![kill(b + 1000, "ct1", "t1"), kill(b + 1032, "ct1", "t2"), kill(b + 2000, "ct2", "t3"), kill(b + 2032, "ct2", "t4")]);
        let only = detect(&d, &DetectOptions { players: vec!["ct2".into()], min_score: 0.0, ..Default::default() });
        assert_eq!(only.iter().map(|h| h.player.steamid.as_str()).collect::<Vec<_>>(), vec!["ct2"]);
        assert_eq!(detect(&d, &DetectOptions { top_n: 1, min_score: 0.0, ..Default::default() }).len(), 1);
        assert!(detect(&d, &opts(999.0)).is_empty());
        let h = &detect(&d, &DetectOptions { players: vec!["ct1".into()], min_score: 0.0, ..Default::default() })[0];
        assert_eq!((h.start_tick, h.end_tick, h.anchor_tick), (b + 1000 - 5 * 64, b + 1032 + 3 * 64, b + 1000));
    }
}
