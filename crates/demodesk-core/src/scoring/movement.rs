//! Streaming movement observations from recorded jump clocks and velocity.
use super::{Check, Definition, Finding, Measurement, State};
use crate::analysis::native_body::{Movement, PlayerFrame};
use anyhow::{ensure, Result};
use std::collections::BTreeMap;
const IDS: [&str; 2] = ["bhop-speed-retention", "fixed-view-air-strafe"];
const MIN_SPEED: f64 = 200.;
const RETENTION: f64 = 0.98;
#[derive(Clone)]
struct Sample {
    tick: i32,
    round: i32,
    identity: (i32, u32, u32),
    movement: Movement,
    view: Option<[f64; 2]>,
}
struct Chain {
    start: i32,
    end: i32,
    round: i32,
    jumps: usize,
    last_speed: f64,
    minimum_retention: f64,
}
struct Air {
    start: i32,
    end: i32,
    round: i32,
    steps: usize,
    last_delta: f64,
    start_speed: f64,
    end_speed: f64,
    max_view: f64,
}
#[derive(Default)]
struct Player {
    last: Option<Sample>,
    chain: Option<Chain>,
    air: Option<Air>,
    ground_frames: usize,
    evaluated: [usize; 2],
    findings: [Vec<Finding>; 2],
    jumps: usize,
    max_chain: usize,
    max_air: usize,
}
pub struct Match {
    players: BTreeMap<String, Player>,
    rate: f64,
}
fn speed(m: &Movement) -> f64 {
    m.velocity[0].hypot(m.velocity[1])
}
fn wrap(v: f64) -> f64 {
    (v + 180.).rem_euclid(360.) - 180.
}
fn metric(name: &str, value: f64, unit: &str, threshold: Option<f64>) -> Measurement {
    Measurement {
        name: name.into(),
        value,
        unit: unit.into(),
        threshold,
    }
}
fn check(index: usize) -> Check {
    Check {
        definition: Definition {
            id: IDS[index].into(),
            version: "experimental-1".into(),
            name: if index == 0 {
                "連跳保速"
            } else {
                "固定視角交替空中轉向"
            }
            .into(),
            description: if index == 0 {
                "Recorded consecutive jumps with sustained horizontal takeoff speed."
            } else {
                "Alternating airborne velocity direction with near-fixed recorded view."
            }
            .into(),
            category: "movement".into(),
            parameters: serde_json::json!({"minimumSpeed":200,"minimumRetention":0.98,"minimumJumps":3,"maximumGroundFrames":2,"maximumJumpGapSeconds":2,"minimumAlternatingSteps":3,"headingStepDegrees":[2,10],"maximumViewStepDegrees":0.05,"minimumSpeedGainPerStep":0.5,"requiredMoveType":2,"requiredWaterLevel":0,"requiredLadderSurface":-1}),
        },
        state: State::Unavailable,
        reason: "Qualified movement observations are unavailable.".into(),
        reason_code: "movementMissing".into(),
        evaluated_samples: 0,
        findings: vec![],
        observations: vec![],
        occurrences: vec![],
        summary: vec![],
        diagnostics: serde_json::Value::Null,
    }
}
pub fn unavailable() -> Vec<Check> {
    (0..2).map(check).collect()
}
impl Match {
    pub fn new(players: &[String], rate: f64) -> Result<Self> {
        ensure!(rate.is_finite() && rate > 0., "Invalid movement clock");
        Ok(Self {
            players: players
                .iter()
                .map(|id| (id.clone(), Player::default()))
                .collect(),
            rate,
        })
    }
    pub fn push(&mut self, tick: i32, frame: &[PlayerFrame], round: Option<i32>) -> Result<()> {
        ensure!(tick >= 0, "Invalid movement tick");
        for (id, state) in &mut self.players {
            let current = round.filter(|r| *r > 0).and_then(|round| {
                let p = frame
                    .iter()
                    .find(|p| p.player_id == *id && matches!(p.team, 2 | 3))?;
                let m = p.movement.as_ref()?;
                if m.move_type != Some(2)
                    || m.water_level != Some(0.)
                    || m.ladder_surface != Some(-1)
                    || !m
                        .velocity
                        .iter()
                        .chain(m.origin.iter())
                        .all(|v| v.is_finite())
                    || !m.last_jump_fraction.is_finite()
                    || !(0. ..1.).contains(&m.last_jump_fraction)
                {
                    return None;
                }
                Some(Sample {
                    tick,
                    round,
                    identity: p.identity_key,
                    movement: m.clone(),
                    view: p
                        .view
                        .filter(|v| v.iter().all(|x| x.is_finite()) && v[0].abs() <= 90.),
                })
            });
            let Some(current) = current else {
                state.close_chain(id);
                state.close_air(id);
                state.last = None;
                state.ground_frames = 0;
                continue;
            };
            state.evaluated[0] += 1;
            let previous = state.last.replace(current.clone());
            let Some(previous) = previous else {
                continue;
            };
            ensure!(tick > previous.tick, "Movement ticks must increase");
            if tick != previous.tick + 1
                || current.round != previous.round
                || current.identity != previous.identity
            {
                state.close_chain(id);
                state.close_air(id);
                state.ground_frames = 0;
                continue;
            }
            let m = &current.movement;
            let before = &previous.movement;
            let horizontal = speed(m);
            let previous_speed = speed(before);
            let changed = m.last_jump_tick != before.last_jump_tick
                || m.last_jump_fraction != before.last_jump_fraction;
            if changed {
                state.close_air(id);
                let fresh = i64::from(m.network_tick) - i64::from(m.last_jump_tick);
                let newer = f64::from(m.last_jump_tick) + m.last_jump_fraction
                    > f64::from(before.last_jump_tick) + before.last_jump_fraction;
                if newer && (0..=1).contains(&fresh) && m.flags & 1 == 0 && m.velocity[2] > 0. {
                    state.jumps += 1;
                    let linked = state.chain.as_ref().is_some_and(|c| {
                        tick - c.end > 0
                            && f64::from(tick - c.end) <= self.rate * 2.
                            && state.ground_frames <= 2
                            && horizontal >= MIN_SPEED
                            && horizontal / c.last_speed >= RETENTION
                    });
                    if !linked {
                        state.close_chain(id);
                    }
                    if horizontal >= MIN_SPEED {
                        if let Some(c) = &mut state.chain {
                            c.minimum_retention =
                                c.minimum_retention.min(horizontal / c.last_speed);
                            c.end = tick;
                            c.jumps += 1;
                            c.last_speed = horizontal;
                        } else {
                            state.chain = Some(Chain {
                                start: tick,
                                end: tick,
                                round: current.round,
                                jumps: 1,
                                last_speed: horizontal,
                                minimum_retention: f64::INFINITY,
                            });
                        }
                    }
                    state.ground_frames = 0;
                } else {
                    state.close_chain(id);
                    state.ground_frames = 0;
                }
            } else {
                if m.flags & 1 != 0 {
                    state.ground_frames += 1;
                }
                if state.ground_frames > 2
                    || state
                        .chain
                        .as_ref()
                        .is_some_and(|c| f64::from(tick - c.end) > self.rate * 2.)
                {
                    state.close_chain(id);
                }
            }
            let views = previous.view.zip(current.view);
            if !changed && m.flags & 1 == 0 && before.flags & 1 == 0 {
                if let Some((a, b)) = views {
                    state.evaluated[1] += 1;
                    let view = wrap(b[1] - a[1]).abs().max((b[0] - a[0]).abs());
                    let delta = wrap(
                        m.velocity[1].atan2(m.velocity[0]).to_degrees()
                            - before.velocity[1].atan2(before.velocity[0]).to_degrees(),
                    );
                    if previous_speed >= MIN_SPEED
                        && (2. ..=10.).contains(&delta.abs())
                        && view <= 0.05
                        && horizontal - previous_speed >= 0.5
                    {
                        if state
                            .air
                            .as_ref()
                            .is_some_and(|run| run.last_delta * delta >= 0.)
                        {
                            state.close_air(id);
                        }
                        let run = state.air.get_or_insert(Air {
                            // Steps own their destination ticks, avoiding overlap between
                            // distinct runs that share one boundary observation.
                            start: tick,
                            end: tick,
                            round: current.round,
                            steps: 0,
                            last_delta: delta,
                            start_speed: previous_speed,
                            end_speed: horizontal,
                            max_view: view,
                        });
                        run.end = tick;
                        run.steps += 1;
                        run.last_delta = delta;
                        run.end_speed = horizontal;
                        run.max_view = run.max_view.max(view);
                        continue;
                    }
                }
            }
            state.close_air(id);
        }
        Ok(())
    }
    pub fn finish(mut self) -> BTreeMap<String, Vec<Check>> {
        self.players.iter_mut().for_each(|(id, p)| {
            p.close_chain(id);
            p.close_air(id);
        });
        self.players
            .into_iter()
            .map(|(id, mut p)| {
                let checks = (0..2)
                    .map(|i| {
                        let mut c = check(i);
                        c.evaluated_samples = p.evaluated[i];
                        c.findings = std::mem::take(&mut p.findings[i]);
                        if c.evaluated_samples > 0 {
                            c.state = if c.findings.is_empty() {
                                State::Passed
                            } else {
                                State::Findings
                            };
                            c.reason_code = "experimentalMeasurements".into();
                            c.reason =
                                "Recorded movement patterns; not proof of automated input.".into();
                        }
                        c.summary = if c.state == State::Unavailable {
                            vec![]
                        } else if i == 0 {
                            vec![
                                metric("recordedJumps", p.jumps as f64, "", None),
                                metric(
                                    "longestMaintainedJumpSequence",
                                    p.max_chain as f64,
                                    "jumps",
                                    Some(3.),
                                ),
                            ]
                        } else {
                            vec![metric(
                                "longestAlternatingAirSteps",
                                p.max_air as f64,
                                "ticks",
                                Some(3.),
                            )]
                        };
                        c
                    })
                    .collect();
                (id, checks)
            })
            .collect()
    }
}
impl Player {
    fn close_chain(&mut self, id: &str) {
        let Some(c) = self.chain.take() else {
            return;
        };
        self.max_chain = self.max_chain.max(c.jumps);
        if c.jumps >= 3 {
            self.findings[0].push(Finding {
                id: format!("jump-{}-{}", c.start, c.end),
                group: IDS[0].into(),
                round: c.round,
                start_tick: c.start,
                end_tick: c.end,
                target_id: id.into(),
                reason: "Consecutive recorded jumps retained horizontal takeoff speed.".into(),
                measurements: vec![
                    metric("jumps", c.jumps as f64, "", Some(3.)),
                    metric(
                        "minimumSpeedRetention",
                        c.minimum_retention,
                        "ratio",
                        Some(RETENTION),
                    ),
                    metric("lastTakeoffSpeed", c.last_speed, "HU/s", Some(MIN_SPEED)),
                ],
            });
        }
    }
    fn close_air(&mut self, id: &str) {
        let Some(r) = self.air.take() else {
            return;
        };
        self.max_air = self.max_air.max(r.steps);
        if r.steps >= 3 {
            self.findings[1].push(Finding {
                id: format!("air-{}-{}", r.start, r.end),
                group: IDS[1].into(),
                round: r.round,
                start_tick: r.start,
                end_tick: r.end,
                target_id: id.into(),
                reason: "Airborne velocity alternated while recorded view stayed nearly fixed."
                    .into(),
                measurements: vec![
                    metric("alternatingSteps", r.steps as f64, "ticks", Some(3.)),
                    metric(
                        "horizontalSpeedGain",
                        r.end_speed - r.start_speed,
                        "HU/s",
                        None,
                    ),
                    metric("maximumViewStep", r.max_view, "deg", Some(0.05)),
                ],
            });
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn player(t: i32) -> PlayerFrame {
        PlayerFrame {
            player_id: "p".into(),
            identity: "pawn".into(),
            identity_key: (1, 2, 3),
            team: 2,
            eye: None,
            view: Some([0., 0.]),
            points: vec![],
            movement: Some(Movement {
                network_tick: 100 + t as u32,
                flags: 0,
                last_jump_tick: 99,
                last_jump_fraction: 0.,
                velocity: [250., 0., 100.],
                origin: [0.; 3],
                move_type: Some(2),
                water_level: Some(0.),
                ladder_surface: Some(-1),
            }),
        }
    }
    fn evaluate(mut rows: Vec<PlayerFrame>) -> Vec<Check> {
        let mut m = Match::new(&["p".into()], 64.).unwrap();
        for (i, p) in rows.drain(..).enumerate() {
            m.push(i as i32, &[p], Some(1)).unwrap();
        }
        m.finish().remove("p").unwrap()
    }
    fn jumps() -> Vec<PlayerFrame> {
        (0..8)
            .map(|t| {
                let mut p = player(t);
                p.movement.as_mut().unwrap().last_jump_tick = if t == 0 {
                    98
                } else {
                    99 + ((t - 1) / 2) * 2 + 1
                };
                p
            })
            .collect()
    }
    #[test]
    fn recorded_jump_chain_requires_continuity_and_environment() {
        let rows = jumps();
        let checks = evaluate(rows.clone());
        assert_eq!(checks[0].findings.len(), 1);
        assert_eq!(checks[0].findings[0].measurements[0].value, 4.);
        for variant in 0..4 {
            let mut r = rows.clone();
            for p in r.iter_mut().skip(3) {
                let m = p.movement.as_mut().unwrap();
                match variant {
                    0 => m.velocity[0] = 100.,
                    1 => m.water_level = None,
                    2 => p.identity_key.1 = 9,
                    _ => m.last_jump_tick = 1,
                }
            }
            assert!(evaluate(r)[0].findings.is_empty());
        }
    }
    #[test]
    fn alternating_air_steps_reset_on_jump_or_view_motion() {
        let rows: Vec<_> = (0..6)
            .map(|t| {
                let mut p = player(t);
                let a: f64 = if t % 2 == 0 { 0. } else { 5. };
                let s = 250. + t as f64;
                p.movement.as_mut().unwrap().velocity =
                    [s * a.to_radians().cos(), s * a.to_radians().sin(), 10.];
                p
            })
            .collect();
        assert_eq!(evaluate(rows.clone())[1].findings.len(), 1);
        let mut r = rows.clone();
        r[2].view = Some([1., 0.]);
        assert!(evaluate(r)[1].findings.is_empty());
        let mut r = rows;
        for p in r.iter_mut().skip(3) {
            p.movement.as_mut().unwrap().last_jump_tick = 102;
        }
        assert!(evaluate(r)[1].findings.is_empty());
    }
    #[test]
    fn takeoff_retention_excludes_in_flight_speed_gain() {
        let mut rows = jumps();
        for (i, p) in rows.iter_mut().enumerate() {
            // The previous airborne sample can exceed the next takeoff speed.
            p.movement.as_mut().unwrap().velocity[0] =
                if i % 2 == 0 { 350. } else { 250. + i as f64 };
        }
        let checks = evaluate(rows);
        assert_eq!(checks[0].findings.len(), 1);
        let retention = &checks[0].findings[0].measurements[1];
        assert!((retention.value - 257. / 255.).abs() < 1e-12);
        assert!(retention.value > 1.);
    }
    #[test]
    fn separate_air_runs_do_not_merge_at_boundary() {
        let headings: [f64; 7] = [0., 5., 0., 5., 10., 5., 10.];
        let rows = headings
            .iter()
            .enumerate()
            .map(|(t, a)| {
                let mut p = player(t as i32);
                let s = 250. + t as f64;
                p.movement.as_mut().unwrap().velocity =
                    [s * a.to_radians().cos(), s * a.to_radians().sin(), 10.];
                p
            })
            .collect();
        let mut checks = evaluate(rows);
        assert_eq!(checks[1].findings.len(), 2);
        assert_eq!(
            (
                checks[1].findings[0].start_tick,
                checks[1].findings[0].end_tick
            ),
            (1, 3)
        );
        assert_eq!(
            (
                checks[1].findings[1].start_tick,
                checks[1].findings[1].end_tick
            ),
            (4, 6)
        );
        super::super::statistics::summarize(&mut checks);
        assert_eq!(checks[1].occurrences.len(), 2);
    }
    #[test]
    fn missing_environment_is_not_a_normal_zero() {
        let mut p = player(0);
        p.movement.as_mut().unwrap().move_type = None;
        let checks = evaluate(vec![p]);
        assert!(checks
            .iter()
            .all(|c| c.state == State::Unavailable && c.summary.is_empty()));
    }
}
