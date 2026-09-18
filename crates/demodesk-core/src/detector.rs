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
    pub const POSTHUMOUS: f64 = 1.0;
    pub const POSTHUMOUS_MAX_KILLS: usize = 2;
    pub const LONG_RANGE_DISTANCE: f64 = 35.0;
    pub const LONG_RANGE: f64 = 1.0;
    pub const CLUTCH_WON: [f64; 6] = [0.0, 2.0, 4.0, 7.0, 10.0, 14.0];
    pub const CLUTCH_ATTEMPT_PER_KILL: f64 = 0.5;
    pub const CLUTCH_ATTEMPT_MAX: f64 = 2.0;
    pub const NINJA_DEFUSE: f64 = 4.0;
    pub const PISTOL_ROUND_MULTIPLIER: f64 = 1.15;
    pub const MATCH_POINT_MULTIPLIER: f64 = 1.2;
}

const SCOPED_WEAPONS: &[&str] = &["awp", "ssg08", "scar20", "g3sg1"];

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
        if let Some(round) = demo
            .rounds
            .iter()
            .find(|r| kill.tick >= r.start_tick && kill.tick <= r.officially_ended_tick)
        {
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
    let mut sorted: Vec<&KillEvent> = kills
        .iter()
        .copied()
        .filter(|k| k.tick <= round.end_tick)
        .collect();
    sorted.sort_by_key(|k| k.tick);

    let mut found: Vec<ClutchSituation> = vec![];
    for kill in &sorted {
        if let Some(team) = round.roster.get(&kill.victim.steamid) {
            alive
                .get_mut(team)
                .unwrap()
                .remove(kill.victim.steamid.as_str());
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
        // A T can defend the planted bomb successfully even after dying.
        s.won &= (s.team == Team::T && round.bomb_exploded_tick.is_some())
            || !sorted
                .iter()
                .any(|k| k.victim.steamid == s.player && k.tick < round.end_tick);
        s.kills = sorted
            .iter()
            .filter(|k| {
                k.attacker
                    .as_ref()
                    .map(|a| a.steamid == s.player)
                    .unwrap_or(false)
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
    let dead = kills
        .iter()
        .filter(|k| k.tick <= tick && round.roster.get(&k.victim.steamid) == Some(&enemy))
        .count();
    total.saturating_sub(dead)
}

struct Moment<'a> {
    steamid: SteamId,
    name: String,
    round: &'a RoundInfo,
    kills: Vec<KillEvent>,
    clutch: Option<ClutchSituation>,
    ninja_defuse: bool,
    defused_tick: Option<i32>,
    is_match_point: bool,
}

fn push_moment<'a>(
    moments: &mut Vec<Moment<'a>>,
    current: &mut Vec<KillEvent>,
    attacker: &str,
    round: &'a RoundInfo,
    match_point: bool,
) {
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
        defused_tick: None,
        is_match_point: match_point,
    });
}

fn add_tag(tags: &mut Vec<String>, tag: &str) {
    if !tags.iter().any(|t| t == tag) {
        tags.push(tag.to_string());
    }
}

fn score_multikill(m: &Moment, tick_rate: f64, tags: &mut Vec<String>) -> f64 {
    let kills: Vec<_> = m
        .kills
        .iter()
        .filter(|k| k.tick <= m.round.end_tick)
        .collect();
    let n = kills.len().min(5);
    if n < 2 {
        return 0.0;
    }
    let tag = if n == 5 {
        "ace".to_string()
    } else {
        format!("{n}k")
    };
    add_tag(tags, &tag);
    let mut score = Score::MULTIKILL[n];
    let mut left = 0;
    let mut fastest = 1;
    for right in 0..kills.len() {
        while (kills[right].tick - kills[left].tick) as f64 / tick_rate > Score::FAST_WINDOW_SECONDS
        {
            left += 1;
        }
        fastest = fastest.max(right - left + 1);
    }
    if fastest >= 2 {
        score += Score::FAST_BONUS_PER_KILL * (fastest.min(5) - 1) as f64;
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
            if (1..=Score::LOW_HP_THRESHOLD).contains(&a.health)
                && low_hp_kills < Score::LOW_HP_MAX_KILLS
            {
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

fn score_posthumous(m: &Moment, tags: &mut Vec<String>) -> f64 {
    let kills = m
        .kills
        .iter()
        .filter(|k| {
            k.attacker.as_ref().is_some_and(|a| a.health == 0)
                && matches!(
                    k.weapon.as_str(),
                    "hegrenade" | "inferno" | "molotov" | "incgrenade"
                )
        })
        .count()
        .min(Score::POSTHUMOUS_MAX_KILLS);
    if kills > 0 {
        add_tag(tags, "posthumous");
    }
    kills as f64 * Score::POSTHUMOUS
}

fn score_clutch(m: &Moment, tags: &mut Vec<String>) -> f64 {
    let Some(c) = &m.clutch else { return 0.0 };
    let base = Score::CLUTCH_WON[c.versus.min(5)];
    if c.won {
        add_tag(tags, "clutch");
        base
    } else if c.kills.len() >= 2 {
        add_tag(tags, "clutch-attempt");
        (c.kills.len() as f64 * Score::CLUTCH_ATTEMPT_PER_KILL).min(Score::CLUTCH_ATTEMPT_MAX)
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

fn context_rounds(demo: &DemoData) -> (i32, HashSet<i32>) {
    // ponytail: infer regulation halftime from the roster; assume MR12/MR3 overtime
    // when absent. Parse game-rule settings if custom formats need full support.
    let halftime = demo
        .rounds
        .first()
        .and_then(|first| {
            demo.rounds
                .iter()
                .find(|r| {
                    r.round > 1
                        && r.round <= 16
                        && !first.roster.is_empty()
                        && first
                            .roster
                            .iter()
                            .filter(|(id, side)| {
                                r.roster.get(*id).is_some_and(|current| current != *side)
                            })
                            .count()
                            * 2
                            > first.roster.len()
                })
                .map(|r| r.round - 1)
        })
        .unwrap_or(12);
    let team_of = crate::stats::team_of_player(demo);
    let mut score = [0, 0];
    let mut match_points = HashSet::new();
    for r in &demo.rounds {
        let target = if r.round <= halftime * 2 {
            halftime + 1
        } else {
            halftime + 4 + 3 * ((r.round - halftime * 2 - 1) / 6)
        };
        if score.contains(&(target - 1)) {
            match_points.insert(r.round);
        }
        if let Some(winner) = crate::stats::round_winner_key(r, &team_of) {
            score[if winner == crate::stats::TeamKey::A {
                0
            } else {
                1
            }] += 1;
        }
    }
    (halftime + 1, match_points)
}

fn describe(m: &Moment, tags: &[String]) -> String {
    let mut parts: Vec<String> = vec![];
    if let Some(kt) = tags
        .iter()
        .find(|t| *t == "ace" || (t.len() == 2 && t.ends_with('k')))
    {
        parts.push(kt.to_uppercase());
    } else if m.kills.len() == 1 {
        parts.push("1k".into());
    }
    let hs = m.kills.iter().filter(|k| k.headshot).count();
    let extras: Vec<&str> = tags
        .iter()
        .map(|s| s.as_str())
        .filter(|t| {
            !((t.len() == 2 && t.ends_with('k'))
                || matches!(
                    *t,
                    "ace" | "fast" | "headshot" | "pistol-round" | "match-point"
                ))
        })
        .collect();
    let mut detail: Vec<String> = vec![];
    if hs > 0 {
        detail.push(format!("{hs} HS"));
    }
    detail.extend(extras.iter().map(|s| {
        if *s == "ninja-defuse" {
            "possible ninja defuse".into()
        } else {
            s.to_string()
        }
    }));
    if !detail.is_empty() {
        parts.push(format!("({})", detail.join(", ")));
    }
    if let Some(c) = &m.clutch {
        parts.push(format!(
            "1v{}{}",
            c.versus,
            if c.won && c.team == Team::T && m.round.bomb_exploded_tick.is_some() {
                " bomb defended"
            } else if c.won {
                " won"
            } else {
                " lost"
            }
        ));
    }
    format!("{} — {} · R{}", m.name, parts.join(" "), m.round.round)
}

pub fn detect(demo: &DemoData, opts: &DetectOptions) -> Vec<Highlight> {
    let tick_rate = demo.info.tick_rate;
    let gap_ticks = seconds_to_ticks(opts.cluster_gap_seconds, tick_rate);
    let lead_in = seconds_to_ticks(opts.lead_in_seconds, tick_rate);
    let lead_out = seconds_to_ticks(opts.lead_out_seconds, tick_rate);
    let player_filter: HashSet<&str> = opts.players.iter().map(|s| s.as_str()).collect();
    let name_of = |sid: &str| {
        demo.info
            .players
            .iter()
            .find(|p| p.steamid == sid)
            .map(|p| p.name.clone())
            .unwrap_or_else(|| sid.to_string())
    };

    let per_round = kills_by_round(demo);
    let (second_pistol, match_points) = context_rounds(demo);
    let mut highlights: Vec<Highlight> = vec![];

    for round in &demo.rounds {
        let kills: Vec<&KillEvent> = per_round.get(&round.round).cloned().unwrap_or_default();
        let match_point = match_points.contains(&round.round);

        // 1. Cluster enemy kills per attacker.
        let mut by_attacker: BTreeMap<&str, Vec<&KillEvent>> = BTreeMap::new();
        for k in &kills {
            let Some(a) = &k.attacker else { continue };
            let attacker_team = round.roster.get(&a.steamid).copied().unwrap_or(a.team);
            let victim_team = round
                .roster
                .get(&k.victim.steamid)
                .copied()
                .unwrap_or(k.victim.team);
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
                .filter(|(_, m)| {
                    m.steamid == situation.player
                        && m.kills.iter().any(|k| k.tick >= situation.start_tick)
                })
                .map(|(i, _)| i)
                .collect();
            if let Some(&target) = involved.first() {
                let mut merged: Vec<KillEvent> = involved
                    .iter()
                    .flat_map(|&i| moments[i].kills.clone())
                    .collect();
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
                    defused_tick: None,
                    is_match_point: match_point,
                });
            }
        }

        // Attach the successful defuse to the player's latest moment only.
        if let (Some(defused_tick), Some(defuser)) = (round.bomb_defused_tick, &round.bomb_defuser)
        {
            let ninja = enemies_alive_at(round, &kills, Team::Ct, defused_tick) >= 1;
            if let Some(m) = moments.iter_mut().rev().find(|m| &m.steamid == defuser) {
                m.ninja_defuse = ninja;
                m.defused_tick = Some(defused_tick);
            } else if ninja {
                moments.push(Moment {
                    steamid: defuser.clone(),
                    name: name_of(defuser),
                    round,
                    kills: vec![],
                    clutch: None,
                    ninja_defuse: true,
                    defused_tick: Some(defused_tick),
                    is_match_point: match_point,
                });
            }
        }

        // 4. Score.
        for m in moments {
            if !player_filter.is_empty() && !player_filter.contains(m.steamid.as_str()) {
                continue;
            }
            let mut tags: Vec<String> = vec![];
            if m.kills.iter().any(|k| k.tick > round.end_tick) {
                add_tag(&mut tags, "post-round");
            }
            let mut breakdown = BTreeMap::new();
            let mut score = 0.0;
            for (name, value) in [
                ("multikill", score_multikill(&m, tick_rate, &mut tags)),
                ("specialKills", score_special_kills(&m, &mut tags)),
                ("posthumous", score_posthumous(&m, &mut tags)),
                ("clutch", score_clutch(&m, &mut tags)),
                ("ninjaDefuse", score_ninja(&m, &mut tags)),
            ] {
                if value != 0.0 {
                    breakdown.insert(name.to_string(), round2(value));
                }
                score += value;
            }
            let mut factor = 1.0;
            if round.round == 1 || round.round == second_pistol {
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
            let round_result = m
                .clutch
                .as_ref()
                .filter(|c| round.winner == Some(c.team))
                .and_then(|_| {
                    let death = kills
                        .iter()
                        .find(|k| k.victim.steamid == m.steamid && k.tick < round.end_tick)?;
                    let alive = |sid: &str| {
                        !kills
                            .iter()
                            .any(|k| k.victim.steamid == sid && k.tick < round.end_tick)
                    };
                    let target = death
                        .attacker
                        .as_ref()
                        .map(|a| &a.steamid)
                        .filter(|sid| alive(sid))
                        .or_else(|| round.roster.keys().find(|sid| alive(sid)))?;
                    Some(RoundResultView {
                        from_tick: death.tick,
                        player: target.clone(),
                    })
                });
            let defused_tick = m.defused_tick;
            let first_tick = m
                .kills
                .first()
                .map(|k| k.tick)
                .or(m.clutch.as_ref().map(|c| c.start_tick))
                .or(defused_tick)
                .unwrap_or(round.freeze_end_tick);
            let first_tick = first_tick.min(defused_tick.unwrap_or(first_tick));
            let last_tick = m
                .kills
                .last()
                .map(|k| k.tick)
                .unwrap_or(first_tick)
                .max(defused_tick.unwrap_or(first_tick))
                .max(
                    m.clutch
                        .as_ref()
                        .filter(|c| c.won || round_result.is_some())
                        .map(|_| round.end_tick)
                        .unwrap_or(first_tick),
                );
            let start_tick = (first_tick - lead_in).max(round.freeze_end_tick);
            let end_tick = (last_tick + lead_out).min(round.officially_ended_tick);
            let mut key_moments: Vec<[i32; 2]> = m
                .kills
                .iter()
                .map(|k| {
                    [
                        (k.tick - seconds_to_ticks(5.0, tick_rate)).max(round.freeze_end_tick),
                        (k.tick + seconds_to_ticks(3.0, tick_rate))
                            .min(round.officially_ended_tick),
                    ]
                })
                .collect();
            if let Some(ticks) = demo.damage_ticks.get(&m.steamid) {
                key_moments.extend(
                    ticks
                        .iter()
                        .copied()
                        .filter(|&tick| tick >= start_tick && tick <= end_tick)
                        .map(|tick| {
                            [
                                (tick - seconds_to_ticks(5.0, tick_rate)).max(start_tick),
                                (tick + seconds_to_ticks(3.0, tick_rate)).min(end_tick),
                            ]
                        }),
                );
                key_moments.sort_unstable();
                key_moments.dedup();
            }
            if let Some(view) = &round_result {
                let padding = seconds_to_ticks(2.0, tick_rate);
                key_moments.push([
                    (view.from_tick - padding).max(round.freeze_end_tick),
                    (view.from_tick + padding).min(round.officially_ended_tick),
                ]);
            }
            if let Some(tick) = defused_tick {
                let padding = seconds_to_ticks(2.0, tick_rate);
                key_moments.push([
                    (tick - padding).max(round.freeze_end_tick),
                    (tick + padding).min(round.officially_ended_tick),
                ]);
            } else if m.clutch.as_ref().is_some_and(|c| c.won) || round_result.is_some() {
                let padding = seconds_to_ticks(2.0, tick_rate);
                key_moments.push([
                    (round.end_tick - padding).max(round.freeze_end_tick),
                    (round.end_tick + padding).min(round.officially_ended_tick),
                ]);
            }
            highlights.push(Highlight {
                id: format!("r{}-{}-{}", round.round, m.steamid, first_tick),
                player: HighlightPlayer {
                    steamid: m.steamid.clone(),
                    name: m.name.clone(),
                },
                round: round.round,
                start_tick,
                end_tick,
                anchor_tick: first_tick,
                key_moments,
                round_result,
                score,
                title: describe(&m, &tags),
                tags,
                kills: m.kills,
                breakdown,
            });
        }
    }

    highlights.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then(a.start_tick.cmp(&b.start_tick))
    });
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
            attacker: Some(Attacker {
                steamid: attacker.into(),
                name: attacker.into(),
                team: team(attacker),
                health: 100,
                weapon_name: "AK-47".into(),
            }),
            victim: Victim {
                steamid: victim.into(),
                name: victim.into(),
                team: team(victim),
                weapon_name: "M4A1".into(),
            },
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
        let players = CT
            .iter()
            .chain(TS.iter())
            .map(|id| PlayerInfo {
                name: id.to_string(),
                steamid: id.to_string(),
                team_number: if id.starts_with("ct") { 3 } else { 2 },
                user_id: None,
            })
            .collect();
        DemoData {
            round_metrics: Default::default(),
            info: DemoInfo {
                path: "x.dem".into(),
                map_name: "de_test".into(),
                server_name: String::new(),
                tick_rate: TR,
                players,
            },
            kills,
            rounds,
            activity: Default::default(),
            aim: Default::default(),
            recoil: Default::default(),
            recoil_reference: Default::default(),
            damage: Default::default(),
            damage_ticks: Default::default(),
        }
    }

    fn opts(min_score: f64) -> DetectOptions {
        DetectOptions {
            min_score,
            ..Default::default()
        }
    }

    #[test]
    fn splits_clusters_by_gap_and_ignores_team_kills() {
        let r = round(2, Team::Ct);
        let f = r.freeze_end_tick;
        let d = demo(
            vec![r],
            vec![
                kill(f + 100, "ct1", "t1"),
                kill(f + 200, "ct1", "t2"),
                kill(f + 200 + 30 * 64, "ct1", "t3"),
                kill(f + 200 + 31 * 64, "ct1", "t4"),
                kill(f + 300, "ct2", "ct3"),
            ],
        );
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
        let mut kills: Vec<KillEvent> = TS
            .iter()
            .enumerate()
            .map(|(i, v)| kill(t0 + i as i32 * 32, "ct1", v))
            .collect();
        kills.extend(
            ["t1", "t2", "t3"]
                .iter()
                .enumerate()
                .map(|(i, v)| kill(t0 + 3000 + i as i32 * 640, "ct2", v)),
        );
        let hl = detect(&demo(vec![r], kills), &opts(0.0));
        assert_eq!(hl[0].player.steamid, "ct1");
        assert!(
            hl[0].tags.contains(&"ace".to_string()) && hl[0].tags.contains(&"fast".to_string())
        );
        assert!(
            hl[1].tags.contains(&"3k".to_string()) && !hl[1].tags.contains(&"fast".to_string())
        );
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
        let hl = detect(
            &demo(
                vec![r1.clone(), r2.clone()],
                vec![
                    awp,
                    ak,
                    kill(r1.freeze_end_tick + 100, "ct3", "t3"),
                    kill(r1.freeze_end_tick + 132, "ct3", "t4"),
                    kill(r2.freeze_end_tick + 100, "ct4", "t3"),
                    kill(r2.freeze_end_tick + 132, "ct4", "t4"),
                ],
            ),
            &opts(0.0),
        );
        let find = |id: &str| hl.iter().find(|h| h.player.steamid == id).unwrap();
        assert!(find("ct1").tags.contains(&"noscope".to_string()));
        assert!(!find("ct2").tags.contains(&"noscope".to_string()));
        assert!(find("ct3").tags.contains(&"pistol-round".to_string()));
        assert!(find("ct3").score > find("ct4").score);
    }

    #[test]
    fn nonlethal_damage_is_kept_between_key_kills_without_changing_score() {
        let r = round(2, Team::Ct);
        let b = r.freeze_end_tick;
        let mut d = demo(
            vec![r],
            vec![kill(b + 400, "ct1", "t1"), kill(b + 1600, "ct1", "t2")],
        );
        let before = detect(&d, &opts(0.0));
        d.damage_ticks
            .insert("ct1".into(), vec![b + 1000, b + 9000]);
        let after = detect(&d, &opts(0.0));
        let h = after.iter().find(|h| h.player.steamid == "ct1").unwrap();
        assert_eq!(
            h.score,
            before
                .iter()
                .find(|h| h.player.steamid == "ct1")
                .unwrap()
                .score
        );
        assert!(h.key_moments.contains(&[b + 680, b + 1192]));
        assert_eq!(h.key_moments.len(), 3);
    }

    #[test]
    fn bomb_defense_after_death_scores_clutch_and_keeps_round_result_view() {
        let mut r = round(2, Team::T);
        r.roster
            .retain(|id, _| ["t1", "t2", "ct1", "ct2", "ct3"].contains(&id.as_str()));
        r.reason = "bomb_exploded".into();
        r.bomb_planted_tick = Some(r.end_tick - 2600);
        r.bomb_exploded_tick = Some(r.end_tick);
        let b = r.freeze_end_tick;
        let mut kills = vec![
            kill(b + 100, "t1", "ct1"),
            kill(b + 200, "ct2", "t2"),
            kill(b + 700, "t1", "ct2"),
        ];
        // The same bomb win is a successful clutch while the player survives.
        let surviving = detect(&demo(vec![r.clone()], kills.clone()), &opts(0.0));
        assert!(surviving
            .iter()
            .find(|h| h.player.steamid == "t1")
            .unwrap()
            .tags
            .contains(&"clutch".into()));
        kills.push(kill(r.end_tick - 431, "ct3", "t1"));
        let highlights = detect(&demo(vec![r.clone()], kills.clone()), &opts(0.0));
        let h = highlights
            .iter()
            .find(|h| h.player.steamid == "t1")
            .unwrap();
        assert_eq!(h.kills.len(), 2);
        assert!(h.tags.contains(&"clutch".into()));
        assert_eq!(h.breakdown["clutch"], 4.0);
        assert!(h.title.contains("1v2 bomb defended"));
        assert_eq!(h.key_moments.len(), 4);
        let view = h.round_result.as_ref().unwrap();
        assert_eq!(view.player, "ct3");
        assert_eq!(view.from_tick, r.end_tick - 431);
        assert!(!highlights
            .iter()
            .any(|h| h.tags.contains(&"ninja-defuse".into())));

        r.winner = Some(Team::Ct);
        r.bomb_exploded_tick = None;
        r.reason = "bomb_defused".into();
        let lost = detect(&demo(vec![r], kills), &opts(0.0));
        let h = lost.iter().find(|h| h.player.steamid == "t1").unwrap();
        assert!(!h.tags.contains(&"clutch".into()));
        assert_eq!(h.breakdown.get("clutch").copied().unwrap_or(0.0), 0.0);
        assert!(h.title.contains("1v2 lost"));
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
        let s = find_clutches(&r, &refs)
            .into_iter()
            .find(|s| s.player == "ct1")
            .unwrap();
        assert_eq!((s.versus, s.won, s.kills.len()), (5, true, 5));
        let hl = detect(&demo(vec![r], kills), &opts(0.0));
        let ct1: Vec<_> = hl.iter().filter(|h| h.player.steamid == "ct1").collect();
        assert_eq!(ct1.len(), 1);
        assert!(
            ct1[0].tags.contains(&"ace".to_string()) && ct1[0].tags.contains(&"clutch".to_string())
        );
        assert_eq!(ct1[0].kills.len(), 5);
    }

    #[test]
    fn kill_less_clutch_with_ninja_defuse_and_lost_attempt() {
        let mut r = round(2, Team::Ct);
        r.bomb_planted_tick = Some(r.freeze_end_tick + 500);
        r.bomb_defused_tick = Some(r.freeze_end_tick + 700);
        r.bomb_defuser = Some("ct1".into());
        let b = r.freeze_end_tick;
        let kills: Vec<KillEvent> = ["ct2", "ct3", "ct4", "ct5"]
            .iter()
            .map(|v| kill(b + 100, "t1", v))
            .collect();
        let hl = detect(&demo(vec![r], kills), &opts(0.0));
        let ct1 = hl.iter().find(|h| h.player.steamid == "ct1").unwrap();
        assert!(
            ct1.tags.contains(&"clutch".to_string())
                && ct1.tags.contains(&"ninja-defuse".to_string())
        );
        assert!(ct1.kills.is_empty());

        let r = round(3, Team::T);
        let b = r.freeze_end_tick;
        let mut kills: Vec<KillEvent> = ["ct2", "ct3", "ct4", "ct5"]
            .iter()
            .map(|v| kill(b + 100, "t1", v))
            .collect();
        kills.extend([
            kill(b + 200, "ct1", "t1"),
            kill(b + 300, "ct1", "t2"),
            kill(b + 400, "t3", "ct1"),
        ]);
        let hl = detect(&demo(vec![r], kills), &opts(0.0));
        let ct1 = hl.iter().find(|h| h.player.steamid == "ct1").unwrap();
        assert!(
            ct1.tags.contains(&"clutch-attempt".to_string())
                && !ct1.tags.contains(&"clutch".to_string())
        );
    }

    #[test]
    fn options_filter_top_and_padding() {
        let r = round(2, Team::Ct);
        let b = r.freeze_end_tick;
        let d = demo(
            vec![r],
            vec![
                kill(b + 1000, "ct1", "t1"),
                kill(b + 1032, "ct1", "t2"),
                kill(b + 2000, "ct2", "t3"),
                kill(b + 2032, "ct2", "t4"),
            ],
        );
        let only = detect(
            &d,
            &DetectOptions {
                players: vec!["ct2".into()],
                min_score: 0.0,
                ..Default::default()
            },
        );
        assert_eq!(
            only.iter()
                .map(|h| h.player.steamid.as_str())
                .collect::<Vec<_>>(),
            vec!["ct2"]
        );
        assert_eq!(
            detect(
                &d,
                &DetectOptions {
                    top_n: 1,
                    min_score: 0.0,
                    ..Default::default()
                }
            )
            .len(),
            1
        );
        assert!(detect(&d, &opts(999.0)).is_empty());
        let h = &detect(
            &d,
            &DetectOptions {
                players: vec!["ct1".into()],
                min_score: 0.0,
                ..Default::default()
            },
        )[0];
        assert_eq!(
            (h.start_tick, h.end_tick, h.anchor_tick),
            (b + 1000 - 5 * 64, b + 1032 + 3 * 64, b + 1000)
        );
    }
    #[test]
    fn local_fast_kills_do_not_double_count_overlapping_windows() {
        for (seconds, bonus) in [
            ([0, 20, 23], 1.5),
            ([0, 3, 5], 3.0),
            ([0, 5, 10], 1.5),
            ([0, 6, 13], 1.5),
            ([0, 7, 14], 0.0),
        ] {
            let r = round(2, Team::Ct);
            let kills = seconds
                .iter()
                .zip(TS)
                .map(|(s, victim)| kill(r.freeze_end_tick + 500 + s * 64, "ct1", victim))
                .collect();
            let h = detect(&demo(vec![r], kills), &opts(0.0));
            assert_eq!(h[0].breakdown["multikill"], 5.0 + bonus);
        }
    }

    #[test]
    fn grenade_kills_count_with_distinct_alive_and_posthumous_bonuses() {
        for weapon in ["inferno", "hegrenade"] {
            for health in [0, 1, 20, 21] {
                let r = round(2, Team::Ct);
                let kills = TS[..3]
                    .iter()
                    .enumerate()
                    .map(|(i, victim)| {
                        let mut k = kill(r.freeze_end_tick + 500 + i as i32 * 64, "ct1", victim);
                        k.weapon = weapon.into();
                        k.attacker.as_mut().unwrap().health = health;
                        k
                    })
                    .collect();
                let h = detect(&demo(vec![r], kills), &opts(0.0));
                assert_eq!(h[0].breakdown["multikill"], 8.0);
                assert_eq!(
                    h[0].tags.contains(&"lowhp".into()),
                    (1..=20).contains(&health)
                );
                assert_eq!(
                    h[0].breakdown.get("posthumous").copied().unwrap_or(0.0),
                    if health == 0 { 2.0 } else { 0.0 }
                );
            }
        }
    }

    #[test]
    fn defuse_only_has_correct_full_and_key_windows() {
        let mut r = round(2, Team::Ct);
        r.bomb_defused_tick = Some(r.end_tick);
        r.bomb_defuser = Some("ct1".into());
        let tick = r.end_tick;
        let h = detect(&demo(vec![r], vec![]), &opts(0.0));
        assert_eq!(
            (h[0].start_tick, h[0].end_tick),
            (tick - 5 * 64, tick + 3 * 64)
        );
        assert_eq!(h[0].key_moments, vec![[tick - 2 * 64, tick + 2 * 64]]);
    }

    #[test]
    fn post_round_kill_does_not_turn_four_kills_into_ace() {
        let r = round(2, Team::Ct);
        let mut kills: Vec<_> = TS[..4]
            .iter()
            .enumerate()
            .map(|(i, victim)| kill(r.end_tick - 1000 + i as i32 * 64, "ct1", victim))
            .collect();
        kills.push(kill(r.end_tick + 100, "ct1", "t5"));
        let h = detect(&demo(vec![r], kills), &opts(0.0));
        assert_eq!(h[0].kills.len(), 5);
        assert!(h[0].tags.contains(&"4k".into()));
        assert!(h[0].tags.contains(&"post-round".into()));
        assert!(!h[0].tags.contains(&"ace".into()));
    }

    #[test]
    fn failed_clutch_bonus_tracks_kills_instead_of_initial_opponents() {
        let r = round(2, Team::T);
        let mut kills: Vec<_> = CT[1..]
            .iter()
            .map(|victim| kill(r.freeze_end_tick + 100, "t1", victim))
            .collect();
        kills.extend([
            kill(r.freeze_end_tick + 200, "ct1", "t1"),
            kill(r.freeze_end_tick + 264, "ct1", "t2"),
        ]);
        let h = detect(&demo(vec![r], kills), &opts(0.0));
        let h = h.iter().find(|h| h.player.steamid == "ct1").unwrap();
        assert_eq!(h.breakdown["clutch"], 1.0);
        assert_eq!(h.score, 4.5);
    }

    #[test]
    fn match_point_follows_teams_through_halftime_and_overtime() {
        for half in [12, 15] {
            let mut rounds = Vec::new();
            for n in 1..=2 * half + 6 {
                let mut r = round(n, Team::Ct);
                let swap = n > half && (n <= 2 * half + 3);
                if swap {
                    for side in r.roster.values_mut() {
                        *side = side.enemy();
                    }
                }
                // First team wins first half, second wins second half; first wins OT.
                r.winner = Some(if n <= 2 * half {
                    Team::Ct
                } else if swap {
                    Team::T
                } else {
                    Team::Ct
                });
                rounds.push(r);
            }
            let (pistol, points) = context_rounds(&demo(rounds, vec![]));
            assert_eq!(pistol, half + 1);
            assert!(points.contains(&(half + 1)));
            assert!(!points.contains(&(2 * half + 1)));
            assert!(!points.contains(&(2 * half + 3)));
            assert!(points.contains(&(2 * half + 4)));
        }
    }
    #[test]
    fn successful_defuse_is_attached_to_latest_moment_only() {
        let mut r = round(2, Team::Ct);
        let first = r.freeze_end_tick + 100;
        let last = first + 30 * 64;
        r.bomb_defused_tick = Some(r.end_tick);
        r.bomb_defuser = Some("ct1".into());
        let defuse = r.end_tick;
        let h = detect(
            &demo(
                vec![r],
                vec![kill(first, "ct1", "t1"), kill(last, "ct1", "t2")],
            ),
            &opts(0.0),
        );
        let first_h = h.iter().find(|h| h.anchor_tick == first).unwrap();
        let last_h = h.iter().find(|h| h.anchor_tick == last).unwrap();
        assert_eq!(first_h.key_moments.len(), 1);
        assert_eq!(last_h.key_moments.len(), 2);
        assert!(last_h.end_tick >= defuse);
        assert_eq!(last_h.key_moments[1], [defuse - 128, defuse + 128]);
    }
}
