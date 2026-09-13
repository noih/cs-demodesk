use super::*;

fn sample(tick: i32, yaw: f64, target_yaw: f64) -> Sample {
    let a = target_yaw.to_radians();
    Sample {
        tick,
        eye: [0.0; 3],
        view: [0.0, yaw],
        target: [100.0 * a.cos(), 100.0 * a.sin(), 0.0],
        obstruction: Obstruction::Unknown,
    }
}
fn input(samples: Vec<Sample>) -> Input {
    Input {
        demo_fingerprint: "synthetic".into(),
        player_id: "subject".into(),
        measurement_source: "synthetic exact directions".into(),
        tick_rate: 64.0,
        sample_step_ticks: 1,
        angular_resolution_degrees: 0.01,
        tracks: vec![Track {
            round: 1,
            target_id: "opponent".into(),
            point_id: "arbitrary-body-point".into(),
            enemy: true,
            samples,
        }],
    }
}
fn tracking() -> Input {
    input(
        (0..65)
            .map(|t| sample(t, f64::from(t) * 0.2, f64::from(t) * 0.2))
            .collect(),
    )
}
fn snap() -> Input {
    input(vec![
        sample(0, -20.0, 0.0),
        sample(1, 0.0, 0.0),
        sample(2, 0.0, 0.0),
    ])
}
fn run(i: &Input) -> Report {
    evaluate(i, &Parameters::default()).unwrap()
}

#[test]
fn stationary_alignment_is_not_locking_but_a_single_acquisition_is_measured() {
    let still = run(&input((0..65).map(|t| sample(t, 0.0, 0.0)).collect()));
    assert!(still.evidence.is_empty());
    let report = run(&snap());
    let e = &report.evidence[0];
    assert!(e.rapid_acquisition && !e.sustained_follow);
    assert_eq!((e.start_tick, e.lock_tick, e.end_tick), (0, 1, 2));
    assert!((e.acquisition_speed - 1280.0).abs() < 1e-8);
    assert_eq!(e.samples.len(), 3);
}

#[test]
fn follows_any_fixed_body_point_without_shots_or_recoil_and_rejects_loose_tracking() {
    let report = run(&tracking());
    assert_eq!(report.evidence.len(), 1);
    let e = &report.evidence[0];
    assert!(e.sustained_follow && !e.rapid_acquisition);
    assert_eq!(e.follow_seconds, 1.0);
    assert!(e.max_error_degrees < 1e-10);
    let mut loose = tracking();
    for s in &mut loose.tracks[0].samples {
        s.view[1] += 1.0;
    }
    assert!(run(&loose).evidence.is_empty());
}

#[test]
fn yaw_wrap_is_short_not_a_360_degree_snap() {
    let wrapped = input(vec![
        sample(0, 179.9, -179.9),
        sample(1, -179.9, -179.9),
        sample(2, -179.9, -179.9),
    ]);
    assert!(run(&wrapped).evidence.is_empty());
    let mut a = tracking();
    let mut b = tracking();
    for (x, y) in a.tracks[0].samples.iter_mut().zip(&mut b.tracks[0].samples) {
        *x = sample(
            x.tick,
            179.0 + f64::from(x.tick) * 0.2,
            179.0 + f64::from(x.tick) * 0.2,
        );
        *y = x.clone();
        y.view[1] -= 360.0;
    }
    assert!(
        (run(&a).evidence[0].follow_travel_degrees - run(&b).evidence[0].follow_travel_degrees)
            .abs()
            < 1e-8
    );
}

#[test]
fn missing_ticks_and_coarse_sampling_cannot_create_a_snap_or_long_lock() {
    let mut gapped = snap();
    gapped.tracks[0].samples[1].tick = 100;
    gapped.tracks[0].samples[2].tick = 101;
    let report = run(&gapped);
    assert_eq!(report.status, Status::InsufficientData);
    assert!(report.evidence.is_empty());
    let mut coarse = tracking();
    coarse.sample_step_ticks = 4;
    for s in &mut coarse.tracks[0].samples {
        s.tick *= 4;
    }
    assert_eq!(run(&coarse).status, Status::InsufficientData);
    let mut rounded = tracking();
    rounded.angular_resolution_degrees = 1.0;
    assert_eq!(run(&rounded).status, Status::InsufficientData);
}

#[test]
fn unknown_occlusion_remains_unknown_and_confirmed_context_is_retained() {
    assert_eq!(
        run(&tracking()).evidence[0].obstruction,
        Obstruction::Unknown
    );
    let mut mixed = tracking();
    for s in &mut mixed.tracks[0].samples {
        s.obstruction = if s.tick < 16 {
            Obstruction::Smoke
        } else if s.tick < 32 {
            Obstruction::Blind
        } else {
            Obstruction::Wall
        };
    }
    let report = run(&mixed);
    assert_eq!(report.evidence[0].obstruction, Obstruction::Wall);
    mixed.tracks[0]
        .samples
        .iter_mut()
        .for_each(|s| s.obstruction = Obstruction::Unknown);
    mixed.tracks[0].samples[30].obstruction = Obstruction::Wall;
}

#[test]
fn release_reacquire_and_separate_rounds_remain_distinct_as_independent_measurements() {
    let mut i = snap();
    i.tracks[0].samples.extend([
        sample(3, -20.0, 0.0),
        sample(4, 0.0, 0.0),
        sample(5, 0.0, 0.0),
    ]);
    let report = run(&i);
    assert_eq!(report.evidence.len(), 2);
    let mut next_round = i.tracks[0].clone();
    next_round.round = 2;
    i.tracks.push(next_round);
}

#[test]
fn invalid_input_fails_instead_of_passing_and_no_enemies_is_unavailable() {
    let mut i = tracking();
    i.tracks[0].enemy = false;
    assert_eq!(run(&i).status, Status::InsufficientData);
    i.tracks[0].enemy = true;
    i.tracks[0].samples[0].view[0] = f64::NAN;
    assert!(evaluate(&i, &Parameters::default()).is_err());
    let mut i = tracking();
    i.tracks[0].samples[1].tick = 0;
    assert!(evaluate(&i, &Parameters::default()).is_err());
    let mut i = tracking();
    i.tracks.push(i.tracks[0].clone());
    assert!(evaluate(&i, &Parameters::default()).is_err());
    let p = Parameters {
        min_straightness: 2.,
        ..Parameters::default()
    };
    assert!(evaluate(&tracking(), &p).is_err());
}

#[test]
fn straight_acquisition_measures_path_speed_and_curved_slow_approach_does_not_trigger() {
    let straight = input(
        (0..8)
            .map(|t| sample(t, (-24.0 + f64::from(t) * 4.0).min(0.0), 0.0))
            .collect(),
    );
    let report = run(&straight);
    assert_eq!(report.evidence.len(), 1);
    assert!(report.evidence[0].straightness > 0.999);
    assert!((report.evidence[0].acquisition_speed - 256.0).abs() < 1e-8);
    let mut curved = straight;
    for s in curved.tracks[0].samples.iter_mut().take(6) {
        s.view[0] = if s.tick % 2 == 0 { 3.0 } else { -3.0 };
    }
    assert!(run(&curved).evidence.is_empty());
}

#[test]
fn common_world_translation_does_not_change_measurements() {
    let original = tracking();
    let mut shifted = tracking();
    for s in &mut shifted.tracks[0].samples {
        for i in 0..3 {
            s.eye[i] += 1200.0;
            s.target[i] += 1200.0;
        }
    }
    let a = run(&original);
    let b = run(&shifted);
    assert!(
        (a.evidence[0].follow_travel_degrees - b.evidence[0].follow_travel_degrees).abs() < 1e-8
    );
}

#[test]
fn streaming_matches_batch_oracle_at_every_boundary_and_bounds_long_lock_evidence() {
    let p = Parameters::default();
    let mut mixed = tracking();
    mixed.tracks[0].samples.extend((70..150).map(|t| {
        let mut s = sample(
            t,
            if t % 29 == 0 {
                -20.0
            } else {
                f64::from(t) * 0.2
            },
            f64::from(t) * 0.2,
        );
        s.obstruction = if t < 100 {
            Obstruction::Wall
        } else {
            Obstruction::Smoke
        };
        s
    }));
    let compact = |report: Report| {
        let mut value = serde_json::to_value(report).unwrap();
        for e in value["evidence"].as_array_mut().unwrap() {
            e.as_object_mut().unwrap().remove("samples");
        }
        value
    };
    for original in [
        snap(),
        tracking(),
        mixed,
        input(vec![
            sample(0, -20.0, 0.0),
            sample(1, 0.0, 0.0),
            sample(9, 0.0, 0.0),
            sample(10, 0.0, 0.0),
            sample(11, 0.0, 0.0),
        ]),
    ] {
        let expected = compact(reference(&original, &p).unwrap());
        for split in 0..=original.tracks[0].samples.len() {
            let mut stream = Stream::new(&original, &p).unwrap();
            for samples in [
                &original.tracks[0].samples[..split],
                &original.tracks[0].samples[split..],
            ] {
                let mut part = original.clone();
                part.tracks[0].samples = samples.to_vec();
                stream.push(&part).unwrap();
            }
            assert_eq!(compact(stream.finish()), expected, "split {split}");
        }
    }
    let mut long = tracking();
    long.tracks[0].samples = (0..100_000)
        .map(|t| sample(t, f64::from(t) * 0.2, f64::from(t) * 0.2))
        .collect();
    let result = evaluate(&long, &p).unwrap();
    assert_eq!(result.evaluated_samples, 100_000);
    assert_eq!(result.evidence.len(), 1);
    assert_eq!(result.evidence[0].samples.len(), 2);
    assert_eq!(result.evidence[0].sample_count, 100_000);
    assert_eq!(result.evidence[0].end_tick, 99_999);
    let mut stream = Stream::new(&snap(), &p).unwrap();
    stream.push(&snap()).unwrap();
    assert!(stream.push(&snap()).is_err());
}

#[test]
fn straightness_requires_an_intermediate_view_sample() {
    let two = input(vec![sample(0,-6.0,0.0),sample(1,0.0,0.0),sample(2,0.0,0.0)]);
    assert!(run(&two).evidence.is_empty());
    let three = input(vec![sample(0,-6.0,0.0),sample(1,-3.0,0.0),sample(2,0.0,0.0)]);
    assert!(run(&three).evidence.iter().any(|e| e.rapid_acquisition));
    assert!(run(&snap()).evidence.iter().any(|e| e.rapid_acquisition));
}
