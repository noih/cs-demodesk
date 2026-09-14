//! Experimental crosshair-lock measurements; per-rule statistics own occurrence grouping.
//! Inputs require measured body-attached points, not estimated heights or replay interpolation.
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const RULE_ID: &str = "crosshair-lock";
pub const RULE_VERSION: &str = "experimental-3";
mod stream;
pub use stream::{evaluate_journal, evaluate_journal_match, Stream};
pub const RULE_NAME: &str = "準星吸附";

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Obstruction {
    #[default]
    Unknown,
    Visible,
    Smoke,
    Blind,
    Wall,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Sample {
    pub tick: i32,
    pub eye: [f64; 3],
    /// Degrees: pitch then yaw, before any visualization rounding.
    pub view: [f64; 2],
    /// Measured world position of the SAME body-attached point throughout the track.
    pub target: [f64; 3],
    /// Highest independently verified obstruction at this tick; unknown is never visible.
    #[serde(default)]
    pub obstruction: Obstruction,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Track {
    pub round: i32,
    pub target_id: String,
    pub point_id: String,
    pub enemy: bool,
    /// End a track at death, respawn, team change or point identity change.
    /// Missing measurements are represented by tick gaps, never filled or interpolated.
    pub samples: Vec<Sample>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Input {
    pub demo_fingerprint: String,
    pub player_id: String,
    pub measurement_source: String,
    pub tick_rate: f64,
    pub sample_step_ticks: u32,
    pub angular_resolution_degrees: f64,
    pub tracks: Vec<Track>,
}

/// All defaults are hypotheses for synthetic checks, NOT human/cheater boundaries.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Parameters {
    pub max_sample_seconds: f64,
    pub max_error_degrees: f64,
    pub acquisition_window_seconds: f64,
    pub min_acquisition_degrees: f64,
    pub min_acquisition_speed: f64,
    pub min_straightness: f64,
    pub snap_speed: f64,
    pub min_follow_seconds: f64,
    pub min_follow_travel_degrees: f64,
    pub long_follow_seconds: f64,
}
impl Default for Parameters {
    fn default() -> Self {
        Self {
            max_sample_seconds: 0.02,
            max_error_degrees: 0.15,
            acquisition_window_seconds: 0.2,
            min_acquisition_degrees: 5.0,
            min_acquisition_speed: 180.0,
            min_straightness: 0.98,
            snap_speed: 720.0,
            min_follow_seconds: 0.15,
            min_follow_travel_degrees: 1.0,
            long_follow_seconds: 0.75,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Evidence {
    pub round: i32,
    pub target_id: String,
    pub point_id: String,
    pub start_tick: i32,
    pub lock_tick: i32,
    pub end_tick: i32,
    pub acquisition_degrees: f64,
    pub acquisition_seconds: f64,
    pub acquisition_speed: f64,
    pub straightness: f64,
    pub follow_seconds: f64,
    pub follow_travel_degrees: f64,
    pub max_error_degrees: f64,
    pub rapid_acquisition: bool,
    pub sustained_follow: bool,
    pub obstruction: Obstruction,
    pub obstruction_seconds: f64,

    pub sample_count: usize,
    /// Bounded proof excerpt: acquisition, lock start and last sample. Full data stays in source journals.
    pub samples: Vec<Sample>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    Experimental,
    InsufficientData,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub rule_id: &'static str,
    pub rule_version: &'static str,
    pub rule_name: &'static str,
    pub demo_fingerprint: String,
    pub player_id: String,
    pub measurement_source: String,
    pub tick_rate: f64,
    pub sample_step_ticks: u32,
    pub angular_resolution_degrees: f64,
    pub parameters: Parameters,
    pub status: Status,
    pub reason: &'static str,

    pub evaluated_samples: usize,
    pub evidence: Vec<Evidence>,
}

pub fn evaluate(input: &Input, parameters: &Parameters) -> Result<Report> {
    let mut stream = Stream::new(input, parameters)?;
    stream.push(input)?;
    Ok(stream.finish())
}

#[cfg(test)]
fn reference(input: &Input, parameters: &Parameters) -> Result<Report> {
    validate(input, parameters)?;
    let mut evidence = Vec::new();
    let mut evaluated_samples = 0;
    let sufficient_resolution = f64::from(input.sample_step_ticks) / input.tick_rate
        <= parameters.max_sample_seconds
        && input.angular_resolution_degrees * 2.0 <= parameters.max_error_degrees;
    if sufficient_resolution {
        for track in input.tracks.iter().filter(|t| t.enemy) {
            let mut start = 0;
            // A discontinuity must not become a high-speed acquisition or a long lock.
            for end in 1..=track.samples.len() {
                if end == track.samples.len()
                    || i64::from(track.samples[end].tick) - i64::from(track.samples[end - 1].tick)
                        != i64::from(input.sample_step_ticks)
                {
                    let samples = &track.samples[start..end];
                    if samples.len() >= 3 {
                        evaluated_samples += samples.len();
                        measure(track, samples, input.tick_rate, parameters, &mut evidence);
                    }
                    start = end;
                }
            }
        }
    }
    Ok(report(input, parameters, evaluated_samples, evidence))
}

pub(super) fn report(
    input: &Input,
    parameters: &Parameters,
    evaluated_samples: usize,
    mut evidence: Vec<Evidence>,
) -> Report {
    let evaluated = evaluated_samples > 0;
    evidence.sort_by(|a, b| {
        (a.round, a.start_tick, a.end_tick, &a.target_id, &a.point_id).cmp(&(
            b.round,
            b.start_tick,
            b.end_tick,
            &b.target_id,
            &b.point_id,
        ))
    });
    Report {
        rule_id: RULE_ID,
        rule_version: RULE_VERSION,
        rule_name: RULE_NAME,
        demo_fingerprint: input.demo_fingerprint.clone(),
        player_id: input.player_id.clone(),
        measurement_source: input.measurement_source.clone(),
        tick_rate: input.tick_rate,
        sample_step_ticks: input.sample_step_ticks,
        angular_resolution_degrees: input.angular_resolution_degrees,
        parameters: parameters.clone(),
        status: if evaluated {
            Status::Experimental
        } else {
            Status::InsufficientData
        },
        reason: if evaluated {
            "Experimental measurements; not proof of cheating."
        } else {
            "No sufficiently precise contiguous enemy samples."
        },
        evaluated_samples,
        evidence,
    }
}

fn validate(input: &Input, p: &Parameters) -> Result<()> {
    ensure!(
        !input.demo_fingerprint.trim().is_empty()
            && !input.player_id.trim().is_empty()
            && !input.measurement_source.trim().is_empty(),
        "missing input identity or measurement source"
    );
    ensure!(
        input.tick_rate.is_finite() && input.tick_rate > 0.0 && input.sample_step_ticks > 0,
        "invalid sampling rate"
    );
    ensure!(
        input.angular_resolution_degrees.is_finite() && input.angular_resolution_degrees > 0.0,
        "invalid angular resolution"
    );
    for n in [
        p.max_sample_seconds,
        p.max_error_degrees,
        p.acquisition_window_seconds,
        p.min_acquisition_degrees,
        p.min_acquisition_speed,
        p.snap_speed,
        p.min_follow_seconds,
        p.min_follow_travel_degrees,
        p.long_follow_seconds,
    ] {
        ensure!(
            n.is_finite() && n > 0.0,
            "thresholds must be finite and positive"
        );
    }
    ensure!(
        p.min_straightness.is_finite() && p.min_straightness > 0.0 && p.min_straightness <= 1.0,
        "invalid straightness"
    );
    ensure!(
        p.max_error_degrees < p.min_acquisition_degrees && p.min_acquisition_degrees < 180.0,
        "invalid acquisition angle"
    );
    ensure!(
        p.acquisition_window_seconds >= p.max_sample_seconds
            && p.long_follow_seconds >= p.min_follow_seconds
            && p.snap_speed >= p.min_acquisition_speed,
        "inconsistent time or speed thresholds"
    );
    let mut keys = BTreeSet::new();
    for track in &input.tracks {
        ensure!(
            track.round > 0
                && !track.target_id.trim().is_empty()
                && !track.point_id.trim().is_empty()
                && track.target_id != input.player_id,
            "invalid track identity"
        );
        ensure!(
            keys.insert((track.round, &track.target_id, &track.point_id)),
            "duplicate track identity; use gaps for missing samples"
        );
        let mut previous = None;
        for s in &track.samples {
            ensure!(
                s.tick >= 0 && previous.is_none_or(|tick| s.tick > tick),
                "ticks must increase within a track"
            );
            ensure!(
                s.eye
                    .iter()
                    .chain(&s.view)
                    .chain(&s.target)
                    .all(|n| n.is_finite())
                    && s.view[0].abs() <= 90.0,
                "invalid measurement"
            );
            let distance = s
                .target
                .iter()
                .zip(s.eye)
                .map(|(a, b)| (a - b).powi(2))
                .sum::<f64>();
            ensure!(
                distance.is_finite() && distance > 0.0,
                "invalid target direction"
            );
            previous = Some(s.tick);
        }
    }
    Ok(())
}

type Direction = [f64; 3];
fn view(s: &Sample) -> Direction {
    let (pitch, yaw) = (s.view[0].to_radians(), (s.view[1] % 360.0).to_radians());
    [
        pitch.cos() * yaw.cos(),
        pitch.cos() * yaw.sin(),
        -pitch.sin(),
    ]
}
fn target(s: &Sample) -> Direction {
    let d = std::array::from_fn::<_, 3, _>(|i| s.target[i] - s.eye[i]);
    let length = d.iter().map(|n| n * n).sum::<f64>().sqrt();
    d.map(|n| n / length)
}
fn angle(a: Direction, b: Direction) -> f64 {
    let cross = [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ];
    cross
        .iter()
        .map(|v| v * v)
        .sum::<f64>()
        .sqrt()
        .atan2(a.iter().zip(b).map(|(x, y)| x * y).sum::<f64>())
        .to_degrees()
}
fn seconds(a: i32, b: i32, rate: f64) -> f64 {
    (f64::from(b) - f64::from(a)) / rate
}
fn travel(samples: &[Sample], direction: fn(&Sample) -> Direction) -> f64 {
    samples
        .windows(2)
        .map(|w| angle(direction(&w[0]), direction(&w[1])))
        .sum()
}

#[cfg(test)]
fn measure(
    track: &Track,
    samples: &[Sample],
    rate: f64,
    p: &Parameters,
    output: &mut Vec<Evidence>,
) {
    let errors: Vec<_> = samples.iter().map(|s| angle(view(s), target(s))).collect();
    let mut lock = 0;
    while lock < samples.len() {
        if errors[lock] > p.max_error_degrees {
            lock += 1;
            continue;
        }
        let mut end = lock + 1;
        while end < samples.len() && errors[end] <= p.max_error_degrees {
            end += 1;
        }
        let held = &samples[lock..end];
        let duration = seconds(held[0].tick, held[held.len() - 1].tick, rate);
        let follow_travel = travel(held, target);
        let follows = duration >= p.min_follow_seconds
            && follow_travel >= p.min_follow_travel_degrees
            && travel(held, view) >= p.min_follow_travel_degrees;
        let mut start = lock;
        let (mut displacement, mut acquisition_time, mut speed, mut straightness) =
            (0.0, 0.0, 0.0, 0.0);
        let mut rapid = false;
        for candidate in (0..lock).rev() {
            let dt = seconds(samples[candidate].tick, samples[lock].tick, rate);
            if dt > p.acquisition_window_seconds {
                break;
            }
            // Stop before a previous lock; never join two activations through a release.
            if errors[candidate] <= p.max_error_degrees {
                break;
            }
            let moved = angle(view(&samples[candidate]), view(&samples[lock]));
            if moved < p.min_acquisition_degrees || errors[candidate] < p.min_acquisition_degrees {
                continue;
            }
            let path = travel(&samples[candidate..=lock], view);
            let ratio = (moved / path).min(1.0);
            let velocity = moved / dt;
            if velocity >= p.snap_speed
                || (lock - candidate >= 2
                    && velocity >= p.min_acquisition_speed
                    && ratio >= p.min_straightness)
            {
                start = candidate;
                displacement = moved;
                acquisition_time = dt;
                speed = velocity;
                straightness = ratio;
                rapid = true;
                break;
            }
        }
        if rapid || follows {
            let (obstruction, obstruction_seconds) = obstruction(held, rate, p);
            output.push(Evidence {
                round: track.round,
                target_id: track.target_id.clone(),
                point_id: track.point_id.clone(),
                start_tick: samples[start].tick,
                lock_tick: samples[lock].tick,
                end_tick: samples[end - 1].tick,
                acquisition_degrees: displacement,
                acquisition_seconds: acquisition_time,
                acquisition_speed: speed,
                straightness,
                follow_seconds: duration,
                follow_travel_degrees: follow_travel,
                max_error_degrees: errors[lock..end].iter().copied().fold(0.0, f64::max),
                rapid_acquisition: rapid,
                sustained_follow: follows,
                obstruction,
                obstruction_seconds,

                sample_count: end - start,
                samples: samples[start..end].to_vec(),
            });
        }
        lock = end;
    }
}

#[cfg(test)]
fn obstruction(samples: &[Sample], rate: f64, p: &Parameters) -> (Obstruction, f64) {
    let mut best = (Obstruction::Unknown, 0.0);
    for kind in [Obstruction::Smoke, Obstruction::Blind, Obstruction::Wall] {
        let mut longest: f64 = 0.0;
        let mut run: f64 = 0.0;
        for w in samples.windows(2) {
            if w[0].obstruction == kind && w[1].obstruction == kind {
                run += seconds(w[0].tick, w[1].tick, rate);
            } else {
                run = 0.0;
            }
            longest = longest.max(run);
        }
        if longest >= p.min_follow_seconds {
            best = (kind, longest);
        }
    }
    if best.0 == Obstruction::Unknown
        && samples
            .iter()
            .all(|s| s.obstruction == Obstruction::Visible)
    {
        best.0 = Obstruction::Visible;
    }
    best
}

#[cfg(test)]
mod tests;
