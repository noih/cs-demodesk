//! Incremental measurements: retain acquisition lookback and lock aggregates, never a full track.
use super::*;
use std::collections::{BTreeMap, VecDeque};

type Key = (i32, String, String);
pub struct Stream {
    input: Input,
    parameters: Parameters,
    tracks: BTreeMap<Key, Tracking>,
    evidence: Vec<Evidence>,
    evaluated: usize,
}
impl Stream {
    pub fn new(input: &Input, parameters: &Parameters) -> Result<Self> {
        let metadata = Input {
            demo_fingerprint: input.demo_fingerprint.clone(),
            player_id: input.player_id.clone(),
            measurement_source: input.measurement_source.clone(),
            tick_rate: input.tick_rate,
            sample_step_ticks: input.sample_step_ticks,
            angular_resolution_degrees: input.angular_resolution_degrees,
            tracks: vec![],
        };
        validate(&metadata, parameters)?;
        Ok(Self {
            input: metadata,
            parameters: parameters.clone(),
            tracks: BTreeMap::new(),
            evidence: vec![],
            evaluated: 0,
        })
    }
    /// Chunks may split any track. Missing ticks break continuity; repeated ticks are errors.
    /// An error invalidates this stream; the caller must discard it rather than publish partial results.
    pub fn push(&mut self, input: &Input) -> Result<()> {
        validate(input, &self.parameters)?;
        ensure!(
            input.demo_fingerprint == self.input.demo_fingerprint
                && input.player_id == self.input.player_id
                && input.measurement_source == self.input.measurement_source
                && input.tick_rate == self.input.tick_rate
                && input.sample_step_ticks == self.input.sample_step_ticks
                && input.angular_resolution_degrees == self.input.angular_resolution_degrees,
            "stream metadata changed"
        );
        let sufficient = f64::from(input.sample_step_ticks) / input.tick_rate
            <= self.parameters.max_sample_seconds
            && input.angular_resolution_degrees * 2.0 <= self.parameters.max_error_degrees;
        for track in &input.tracks {
            let key = (track.round, track.target_id.clone(), track.point_id.clone());
            let state = self
                .tracks
                .entry(key)
                .or_insert_with(|| Tracking::new(track));
            ensure!(
                state.identity.enemy == track.enemy,
                "track relationship changed without a new identity"
            );
            for sample in &track.samples {
                ensure!(
                    state.last_tick.is_none_or(|tick| sample.tick > tick),
                    "stream ticks must increase"
                );
                if state.last_tick.is_some_and(|tick| {
                    i64::from(sample.tick) - i64::from(tick) != i64::from(input.sample_step_ticks)
                }) {
                    state.close(&mut self.evidence, input.tick_rate, &self.parameters);
                    state.count = 0;
                    state.pending.clear();
                    state.recent.clear();
                }
                state.last_tick = Some(sample.tick);
                if !sufficient || !track.enemy {
                    continue;
                }
                state.count += 1;
                self.evaluated += match state.count {
                    3 => 3,
                    n if n > 3 => 1,
                    _ => 0,
                };
                state.push(
                    sample,
                    &mut self.evidence,
                    input.tick_rate,
                    &self.parameters,
                );
            }
        }
        Ok(())
    }
    pub fn finish(mut self) -> Report {
        for track in self.tracks.values_mut() {
            track.close(&mut self.evidence, self.input.tick_rate, &self.parameters);
        }
        report(&self.input, &self.parameters, self.evaluated, self.evidence)
    }
}

struct Tracking {
    identity: Track,
    last_tick: Option<i32>,
    count: usize,
    recent: VecDeque<Sample>,
    held: Option<Held>,
    pending: Vec<Evidence>,
}
impl Tracking {
    fn new(track: &Track) -> Self {
        Self {
            identity: Track {
                round: track.round,
                target_id: track.target_id.clone(),
                point_id: track.point_id.clone(),
                enemy: track.enemy,
                samples: vec![],
            },
            last_tick: None,
            count: 0,
            recent: VecDeque::new(),
            held: None,
            pending: vec![],
        }
    }
    fn push(&mut self, sample: &Sample, output: &mut Vec<Evidence>, rate: f64, p: &Parameters) {
        if self.count >= 3 {
            output.append(&mut self.pending);
        }
        if angle(view(sample), target(sample)) <= p.max_error_degrees {
            if let Some(held) = &mut self.held {
                held.push(sample, rate);
            } else {
                self.held = Some(Held::new(&self.identity, &self.recent, sample, rate, p));
            }
            self.recent.clear();
        } else {
            self.close(output, rate, p);
            self.recent.push_back(sample.clone());
            while self.recent.front().is_some_and(|first| {
                seconds(first.tick, sample.tick, rate) > p.acquisition_window_seconds
            }) {
                self.recent.pop_front();
            }
        }
    }
    fn close(&mut self, output: &mut Vec<Evidence>, rate: f64, p: &Parameters) {
        if let Some(held) = self.held.take() {
            if let Some(e) = held.finish(rate, p) {
                self.pending.push(e);
            }
        }
        if self.count >= 3 {
            output.append(&mut self.pending);
        }
        // A short segment can still become valid at its next sample. Gaps clear it below.
    }
}
struct Held {
    evidence: Evidence,
    last: Sample,
    view_travel: f64,
    runs: [f64; 3],
    longest: [f64; 3],
    all_visible: bool,
}
impl Held {
    fn new(
        track: &Track,
        recent: &VecDeque<Sample>,
        sample: &Sample,
        rate: f64,
        p: &Parameters,
    ) -> Self {
        let mut e = Evidence {
            round: track.round,
            target_id: track.target_id.clone(),
            point_id: track.point_id.clone(),
            start_tick: sample.tick,
            lock_tick: sample.tick,
            end_tick: sample.tick,
            acquisition_degrees: 0.0,
            acquisition_seconds: 0.0,
            acquisition_speed: 0.0,
            straightness: 0.0,
            follow_seconds: 0.0,
            follow_travel_degrees: 0.0,
            max_error_degrees: angle(view(sample), target(sample)),
            rapid_acquisition: false,
            sustained_follow: false,
            obstruction: Obstruction::Unknown,
            obstruction_seconds: 0.0,

            sample_count: 1,
            samples: vec![sample.clone()],
        };
        // Only the short acquisition window is materialized; the held interval is aggregated.
        let mut acquisition: Vec<_> = recent.iter().cloned().collect();
        acquisition.push(sample.clone());
        for candidate in (0..recent.len()).rev() {
            let first = &acquisition[candidate];
            let dt = seconds(first.tick, sample.tick, rate);
            if dt > p.acquisition_window_seconds {
                break;
            }
            let moved = angle(view(first), view(sample));
            if moved < p.min_acquisition_degrees
                || angle(view(first), target(first)) < p.min_acquisition_degrees
            {
                continue;
            }
            let ratio = (moved / travel(&acquisition[candidate..], view)).min(1.0);
            let velocity = moved / dt;
            if velocity >= p.snap_speed
                || (acquisition.len() - candidate >= 3 && velocity >= p.min_acquisition_speed && ratio >= p.min_straightness)
            {
                e.start_tick = first.tick;
                e.acquisition_degrees = moved;
                e.acquisition_seconds = dt;
                e.acquisition_speed = velocity;
                e.straightness = ratio;
                e.rapid_acquisition = true;
                e.samples = acquisition[candidate..].to_vec();
                e.sample_count = e.samples.len();
                break;
            }
        }
        Self {
            evidence: e,
            last: sample.clone(),
            view_travel: 0.0,
            runs: [0.0; 3],
            longest: [0.0; 3],
            all_visible: sample.obstruction == Obstruction::Visible,
        }
    }
    fn push(&mut self, sample: &Sample, rate: f64) {
        self.evidence.sample_count += 1;
        self.evidence.follow_travel_degrees += angle(target(&self.last), target(sample));
        self.view_travel += angle(view(&self.last), view(sample));
        self.evidence.max_error_degrees = self
            .evidence
            .max_error_degrees
            .max(angle(view(sample), target(sample)));
        for (i, kind) in [Obstruction::Smoke, Obstruction::Blind, Obstruction::Wall]
            .iter()
            .enumerate()
        {
            self.runs[i] = if self.last.obstruction == *kind && sample.obstruction == *kind {
                self.runs[i] + seconds(self.last.tick, sample.tick, rate)
            } else {
                0.0
            };
            self.longest[i] = self.longest[i].max(self.runs[i]);
        }
        self.all_visible &= sample.obstruction == Obstruction::Visible;
        self.last = sample.clone();
    }
    fn finish(mut self, rate: f64, p: &Parameters) -> Option<Evidence> {
        let e = &mut self.evidence;
        e.end_tick = self.last.tick;
        e.follow_seconds = seconds(e.lock_tick, e.end_tick, rate);
        e.sustained_follow = e.follow_seconds >= p.min_follow_seconds
            && e.follow_travel_degrees >= p.min_follow_travel_degrees
            && self.view_travel >= p.min_follow_travel_degrees;
        if !e.rapid_acquisition && !e.sustained_follow {
            return None;
        }
        for (i, kind) in [Obstruction::Smoke, Obstruction::Blind, Obstruction::Wall]
            .into_iter()
            .enumerate()
        {
            if self.longest[i] >= p.min_follow_seconds {
                e.obstruction = kind;
                e.obstruction_seconds = self.longest[i];
            }
        }
        if e.obstruction == Obstruction::Unknown && self.all_visible {
            e.obstruction = Obstruction::Visible;
        }
        if e.end_tick != e.lock_tick {
            e.samples.push(self.last);
        }
        Some(self.evidence)
    }
}

/// Decode once and route each active track to its owner's streaming state.
pub fn evaluate_journal_match(
    reader: impl std::io::BufRead,
    fingerprint: &str,
    players: &[String],
    p: &Parameters,
) -> Result<BTreeMap<String, Report>> {
    let mut streams = BTreeMap::new();
    let mut inputs = BTreeMap::new();
    let header = crate::analysis::body_journal::visit(reader, |header, frame| {
        ensure!(
            header.source.demo_fingerprint.as_deref() == Some(fingerprint),
            "body journal demo mismatch"
        );
        if inputs.is_empty() {
            for player in players {
                let input = journal_input(header, fingerprint, player);
                streams.insert(player.clone(), Stream::new(&input, p)?);
                inputs.insert(player.clone(), input);
            }
        }
        for input in inputs.values_mut() {
            input.tracks.clear();
        }
        for t in frame.tracks {
            let Some(input) = inputs.get_mut(&t.player_id) else {
                continue;
            };
            let camera = &frame.views[&t.player_id];
            input.tracks.push(Track {
                round: t.round,
                target_id: t.target_id.clone(),
                point_id: t.point_id.clone(),
                enemy: t.enemy,
                samples: vec![Sample {
                    tick: frame.tick,
                    eye: camera.eye,
                    view: camera.view,
                    target: frame.points[&t.point_key],
                    obstruction: Obstruction::Unknown,
                }],
            });
        }
        for (player, input) in &inputs {
            streams
                .get_mut(player)
                .expect("initialized together")
                .push(input)?;
        }
        Ok(())
    })?;
    ensure!(
        header.source.demo_fingerprint.as_deref() == Some(fingerprint),
        "body journal demo mismatch"
    );
    for player in players {
        if !streams.contains_key(player) {
            streams.insert(
                player.clone(),
                Stream::new(&journal_input(&header, fingerprint, player), p)?,
            );
        }
    }
    Ok(streams
        .into_iter()
        .map(|(player, stream)| (player, stream.finish()))
        .collect())
}
fn journal_input(
    header: &crate::analysis::Artifact<crate::analysis::body_journal::BodyClock>,
    fingerprint: &str,
    player: &str,
) -> Input {
    Input {
        demo_fingerprint: fingerprint.into(),
        player_id: player.into(),
        measurement_source: header.data.measurement_source.clone(),
        tick_rate: header.data.tick_rate,
        sample_step_ticks: header.data.sample_step_ticks,
        angular_resolution_degrees: header.data.angular_resolution_degrees,
        tracks: vec![],
    }
}
pub fn evaluate_journal(
    reader: impl std::io::BufRead,
    fingerprint: &str,
    player: &str,
    p: &Parameters,
) -> Result<Report> {
    evaluate_journal_match(reader, fingerprint, &[player.into()], p)?
        .remove(player)
        .ok_or_else(|| anyhow::anyhow!("missing player report"))
}
