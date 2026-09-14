use super::*;
// Producers run once; these rules only project their shared measurements.
pub const ENABLED: &[fn(&Context<'_>) -> Check] = &[snap, linear, tracking, view_angles];
fn view_angles(context: &Context<'_>) -> Check {
    context
        .prepared_view
        .cloned()
        .unwrap_or_else(super::view_angles::unavailable)
}
#[derive(Clone, Copy)]
enum AimRule {
    Snap,
    Linear,
    Tracking,
}
impl AimRule {
    fn id(self) -> &'static str {
        match self {
            Self::Snap => "aim-snap",
            Self::Linear => "aim-linear-acquisition",
            Self::Tracking => "aim-fixed-tracking",
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::Snap => "瞬間對準",
            Self::Linear => "快速直線對準",
            Self::Tracking => "持續固定點追蹤",
        }
    }
    fn reason(self) -> &'static str {
        match self {
            Self::Snap => {
                "Instant acquisition followed by a sustained hold on an enemy body point."
            }
            Self::Linear => {
                "Rapid straight acquisition followed by a sustained hold on an enemy body point."
            }
            Self::Tracking => "Sustained tracking of a fixed body point.",
        }
    }
    fn accepts(self, e: &crosshair_lock::Evidence, p: &crosshair_lock::Parameters) -> bool {
        match self {
            Self::Snap => {
                e.rapid_acquisition
                    && e.acquisition_speed >= p.snap_speed
                    && e.follow_seconds >= p.min_follow_seconds
            }
            Self::Linear => {
                e.rapid_acquisition
                    && e.acquisition_speed < p.snap_speed
                    && e.follow_seconds >= p.min_follow_seconds
            }
            Self::Tracking => e.sustained_follow,
        }
    }
}
fn snap(c: &Context<'_>) -> Check {
    project(c.prepared_body, AimRule::Snap)
}
fn linear(c: &Context<'_>) -> Check {
    project(c.prepared_body, AimRule::Linear)
}
fn tracking(c: &Context<'_>) -> Check {
    project(c.prepared_body, AimRule::Tracking)
}
pub(super) fn outcome(
    context: &Context<'_>,
    parameters: &crosshair_lock::Parameters,
) -> Option<anyhow::Result<crosshair_lock::Report>> {
    if let Some(report) = context.prepared_body {
        return Some(report.cloned().map_err(|e| anyhow::anyhow!("{e}")));
    }
    if context.body_journal.is_none() && !context.measurements.contains_key(crosshair_lock::RULE_ID)
    {
        return None;
    }
    Some((|| {
        if let Some(file) = context.body_journal {
            crosshair_lock::evaluate_journal(
                crate::analysis::body_journal::reader(file)?,
                context.demo_fingerprint,
                context.player_id,
                parameters,
            )
        } else {
            let input: crosshair_lock::Input =
                serde_json::from_value(context.measurements[crosshair_lock::RULE_ID].clone())?;
            anyhow::ensure!(
                input.demo_fingerprint == context.demo_fingerprint
                    && input.player_id == context.player_id,
                "measurement source/player mismatch"
            );
            crosshair_lock::evaluate(&input, parameters)
        }
    })())
}
fn project(outcome: Option<Result<&crosshair_lock::Report, &str>>, rule: AimRule) -> Check {
    let p = crosshair_lock::Parameters::default();
    let mut check = Check {
        definition: Definition {
            id: rule.id().into(),
            version: if matches!(rule, AimRule::Tracking) {
                "experimental-1"
            } else {
                "experimental-2"
            }
            .into(),
            name: rule.name().into(),
            description: rule.reason().into(),
            category: "aim-assistance".into(),

            parameters: serde_json::to_value(&p).expect("finite parameters"),
        },
        state: State::Unavailable,
        reason: "Measured body-point tracks are not available.".into(),
        reason_code: "missing".into(),
        evaluated_samples: 0,
        findings: vec![],
        observations: vec![],
        occurrences: vec![],
        summary: vec![],
        diagnostics: serde_json::Value::Null,
    };
    let Some(outcome) = outcome else { return check };
    let report = match outcome {
        Ok(report) => report,
        Err(error) => {
            check.state = State::Failed;
            check.reason_code = "failed".into();
            check.reason = error.into();
            return check;
        }
    };
    let selected: Vec<_> = report
        .evidence
        .iter()
        .enumerate()
        .filter(|(_, e)| rule.accepts(e, &p))
        .collect();
    check.findings = selected
        .iter()
        .map(|(index, e)| {
            let mut measurements = vec![Measurement {
                name: "error".into(),
                value: e.max_error_degrees,
                unit: "deg".into(),
                threshold: Some(p.max_error_degrees),
            }];
            if matches!(rule, AimRule::Tracking) {
                measurements.extend([
                    Measurement {
                        name: "duration".into(),
                        value: e.follow_seconds,
                        unit: "s".into(),
                        threshold: Some(p.min_follow_seconds),
                    },
                    Measurement {
                        name: "targetTravel".into(),
                        value: e.follow_travel_degrees,
                        unit: "deg".into(),
                        threshold: Some(p.min_follow_travel_degrees),
                    },
                ]);
            } else {
                measurements.push(Measurement {
                    name: "speed".into(),
                    value: e.acquisition_speed,
                    unit: "deg/s".into(),
                    threshold: Some(if matches!(rule, AimRule::Snap) {
                        p.snap_speed
                    } else {
                        p.min_acquisition_speed
                    }),
                });
                if matches!(rule, AimRule::Linear) {
                    measurements.push(Measurement {
                        name: "duration".into(),
                        value: e.follow_seconds,
                        unit: "s".into(),
                        threshold: Some(p.min_follow_seconds),
                    });
                    measurements.push(Measurement {
                        name: "straightness".into(),
                        value: e.straightness,
                        unit: String::new(),
                        threshold: Some(p.min_straightness),
                    });
                }
            }
            Finding {
                id: format!("{}-{index}", rule.id()),
                group: "aim-lock".into(),
                round: e.round,
                start_tick: e.start_tick,
                end_tick: e.end_tick,
                target_id: e.target_id.clone(),
                reason: rule.reason().into(),

                measurements,
            }
        })
        .collect();
    check.evaluated_samples = report.evaluated_samples;
    check.state = if report.evaluated_samples == 0 {
        State::Unavailable
    } else if check.findings.is_empty() {
        State::Passed
    } else {
        State::Findings
    };
    check.reason_code = if report.evaluated_samples == 0 {
        "missing"
    } else {
        "experimentalMeasurements"
    }
    .into();
    check.reason =
        "Experimental behavior measurements; not an independently validated cheat classifier."
            .into();
    let evidence: Vec<_> = selected.iter().map(|(_, e)| *e).collect();
    check.diagnostics = serde_json::json!({"ruleId":rule.id(),"ruleVersion":check.definition.version,
        "demoFingerprint":report.demo_fingerprint,"playerId":report.player_id,"measurementSource":report.measurement_source,
        "tickRate":report.tick_rate,"sampleStepTicks":report.sample_step_ticks,"angularResolutionDegrees":report.angular_resolution_degrees,
        "evaluatedSamples":report.evaluated_samples,"parameters":check.definition.parameters,"evidence":evidence});
    check
}

/// All enabled rules operate on the match. The body journal is decoded once.
pub fn evaluate_match(
    fingerprint: &str,
    players: &[String],
    inputs: &history::Inputs,
    legacy: &BTreeMap<String, history::Inputs>,
) -> BTreeMap<String, Vec<Check>> {
    let parameters = crosshair_lock::Parameters::default();
    let reports = inputs.body_journal.as_ref().map(|file| {
        crate::analysis::body_journal::reader(file).and_then(|reader| {
            crosshair_lock::evaluate_journal_match(reader, fingerprint, players, &parameters)
        })
    });
    let error = reports
        .as_ref()
        .and_then(|r| r.as_ref().err())
        .map(|e| format!("{e:#}"));
    players
        .iter()
        .map(|player| {
            let prepared_body = if let Some(error) = &error {
                Some(Err(error.as_str()))
            } else {
                reports
                    .as_ref()
                    .and_then(|r| r.as_ref().ok())
                    .and_then(|r| r.get(player))
                    .map(Ok)
            };
            let context = Context {
                demo_fingerprint: fingerprint,
                player_id: player,
                measurements: legacy.get(player).map_or(&inputs.values, |i| &i.values),
                body_journal: None,
                prepared_body,
                prepared_view: None,
                prepared_engagement: None,
            };
            (player.clone(), evaluate(&context))
        })
        .collect()
}

#[cfg(test)]
mod split_tests {
    use super::*;
    fn report(views: Vec<(f64, f64)>) -> crosshair_lock::Report {
        let samples = views
            .into_iter()
            .enumerate()
            .map(|(tick, (yaw, target))| {
                let a = target.to_radians();
                crosshair_lock::Sample {
                    tick: tick as i32,
                    eye: [0.; 3],
                    view: [0., yaw],
                    target: [100. * a.cos(), 100. * a.sin(), 0.],
                    obstruction: crosshair_lock::Obstruction::Unknown,
                }
            })
            .collect();
        crosshair_lock::evaluate(
            &crosshair_lock::Input {
                demo_fingerprint: "fixture".into(),
                player_id: "subject".into(),
                measurement_source: "synthetic".into(),
                tick_rate: 64.,
                sample_step_ticks: 1,
                angular_resolution_degrees: 0.01,
                tracks: vec![crosshair_lock::Track {
                    round: 1,
                    target_id: "enemy".into(),
                    point_id: "fixed".into(),
                    enemy: true,
                    samples,
                }],
            },
            &crosshair_lock::Parameters::default(),
        )
        .unwrap()
    }
    #[test]
    fn snap_and_follow_are_independent_counts_of_the_same_event() {
        let r = report(
            (0..66)
                .map(|tick| {
                    let target = tick as f64 * 0.2;
                    (if tick == 0 { -20. } else { target }, target)
                })
                .collect(),
        );
        let snap = project(Some(Ok(&r)), AimRule::Snap);
        let tracking = project(Some(Ok(&r)), AimRule::Tracking);
        assert_eq!(snap.findings.len(), 1);
        assert_eq!(tracking.findings.len(), 1);
        let a = &snap.findings[0];
        let b = &tracking.findings[0];
        assert_eq!(
            (&a.group, a.start_tick, a.end_tick),
            (&b.group, b.start_tick, b.end_tick)
        );
        let mut checks = vec![snap, tracking];
        statistics::summarize(&mut checks);
        assert_eq!(checks[0].occurrences.len(), 1);
        assert_eq!(checks[1].occurrences.len(), 1);
        assert!(project(Some(Ok(&r)), AimRule::Linear).findings.is_empty());
    }
    #[test]
    fn snap_crossing_without_a_hold_is_not_an_occurrence() {
        for tail in [vec![(0., 0.), (20., 0.)], vec![(0., 0.); 10]] {
            let r = report([(-20., 0.)].into_iter().chain(tail).collect());
            assert!(project(Some(Ok(&r)), AimRule::Snap).findings.is_empty());
        }
        let r = report([(-20., 0.)].into_iter().chain([(0., 0.); 11]).collect());
        assert_eq!(project(Some(Ok(&r)), AimRule::Snap).findings.len(), 1);
    }

    #[test]
    fn linear_requires_a_continuous_body_hold_even_when_obstructed() {
        let approach = [(-20., 0.), (-15., 0.), (-10., 0.), (-5., 0.)];
        for tail in [
            vec![(0., 0.), (5., 0.), (10., 0.)],
            vec![(0., 0.); 10],
            vec![(2., 0.); 20],
        ] {
            let r = report(approach.into_iter().chain(tail).collect());
            assert!(project(Some(Ok(&r)), AimRule::Linear).findings.is_empty());
        }
        // Consecutive intervals cannot be joined across a release from the body point.
        let tail = [(0., 0.); 6]
            .into_iter()
            .chain([(1., 0.)])
            .chain([(0., 0.); 6]);
        let r = report(approach.into_iter().chain(tail).collect());
        assert!(project(Some(Ok(&r)), AimRule::Linear).findings.is_empty());
        let mut r = report(approach.into_iter().chain([(0., 0.); 11]).collect());
        for obstruction in [
            crosshair_lock::Obstruction::Visible,
            crosshair_lock::Obstruction::Wall,
            crosshair_lock::Obstruction::Smoke,
            crosshair_lock::Obstruction::Blind,
            crosshair_lock::Obstruction::Unknown,
        ] {
            for evidence in &mut r.evidence {
                evidence.obstruction = obstruction;
            }
            let check = project(Some(Ok(&r)), AimRule::Linear);
            assert_eq!(check.findings.len(), 1);
            assert!(check.findings[0]
                .measurements
                .iter()
                .any(|m| m.name == "duration" && m.value >= 0.15));
        }
    }

    #[test]
    fn linear_acquisition_is_exclusive_and_tracking_can_stand_alone() {
        let r = report(
            [(-20., 0.), (-15., 0.), (-10., 0.), (-5., 0.)]
                .into_iter()
                .chain(std::iter::repeat_n((0., 0.), 12))
                .collect(),
        );
        assert_eq!(project(Some(Ok(&r)), AimRule::Linear).findings.len(), 1);
        assert!(project(Some(Ok(&r)), AimRule::Snap).findings.is_empty());
        assert!(project(Some(Ok(&r)), AimRule::Tracking).findings.is_empty());
        let r = report(
            (0..66)
                .map(|tick| (tick as f64 * 0.2, tick as f64 * 0.2))
                .collect(),
        );
        let tracking = project(Some(Ok(&r)), AimRule::Tracking);
        assert_eq!(tracking.findings.len(), 1);
        for rule in [AimRule::Snap, AimRule::Linear] {
            let check = project(Some(Ok(&r)), rule);
            assert_eq!(check.state, State::Passed);
            assert!(check.findings.is_empty());
            assert!(check.diagnostics["evidence"].as_array().unwrap().is_empty());
        }
        assert_eq!(tracking.diagnostics["demoFingerprint"], "fixture");
        assert_eq!(tracking.diagnostics["playerId"], "subject");
    }
}
