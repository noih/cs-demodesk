//! Match-wide native measurements: one shared packet scan and bounded acquisition lookback.
use super::crosshair_lock::{self as rule, Input, Sample, Stream, Track};
use crate::analysis::native_body::{self, PlayerFrame, Prepared};
use anyhow::Result;
use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    path::Path,
};

type Pair = ((i32, u32, u32), (i32, u32, u32), i32);
#[derive(Clone, Default)]
struct PointState {
    contiguous: (i32, u32),
    last: Option<(i32, bool)>,
}
struct Player {
    input: Input,
    stream: Stream,
    targets: HashMap<Pair, Vec<PointState>>,
    evaluated: usize,
}
fn live_round(rounds: &[crate::model::RoundInfo], cursor: &mut usize, tick: i32) -> Option<i32> {
    while *cursor < rounds.len() && tick > rounds[*cursor].end_tick {
        *cursor += 1;
    }
    rounds
        .get(*cursor)
        .filter(|r| tick >= r.freeze_end_tick && tick <= r.end_tick)
        .map(|r| r.round)
}
pub fn evaluate(
    path: &Path,
    prepared: &Prepared,
    fingerprint: &str,
    players: &[String],
    rounds: &[crate::model::RoundInfo],
    kills: &[crate::model::KillEvent],
) -> Result<(BTreeMap<String, Vec<super::Check>>, native_body::Coverage)> {
    let mut evaluator = Match::new(
        fingerprint,
        players,
        &prepared.assets.resource_content_id,
        prepared.header.data.tick_rate,
        prepared.point_names.clone(),
    )?;
    let mut views = super::view_angles::Match::new(players, prepared.header.data.tick_rate)?;
    let mut shot_views = super::shot_view::Match::new(players, prepared.header.data.tick_rate)?;
    shot_views.register_shots(&prepared.events.shots, rounds);
    let mut movement = super::movement::Match::new(players, prepared.header.data.tick_rate)?;
    let mut round_cursor = 0;
    let mut measurement_cursor = 0;
    let coverage = native_body::visit_when(
        path,
        prepared,
        &prepared.skeleton,
        |tick| live_round(rounds, &mut measurement_cursor, tick).is_some(),
        |tick, frame| {
            let round = live_round(rounds, &mut round_cursor, tick);
            views.push(tick, frame, round)?;
            movement.push(tick, frame, round)?;
            shot_views.push(
                tick,
                frame,
                round,
                prepared
                    .events
                    .shots
                    .get(&tick)
                    .map(Vec::as_slice)
                    .unwrap_or(&[]),
            )?;
            evaluator.push(tick, frame, round)
        },
    )?;
    let views = views.finish();
    let movement = movement.finish();
    let shot_views = shot_views.finish();
    let engagement = super::engagement_context::evaluate_match(players, rounds, kills);
    let combat = super::combat_stats::evaluate_match(
        &prepared.events.raw,
        players,
        rounds,
        prepared.header.data.tick_rate,
    );
    let measurements = BTreeMap::new();
    let checks = evaluator
        .finish()
        .into_iter()
        .map(|(player, report)| {
            let mut checks = super::evaluate(&super::Context {
                demo_fingerprint: fingerprint,
                player_id: &player,
                measurements: &measurements,
                body_journal: None,
                prepared_body: Some(Ok(&report)),
                prepared_view: views.get(&player),
                prepared_engagement: engagement.get(&player).map(Vec::as_slice),
            });
            if let Some(checks_for_player) = movement.get(&player) {
                checks.extend(checks_for_player.iter().cloned());
            }
            if let Some(check) = shot_views.get(&player) {
                checks.push(check.clone());
            }
            if let Some(context) = combat.get(&player) {
                checks.extend(context.iter().cloned());
            }
            (player, checks)
        })
        .collect();
    Ok((checks, coverage))
}

struct Match {
    states: BTreeMap<String, Player>,
    parameters: rule::Parameters,
    recent: VecDeque<(i32, Vec<PlayerFrame>)>,
    keep: usize,
    round: Option<i32>,
    point_names: Vec<String>,
    sufficient: bool,
    body_points: Vec<bool>,
    near_cos_squared: f64,
}
impl Match {
    fn new(
        fingerprint: &str,
        players: &[String],
        resource_id: &str,
        tick_rate: f64,
        point_names: Vec<String>,
    ) -> Result<Self> {
        let parameters = rule::Parameters::default();
        let mut states = BTreeMap::new();
        for player in players {
            let input = Input {
                demo_fingerprint: fingerprint.into(),
                player_id: player.clone(),
                measurement_source: format!("native-animgraph2:{}", resource_id),
                tick_rate,
                sample_step_ticks: 1,
                angular_resolution_degrees: 360. / 65536.,
                tracks: vec![],
            };
            let stream = Stream::new(&input, &parameters)?;
            states.insert(
                player.clone(),
                Player {
                    input,
                    stream,
                    targets: HashMap::new(),
                    evaluated: 0,
                },
            );
        }
        let keep = (parameters.acquisition_window_seconds * tick_rate).ceil() as usize + 3;
        let sufficient = 1. / tick_rate <= parameters.max_sample_seconds
            && (360. / 65536.) * 2. <= parameters.max_error_degrees;
        let body_points = point_names
            .iter()
            .map(|name| native_body::is_body_attached_point(name))
            .collect();
        let near_cos_squared = (parameters.max_error_degrees + 1e-6)
            .to_radians()
            .cos()
            .powi(2);
        Ok(Self {
            body_points,
            near_cos_squared,
            states,
            parameters,
            recent: VecDeque::new(),
            keep,
            round: None,
            point_names,
            sufficient,
        })
    }
    fn push(&mut self, tick: i32, frame: &[PlayerFrame], round: Option<i32>) -> Result<()> {
        if self.round != round {
            self.recent.clear();
            self.round = round;
        }
        let Some(round) = round else {
            self.recent.clear();
            return Ok(());
        };
        self.recent.push_back((tick, frame.to_vec()));
        while self.recent.len() > self.keep {
            self.recent.pop_front();
        }
        // Reuse the present anatomical points for every observer in this frame.
        let targets: Vec<_> = frame
            .iter()
            .map(|target| {
                let points = target
                    .points
                    .iter()
                    .enumerate()
                    .filter_map(|(index, point)| {
                        if self.body_points.get(index).copied().unwrap_or(false) {
                            point.as_ref().map(|point| (index, point))
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>();
                (target, points)
            })
            .collect();
        for observer in frame {
            let (Some(eye), Some(view), Some(state)) = (
                observer.eye,
                observer.view,
                self.states.get_mut(&observer.player_id),
            ) else {
                continue;
            };
            let pitch = view[0].to_radians();
            let yaw = view[1].to_radians();
            let direction = [
                pitch.cos() * yaw.cos(),
                pitch.cos() * yaw.sin(),
                -pitch.sin(),
            ];
            for (target, active_points) in &targets {
                if active_points.is_empty()
                    || target.team == observer.team
                    || target.player_id == observer.player_id
                {
                    continue;
                }
                let points = state
                    .targets
                    .entry((observer.identity_key, target.identity_key, round))
                    .or_insert_with(|| vec![PointState::default(); self.point_names.len()]);
                for &(index, point) in active_points {
                    let point_state = &mut points[index];
                    let sequence = &mut point_state.contiguous;
                    if sequence.0 != tick - 1 {
                        sequence.1 = 0;
                    }
                    sequence.0 = tick;
                    sequence.1 += 1;
                    if self.sufficient {
                        state.evaluated += match sequence.1 {
                            3 => 3,
                            n if n > 3 => 1,
                            _ => 0,
                        };
                    }
                    let delta = [point[0] - eye[0], point[1] - eye[1], point[2] - eye[2]];
                    let dot = delta.iter().zip(direction).map(|(a, b)| a * b).sum::<f64>();
                    let length2 = delta.iter().map(|v| v * v).sum::<f64>();
                    // Necessary spherical-angle condition for the rule, with a rounding margin.
                    // This only avoids work inside this rule; every native body point remains shared.
                    let near = dot > 0. && dot * dot >= length2 * self.near_cos_squared;
                    let last = point_state.last;
                    if !near && !last.is_some_and(|(_, active)| active) {
                        continue;
                    }
                    let mut samples = vec![];
                    for (sample_tick, snapshot) in &self.recent {
                        if last.is_some_and(|(last, _)| *sample_tick <= last) {
                            continue;
                        }
                        let Some(source) = snapshot.iter().find(|p| {
                            p.player_id == observer.player_id && p.identity == observer.identity
                        }) else {
                            continue;
                        };
                        let Some(target) = snapshot.iter().find(|p| {
                            p.player_id == target.player_id && p.identity == target.identity
                        }) else {
                            continue;
                        };
                        let (Some(eye), Some(view), Some(point)) = (
                            source.eye,
                            source.view,
                            target.points.get(index).copied().flatten(),
                        ) else {
                            continue;
                        };
                        samples.push(Sample {
                            tick: *sample_tick,
                            eye,
                            view,
                            target: point,
                            obstruction: rule::Obstruction::Unknown,
                        });
                    }
                    if samples.is_empty() {
                        continue;
                    }
                    state.input.tracks = vec![Track {
                        round,
                        target_id: target.player_id.clone(),
                        point_id: format!(
                            "{}:{}:{}",
                            observer.identity, target.identity, self.point_names[index]
                        ),
                        enemy: true,
                        samples,
                    }];
                    state.stream.push(&state.input)?;
                    state.input.tracks.clear();
                    // One outside sample closes the preceding lock; do not self.keep streaming distant tracks.
                    point_state.last = Some((tick, near));
                }
            }
        }
        Ok(())
    }
    fn finish(self) -> BTreeMap<String, rule::Report> {
        self.states
            .into_iter()
            .map(|(player, state)| {
                let report = state.stream.finish();
                let report = rule::report(
                    &state.input,
                    &self.parameters,
                    state.evaluated,
                    report.evidence,
                );
                (player, report)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn live_measurement_scope_preserves_first_and_winning_ticks() {
        let round = |number, freeze, end| crate::model::RoundInfo {
            round: number,
            start_tick: freeze - 10,
            freeze_end_tick: freeze,
            end_tick: end,
            officially_ended_tick: end + 10,
            winner: None,
            reason: String::new(),
            roster: BTreeMap::new(),
            bomb_planted_tick: None,
            bomb_defused_tick: None,
            bomb_defuser: None,
            bomb_exploded_tick: None,
        };
        let rounds = [round(1, 10, 20), round(2, 40, 50)];
        let mut cursor = 0;
        let observed: Vec<_> = [0, 9, 10, 20, 21, 39, 40, 50, 51]
            .into_iter()
            .map(|tick| live_round(&rounds, &mut cursor, tick))
            .collect();
        assert_eq!(
            observed,
            vec![
                None,
                None,
                Some(1),
                Some(1),
                None,
                None,
                Some(2),
                Some(2),
                None
            ]
        );
    }
    use super::*;
    fn frame(
        tick: i32,
        pitch: f64,
        observer_yaw: f64,
        target_yaw: f64,
        generation: u32,
        team: u32,
    ) -> Vec<PlayerFrame> {
        let direction = |yaw: f64| {
            let (p, y) = (pitch.to_radians(), yaw.to_radians());
            [
                100. * p.cos() * y.cos(),
                100. * p.cos() * y.sin(),
                -100. * p.sin(),
            ]
        };
        vec![
            PlayerFrame {
                movement: None,
                player_id: "a".into(),
                identity: "1:0:2".into(),
                identity_key: (1, 0, 2),
                team: 2,
                eye: Some([0.; 3]),
                view: Some([pitch, observer_yaw]),
                points: vec![Some([0., 0., 0.])],
            },
            PlayerFrame {
                movement: None,
                player_id: "b".into(),
                identity: format!("2:{generation}:{team}"),
                identity_key: (2, generation, team),
                team,
                eye: Some(direction(target_yaw)),
                view: Some([0., 180. + tick as f64]),
                points: vec![Some(direction(target_yaw))],
            },
        ]
    }
    fn compare(
        frames: &[(i32, Option<i32>, Vec<PlayerFrame>)],
        rate: f64,
    ) -> BTreeMap<String, rule::Report> {
        let players = vec!["a".into(), "b".into()];
        let point_count = frames
            .iter()
            .flat_map(|(_, _, frame)| frame.iter())
            .map(|player| player.points.len())
            .max()
            .unwrap_or(1);
        let names = ["pelvis", "spine_0", "head_0"][..point_count].to_vec();
        let mut fast = Match::new(
            "demo",
            &players,
            "assets",
            rate,
            names.iter().map(|name| (*name).to_owned()).collect(),
        )
        .unwrap();
        let mut full: BTreeMap<String, Input> = fast
            .states
            .iter()
            .map(|(id, s)| (id.clone(), s.input.clone()))
            .collect();
        for (tick, round, frame) in frames {
            fast.push(*tick, frame, *round).unwrap();
            let Some(round) = round else { continue };
            for source in frame {
                let (Some(eye), Some(view)) = (source.eye, source.view) else {
                    continue;
                };
                for target in frame {
                    if target.player_id == source.player_id || source.team == target.team {
                        continue;
                    }
                    for (i, point) in target.points.iter().enumerate() {
                        let Some(point) = point else { continue };
                        let input = full.get_mut(&source.player_id).unwrap();
                        let id = format!("{}:{}:{}", source.identity, target.identity, names[i]);
                        let index = input.tracks.iter().position(|t| {
                            t.round == *round && t.target_id == target.player_id && t.point_id == id
                        });
                        let track = if let Some(index) = index {
                            &mut input.tracks[index]
                        } else {
                            input.tracks.push(Track {
                                round: *round,
                                target_id: target.player_id.clone(),
                                point_id: id,
                                enemy: true,
                                samples: vec![],
                            });
                            input.tracks.last_mut().unwrap()
                        };
                        track.samples.push(Sample {
                            tick: *tick,
                            eye,
                            view,
                            target: *point,
                            obstruction: rule::Obstruction::Unknown,
                        });
                    }
                }
            }
        }
        let fast = fast.finish();
        for (player, input) in full {
            let expected = rule::evaluate(&input, &rule::Parameters::default()).unwrap();
            assert_eq!(
                serde_json::to_value(&fast[&player]).unwrap(),
                serde_json::to_value(expected).unwrap(),
                "player {player}"
            );
        }
        fast
    }
    #[test]
    fn candidate_stream_matches_all_samples_across_reentry_gaps_rounds_and_identity_changes() {
        let mut frames = vec![];
        for tick in 0..420 {
            let phase = tick % 70;
            let target = (tick as f64) * 0.4;
            let view = if !(8..52).contains(&phase) {
                target - 30.
            } else {
                target
            };
            let generation = if tick >= 280 { 1 } else { 0 };
            let team = if (225..240).contains(&tick) { 2 } else { 3 };
            let round = Some(if tick < 140 {
                1
            } else if tick < 280 {
                2
            } else {
                3
            });
            let mut f = frame(tick, 0., view, target, generation, team);
            if (95..100).contains(&tick) || (310..315).contains(&tick) {
                f.pop();
            }
            if tick == 190 {
                f[0].eye = None;
            }
            if tick == 350 {
                f[1].points[0] = None;
            }
            frames.push((tick, round, f));
        }
        let result = compare(&frames, 64.);
        assert!(!result["a"].evidence.is_empty());
        assert!(result["a"].evidence.iter().any(|e| e.rapid_acquisition));
        assert!(result["a"].evidence.iter().any(|e| e.sustained_follow));
        assert!(result["a"]
            .evidence
            .iter()
            .all(|e| e.round != 2 || e.start_tick >= 140));
    }
    #[test]
    fn target_point_states_remain_independent_across_different_gaps() {
        let mut frames = vec![];
        for tick in 0..180 {
            let yaw = tick as f64 * 0.4;
            let mut snapshot = frame(
                tick,
                0.,
                if tick < 10 { yaw - 30. } else { yaw },
                yaw,
                u32::from(tick >= 120),
                3,
            );
            for player in &mut snapshot {
                let point = player.points[0];
                player.points = vec![point, point, point];
            }
            if (40..46).contains(&tick) {
                snapshot[1].points[0] = None;
            }
            if (65..71).contains(&tick) {
                snapshot[1].points[1] = None;
            }
            if (88..95).contains(&tick) {
                snapshot[1].points[2] = None;
            }
            frames.push((tick, Some(if tick < 100 { 1 } else { 2 }), snapshot));
        }
        let reports = compare(&frames, 64.);
        assert!(!reports["a"].evidence.is_empty());
    }

    #[test]
    fn spherical_cone_retains_polar_and_wraparound_locks() {
        for pitch in [-90., -89.999, -89., 0., 89., 89.999, 90.] {
            let mut frames = vec![];
            for tick in 0..80 {
                let target = 179. + tick as f64 * 0.4;
                let view = if tick < 8 {
                    target - 30.
                } else {
                    target + 360.
                };
                frames.push((tick, Some(1), frame(tick, pitch, view, target, 0, 3)));
            }
            compare(&frames, 64.);
        }
    }
    #[test]
    fn shared_helper_points_are_preserved_but_do_not_become_body_evidence() {
        let mut evaluator = Match::new(
            "demo",
            &["a".into()],
            "assets",
            64.,
            vec![
                "root_motion".into(),
                "attachWorld".into(),
                "wpnAimIntent".into(),
            ],
        )
        .unwrap();
        for tick in 0..80 {
            let mut snapshot = frame(tick, 0., tick as f64, tick as f64, 0, 3);
            for player in &mut snapshot {
                player.points = vec![player.points[0]; 3];
            }
            evaluator.push(tick, &snapshot, Some(1)).unwrap();
            assert_eq!(snapshot[1].points.iter().flatten().count(), 3);
        }
        let report = evaluator.finish();
        assert_eq!(report["a"].evaluated_samples, 0);
        assert!(report["a"].evidence.is_empty());
    }

    #[test]
    fn varied_candidate_windows_match_full_stream_reports() {
        for seed in 0..12u64 {
            let mut random = seed + 1;
            let mut frames = vec![];
            let mut yaw = 170.;
            let mut offset = 25.;
            for tick in 0..600 {
                random = random.wrapping_mul(6364136223846793005).wrapping_add(1);
                if tick % 19 == 0 {
                    offset = if random >> 63 == 0 { 0. } else { 25. };
                }
                yaw += 0.2;
                let mut f = frame(
                    tick,
                    if seed % 2 == 0 { 0. } else { 89. },
                    yaw + offset,
                    yaw,
                    0,
                    3,
                );
                if random % 47 == 0 {
                    f[0].view = None;
                }
                if random % 71 == 0 {
                    f[1].points[0] = None;
                }
                if (290..294).contains(&tick) {
                    f.pop();
                }
                let round = if (195..200).contains(&tick) {
                    None
                } else {
                    Some(1 + tick / 200)
                };
                frames.push((tick, round, f));
            }
            compare(&frames, 64.);
        }
    }

    #[test]
    fn insufficient_sampling_rate_does_not_count_unscorable_samples() {
        let frames = (0..30)
            .map(|tick| (tick, Some(1), frame(tick, 0., 0., 30., 0, 3)))
            .collect::<Vec<_>>();
        compare(&frames, 32.);
    }
}
