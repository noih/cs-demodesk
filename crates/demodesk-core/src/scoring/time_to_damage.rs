//! TTD clocks consume qualified body-surface visibility, never spotted flags or aim points.
use super::{Check, Definition, Finding, Measurement, State};
use anyhow::{ensure, Result};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Visibility {
    Hidden,
    Visible,
    Unknown,
}

type PawnIdentity = (i32, u32, u32);

/// Entity serial, team and round identify a pair; absent pawns retire their clocks.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Pair {
    pub observer: String,
    pub observer_life: (i32, u32, u32),
    pub target: String,
    pub target_life: (i32, u32, u32),
    pub round: i32,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Sample {
    pub pair: Pair,
    pub last_hidden_tick: i32,
    pub first_visible_tick: i32,
    pub damage_tick: i32,
    pub lower_ms: f64,
    pub upper_ms: f64,
}
#[derive(Default)]
struct Clock {
    last: Option<(i32, Visibility)>,
    last_hidden: Option<i32>,
    onset: Option<(i32, i32)>,
    damaged: bool,
}
pub struct Match {
    rate: f64,
    step: i32,
    clocks: BTreeMap<Pair, Clock>,
    pub samples: Vec<Sample>,
}
impl Match {
    pub fn new(rate: f64, step: i32) -> Result<Self> {
        ensure!(
            rate.is_finite() && rate > 0. && step > 0,
            "invalid visibility clock"
        );
        Ok(Self {
            rate,
            step,
            clocks: BTreeMap::new(),
            samples: vec![],
        })
    }
    /// Feed every live observer/target pair, including unknown visibility, once per sample.
    pub fn visibility(&mut self, pair: Pair, tick: i32, visible: Visibility) -> Result<()> {
        ensure!(
            tick >= 0 && pair.round > 0 && pair.observer != pair.target,
            "invalid visibility pair"
        );
        let clock = self.clocks.entry(pair).or_default();
        ensure!(
            clock.last.is_none_or(|(previous, _)| tick > previous),
            "visibility ticks must increase"
        );
        let contiguous = clock.last.is_some_and(|(previous, _)| {
            i64::from(tick) - i64::from(previous) == i64::from(self.step)
        });
        if !contiguous {
            *clock = Clock::default();
        }
        match visible {
            Visibility::Hidden => {
                clock.last_hidden = Some(tick);
                clock.onset = None;
                clock.damaged = false;
            }
            Visibility::Visible => {
                if let Some(hidden) = clock.last_hidden.take() {
                    clock.onset = Some((hidden, tick));
                }
            }
            // Before first visibility, retain the lower bound on onset. After it,
            // unknown visibility could be a new occlusion, so retire that onset.
            Visibility::Unknown => clock.onset = None,
        }
        clock.last = Some((tick, visible));
        Ok(())
    }
    /// Caller supplies an enemy damage event after visibility at the same sampled tick.
    /// Missing current visibility and occluded damage cannot fabricate a zero TTD.
    pub fn damage(&mut self, pair: &Pair, tick: i32) -> Option<&Sample> {
        self.record_damage(pair, tick)
    }
    fn record_damage(&mut self, pair: &Pair, tick: i32) -> Option<&Sample> {
        let clock = self.clocks.get_mut(pair)?;
        if clock.damaged {
            return None;
        }
        clock.damaged = true;
        clock.last_hidden = None;
        if clock.last != Some((tick, Visibility::Visible)) {
            clock.onset = None;
            return None;
        }
        let (hidden, visible) = clock.onset?;
        self.samples.push(Sample {
            pair: pair.clone(),
            last_hidden_tick: hidden,
            first_visible_tick: visible,
            damage_tick: tick,
            lower_ms: f64::from(tick - visible) * 1000. / self.rate,
            upper_ms: f64::from(tick - hidden) * 1000. / self.rate,
        });
        self.samples.last()
    }
    /// Death removes the live pawn before the packet snapshot. A known onset can
    /// still end at its recorded damage tick; this does not infer visibility there.
    fn death_damage(
        &mut self,
        observer: &str,
        victim: &str,
        life: (i32, u32, u32),
        round: i32,
        tick: i32,
    ) {
        let key = self
            .clocks
            .keys()
            .find(|p| {
                p.observer == observer
                    && p.target == victim
                    && p.observer_life == life
                    && p.round == round
            })
            .cloned();
        let Some(pair) = key else {
            return;
        };
        let clock = self.clocks.get_mut(&pair).unwrap();
        if clock.damaged || clock.last != Some((tick - self.step, Visibility::Visible)) {
            return;
        }
        let Some((hidden, visible)) = clock.onset else {
            return;
        };
        clock.damaged = true;
        self.samples.push(Sample {
            pair,
            last_hidden_tick: hidden,
            first_visible_tick: visible,
            damage_tick: tick,
            lower_ms: f64::from(tick - visible) * 1000. / self.rate,
            upper_ms: f64::from(tick - hidden) * 1000. / self.rate,
        });
    }
    /// Discard retired rounds/lives without keeping a match-length per-frame journal.
    pub fn retain(&mut self, mut live: impl FnMut(&Pair) -> bool) {
        self.clocks.retain(|pair, _| live(pair));
    }
}

fn metric(name: &str, value: f64, unit: &str) -> Measurement {
    Measurement {
        name: name.into(),
        value,
        unit: unit.into(),
        threshold: None,
    }
}
pub fn unavailable(reason: &str) -> Check {
    Check {
        definition: Definition {
            id: "time-to-damage".into(), version: "4-bounded-onset".into(), name: "TTD".into(),
            description: "Estimated time from first unobstructed enemy hitbox in front of the eyes to first firearm damage, using demo poses and installed map physics.".into(),
            category: "reaction".into(),
            parameters: serde_json::json!({
                "frontHalfSpace": true, "shortUpperBoundMsInclusive": 150,
                "requiresContiguousVisibility": true, "firearmDamageOnly": true,
                "unknownBeforeOnset": "widenBounds", "unknownAfterOnset": "discard",
                "bodySource": "reconstructedHitboxCapsules", "geometrySource": "installedMapPhysics",
                "smokeVisibility": "conservativeRecordedGridBounds", "experimental": true
            }),
        },
        state: State::Unavailable, reason: reason.into(), reason_code: "visibilityOnsetMissing".into(),
        evaluated_samples: 0, findings: vec![], observations: vec![], occurrences: vec![],
        summary: vec![], diagnostics: serde_json::Value::Null,
    }
}
impl Match {
    pub fn check(&self, player: &str) -> Check {
        let samples: Vec<_> = self
            .samples
            .iter()
            .filter(|s| s.pair.observer == player)
            .collect();
        let mut check =
            unavailable("No measured first-damage encounters with a qualified visibility onset.");
        if samples.is_empty() {
            return check;
        }
        check.state = State::Passed;
        check.reason.clear();
        check.reason_code.clear();
        check.evaluated_samples = samples.len();
        let mut bounds: Vec<_> = samples.iter().map(|s| s.upper_ms).collect();
        bounds.sort_by(f64::total_cmp);
        for s in samples {
            let event = Finding {
                id: format!(
                    "ttd:{}:{}:{}:{}",
                    player, s.pair.target, s.first_visible_tick, s.damage_tick
                ),
                group: "ttd".into(),
                round: s.pair.round,
                start_tick: s.last_hidden_tick,
                end_tick: s.damage_tick,
                target_id: s.pair.target.clone(),
                reason: "First damage following a qualified body line-of-sight onset.".into(),
                measurements: vec![
                    metric("ttdLowerMs", s.lower_ms, "ms"),
                    metric("ttdUpperMs", s.upper_ms, "ms"),
                ],
            };
            if s.upper_ms <= 150. {
                check.findings.push(event);
            } else {
                check.observations.push(event);
            }
        }
        if !check.findings.is_empty() {
            check.state = State::Findings;
        }
        check.summary.extend([
            metric("measuredTtdEncounters", bounds.len() as f64, "encounters"),
            metric(
                "shortTtdEncounters",
                check.findings.len() as f64,
                "encounters",
            ),
            metric(
                "shortTtdPercent",
                100. * check.findings.len() as f64 / bounds.len() as f64,
                "percent",
            ),
            metric(
                "medianTtdUpperMs",
                if bounds.len() % 2 == 0 {
                    (bounds[bounds.len() / 2 - 1] + bounds[bounds.len() / 2]) * 0.5
                } else {
                    bounds[bounds.len() / 2]
                },
                "ms",
            ),
        ]);
        check
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pair() -> Pair {
        Pair {
            observer: "one".into(),
            target: "two".into(),
            observer_life: (1, 1, 1),
            target_life: (2, 1, 1),
            round: 1,
        }
    }
    #[test]
    fn short_statistics_include_150_ms_boundary() {
        let mut m = Match::new(1000., 1).unwrap();
        for (index, upper) in [119, 120, 150, 151].into_iter().enumerate() {
            let mut p = pair();
            p.target = index.to_string();
            m.samples.push(Sample {
                pair: p,
                last_hidden_tick: 0,
                first_visible_tick: 1,
                damage_tick: upper,
                lower_ms: f64::from(upper - 1),
                upper_ms: f64::from(upper),
            });
        }
        let c = m.check("one");
        assert_eq!(c.findings.len(), 3);
        assert_eq!(
            c.summary
                .iter()
                .find(|m| m.name == "shortTtdEncounters")
                .unwrap()
                .value,
            3.
        );
    }
    #[test]
    fn unknown_before_visibility_widens_bounds_without_losing_the_encounter() {
        let mut m = Match::new(64., 1).unwrap();
        let p = pair();
        for (tick, visibility) in [
            Visibility::Hidden,
            Visibility::Unknown,
            Visibility::Unknown,
            Visibility::Visible,
            Visibility::Visible,
        ]
        .into_iter()
        .enumerate()
        {
            m.visibility(p.clone(), tick as i32, visibility).unwrap();
        }
        let sample = m
            .damage(&p, 4)
            .expect("bounded onset survives unknown ticks");
        assert_eq!((sample.lower_ms, sample.upper_ms), (15.625, 62.5));
        assert!(m.damage(&p, 4).is_none());

        m.visibility(p.clone(), 5, Visibility::Unknown).unwrap();
        m.visibility(p.clone(), 6, Visibility::Visible).unwrap();
        assert!(
            m.damage(&p, 6).is_none(),
            "unknown after visibility cannot restart the encounter"
        );
    }

    #[test]
    fn unknown_after_visibility_invalidates_even_an_undamaged_onset() {
        let mut m = Match::new(64., 1).unwrap();
        let p = pair();
        for (tick, visibility) in [
            Visibility::Hidden,
            Visibility::Visible,
            Visibility::Unknown,
            Visibility::Visible,
        ]
        .into_iter()
        .enumerate()
        {
            m.visibility(p.clone(), tick as i32, visibility).unwrap();
        }
        assert!(m.damage(&p, 3).is_none());
    }

    #[test]
    fn wide_onset_bounds_are_valid_samples_without_becoming_short_findings() {
        let mut m = Match::new(64., 1).unwrap();
        let p = pair();
        m.visibility(p.clone(), 0, Visibility::Hidden).unwrap();
        for tick in 1..20 {
            m.visibility(p.clone(), tick, Visibility::Unknown).unwrap();
        }
        m.visibility(p.clone(), 20, Visibility::Visible).unwrap();
        let sample = m.damage(&p, 20).unwrap();
        assert_eq!((sample.lower_ms, sample.upper_ms), (0., 312.5));
        let check = m.check("one");
        assert_eq!(check.evaluated_samples, 1);
        assert_eq!(check.observations.len(), 1);
        assert!(check.findings.is_empty());
    }

    #[test]
    fn unmeasurable_first_damage_cannot_be_replaced_by_a_later_hit() {
        let mut m = Match::new(64., 1).unwrap();
        let p = pair();
        m.visibility(p.clone(), 0, Visibility::Hidden).unwrap();
        m.visibility(p.clone(), 1, Visibility::Unknown).unwrap();
        assert!(m.damage(&p, 1).is_none());
        m.visibility(p.clone(), 2, Visibility::Visible).unwrap();
        assert!(m.damage(&p, 2).is_none());
        m.visibility(p.clone(), 3, Visibility::Hidden).unwrap();
        m.visibility(p.clone(), 4, Visibility::Unknown).unwrap();
        m.visibility(p.clone(), 5, Visibility::Visible).unwrap();
        let sample = m.damage(&p, 5).unwrap();
        assert_eq!((sample.lower_ms, sample.upper_ms), (0., 31.25));
    }

    #[test]
    fn bounded_onsets_still_require_an_unbroken_same_life_history() {
        let p = pair();
        for gap in [false, true] {
            let mut m = Match::new(64., 1).unwrap();
            m.visibility(p.clone(), 0, Visibility::Hidden).unwrap();
            m.visibility(p.clone(), 1, Visibility::Unknown).unwrap();
            let mut current = p.clone();
            if !gap {
                current.target_life.1 += 1;
            }
            let tick = if gap { 3 } else { 2 };
            m.visibility(current.clone(), tick, Visibility::Visible)
                .unwrap();
            assert!(m.damage(&current, tick).is_none());
        }
        let mut m = Match::new(64., 1).unwrap();
        m.visibility(p.clone(), 0, Visibility::Unknown).unwrap();
        m.visibility(p.clone(), 1, Visibility::Visible).unwrap();
        assert!(m.damage(&p, 1).is_none());
    }

    #[test]
    fn clock_preserves_tick_bounds_and_requires_a_contiguous_visible_onset() {
        let mut m = Match::new(64., 1).unwrap();
        let p = pair();
        m.visibility(p.clone(), 0, Visibility::Visible).unwrap();
        assert!(m.damage(&p, 0).is_none()); // Already visible when sampling begins.
        m.visibility(p.clone(), 1, Visibility::Hidden).unwrap();
        for tick in 2..=8 {
            m.visibility(p.clone(), tick, Visibility::Visible).unwrap();
        }
        let s = m.damage(&p, 8).unwrap();
        assert_eq!((s.lower_ms, s.upper_ms), (93.75, 109.375));
        assert!(m.damage(&p, 8).is_none());
        m.visibility(p.clone(), 9, Visibility::Visible).unwrap();
        assert!(m.damage(&p, 9).is_none());
        m.visibility(p.clone(), 10, Visibility::Unknown).unwrap();
        m.visibility(p.clone(), 11, Visibility::Visible).unwrap();
        assert!(m.damage(&p, 11).is_none());
        m.visibility(p.clone(), 12, Visibility::Hidden).unwrap();
        m.visibility(p.clone(), 14, Visibility::Visible).unwrap();
        assert!(m.damage(&p, 14).is_none());
        m.visibility(p.clone(), 15, Visibility::Hidden).unwrap();
        m.visibility(p.clone(), 16, Visibility::Visible).unwrap();
        let s = m.damage(&p, 16).unwrap();
        assert_eq!((s.lower_ms, s.upper_ms), (0., 15.625));
        assert_eq!(m.samples.len(), 2);
        let mut respawn = p.clone();
        respawn.target_life.2 += 1;
        m.visibility(respawn.clone(), 17, Visibility::Visible)
            .unwrap();
        assert!(m.damage(&respawn, 17).is_none());
        m.visibility(p.clone(), 17, Visibility::Hidden).unwrap();
        assert!(m.damage(&p, 17).is_none());
        assert!(m.visibility(p.clone(), 17, Visibility::Hidden).is_err());
        assert!(m.damage(&p, 18).is_none());
    }
}

/// One shared-frame adapter for every observer/target pair and the damage event index.
pub struct Stream<'a> {
    pub clocks: Match,
    world: Option<&'a crate::analysis::line_of_sight::World>,
    events: BTreeMap<i32, Vec<&'a serde_json::Value>>,
    last_damage: BTreeMap<i32, BTreeMap<String, BTreeMap<String, i32>>>,
    previous_tick: Option<i32>,
    wall_cache: BTreeMap<(PawnIdentity, PawnIdentity), Vec<Option<usize>>>,
    smoke: std::collections::BTreeSet<i64>,
    blind_until: BTreeMap<String, f64>,
    pub unknown_pairs: usize,
    pub unknown_reasons: BTreeMap<&'static str, usize>,
}
impl<'a> Stream<'a> {
    pub fn new(
        rate: f64,
        world: Option<&'a crate::analysis::line_of_sight::World>,
        events: &'a [serde_json::Value],
        rounds: &[crate::model::RoundInfo],
    ) -> Result<Self> {
        let mut indexed = BTreeMap::<i32, Vec<_>>::new();
        let mut last_damage = BTreeMap::<i32, BTreeMap<String, BTreeMap<String, i32>>>::new();
        for event in events {
            if matches!(
                event["event_name"].as_str(),
                Some(
                    "round_start"
                        | "player_hurt"
                        | "player_death"
                        | "player_blind"
                        | "smokegrenade_detonate"
                        | "smokegrenade_expired"
                )
            ) {
                if let Some(tick) = event["tick"].as_i64().and_then(|n| i32::try_from(n).ok()) {
                    indexed.entry(tick).or_default().push(event);
                    if event["event_name"] == "player_hurt"
                        && event["dmg_health"].as_f64().is_some_and(|n| n > 0.)
                        && crate::aim::group(
                            event["weapon"]
                                .as_str()
                                .unwrap_or("")
                                .trim_start_matches("weapon_"),
                        )
                        .is_some()
                    {
                        if let (Some(observer), Some(target)) = (
                            event["attacker_steamid"].as_str(),
                            event["user_steamid"].as_str(),
                        ) {
                            let Some(round) = rounds
                                .iter()
                                .find(|r| r.freeze_end_tick <= tick && tick <= r.end_tick)
                            else {
                                continue;
                            };
                            let last = last_damage
                                .entry(round.round)
                                .or_default()
                                .entry(observer.into())
                                .or_default()
                                .entry(target.into())
                                .or_insert(tick);
                            *last = (*last).max(tick);
                        }
                    }
                }
            }
        }
        Ok(Self {
            clocks: Match::new(rate, 1)?,
            world,
            events: indexed,
            last_damage,
            previous_tick: None,
            wall_cache: BTreeMap::new(),
            smoke: Default::default(),
            blind_until: BTreeMap::new(),
            unknown_pairs: 0,
            unknown_reasons: BTreeMap::new(),
        })
    }
    pub fn push(
        &mut self,
        tick: i32,
        frame: &[crate::analysis::native_body::PlayerFrame],
        round: Option<i32>,
        scene: &crate::analysis::native_body::SceneOcclusion,
    ) -> Result<()> {
        use crate::analysis::line_of_sight::{Bounds, Occlusion};
        let begin = self.previous_tick.map_or(i32::MIN, |t| t.saturating_add(1));
        let events: Vec<_> = self
            .events
            .range(begin..=tick)
            .flat_map(|(_, events)| events.iter().copied())
            .collect();
        for event in &events {
            match event["event_name"].as_str() {
                Some("round_start") => {
                    self.smoke.clear();
                    self.blind_until.clear();
                }
                Some("smokegrenade_detonate") => {
                    self.smoke.insert(event["entityid"].as_i64().unwrap_or(-1));
                }
                Some("smokegrenade_expired") => {
                    self.smoke.remove(&event["entityid"].as_i64().unwrap_or(-1));
                }
                Some("player_blind") => {
                    if let (Some(player), Some(duration), Some(at)) = (
                        event["user_steamid"].as_str(),
                        event["blind_duration"].as_f64(),
                        event["tick"].as_i64(),
                    ) {
                        if duration.is_finite() && duration > 0. {
                            let until = at as f64 + duration * self.clocks.rate;
                            let old = self.blind_until.entry(player.into()).or_insert(until);
                            *old = old.max(until);
                        }
                    }
                }
                _ => {}
            }
        }
        let Some(round) = round else {
            self.clocks.retain(|_| false);
            self.wall_cache.clear();
            self.previous_tick = Some(tick);
            return Ok(());
        };
        for event in events.iter().filter(|e| {
            e["event_name"] == "player_hurt"
                && e["tick"].as_i64() == Some(i64::from(tick))
                && e["dmg_health"].as_f64().is_some_and(|d| d > 0.)
                && crate::aim::group(
                    e["weapon"]
                        .as_str()
                        .unwrap_or("")
                        .trim_start_matches("weapon_"),
                )
                .is_some()
        }) {
            let victim = event["user_steamid"].as_str().unwrap_or("");
            if events.iter().any(|e| {
                e["event_name"] == "player_death"
                    && e["tick"].as_i64() == Some(i64::from(tick))
                    && e["user_steamid"] == victim
            }) {
                if let Some(observer) = frame
                    .iter()
                    .find(|p| p.player_id == event["attacker_steamid"].as_str().unwrap_or(""))
                {
                    self.clocks.death_damage(
                        &observer.player_id,
                        victim,
                        observer.identity_key,
                        round,
                        tick,
                    );
                }
            }
        }
        let pair = |observer: &crate::analysis::native_body::PlayerFrame,
                    target: &crate::analysis::native_body::PlayerFrame| Pair {
            observer: observer.player_id.clone(),
            observer_life: observer.identity_key,
            target: target.player_id.clone(),
            target_life: target.identity_key,
            round,
        };
        let smoke_bounds: Option<Vec<_>> = self
            .smoke
            .iter()
            .map(|id| scene.smoke_bounds.get(id).and_then(Option::as_ref))
            .collect();
        let blockers: Vec<_> = if smoke_bounds.is_some() && !scene.unbounded && self.world.is_some()
        {
            frame
                .iter()
                .map(|player| {
                    (
                        player.identity_key,
                        player
                            .capsules
                            .iter()
                            .map(|c| Bounds {
                                min: std::array::from_fn(|i| c.a[i].min(c.b[i]) - c.radius),
                                max: std::array::from_fn(|i| c.a[i].max(c.b[i]) + c.radius),
                            })
                            .collect::<Vec<_>>(),
                    )
                })
                .collect()
        } else {
            vec![]
        };
        for observer in frame {
            for target in frame.iter().filter(|p| p.team != observer.team) {
                // Visibility after the final damage (or for a pair with no damage) cannot produce TTD.
                if self
                    .last_damage
                    .get(&round)
                    .and_then(|players| players.get(&observer.player_id))
                    .and_then(|targets| targets.get(&target.player_id))
                    .is_none_or(|last| *last < tick)
                {
                    continue;
                }
                let visibility = if let (Some(world), Some(eye), Some(view)) =
                    (self.world, observer.eye, observer.view)
                {
                    if smoke_bounds.is_none()
                        || scene.unbounded
                        || self
                            .blind_until
                            .get(&observer.player_id)
                            .is_some_and(|end| *end >= f64::from(tick))
                    {
                        let reason = if scene.unbounded {
                            "dynamicBoundsMissing"
                        } else if smoke_bounds.is_none() {
                            "smokeBoundsMissing"
                        } else {
                            "blinded"
                        };
                        *self.unknown_reasons.entry(reason).or_default() += 1;
                        Visibility::Unknown
                    } else {
                        let pitch = view[0].to_radians();
                        let yaw = view[1].to_radians();
                        let forward = [
                            pitch.cos() * yaw.cos(),
                            pitch.cos() * yaw.sin(),
                            -pitch.sin(),
                        ];
                        let result = world.body_with_clearance(
                            eye,
                            forward,
                            &target.capsules,
                            blockers.iter().all(|(id, bounds)| {
                                *id == observer.identity_key
                                    || *id == target.identity_key
                                    || !bounds.is_empty()
                            }),
                            self.wall_cache
                                .entry((observer.identity_key, target.identity_key))
                                .or_default(),
                            |start, end| {
                                // Cloud intersections stay unknown, including possible HE/bullet openings.
                                if smoke_bounds.as_ref().is_some_and(|bounds| {
                                    bounds.iter().any(|b| b.intersects(start, end))
                                }) || scene.uncertain.iter().any(|b| b.intersects(start, end))
                                {
                                    return false;
                                }
                                for (_, bounds) in blockers.iter().filter(|(id, _)| {
                                    *id != observer.identity_key && *id != target.identity_key
                                }) {
                                    if bounds.is_empty()
                                        || bounds.iter().any(|b| b.intersects(start, end))
                                    {
                                        return false;
                                    }
                                }
                                true
                            },
                        );
                        match result {
                            Occlusion::Clear => Visibility::Visible,
                            Occlusion::Blocked => Visibility::Hidden,
                            Occlusion::Unknown => {
                                *self
                                    .unknown_reasons
                                    .entry(if target.capsules.is_empty() {
                                        "bodyMissing"
                                    } else {
                                        "unresolvedGeometry"
                                    })
                                    .or_default() += 1;
                                Visibility::Unknown
                            }
                        }
                    }
                } else {
                    *self.unknown_reasons.entry("eyeOrWorldMissing").or_default() += 1;
                    Visibility::Unknown
                };
                if visibility == Visibility::Unknown {
                    self.unknown_pairs += 1;
                }
                self.clocks
                    .visibility(pair(observer, target), tick, visibility)?;
            }
        }
        for event in events.iter().filter(|e| {
            e["event_name"] == "player_hurt"
                && e["tick"].as_i64() == Some(i64::from(tick))
                && e["dmg_health"].as_f64().is_some_and(|d| d > 0.)
                && crate::aim::group(
                    e["weapon"]
                        .as_str()
                        .unwrap_or("")
                        .strip_prefix("weapon_")
                        .unwrap_or(e["weapon"].as_str().unwrap_or("")),
                )
                .is_some()
        }) {
            let attacker = event["attacker_steamid"].as_str().unwrap_or("");
            let victim = event["user_steamid"].as_str().unwrap_or("");
            let Some(observer) = frame.iter().find(|p| p.player_id == attacker) else {
                continue;
            };
            if let Some(target) = frame
                .iter()
                .find(|p| p.player_id == victim && p.team != observer.team)
            {
                self.clocks.damage(&pair(observer, target), tick);
            }
        }
        self.clocks.retain(|p| {
            p.round == round
                && frame.iter().any(|f| f.identity_key == p.observer_life)
                && frame.iter().any(|f| f.identity_key == p.target_life)
        });
        self.previous_tick = Some(tick);
        Ok(())
    }
}

#[cfg(test)]
mod stream_tests {
    use super::*;
    use crate::analysis::{
        line_of_sight::{Capsule, Triangle, World},
        native_body::{PlayerFrame, SceneOcclusion},
    };
    fn round(round: i32, start: i32, end: i32) -> crate::model::RoundInfo {
        crate::model::RoundInfo {
            round,
            start_tick: start,
            freeze_end_tick: start,
            end_tick: end,
            officially_ended_tick: end,
            winner: None,
            reason: String::new(),
            roster: BTreeMap::new(),
            bomb_planted_tick: None,
            bomb_defused_tick: None,
            bomb_defuser: None,
            bomb_exploded_tick: None,
        }
    }
    fn frame(x: f64) -> Vec<PlayerFrame> {
        vec![("one", 1, 2, [0., 0., 0.]), ("two", 2, 3, [x, 0., 0.])]
            .into_iter()
            .map(|(id, entity, team, position)| PlayerFrame {
                simulation_tick: None,
                movement: None,
                player_id: id.into(),
                identity: id.into(),
                identity_key: (entity, 1, team),
                team,
                eye: Some(position),
                view: Some([0., 0.]),
                points: vec![],
                hitbox_set: None,
                hitbox_transforms: vec![],
                capsules: vec![Capsule {
                    a: position,
                    b: [position[0], 0., 1.],
                    radius: 0.2,
                }],
            })
            .collect()
    }
    #[test]
    fn wall_seam_and_unknown_transition_produce_one_bounded_first_damage() {
        let world = World::new(vec![
            Triangle {
                vertices: [[5., -10., -10.], [5., 10., -10.], [5., 10., 10.]],
                opaque: true,
            },
            Triangle {
                vertices: [[5., -10., -10.], [5., 10., 10.], [5., -10., 10.]],
                opaque: true,
            },
        ])
        .unwrap();
        for fatal in [false, true] {
            let mut events = vec![
                serde_json::json!({"event_name":"player_hurt","tick":4,"attacker_steamid":"one","user_steamid":"two","dmg_health":20,"weapon":"ak47"}),
            ];
            if fatal {
                events.push(
                    serde_json::json!({"event_name":"player_death","tick":4,"user_steamid":"two"}),
                );
            }
            let mut stream = Stream::new(64., Some(&world), &events, &[round(1, 0, 100)]).unwrap();
            for tick in 0..=4 {
                let mut players = frame(if tick == 0 { 10. } else { 4. });
                let mut scene = SceneOcclusion::default();
                if tick == 1 || tick == 2 {
                    scene
                        .uncertain
                        .push(crate::analysis::line_of_sight::Bounds {
                            min: [1., -10., -10.],
                            max: [2., 10., 10.],
                        });
                }
                if fatal && tick == 4 {
                    players.pop();
                }
                stream.push(tick, &players, Some(1), &scene).unwrap();
            }
            assert_eq!(stream.clocks.samples.len(), 1);
            let sample = &stream.clocks.samples[0];
            assert_eq!((sample.lower_ms, sample.upper_ms), (15.625, 62.5));
        }
    }

    #[test]
    fn shared_frames_join_first_damage_and_ignore_repeated_hurt_events() {
        let world = World::new(vec![Triangle {
            vertices: [[5., -100., -100.], [5., 100., -100.], [5., 0., 100.]],
            opaque: true,
        }])
        .unwrap();
        let events = vec![
            serde_json::json!({"event_name":"player_hurt","tick":7,"attacker_steamid":"one","user_steamid":"two","dmg_health":20,"weapon":"ak47"}),
            serde_json::json!({"event_name":"player_hurt","tick":8,"attacker_steamid":"one","user_steamid":"two","dmg_health":20,"weapon":"ak47"}),
        ];
        let mut stream = Stream::new(64., Some(&world), &events, &[round(1, 0, 100)]).unwrap();
        stream
            .push(0, &frame(10.), Some(1), &SceneOcclusion::default())
            .unwrap();
        for tick in 1..=8 {
            stream
                .push(tick, &frame(4.), Some(1), &SceneOcclusion::default())
                .unwrap();
        }
        let check = stream.clocks.check("one");
        assert_eq!(check.evaluated_samples, 1);
        assert_eq!(check.findings.len(), 1);
        assert_eq!(stream.clocks.samples[0].upper_ms, 109.375);
        assert!(stream
            .clocks
            .samples
            .iter()
            .all(|s| s.pair.observer == "one")); // Second player's enemy is behind them.
        let mut checks = vec![check];
        super::super::statistics::summarize(&mut checks);
        assert_eq!(checks[0].occurrences.len(), 1);
    }
    #[test]
    fn smoke_only_excludes_rays_that_might_cross_its_recorded_grid() {
        let world = World::new(vec![Triangle {
            vertices: [[5., -100., -100.], [5., 100., -100.], [5., 0., 100.]],
            opaque: true,
        }])
        .unwrap();
        let events = vec![
            serde_json::json!({"event_name":"smokegrenade_detonate","tick":0,"entityid":7}),
            serde_json::json!({"event_name":"player_hurt","tick":7,"attacker_steamid":"one","user_steamid":"two","dmg_health":20,"weapon":"ak47"}),
        ];
        for (origin, expected) in [
            (Some([1000., 1000., 0.]), 1),
            (Some([1000., 0., 0.]), 1),
            (Some([0.; 3]), 0),
            (None, 0),
        ] {
            let mut scene = SceneOcclusion::default();
            if let Some(origin) = origin {
                scene
                    .smoke_bounds
                    .insert(7, crate::analysis::smoke::bounds(origin));
            }
            let mut stream = Stream::new(64., Some(&world), &events, &[round(1, 0, 100)]).unwrap();
            stream.push(0, &frame(10.), Some(1), &scene).unwrap();
            for tick in 1..=7 {
                stream.push(tick, &frame(4.), Some(1), &scene).unwrap();
            }
            assert_eq!(stream.clocks.samples.len(), expected, "origin: {origin:?}");
        }
    }

    #[test]
    fn round_reset_and_recorded_death_finish_a_known_exposure() {
        let world = World::new(vec![Triangle {
            vertices: [[5., -100., -100.], [5., 100., -100.], [5., 0., 100.]],
            opaque: true,
        }])
        .unwrap();
        let events = vec![
            serde_json::json!({"event_name":"smokegrenade_detonate","tick":0,"entityid":7}),
            serde_json::json!({"event_name":"round_start","tick":1}),
            serde_json::json!({"event_name":"player_hurt","tick":7,"attacker_steamid":"one","user_steamid":"two","dmg_health":100,"weapon":"ak47"}),
            serde_json::json!({"event_name":"player_death","tick":7,"user_steamid":"two"}),
        ];
        let mut stream = Stream::new(
            64.,
            Some(&world),
            &events,
            &[round(1, 0, 0), round(2, 1, 100)],
        )
        .unwrap();
        stream
            .push(0, &frame(10.), Some(1), &SceneOcclusion::default())
            .unwrap();
        stream
            .push(1, &frame(10.), Some(2), &SceneOcclusion::default())
            .unwrap();
        for tick in 2..7 {
            stream
                .push(tick, &frame(4.), Some(2), &SceneOcclusion::default())
                .unwrap();
        }
        let mut live = frame(4.);
        live.pop().unwrap();
        stream
            .push(7, &live, Some(2), &SceneOcclusion::default())
            .unwrap();
        assert_eq!(stream.clocks.samples.len(), 1);
        assert_eq!(stream.clocks.samples[0].upper_ms, 93.75);
    }
}
