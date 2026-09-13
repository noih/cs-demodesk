//! Counts one-tick spherical view turns coinciding with recorded firearm shots.
use super::{Check, Definition, Finding, Measurement, State};
use crate::analysis::{event_context::Shot, native_body::PlayerFrame};
use anyhow::{ensure, Result};
use std::collections::BTreeMap;
pub const RULE_ID: &str = "shot-synchronous-view-turn";
const MIN_TURN: f64 = 30.0;
const MAX_RETURN_ERROR: f64 = 3.0;
#[derive(Clone, Copy)]
struct Sample {
    tick: i32,
    round: i32,
    identity: (i32, u32, u32),
    view: [f64; 2],
}
#[derive(Default)]
struct Player {
    last: Option<Sample>,
    pending: Option<(usize, Sample, Sample)>,
    findings: Vec<Finding>,
    source_shots: usize,
    measured: usize,
    total_speed: f64,
}
pub struct Match {
    rate: f64,
    source_registered: bool,
    players: BTreeMap<String, Player>,
}
fn angle(a: [f64; 2], b: [f64; 2]) -> f64 {
    let (p, q, y) = (
        a[0].to_radians(),
        b[0].to_radians(),
        (a[1] - b[1]).to_radians(),
    );
    (p.sin() * q.sin() + p.cos() * q.cos() * y.cos())
        .clamp(-1.0, 1.0)
        .acos()
        .to_degrees()
}
fn measurement(name: &str, value: f64, unit: &str, threshold: Option<f64>) -> Measurement {
    Measurement {
        name: name.into(),
        value,
        unit: unit.into(),
        threshold,
    }
}
impl Match {
    pub fn new(players: &[String], rate: f64) -> Result<Self> {
        ensure!(
            rate.is_finite() && rate > 0.0,
            "Invalid shot-view tick rate"
        );
        Ok(Self {
            rate,
            source_registered: false,
            players: players
                .iter()
                .map(|id| (id.clone(), Player::default()))
                .collect(),
        })
    }
    /// Count source shots independently of missing packet/view samples.
    pub fn register_shots(
        &mut self,
        shots: &BTreeMap<i32, Vec<Shot>>,
        rounds: &[crate::model::RoundInfo],
    ) {
        self.source_registered = true;
        for state in self.players.values_mut() {
            state.source_shots = 0;
        }
        for (&tick, shots) in shots {
            let Some(round) = rounds
                .iter()
                .find(|r| tick >= r.freeze_end_tick && tick <= r.end_tick)
            else {
                continue;
            };
            let mut seen = std::collections::BTreeSet::new();
            for shot in shots {
                if shot.tick != tick
                    || !round.roster.contains_key(&shot.player_id)
                    || crate::aim::group(
                        shot.weapon.strip_prefix("weapon_").unwrap_or(&shot.weapon),
                    )
                    .is_none()
                    || !seen.insert(&shot.player_id)
                {
                    continue;
                }
                if let Some(state) = self.players.get_mut(&shot.player_id) {
                    state.source_shots += 1;
                }
            }
        }
    }
    pub fn push(
        &mut self,
        tick: i32,
        frame: &[PlayerFrame],
        round: Option<i32>,
        shots: &[Shot],
    ) -> Result<()> {
        ensure!(tick >= 0, "Invalid shot-view tick");
        for (id, state) in &mut self.players {
            // Parser fire_bullets.round may already advance on a winning shot.
            // Shared live RoundInfo bounds determine this observation round.
            let shot = round.filter(|r| *r > 0).and_then(|_| {
                shots.iter().find(|s| {
                    s.tick == tick
                        && s.player_id == *id
                        && crate::aim::group(s.weapon.strip_prefix("weapon_").unwrap_or(&s.weapon))
                            .is_some()
                })
            });
            if shot.is_some() && !self.source_registered {
                state.source_shots += 1;
            }
            let current = round.filter(|r| *r > 0).and_then(|round| {
                let p = frame
                    .iter()
                    .find(|p| p.player_id == *id && matches!(p.team, 2 | 3))?;
                let view = p.view?;
                (view.iter().all(|a| a.is_finite()) && view[0].abs() <= 90.0).then_some(Sample {
                    tick,
                    round,
                    identity: p.identity_key,
                    view,
                })
            });
            let continuous = |a: Sample, b: Sample| {
                b.tick - a.tick == 1 && a.round == b.round && a.identity == b.identity
            };
            if let Some((index, before, at)) = state.pending.take() {
                if let Some(after) = current.filter(|after| continuous(at, *after)) {
                    let finding = &mut state.findings[index];
                    finding.end_tick = after.tick;
                    finding.measurements.push(measurement(
                        "postShotTurnDegrees",
                        angle(at.view, after.view),
                        "degrees",
                        None,
                    ));
                    finding.measurements.push(measurement(
                        "returnToPreShotDegrees",
                        angle(before.view, after.view),
                        "degrees",
                        None,
                    ));
                    finding.measurements.push(measurement(
                        "recoveryObservationSeconds",
                        1.0 / self.rate,
                        "seconds",
                        None,
                    ));
                }
            }
            if let (Some(previous), Some(current)) = (state.last, current) {
                ensure!(tick > previous.tick, "Shot-view ticks must increase");
                if continuous(previous, current) {
                    if let Some(shot) = shot {
                        let turn = angle(previous.view, current.view);
                        let speed = turn * self.rate;
                        state.measured += 1;
                        state.total_speed += speed;
                        if turn >= MIN_TURN {
                            let mut measurements = vec![
                                measurement(
                                    "sphericalTurnDegrees",
                                    turn,
                                    "degrees",
                                    Some(MIN_TURN),
                                ),
                                measurement(
                                    "turnDurationSeconds",
                                    1.0 / self.rate,
                                    "seconds",
                                    None,
                                ),
                                measurement(
                                    "angularSpeedDegreesPerSecond",
                                    speed,
                                    "degrees/second",
                                    None,
                                ),
                            ];
                            if let Some(direction) = shot.direction {
                                measurements.push(measurement(
                                    "shotDirectionToPacketViewDegrees",
                                    angle(direction, current.view),
                                    "degrees",
                                    None,
                                ));
                            }
                            let index = state.findings.len();
                            state.findings.push(Finding {
                                id:format!("{RULE_ID}:{id}:{tick}"),group: RULE_ID.into(),round:current.round,
                                start_tick:previous.tick,end_tick:tick,target_id:String::new(),
                                reason:"A recorded firearm shot coincided with a spherical view turn of at least 30 degrees within one packet tick.".into(),measurements,
                            });
                            state.pending = Some((index, previous, current));
                        }
                    }
                }
            }
            state.last = current;
        }
        Ok(())
    }
    pub fn finish(self) -> BTreeMap<String, Check> {
        self.players.into_iter().map(|(id,mut p)| {
            p.findings.retain(|f| f.measurements.iter().any(|m| m.name == "returnToPreShotDegrees" && m.value <= MAX_RETURN_ERROR)
                && f.measurements.iter().any(|m| m.name == "postShotTurnDegrees" && m.value >= MIN_TURN));
            let count=p.findings.len();
            let mut summary=vec![measurement("recordedFirearmShots",p.source_shots as f64,"shots",None),measurement("measuredShots",p.measured as f64,"shots",None),measurement("unmeasuredShots",p.source_shots.saturating_sub(p.measured) as f64,"shots",None),measurement("synchronousTurnShots",count as f64,"shots",None)];
            if p.measured>0 {
                summary.push(measurement("meanShotAngularSpeed",p.total_speed/p.measured as f64,"degrees/second",None));
                summary.push(measurement("synchronousTurnRate",count as f64/p.measured as f64,"ratio",None));
            }
            (id,Check {
                definition:Definition {id:RULE_ID.into(),version:"3-shot-view-return".into(),name:"射擊同步急轉".into(),
                    description:"Requires a shot-tick turn of at least 30 degrees and a next-tick return within 3 degrees of the preceding view.".into(),
                    category:"view-motion".into(),parameters:serde_json::json!({"minimumTurnDegrees":MIN_TURN,"maximumReturnErrorDegrees":MAX_RETURN_ERROR,"requiresNextTickReturn":true,"maximumStepTicks":1,"tickRate":self.rate})},
                state:if p.measured==0 {State::Unavailable}else if count==0 {State::Passed}else{State::Findings},
                reason:if p.measured==0 {"No firearm shots had consecutive same-life live-round view samples."}else{"Recorded shot-synchronous view-turn counts; not a determination of cheating."}.into(),
                reason_code:if p.measured==0 {"shotViewMissing"}else{"measuredBehavior"}.into(),evaluated_samples:p.measured,
                findings:p.findings,observations:vec![],occurrences:vec![],summary,diagnostics:serde_json::Value::Null,
            })
        }).collect()
    }
}
pub fn unavailable() -> Check {
    Match::new(&[String::new()], 1.0)
        .expect("valid constant rate")
        .finish()
        .remove("")
        .expect("one player")
}
#[cfg(test)]
mod tests {
    use super::*;
    fn frame(_tick: i32, pitch: f64, yaw: f64, serial: u32) -> Vec<PlayerFrame> {
        vec![PlayerFrame {
            movement: None,
            player_id: "a".into(),
            identity: format!("1:{serial}:2"),
            identity_key: (1, serial, 2),
            team: 2,
            eye: None,
            view: Some([pitch, yaw]),
            points: vec![],
        }]
    }
    fn shot(tick: i32) -> Shot {
        Shot {
            tick,
            player_id: "a".into(),
            weapon: "weapon_ak47".into(),
            round: Some(1),
            direction: Some([0.0, 90.0]),
        }
    }
    #[test]
    fn shot_turn_is_one_count_with_recovery_and_independent_player_denominators() {
        let mut m = Match::new(&["a".into(), "b".into()], 64.0).unwrap();
        m.push(1, &frame(1, 0., 0., 1), Some(1), &[]).unwrap();
        m.push(2, &frame(2, 0., 90., 1), Some(1), &[shot(2), shot(2)])
            .unwrap();
        m.push(3, &frame(3, 0., 0., 1), Some(1), &[]).unwrap();
        let r = m.finish();
        assert_eq!(r["a"].findings.len(), 1);
        assert_eq!(r["a"].evaluated_samples, 1);
        assert_eq!(r["b"].evaluated_samples, 0);
        let f = &r["a"].findings[0];
        assert_eq!(f.start_tick, 1);
        assert_eq!(f.end_tick, 3);
        assert!(f.target_id.is_empty());
        assert_eq!(
            f.measurements
                .iter()
                .find(|v| v.name == "returnToPreShotDegrees")
                .unwrap()
                .value,
            0.0
        );
        assert_eq!(
            f.measurements
                .iter()
                .find(|v| v.name == "angularSpeedDegreesPerSecond")
                .unwrap()
                .value,
            5760.0
        );
    }
    #[test]
    fn ordinary_flick_and_missing_recovery_do_not_count() {
        for after in [None, Some((3, 90.)), Some((3, 60.)), Some((4, 0.))] {
            let mut m = Match::new(&["a".into()], 64.).unwrap();
            m.push(1, &frame(1, 0., 0., 1), Some(1), &[]).unwrap();
            m.push(2, &frame(2, 0., 90., 1), Some(1), &[shot(2)]).unwrap();
            if let Some((tick,yaw)) = after { m.push(tick, &frame(tick, 0., yaw, 1), Some(1), &[]).unwrap(); }
            let r = m.finish();
            assert!(r["a"].findings.is_empty());
            assert_eq!(r["a"].evaluated_samples, 1);
        }
    }

    #[test]
    fn ledger_denominator_includes_shots_without_any_packet_callback() {
        let round = crate::model::RoundInfo {
            round: 1,
            start_tick: 0,
            freeze_end_tick: 1,
            end_tick: 3,
            officially_ended_tick: 3,
            winner: None,
            reason: String::new(),
            roster: BTreeMap::from([("a".into(), crate::model::Team::Ct)]),
            bomb_planted_tick: None,
            bomb_defused_tick: None,
            bomb_defuser: None,
            bomb_exploded_tick: None,
        };
        let mut m = Match::new(&["a".into()], 64.).unwrap();
        m.register_shots(
            &BTreeMap::from([(0, vec![shot(0)]), (2, vec![shot(2), shot(2)])]),
            &[round],
        );
        let results = m.finish();
        let check = &results["a"];
        assert_eq!(check.evaluated_samples, 0);
        assert_eq!(
            check
                .summary
                .iter()
                .find(|v| v.name == "recordedFirearmShots")
                .unwrap()
                .value,
            1.
        );
        assert_eq!(
            check
                .summary
                .iter()
                .find(|v| v.name == "unmeasuredShots")
                .unwrap()
                .value,
            1.
        );
    }
    #[test]
    fn winning_shot_without_recovery_remains_a_sample_not_a_finding() {
        let mut m = Match::new(&["a".into()], 64.).unwrap();
        m.push(1, &[], Some(1), &[shot(1)]).unwrap();
        m.push(2, &frame(2, 0., 0., 1), Some(1), &[]).unwrap();
        let mut winning = shot(3);
        winning.round = Some(2);
        m.push(3, &frame(3, 0., 90., 1), Some(1), &[winning])
            .unwrap();
        let results = m.finish();
        let check = &results["a"];
        assert!(check.findings.is_empty());
        assert_eq!(check.evaluated_samples, 1);
        assert_eq!(
            check
                .summary
                .iter()
                .find(|v| v.name == "recordedFirearmShots")
                .unwrap()
                .value,
            2.
        );
        assert_eq!(
            check
                .summary
                .iter()
                .find(|v| v.name == "unmeasuredShots")
                .unwrap()
                .value,
            1.
        );
    }
    #[test]
    fn poles_wraparound_gaps_round_changes_and_respawns_do_not_create_turns() {
        assert!(angle([89.9, 0.0], [89.9, 180.0]) < 1.0);
        assert!(angle([0.0, 179.0], [0.0, -179.0]) < 3.0);
        let mut m = Match::new(&["a".into()], 64.).unwrap();
        m.push(1, &frame(1, 0., 0., 1), Some(1), &[]).unwrap();
        m.push(3, &frame(3, 0., 90., 1), Some(1), &[shot(3)])
            .unwrap();
        m.push(4, &frame(4, 0., 0., 2), Some(1), &[shot(4)])
            .unwrap();
        m.push(5, &frame(5, 0., 90., 2), Some(2), &[shot(5)])
            .unwrap();
        let r = m.finish();
        assert_eq!(r["a"].evaluated_samples, 0);
        assert!(r["a"].findings.is_empty());
    }
}
