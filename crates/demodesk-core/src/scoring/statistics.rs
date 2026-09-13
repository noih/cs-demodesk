//! Per-rule occurrence counts, independent of other rules.
use super::{Check, Measurement, Occurrence, State};

/// Count each rule's qualifying findings independently. Inclusive overlapping
/// intervals merge only within the same round and target. Unknown/failed coverage
/// remains unknown/failed; absence of evaluable data is not a clean result.
pub fn summarize(checks: &mut [Check]) -> State {
    for check in checks.iter_mut() {
        check.occurrences.clear();
        if matches!(check.state, State::Failed | State::Unavailable) {
            continue;
        }
        let mut findings: Vec<_> = check.findings.iter().collect();
        findings.sort_by(|a, b| {
            (a.round, &a.target_id, a.start_tick, a.end_tick, &a.id).cmp(&(
                b.round,
                &b.target_id,
                b.start_tick,
                b.end_tick,
                &b.id,
            ))
        });
        for finding in findings {
            if let Some(event) = check.occurrences.last_mut() {
                if event.round == finding.round
                    && event.target_id == finding.target_id
                    && finding.start_tick <= event.end_tick
                {
                    event.end_tick = event.end_tick.max(finding.end_tick);
                    event.source_ids.push(finding.id.clone());
                    event.measurements.extend(finding.measurements.clone());
                    continue;
                }
            }
            check.occurrences.push(Occurrence {
                id: String::new(),
                round: finding.round,
                start_tick: finding.start_tick,
                end_tick: finding.end_tick,
                target_id: finding.target_id.clone(),
                source_ids: vec![finding.id.clone()],
                measurements: finding.measurements.clone(),
            });
        }
        // Present events chronologically after grouping by target for correct
        // transitive overlap, including interleaved tracks of different targets.
        check.occurrences.sort_by(|a, b| {
            (a.round, a.start_tick, a.end_tick, &a.target_id).cmp(&(
                b.round,
                b.start_tick,
                b.end_tick,
                &b.target_id,
            ))
        });
        for event in &mut check.occurrences {
            event.id = format!(
                "{}:{}:{}:{}:{}",
                check.definition.id, event.round, event.start_tick, event.end_tick, event.target_id
            );
            event.source_ids.sort();
            event.source_ids.dedup();
            // Keep every distinct raw value. There is no averaged or synthetic
            // measurement standing in for disagreeing source findings.
            event.measurements.sort_by(measurement_order);
            event.measurements.dedup_by(|a, b| {
                a.name == b.name
                    && a.unit == b.unit
                    && a.value.to_bits() == b.value.to_bits()
                    && a.threshold.map(f64::to_bits) == b.threshold.map(f64::to_bits)
            });
        }
        check.state = if check.occurrences.is_empty() {
            State::Passed
        } else {
            State::Findings
        };
    }
    if checks.iter().any(|check| check.state == State::Failed) {
        State::Failed
    } else if checks.iter().any(|check| check.state == State::Findings) {
        State::Findings
    } else if checks.is_empty() || checks.iter().any(|check| check.state == State::Unavailable) {
        State::Unavailable
    } else {
        State::Passed
    }
}

fn measurement_order(a: &Measurement, b: &Measurement) -> std::cmp::Ordering {
    (&a.name, &a.unit)
        .cmp(&(&b.name, &b.unit))
        .then_with(|| a.value.total_cmp(&b.value))
        .then_with(|| match (a.threshold, b.threshold) {
            (None, None) => std::cmp::Ordering::Equal,
            (None, Some(_)) => std::cmp::Ordering::Less,
            (Some(_), None) => std::cmp::Ordering::Greater,
            (Some(a), Some(b)) => a.total_cmp(&b),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scoring::{Definition, Finding};
    fn finding(id: &str, target: &str, start: i32, end: i32, value: f64) -> Finding {
        Finding {
            id: id.into(),
            group: "shared".into(),
            round: 1,
            start_tick: start,
            end_tick: end,
            target_id: target.into(),
            reason: "measured".into(),

            measurements: vec![Measurement {
                name: "speed".into(),
                value,
                unit: "deg/s".into(),
                threshold: Some(180.),
            }],
        }
    }
    fn check(id: &str, findings: Vec<Finding>) -> Check {
        Check {
            definition: Definition {
                id: id.into(),
                version: "test".into(),
                name: id.into(),
                description: String::new(),
                category: "same-category".into(),

                parameters: serde_json::Value::Null,
            },
            state: State::Findings,
            reason: String::new(),
            reason_code: String::new(),
            evaluated_samples: 10,
            findings,
            occurrences: vec![],
            observations: vec![],
            summary: vec![],
            diagnostics: serde_json::Value::Null,
        }
    }
    #[test]
    fn transitive_overlap_keeps_targets_rounds_and_rules_independent() {
        let mut second_round = finding("round-two", "a", 10, 20, 4.);
        second_round.round = 2;
        let findings = vec![
            finding("a-first", "a", 10, 15, 1.),
            finding("b", "b", 12, 30, 2.),
            finding("a-last", "a", 19, 24, 3.),
            finding("a-bridge", "a", 15, 20, 1.),
            finding("a-gap", "a", 25, 28, 5.),
            second_round,
        ];
        let mut checks = vec![check("one", findings.clone()), check("two", findings)];
        assert_eq!(summarize(&mut checks), State::Findings);
        for c in &checks {
            assert_eq!(c.occurrences.len(), 4);
            let event = &c.occurrences[0];
            assert_eq!(
                (event.start_tick, event.end_tick, event.target_id.as_str()),
                (10, 24, "a")
            );
            assert_eq!(event.source_ids, vec!["a-bridge", "a-first", "a-last"]);
            assert_eq!(
                event
                    .measurements
                    .iter()
                    .map(|m| m.value)
                    .collect::<Vec<_>>(),
                vec![1., 3.]
            );
        }
        let before = serde_json::to_value(&checks).unwrap();
        checks.iter_mut().for_each(|c| c.findings.reverse());
        summarize(&mut checks);
        for (i, c) in checks.iter().enumerate() {
            assert_eq!(
                serde_json::to_value(&c.occurrences).unwrap(),
                before[i]["occurrences"]
            );
        }
    }
    #[test]
    fn findings_count_but_unknown_and_failed_stay_distinct() {
        let f = finding("zero", "target", 1, 1, 10.);
        let mut checks = vec![
            check("zero", vec![f.clone()]),
            check("unknown", vec![f.clone()]),
            check("failed", vec![f]),
        ];
        checks[1].state = State::Unavailable;
        checks[2].state = State::Failed;
        assert_eq!(summarize(&mut checks), State::Failed);
        assert_eq!(checks[0].occurrences.len(), 1);
        assert!(checks[1].occurrences.is_empty());
        assert!(checks[2].occurrences.is_empty());
        assert_eq!(checks[1].state, State::Unavailable);
        assert_eq!(checks[2].state, State::Failed);
        checks.remove(2);
        assert_eq!(summarize(&mut checks), State::Findings);
        checks[0].findings.clear();
        assert_eq!(summarize(&mut checks), State::Unavailable);
        checks.remove(1);
        assert_eq!(summarize(&mut checks), State::Passed);
        assert_eq!(summarize(&mut []), State::Unavailable);
    }
}
