//! Experimental detection of sustained oscillation in recorded pitch/yaw channels.
//! The yaw channel near a pitch pole is not spherical crosshair angular speed.
use super::{Check, Definition, Finding, Measurement, State};
use crate::analysis::native_body::PlayerFrame;
use anyhow::{ensure, Result};
use serde_json::json;
use std::collections::BTreeMap;
pub const RULE_ID: &str = "view-angle-oscillation";
const MIN_PITCH: f64 = 88.;
const MIN_YAW_STEP: f64 = 30.;
const MIN_SECONDS: f64 = 1.;
const MIN_STEPS: usize = 32;
const MIN_REVERSAL_RATIO: f64 = 0.5;
pub struct Match {
    rate: f64,
    players: BTreeMap<String, Player>,
}
#[derive(Default)]
struct Player {
    last: Option<Sample>,
    bout: Option<Bout>,
    evaluated: usize,
    findings: Vec<Finding>,
}
#[derive(Clone, Copy)]
struct Sample {
    tick: i32,
    round: i32,
    identity: (i32, u32, u32),
    pitch: f64,
    yaw: f64,
}
struct Bout {
    start: Sample,
    end: i32,
    steps: usize,
    reversals: usize,
    last_delta: f64,
}
impl Match {
    pub fn new(players: &[String], tick_rate: f64) -> Result<Self> {
        ensure!(
            tick_rate.is_finite() && tick_rate > 0.,
            "Invalid view sampling rate"
        );
        Ok(Self {
            rate: tick_rate,
            players: players
                .iter()
                .map(|id| (id.clone(), Player::default()))
                .collect(),
        })
    }
    /// `round` must identify a live round; frames must contain only living players.
    pub fn push(&mut self, tick: i32, frame: &[PlayerFrame], round: Option<i32>) -> Result<()> {
        ensure!(tick >= 0, "Invalid view tick");
        for (id, state) in &mut self.players {
            let current = round.filter(|r| *r > 0).and_then(|round| {
                let player = frame
                    .iter()
                    .find(|p| p.player_id == *id && matches!(p.team, 2 | 3))?;
                let [pitch, yaw] = player.view?;
                (pitch.is_finite() && yaw.is_finite() && pitch.abs() <= 90.).then_some(Sample {
                    tick,
                    round,
                    identity: player.identity_key,
                    pitch,
                    yaw,
                })
            });
            let Some(current) = current else {
                state.close(id, self.rate);
                state.last = None;
                continue;
            };
            if let Some(previous) = state.last {
                ensure!(tick > previous.tick, "View ticks must increase");
                let delta = (current.yaw - previous.yaw + 180.).rem_euclid(360.) - 180.;
                let continuous = tick - previous.tick == 1
                    && current.round == previous.round
                    && current.identity == previous.identity;
                if continuous
                    && previous.pitch.abs() >= MIN_PITCH
                    && current.pitch.abs() >= MIN_PITCH
                    && delta.abs() >= MIN_YAW_STEP
                {
                    let bout = state.bout.get_or_insert(Bout {
                        start: previous,
                        end: tick,
                        steps: 0,
                        reversals: 0,
                        last_delta: delta,
                    });
                    if bout.steps > 0 && delta.signum() != bout.last_delta.signum() {
                        bout.reversals += 1;
                    }
                    bout.end = tick;
                    bout.steps += 1;
                    bout.last_delta = delta;
                } else {
                    state.close(id, self.rate);
                }
            }
            state.evaluated += 1;
            state.last = Some(current);
        }
        Ok(())
    }
    pub fn finish(mut self) -> BTreeMap<String, Check> {
        self.players
            .iter_mut()
            .for_each(|(id, state)| state.close(id, self.rate));
        self.players.into_iter().map(|(id, player)| {
            let found = !player.findings.is_empty();
            let state = if found { State::Findings } else if player.evaluated > 0 { State::Passed } else { State::Unavailable };
            (id, Check {
                definition: Definition {
                    id: RULE_ID.into(), version: "experimental-1".into(), name: "持續視角震盪".into(),
                    description: "Experimental sustained extreme-pitch and reversing recorded-yaw pattern; not spherical aim speed or cheating probability.".into(),
                    category: "view-manipulation".into(),
                    parameters: json!({"minAbsolutePitchDegrees":MIN_PITCH,"minWrappedYawStepDegrees":MIN_YAW_STEP,
                        "minSeconds":MIN_SECONDS,"minSteps":MIN_STEPS,"minReversalRatio":MIN_REVERSAL_RATIO,
                        "experimental":true}),
                },
                state, reason: if found { "Sustained recorded view-channel oscillation matched experimental criteria." } else { "No sustained recorded view-channel oscillation matched." }.into(),
                reason_code: if player.evaluated > 0 { "experimentalMeasurements" } else { "viewMissing" }.into(),
                evaluated_samples: player.evaluated, findings: player.findings, observations: vec![], occurrences: vec![], summary: vec![], diagnostics: serde_json::Value::Null,
            })
        }).collect()
    }
}
pub fn unavailable() -> Check {
    Match {
        rate: 1.,
        players: BTreeMap::from([(String::new(), Player::default())]),
    }
    .finish()
    .remove("")
    .expect("one player")
}
impl Player {
    fn close(&mut self, id: &str, rate: f64) {
        let Some(bout) = self.bout.take() else { return };
        let duration = f64::from(bout.end - bout.start.tick) / rate;
        let ratio = bout.reversals as f64 / bout.steps.saturating_sub(1).max(1) as f64;
        if duration < MIN_SECONDS || bout.steps < MIN_STEPS || ratio < MIN_REVERSAL_RATIO {
            return;
        }
        self.findings.push(Finding {
            id: format!("view-{}-{}-{}", bout.start.round, bout.start.tick, bout.end),
            group: RULE_ID.into(), round: bout.start.round, start_tick: bout.start.tick, end_tick: bout.end,
            target_id: id.into(), reason: "Repeated large reversals of recorded yaw while pitch stays extreme; experimental view-channel heuristic.".into(),

            measurements: vec![
                Measurement { name: "duration".into(), value: duration, unit: "s".into(), threshold: Some(MIN_SECONDS) },
                Measurement { name: "steps".into(), value: bout.steps as f64, unit: "ticks".into(), threshold: Some(MIN_STEPS as f64) },
                Measurement { name: "reversalRatio".into(), value: ratio, unit: String::new(), threshold: Some(MIN_REVERSAL_RATIO) },
            ],
        });
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn frames(count: i32, pitch: f64, step: f64) -> Vec<(i32, PlayerFrame, Option<i32>)> {
        let mut yaw: f64 = 170.;
        (0..count)
            .map(|tick| {
                if tick > 0 {
                    yaw += if (tick - 1) % 4 < 2 { step } else { -step };
                }
                (
                    tick,
                    PlayerFrame {
                        simulation_tick: None,
                        hitbox_set: None,
                        hitbox_transforms: vec![],
                        capsules: vec![],
                        player_id: "player".into(),
                        identity: "pawn".into(),
                        identity_key: (1, 2, 3),
                        team: 2,
                        eye: None,
                        view: Some([pitch, (yaw + 180.).rem_euclid(360.) - 180.]),
                        points: vec![],
                        movement: None,
                    },
                    Some(1),
                )
            })
            .collect()
    }
    fn run(rows: Vec<(i32, PlayerFrame, Option<i32>)>) -> Check {
        let mut evaluator = Match::new(&["player".into()], 64.).unwrap();
        for (tick, frame, round) in rows {
            evaluator.push(tick, &[frame], round).unwrap();
        }
        evaluator.finish().remove("player").unwrap()
    }
    #[test]
    fn sustained_boundary_pattern_is_one_finding() {
        // 65 steps contain exactly 32 reversals out of 64 adjacent step pairs.
        let check = run(frames(66, 89., 45.));
        assert_eq!(check.findings.len(), 1);
        assert_eq!(check.findings[0].measurements[2].value, 0.5);
        assert_eq!(run(frames(200, 89., 45.)).findings.len(), 1);
    }
    #[test]
    fn normal_channels_short_bouts_and_discontinuities_do_not_join() {
        for (count, pitch, step) in [
            (64, 89., 45.),
            (200, 20., 45.),
            (200, 89., 0.),
            (200, 89., 2.),
        ] {
            assert!(run(frames(count, pitch, step)).findings.is_empty());
        }
        for boundary in 0..5 {
            let mut rows = frames(100, 89., 45.);
            for (tick, frame, round) in rows.iter_mut().skip(50) {
                match boundary {
                    0 => *tick += 1,
                    1 => frame.identity_key.1 += 1,
                    2 => *round = Some(2),
                    _ => (),
                }
            }
            if boundary == 3 {
                rows[50].1.view = None;
            }
            if boundary == 4 {
                rows[50].2 = None;
            }
            assert!(run(rows).findings.is_empty());
        }
    }
    #[test]
    fn steady_spin_and_isolated_flick_are_not_reversing_oscillation() {
        let mut rows = frames(200, 89., 45.);
        for (tick, p, _) in &mut rows {
            p.view = Some([89., (*tick as f64 * 45. + 180.).rem_euclid(360.) - 180.]);
        }
        assert!(run(rows).findings.is_empty());
        assert!(run(frames(3, 89., 120.)).findings.is_empty());
    }
}
