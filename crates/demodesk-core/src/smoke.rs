//! Smoke journal decoding and deterministic CPU density reconstruction.
//! CPU query values are not rendered opacity or a visibility verdict.
//! Format evidence: https://github.com/osztenkurden/cs2parser/blob/master/docs/smoke-voxel-format.md
pub mod atlas;
pub mod density;
pub mod disturbance;
pub mod effects;
pub mod he;
pub mod noise;
pub mod projection;
pub mod sampling;
pub(crate) mod source;
pub mod spread;
pub mod timeline;
pub mod weapons;

use anyhow::{ensure, Context, Result};
use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Record {
    pub sequence: u16,
    pub heartbeat: bool,
    pub active_flag: u8,
    pub section_flags: u8,
    pub seed_cells: Option<Vec<SeedCell>>,
    /// Undecoded density/palette data is preserved; seed cells are not the visible cloud.
    pub remaining_payload: Vec<u8>,
}
#[derive(Debug, Serialize)]
pub struct SeedCell {
    pub grid: [u8; 3],
    pub state: [u8; 5],
}

/// Native CPU query over eligible volumes in their recorded/native list order.
/// Start times and `now` must use the same clock. This value excludes renderer
/// disturbances and must not be treated as pixel opacity or target visibility.
pub fn line_density<'a>(
    start: [f32; 3],
    end: [f32; 3],
    now: f32,
    volumes: impl IntoIterator<Item = (&'a density::Density, f32)>,
) -> Result<f32> {
    ensure!(
        now.is_finite() && start.iter().chain(&end).all(|v| v.is_finite()),
        "non-finite smoke query"
    );
    let mut total = 0.;
    for (volume, start_time) in volumes {
        let age = now - start_time;
        ensure!(age.is_finite(), "non-finite smoke age");
        let weight = lifetime(age);
        total += weight * volume.line_density(start, end)?;
        if total >= 0.2 {
            return Ok(1.);
        }
    }
    Ok((total / 0.2).clamp(0., 1.))
}

fn lifetime(age: f32) -> f32 {
    fn smooth(a: f32, b: f32, t: f32) -> f32 {
        let u = ((t - a) / (b - a)).clamp(0., 1.);
        (3. - (u + u)) * (u * u)
    }
    smooth(0.1, 1.5, age) * smooth(22., 17., age)
}

/// Certify a fixed density snapshot throughout a recorded shooting-time interval.
/// Callers must also check every possible journal snapshot and eligible volume.
pub fn crosses_during<'a>(
    start: [f32; 3],
    end: [f32; 3],
    now: [f32; 2],
    volumes: impl IntoIterator<Item = (&'a density::Density, f32)>,
) -> Result<Option<bool>> {
    ensure!(
        now.iter().chain(&start).chain(&end).all(|v| v.is_finite()) && now[0] <= now[1],
        "invalid smoke query interval"
    );
    fn smooth_bounds(a: f32, b: f32, ages: [f32; 2]) -> [f32; 2] {
        let u = ages.map(|t| ((t - a) / (b - a)).clamp(0., 1.));
        let (lo, hi) = (u[0].min(u[1]), u[0].max(u[1]));
        // Bound the actual rounded operations, not the ideal cubic's monotonicity.
        [(3. - (hi + hi)) * (lo * lo), (3. - (lo + lo)) * (hi * hi)]
    }
    let contributions = volumes.into_iter().map(|(volume, start_time)| {
        let ages = now.map(|t| t - start_time);
        ensure!(ages.iter().all(|v| v.is_finite()), "non-finite smoke age");
        let grow = smooth_bounds(0.1, 1.5, ages);
        let fade = smooth_bounds(22., 17., ages);
        let raw = volume.line_density(start, end)?;
        // Any single contribution >= 1 already exceeds the native 0.2 threshold.
        Ok([
            (grow[0] * fade[0] * raw).min(1.),
            (grow[1] * fade[1] * raw).min(1.),
        ])
    });
    crossing_ranges(contributions)
}

/// Certify the native threshold without assuming an unrecorded actor-list order.
/// `None` means different floating-point accumulation orders are not ruled out.
/// Inputs are already lifetime-weighted native per-volume values, not opacity.
pub fn crossing_unordered(values: impl IntoIterator<Item = f32>) -> Result<Option<bool>> {
    crossing_ranges(values.into_iter().map(|v| Ok([v, v])))
}
fn crossing_ranges(values: impl IntoIterator<Item = Result<[f32; 2]>>) -> Result<Option<bool>> {
    let (mut low, mut high, mut count) = (0.0_f64, 0.0_f64, 0_u32);
    let mut only = [0.; 2];
    for value in values {
        let value = value?;
        ensure!(
            value
                .iter()
                .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
                && value[0] <= value[1],
            "invalid smoke contribution"
        );
        if value[1] == 0. {
            continue;
        }
        only = value;
        count = count
            .checked_add(1)
            .context("too many smoke contributions")?;
        if count > 1 << 24 {
            return Ok(None);
        }
        low = (low + f64::from(value[0])).next_down();
        high = (high + f64::from(value[1])).next_up();
    }
    if count == 0 {
        return Ok(Some(false));
    }
    // For n <= 2^24, integer n is exact and every partial sum stays <= n.
    // One ULP at n bounds each addition's rounding error in every permutation.
    if count == 1 {
        return Ok(if only[0] >= 0.2 {
            Some(true)
        } else if only[1] < 0.2 {
            Some(false)
        } else {
            None
        });
    }
    let n = count as f32;
    let error = f64::from(n.next_up() - n) * f64::from(count);
    let threshold = f64::from(0.2_f32);
    if (low - error).next_down() >= threshold {
        Ok(Some(true))
    } else if (high + error).next_up() < threshold {
        Ok(Some(false))
    } else {
        Ok(None)
    }
}

/// Conservative world bounds of the 32-cell grid, with 20 HU per cell.
/// A ray outside this box cannot cross the cloud; an intersection proves no opacity.
pub fn bounds(origin: [f32; 3]) -> Option<crate::analysis::line_of_sight::Bounds> {
    let min = origin.map(|v| f64::from((v - 320.).next_down()));
    let max = origin.map(|v| f64::from((v + 320.).next_up()));
    (origin.iter().all(|v| v.is_finite()) && min.iter().chain(&max).all(|v| v.is_finite()))
        .then_some(crate::analysis::line_of_sight::Bounds { min, max })
}

pub fn decode(bytes: &[Option<u8>]) -> Result<Vec<Record>> {
    let data: Vec<u8> = bytes
        .iter()
        .copied()
        .collect::<Option<_>>()
        .context("smoke byte slots were not received")?;
    let mut remaining = data.as_slice();
    let mut records: Vec<Record> = Vec::new();
    while !remaining.is_empty() {
        ensure!(remaining.len() >= 4, "truncated smoke record header");
        let sequence = u16::from_le_bytes([remaining[0], remaining[1]]);
        let length = u16::from_le_bytes([remaining[2], remaining[3]]) as usize;
        ensure!(
            length >= 2 && length <= remaining.len() - 4,
            "invalid smoke payload length"
        );
        if let Some(previous) = records.last() {
            ensure!(
                sequence == previous.sequence.wrapping_add(1),
                "non-contiguous smoke journal"
            );
        } else {
            ensure!(
                sequence == 0,
                "smoke journal does not start at sequence zero"
            );
        }
        let payload = &remaining[4..4 + length];
        let heartbeat = payload == [0, 0, 0];
        let section_flags = payload[1];
        ensure!(section_flags & !3 == 0, "unsupported smoke section flags");
        let mut consumed = if heartbeat { 3 } else { 2 };
        let seed_cells = if section_flags & 1 != 0 {
            let count = *payload.get(2).context("missing smoke cell count")? as usize;
            consumed = 3 + count * 8;
            ensure!(consumed <= payload.len(), "truncated smoke cell list");
            let mut cells = Vec::with_capacity(count);
            for entry in payload[3..consumed].chunks_exact(8) {
                ensure!(
                    entry[..3].iter().all(|v| *v < 32),
                    "invalid smoke grid coordinate"
                );
                cells.push(SeedCell {
                    grid: [entry[0], entry[1], entry[2]],
                    state: entry[3..8].try_into().unwrap(),
                });
            }
            Some(cells)
        } else {
            None
        };
        records.push(Record {
            sequence,
            heartbeat,
            active_flag: payload[0],
            section_flags,
            seed_cells,
            remaining_payload: payload[consumed..].to_vec(),
        });
        remaining = &remaining[4 + length..];
    }
    Ok(records)
}

/// Increment schema for incompatible data changes; implementation for algorithm changes.
pub fn contract() -> crate::analysis::Contract {
    crate::analysis::Contract {
        module: "smoke-journal".into(),
        schema_version: 1,
        implementation_version: "0.2.0".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unordered_threshold_only_returns_certified_results() {
        fn permutations(values: &mut [f32], at: usize, verdict: bool) {
            if at == values.len() {
                assert_eq!(
                    values.iter().copied().fold(0.0_f32, |a, b| a + b) >= 0.2,
                    verdict
                );
            } else {
                for i in at..values.len() {
                    values.swap(at, i);
                    permutations(values, at + 1, verdict);
                    values.swap(at, i);
                }
            }
        }
        for mut values in [[0., 0.01, 0.02, 0.03, 0.04], [0., 0.01, 0.04, 0.08, 0.1]] {
            let verdict = crossing_unordered(values).unwrap().unwrap();
            permutations(&mut values, 0, verdict);
        }
        assert_eq!(crossing_unordered([0.2]).unwrap(), Some(true));
        assert_eq!(crossing_unordered([0.1, 0.1]).unwrap(), None);
        assert_eq!(crossing_unordered([]).unwrap(), Some(false));
        assert!(crossing_unordered([f32::NAN]).is_err());
        assert!(crossing_unordered([-0.1]).is_err());
    }

    #[test]
    fn query_applies_cloud_lifetime_and_validates_clocks() {
        let mut volume = density::Density::new([0.; 3]).unwrap();
        volume
            .step(0, &[0, 1, 1, 16, 16, 16, 0, 0, 0, 0, 0, 0])
            .unwrap();
        let point = [10.; 3];
        let raw = volume.line_density(point, point).unwrap();
        assert!(raw > 0.);
        assert_eq!(
            crosses_during(point, point, [0., 0.1], [(&volume, 0.)]).unwrap(),
            Some(false)
        );
        assert_eq!(
            crosses_during(point, point, [3., 5.], [(&volume, 0.)]).unwrap(),
            Some(raw >= 0.2)
        );
        assert_eq!(
            crosses_during(point, point, [0., 5.], [(&volume, 0.)]).unwrap(),
            None
        );

        assert_eq!(
            line_density(point, point, 10., [(&volume, 0.)]).unwrap(),
            (raw / 0.2).min(1.)
        );
        for now in [0., 0.1, 22., 30.] {
            assert_eq!(
                line_density(point, point, now, [(&volume, 0.)]).unwrap(),
                0.
            );
        }
        assert_eq!(line_density(point, point, 10., []).unwrap(), 0.);
        assert!(line_density(point, point, f32::NAN, []).is_err());
        assert!(line_density(point, point, 10., [(&volume, f32::NAN)]).is_err());
        let mut certified = 0;
        for tick in 0..23 * 64 {
            let interval = [tick as f32 / 64., (tick + 1) as f32 / 64.];
            if let Some(verdict) = crosses_during(point, point, interval, [(&volume, 0.)]).unwrap()
            {
                certified += 1;
                for step in 0..=16 {
                    let now = interval[0] + step as f32 / 1024.;
                    assert_eq!(
                        line_density(point, point, now, [(&volume, 0.)]).unwrap() >= 1.,
                        verdict
                    );
                }
            }
        }
        assert!(certified > 1400);
    }

    #[test]
    fn journal_preserves_seed_state_and_rejects_missing_or_truncated_data() {
        let bytes = [
            0, 0, 13, 0, 1, 3, 1, 2, 3, 4, 128, 255, 0, 6, 7, 90, 91, 1, 0, 3, 0, 0, 0, 0,
        ];
        let mut input: Vec<_> = bytes.into_iter().map(Some).collect();
        let decoded = decode(&input).unwrap();
        assert_eq!(decoded[0].seed_cells.as_ref().unwrap()[0].grid, [2, 3, 4]);
        assert_eq!(
            decoded[0].seed_cells.as_ref().unwrap()[0].state,
            [128, 255, 0, 6, 7]
        );
        assert_eq!(decoded[0].remaining_payload, [90, 91]);
        assert!(decoded[1].heartbeat);
        for end in [1, 3, 5, 16, 18, 23] {
            assert!(decode(&input[..end]).is_err());
        }
        input[10] = None;
        assert!(decode(&input).is_err());
    }
}

/// Analysis consumes directional coverage without depending on journal or density storage.
pub trait ShotCoverage {
    fn is_empty_at_fire(&self) -> bool;
    fn candidates(
        &self,
        tick: i32,
        origin: [f32; 3],
        deltas: &[[f32; 3]],
    ) -> Option<Vec<([f32; 3], [f32; 3])>>;
}
impl ShotCoverage for timeline::Timeline {
    fn is_empty_at_fire(&self) -> bool {
        self.has_no_density_at_fire()
    }
    fn candidates(
        &self,
        tick: i32,
        start: [f32; 3],
        deltas: &[[f32; 3]],
    ) -> Option<Vec<([f32; 3], [f32; 3])>> {
        let volumes = self.stable_volumes_at_fire(tick)?;
        let mut points = Vec::new();
        for delta in deltas {
            let end = std::array::from_fn(|i| start[i] + delta[i]);
            for (density, origin) in &volumes {
                if let Some(point) = density.estimated_entry(start, end) {
                    points.push((*origin, point));
                }
            }
        }
        Some(points)
    }
}
