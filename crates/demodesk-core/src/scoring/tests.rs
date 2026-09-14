use super::*;
#[test]
fn history_keeps_only_latest_and_stays_outside_parse_cache() {
    let root = tempfile::tempdir().unwrap();
    let mut record = Assessment {
        schema_version: 2,
        id: String::new(),
        created_at: crate::store::now(),
        demo_id: "demo/../id".into(),
        demo_fingerprint: "content-a".into(),
        player_id: "player".into(),
        tick_rate: 64.0,
        ruleset_version: "1".into(),
        checks: vec![],
        input_provenance: serde_json::Value::Null,
        state: State::Unavailable,
    };
    history::save_match(root.path(), std::slice::from_mut(&mut record)).unwrap();
    let old_id = record.id.clone();
    record.ruleset_version = "2".into();
    record.demo_fingerprint = "content-b".into();
    history::save_match(root.path(), std::slice::from_mut(&mut record)).unwrap();
    assert_ne!(old_id, record.id);
    let reopened = history::list(root.path(), &record.demo_id, &record.player_id).unwrap();
    assert_eq!(reopened.len(), 1);
    assert!(reopened
        .iter()
        .any(|r| r.ruleset_version == "2" && r.demo_fingerprint == "content-b"));
    assert!(history::list(root.path(), "another", "player")
        .unwrap()
        .is_empty());
    assert!(!root.path().join("parsed").exists());
}
#[test]
fn missing_measurements_remain_unavailable_and_invalid_data_fails() {
    let mut inputs = BTreeMap::new();
    let context = Context {
        demo_fingerprint: "demo",
        player_id: "player",
        measurements: &inputs,
        prepared_body: None,
        prepared_view: None,
        prepared_engagement: None,
        body_journal: None,
    };
    let checks = evaluate(&context);
    assert_eq!(checks[0].state, State::Unavailable);
    assert_eq!(checks[0].evaluated_samples, 0);
    assert_eq!(
        statistics::summarize(&mut checks.clone()),
        State::Unavailable
    );
    inputs.insert(
        crosshair_lock::RULE_ID.into(),
        serde_json::json!({"invalid":true}),
    );
    assert_eq!(
        evaluate(&Context {
            demo_fingerprint: "demo",
            player_id: "player",
            measurements: &inputs,
            prepared_body: None,
            prepared_view: None,
            prepared_engagement: None,
            body_journal: None,
        })[0]
            .state,
        State::Failed
    );
}
#[test]
fn shared_body_journal_is_measured_natively_and_truncation_never_passes() {
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().join("analysis/body-measurements");
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join(format!("{}.ndjson", sha1_smol::Sha1::from("demo").digest()));
    let mut rows = vec![
        serde_json::json!({"contract":{"module":"body-measurement-journal","schemaVersion":1,"implementationVersion":"0.1.0"},"source":{"demoFingerprint":"demo","gameBuild":null,"mapContentFingerprint":null},"dependencies":[],"data":{"tickRate":64.0,"sampleStepTicks":1,"angularResolutionDegrees":0.01,"measurementSource":"synthetic"}}),
        serde_json::json!({"tick":0,"define":{"a":{"playerId":"player","round":1,"targetId":"opponent","pointId":"point","pointKey":"p","enemy":true}},"active":["a"],"views":{"player":{"eye":[0,0,0],"view":[0,-20]}},"points":{"p":[100,0,0]}}),
        serde_json::json!({"tick":1,"views":{"player":{"eye":[0,0,0],"view":[0,0]}}}),
        serde_json::json!({"tick":2}),
        serde_json::json!({"end":3,"lastTick":2}),
    ];
    rows.pop();
    rows.extend((3..=11).map(|tick| serde_json::json!({"tick":tick})));
    rows.push(serde_json::json!({"end":12,"lastTick":11}));
    let encode =
        |rows: &[serde_json::Value]| rows.iter().map(|r| format!("{r}\n")).collect::<String>();
    std::fs::write(&path, encode(&rows)).unwrap();
    {
        let inputs = history::measurements(root.path(), "demo", "player").unwrap();
        assert!(inputs.values.is_empty());
        assert!(inputs.provenance["contentFingerprint"]
            .as_str()
            .unwrap()
            .starts_with("sha1:"));
        let checks = evaluate(&Context {
            demo_fingerprint: "demo",
            player_id: "player",
            measurements: &inputs.values,
            prepared_body: None,
            prepared_view: None,
            prepared_engagement: None,
            body_journal: inputs.body_journal.as_ref(),
        });
        assert_eq!(checks[0].state, State::Findings);
        assert_eq!(checks[0].evaluated_samples, 12);
        assert_eq!(checks[0].findings.len(), 1);
        assert_eq!(
            checks[0].diagnostics["evidence"][0]["acquisitionSpeed"],
            1280.0
        );
        let mut checks = checks;
        statistics::summarize(&mut checks);
        assert_eq!(checks[0].occurrences.len(), 1);
    }
    {
        use std::io::Write;
        let mut versioned = rows.clone();
        versioned[0]["contract"]["schemaVersion"] = serde_json::json!(2);
        versioned.last_mut().unwrap()["alignment"] = serde_json::json!([{"status":"matched"}]);
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(encode(&versioned).as_bytes()).unwrap();
        let compressed = encoder.finish().unwrap();
        let compressed_path = path.with_extension("ndjson.gz");
        for valid in [true, false] {
            std::fs::write(
                &compressed_path,
                if valid {
                    &compressed[..]
                } else {
                    &compressed[..compressed.len() - 4]
                },
            )
            .unwrap();
            let inputs = history::measurements(root.path(), "demo", "player").unwrap();
            let checks = evaluate(&Context {
                demo_fingerprint: "demo",
                player_id: "player",
                measurements: &inputs.values,
                prepared_body: None,
                prepared_view: None,
                prepared_engagement: None,
                body_journal: inputs.body_journal.as_ref(),
            });
            assert_eq!(
                checks[0].state,
                if valid {
                    State::Findings
                } else {
                    State::Failed
                }
            );
            if valid {
                assert_eq!(checks[0].evaluated_samples, 12);
                assert_eq!(checks[0].findings.len(), 1);
            }
        }
        std::fs::remove_file(compressed_path).unwrap();
    }
    for bad in [
        encode(&rows[..4]),
        encode(&[rows[0].clone(), rows[1].clone(), rows[1].clone()]),
        encode(&[
            rows[0].clone(),
            serde_json::json!({"tick":0,"active":["missing"]}),
        ]),
    ] {
        std::fs::write(&path, bad).unwrap();
        let inputs = history::measurements(root.path(), "demo", "player").unwrap();
        let checks = evaluate(&Context {
            demo_fingerprint: "demo",
            player_id: "player",
            measurements: &inputs.values,
            prepared_body: None,
            prepared_view: None,
            prepared_engagement: None,
            body_journal: inputs.body_journal.as_ref(),
        });
        assert_eq!(checks[0].state, State::Failed);
        assert_eq!(statistics::summarize(&mut checks.clone()), State::Failed);
    }
}

#[test]
fn match_stream_routes_all_players_once_and_rejects_partial_results() {
    let rows = [
        serde_json::json!({"contract":{"module":"body-measurement-journal","schemaVersion":2,"implementationVersion":"test"},"source":{"demoFingerprint":"demo","gameBuild":null,"mapContentFingerprint":null},"dependencies":[],"data":{"tickRate":64.0,"sampleStepTicks":1,"angularResolutionDegrees":0.01,"measurementSource":"synthetic"}}),
        serde_json::json!({"tick":0,"define":{
            "a":{"playerId":"one","round":1,"targetId":"two","pointId":"point","pointKey":"p","enemy":true},
            "b":{"playerId":"two","round":1,"targetId":"one","pointId":"point","pointKey":"q","enemy":true}
        },"active":["a","b"],"views":{"one":{"eye":[0,0,0],"view":[0,-20]},"two":{"eye":[0,0,0],"view":[0,0]}},"points":{"p":[100,0,0],"q":[100,0,0]}}),
        serde_json::json!({"tick":1,"views":{"one":{"eye":[0,0,0],"view":[0,0]},"two":{"eye":[0,0,0],"view":[0,0]}}}),
        serde_json::json!({"tick":2}),
        serde_json::json!({"tick":8}),
        serde_json::json!({"end":4,"lastTick":8}),
    ];
    let bytes = rows
        .iter()
        .map(|r| format!("{r}\n"))
        .collect::<String>()
        .into_bytes();
    let players = vec!["one".into(), "two".into(), "missing".into()];
    let p = crosshair_lock::Parameters::default();
    let batch =
        crosshair_lock::evaluate_journal_match(bytes.as_slice(), "demo", &players, &p).unwrap();
    for player in &players {
        let single =
            crosshair_lock::evaluate_journal(bytes.as_slice(), "demo", player, &p).unwrap();
        assert_eq!(
            serde_json::to_value(&batch[player]).unwrap(),
            serde_json::to_value(single).unwrap()
        );
    }
    assert_eq!(batch["one"].evaluated_samples, 3);
    assert_eq!(batch["two"].evaluated_samples, 3);
    assert_eq!(batch["missing"].evaluated_samples, 0);
    // A single shared scan must retain independent rule evidence for every player.
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().join("analysis/body-measurements");
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join(format!("{}.ndjson", sha1_smol::Sha1::from("demo").digest()));
    std::fs::write(path, &bytes).unwrap();
    let inputs = history::measurements(root.path(), "demo", "").unwrap();
    let checks = evaluate_match("demo", &players, &inputs, &BTreeMap::new());
    assert!(
        checks["one"][0].findings.is_empty(),
        "a gap cannot extend a brief acquisition into a hold"
    );
    assert!(checks["two"][0].findings.is_empty());
    assert_eq!(checks["missing"][0].evaluated_samples, 0);
    assert_eq!(
        statistics::summarize(&mut checks["missing"].clone()),
        State::Unavailable
    );

    assert!(crosshair_lock::evaluate_journal_match(
        &bytes[..bytes.len() - 15],
        "demo",
        &players,
        &p
    )
    .is_err());
    assert!(
        crosshair_lock::evaluate_journal_match(bytes.as_slice(), "different", &players, &p)
            .is_err()
    );
}

#[test]
fn match_history_replaces_all_players_together_and_removes_old_runs() {
    let root = tempfile::tempdir().unwrap();
    let make = |player: &str| Assessment {
        schema_version: 2,
        id: String::new(),
        created_at: crate::store::now(),
        demo_id: "match".into(),
        demo_fingerprint: "source".into(),
        player_id: player.into(),
        tick_rate: 64.0,
        ruleset_version: RULESET_VERSION.into(),
        checks: vec![],
        input_provenance: serde_json::Value::Null,
        state: State::Unavailable,
    };
    let mut batch = vec![make("one"), make("two")];
    history::save_match(root.path(), &mut batch).unwrap();
    let first = batch.clone();
    let dir = root
        .path()
        .join("behavior-analysis/matches")
        .join(sha1_smol::Sha1::from("match").digest().to_string());
    std::fs::write(
        dir.join("match-old.json"),
        serde_json::to_vec(&first).unwrap(),
    )
    .unwrap();
    history::save_match(root.path(), &mut batch).unwrap();
    for player in ["one", "two"] {
        let records = history::list(root.path(), "match", player).unwrap();
        assert_eq!(records.len(), 1);
        assert_ne!(
            records[0].id,
            first.iter().find(|r| r.player_id == player).unwrap().id
        );
        assert_eq!(
            records[0].id,
            batch.iter().find(|r| r.player_id == player).unwrap().id
        );
    }
    assert!(history::save_match(root.path(), &mut [make("one"), make("one")]).is_err());
    assert_eq!(
        history::list(root.path(), "match", "two").unwrap()[0].id,
        batch[1].id
    );
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
    assert!(dir.join("latest.json").is_file());
    let latest = history::list_match(root.path(), "match").unwrap();
    assert_eq!(latest.len(), 2);
    assert_eq!(latest["one"][0].id, batch[0].id);
    assert_eq!(latest["two"][0].id, batch[1].id);
    std::fs::write(
        dir.join("latest.json"),
        serde_json::to_vec(&vec![make("one"), make("one")]).unwrap(),
    )
    .unwrap();
    assert!(history::list_match(root.path(), "match").is_err());
}

#[test]
fn legacy_input_size_is_accounted_for_as_diagnostic_storage() {
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().join("analysis/scoring-inputs");
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join(format!(
        "{}.json",
        sha1_smol::Sha1::from("demo\\0player".replace("\\0", "\0")).digest()
    ));
    let bytes = serde_json::to_vec(&serde_json::json!({
        "contract":{"module":"scoring-measurements","schemaVersion":1,"implementationVersion":"test"},
        "source":{"demoFingerprint":"demo","gameBuild":null,"mapContentFingerprint":null},
        "dependencies":[],"data":{}
    })).unwrap();
    std::fs::write(path, &bytes).unwrap();
    let input = history::measurements(root.path(), "demo", "player").unwrap();
    assert_eq!(
        input.provenance["sourceBytes"].as_u64(),
        Some(bytes.len() as u64)
    );
}

#[test]
fn measured_events_reach_independent_rule_statistics() {
    use crosshair_lock::{Input, Obstruction, Parameters, Sample, Track};
    let input = Input {
        demo_fingerprint: "synthetic-only".into(),
        player_id: "subject".into(),
        measurement_source: "synthetic exact body point".into(),
        tick_rate: 64.,
        sample_step_ticks: 1,
        angular_resolution_degrees: 0.01,
        tracks: vec![Track {
            round: 1,
            target_id: "opponent".into(),
            point_id: "fixed-point".into(),
            enemy: true,
            samples: (0..65)
                .map(|tick| {
                    let yaw = f64::from(tick) * 0.2;
                    Sample {
                        tick,
                        eye: [0.; 3],
                        view: [0., if tick == 0 { -20. } else { yaw }],
                        target: [
                            yaw.to_radians().cos() * 100.,
                            yaw.to_radians().sin() * 100.,
                            0.,
                        ],
                        obstruction: Obstruction::Unknown,
                    }
                })
                .collect(),
        }],
    };
    let report = crosshair_lock::evaluate(&input, &Parameters::default()).unwrap();
    assert!(!report.evidence.is_empty());
    let inputs = BTreeMap::new();
    let checks = evaluate(&Context {
        demo_fingerprint: &input.demo_fingerprint,
        player_id: &input.player_id,
        measurements: &inputs,
        body_journal: None,
        prepared_view: None,
        prepared_engagement: None,
        prepared_body: Some(Ok(&report)),
    });
    let mut checks = checks;
    assert_eq!(statistics::summarize(&mut checks), State::Findings);
    assert_eq!(checks[0].occurrences.len(), 1);
    assert_eq!(checks[2].occurrences.len(), 1);
}
