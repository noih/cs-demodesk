//! Small event-context checks. Positive flags describe the kill event, not visibility or intent.
use super::{Check, Definition, Finding, Measurement, State};
use crate::model::{KillEvent, RoundInfo};
use std::collections::BTreeMap;

pub const RULE_IDS: [&str; 3] = ["kill-penetration", "kill-smoke", "kill-blind"];
const NAMES: [&str; 3] = [
    "Surface-penetrating kills",
    "Smoke-path kills",
    "Blind-flagged kills",
];
const DESCRIPTIONS: [&str; 3] = [
    "Counts enemy firearm kills whose event records one or more penetrated surfaces.",
    "Counts enemy firearm kills whose event marks the shot path as passing through smoke.",
    "Counts enemy firearm kills whose event marks the attacker as blinded.",
];
const REASONS: [&str; 3] = [
    "The kill event records penetration through a surface; this does not establish an invisible enemy or absence of legitimate information.",
    "The kill event marks a smoke-crossing shot path; this does not establish an invisible enemy or absence of legitimate information.",
    "The kill event marks the attacker as blinded; this does not establish complete visual impairment or absence of legitimate information.",
];
fn empty(index: usize) -> Check {
    Check {
        definition: Definition {
            id: RULE_IDS[index].into(),
            version: "1-event-context".into(),
            name: NAMES[index].into(),
            description: DESCRIPTIONS[index].into(),
            category: "information-context".into(),

            parameters: serde_json::json!({"positiveEventFlagsOnly":true}),
        },
        state: State::Unavailable,
        reason: "No eligible live-round enemy firearm kills were recorded.".into(),
        reason_code: "noEligibleKills".into(),
        evaluated_samples: 0,
        findings: Vec::new(),
        observations: Vec::new(),
        occurrences: vec![],
        summary: vec![],
        diagnostics: serde_json::Value::Null,
    }
}
pub fn unavailable() -> Vec<Check> {
    (0..3).map(empty).collect()
}

struct Event {
    tick: i32,
    round: i32,
    victim: String,
    surfaces: i32,
    smoke: bool,
    blind: bool,
}

/// Route each raw kill once. Duplicate event flags are merged without multiplying samples or findings.
pub fn evaluate_match(
    players: &[String],
    rounds: &[RoundInfo],
    kills: &[KillEvent],
) -> BTreeMap<String, Vec<Check>> {
    let mut routed: BTreeMap<&str, BTreeMap<(i32, &str), Event>> = players
        .iter()
        .map(|p| (p.as_str(), BTreeMap::new()))
        .collect();
    let mut live = BTreeMap::new();
    for round in rounds {
        // Duplicate round identifiers are ambiguous, not a choice of the nearest interval.
        live.entry(round.round)
            .and_modify(|value| *value = None)
            .or_insert(Some(round));
    }
    for kill in kills {
        let Some(attacker) = &kill.attacker else {
            continue;
        };
        if kill.tick < 0
            || kill.is_freeze_period
            || attacker.steamid.is_empty()
            || kill.victim.steamid.is_empty()
            || attacker.steamid == kill.victim.steamid
            || attacker.team == kill.victim.team
            || crate::aim::group(kill.weapon.strip_prefix("weapon_").unwrap_or(&kill.weapon))
                .is_none()
        {
            continue;
        }
        let Some(round_number) = kill.round.checked_add(1).filter(|r| *r > 0) else {
            continue;
        };
        let Some(Some(round)) = live.get(&round_number) else {
            continue;
        };
        if round.freeze_end_tick < round.start_tick
            || round.end_tick < round.freeze_end_tick
            || kill.tick < round.freeze_end_tick
            || kill.tick > round.end_tick
            || round.roster.get(&attacker.steamid) != Some(&attacker.team)
            || round.roster.get(&kill.victim.steamid) != Some(&kill.victim.team)
        {
            continue;
        }
        let Some(events) = routed.get_mut(attacker.steamid.as_str()) else {
            continue;
        };
        let event = events
            .entry((kill.tick, kill.victim.steamid.as_str()))
            .or_insert_with(|| Event {
                tick: kill.tick,
                round: round_number,
                victim: kill.victim.steamid.clone(),
                surfaces: 0,
                smoke: false,
                blind: false,
            });
        event.surfaces = event.surfaces.max(kill.penetrated);
        event.smoke |= kill.thru_smoke;
        event.blind |= kill.attacker_blind;
    }
    routed
        .into_iter()
        .map(|(player, events)| {
            let mut checks = unavailable();
            for check in &mut checks {
                check.evaluated_samples = events.len();
            }
            for event in events.values() {
                let values = [
                    f64::from(event.surfaces),
                    f64::from(u8::from(event.smoke)),
                    f64::from(u8::from(event.blind)),
                ];
                for (index, value) in values.into_iter().enumerate() {
                    if value <= 0.0 {
                        continue;
                    }
                    checks[index].findings.push(Finding {
                        id: format!(
                            "{}:{player}:{}:{}",
                            RULE_IDS[index], event.tick, event.victim
                        ),
                        group: "engagement-context".into(),
                        round: event.round,
                        start_tick: event.tick,
                        end_tick: event.tick,
                        target_id: event.victim.clone(),
                        reason: REASONS[index].into(),

                        measurements: vec![Measurement {
                            name: if index == 0 {
                                "surfaceCount"
                            } else if index == 1 {
                                "throughSmoke"
                            } else {
                                "attackerBlind"
                            }
                            .into(),
                            value,
                            unit: if index == 0 { "surfaces" } else { "flag" }.into(),
                            threshold: Some(0.0),
                        }],
                    });
                }
            }
            if !events.is_empty() {
                for check in &mut checks {
                    check.state = if check.findings.is_empty() {
                        State::Passed
                    } else {
                        State::Findings
                    };
                    check.reason_code = "experimentalMeasurements".into();
                    check.reason = if check.findings.is_empty() {
                        "No eligible kill events carried this positive context flag."
                    } else {
                        "Positive kill-event context flags matched experimental criteria."
                    }
                    .into();
                }
            }
            (player.to_owned(), checks)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Attacker, Team, Victim};
    fn round() -> RoundInfo {
        RoundInfo {
            round: 1,
            start_tick: 10,
            freeze_end_tick: 20,
            end_tick: 100,
            officially_ended_tick: 110,
            winner: None,
            reason: String::new(),
            roster: BTreeMap::from([("a".into(), Team::Ct), ("b".into(), Team::T)]),
            bomb_planted_tick: None,
            bomb_defused_tick: None,
            bomb_defuser: None,
            bomb_exploded_tick: None,
        }
    }
    fn kill() -> KillEvent {
        KillEvent {
            tick: 50,
            round: 0,
            attacker: Some(Attacker {
                steamid: "a".into(),
                name: String::new(),
                team: Team::Ct,
                health: 100,
                weapon_name: "ak47".into(),
            }),
            victim: Victim {
                steamid: "b".into(),
                name: String::new(),
                team: Team::T,
                weapon_name: "ak47".into(),
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
            distance: 100.0,
            hitgroup: "chest".into(),
            is_freeze_period: false,
        }
    }
    #[test]
    fn positive_contexts_merge_duplicate_kills_and_keep_players_independent() {
        let mut first = kill();
        first.penetrated = 2;
        let mut duplicate = first.clone();
        duplicate.thru_smoke = true;
        duplicate.attacker_blind = true;
        let result = evaluate_match(
            &["a".into(), "b".into()],
            &[round()],
            &[first, duplicate.clone(), duplicate],
        );
        let checks = &result["a"];
        for (index, check) in checks.iter().enumerate() {
            assert_eq!(check.evaluated_samples, 1);
            assert_eq!(check.state, State::Findings);
            assert_eq!(check.findings.len(), 1);
            let finding = &check.findings[0];
            assert_eq!(finding.round, 1);
            assert_eq!(finding.start_tick, 50);
            assert_eq!(finding.end_tick, 50);
            assert_eq!(finding.group, "engagement-context");
            assert_eq!(
                finding.measurements[0].value,
                if index == 0 { 2.0 } else { 1.0 }
            );
        }
        assert!(result["b"].iter().all(|c| c.state == State::Unavailable));
        let mut checks = checks.to_vec();
        super::super::statistics::summarize(&mut checks);
        assert!(checks.iter().all(|c| c.occurrences.len() == 1));
    }
    #[test]
    fn no_flags_produce_zero_occurrences_and_ineligible_kills_never_match() {
        let players = vec!["a".into(), "b".into()];
        let clean = evaluate_match(&players, &[round()], &[kill()]);
        assert!(clean["a"].iter().all(|c| c.state == State::Passed
            && c.findings.is_empty()
            && c.evaluated_samples == 1));
        let mut checks = clean["a"].clone();
        assert_eq!(
            super::super::statistics::summarize(&mut checks),
            State::Passed
        );
        assert!(checks.iter().all(|c| c.occurrences.is_empty()));
        let mut bad = Vec::new();
        for weapon in [
            "world",
            "knife",
            "knife_karambit",
            "taser",
            "hegrenade",
            "inferno",
            "unknown",
        ] {
            let mut k = kill();
            k.weapon = weapon.into();
            bad.push(k);
        }
        let mut k = kill();
        k.round = -1;
        bad.push(k);
        let mut k = kill();
        k.round = 1;
        bad.push(k);
        let mut k = kill();
        k.tick = 19;
        bad.push(k);
        let mut k = kill();
        k.tick = 101;
        bad.push(k);
        let mut k = kill();
        k.is_freeze_period = true;
        bad.push(k);
        let mut k = kill();
        k.attacker = None;
        bad.push(k);
        let mut k = kill();
        k.victim.steamid = "a".into();
        bad.push(k);
        let mut k = kill();
        k.victim.team = Team::Ct;
        bad.push(k);
        let mut k = kill();
        k.victim.steamid = "unknown".into();
        bad.push(k);
        for k in &mut bad {
            k.penetrated = 2;
            k.thru_smoke = true;
            k.attacker_blind = true;
        }
        let result = evaluate_match(&players, &[round()], &bad);
        assert!(result
            .values()
            .flatten()
            .all(|c| c.state == State::Unavailable
                && c.reason_code == "noEligibleKills"
                && c.findings.is_empty()));
    }
}
