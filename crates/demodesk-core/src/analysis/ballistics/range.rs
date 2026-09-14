//! Native ballistic range expansion over qualified scene queries.
//! The callback must run BE5700 and A3FED0, preserving raw hit order and merged intervals.
use super::Interval;
use crate::analysis::collision::asset::Hit;
use anyhow::{ensure, Result};
#[derive(Debug)]
pub struct Query {
    pub delta: [f32; 3],
    pub max_fraction: f32,
    pub hits: Vec<Hit>,
    pub intervals: Vec<Interval>,
}
impl Query {
    /// The weapon caller clears its union-state flag and requests direct all-hit
    /// collection. That flag is not the nearest-query start-solid result.
    pub fn from_hits(delta: [f32; 3], extension: f32, mut hits: Vec<Hit>) -> Result<Self> {
        ensure!(extension.is_finite(), "nonfinite query extension");
        let max_fraction = initial_max_fraction(
            delta,
            hits.first().map(|hit| hit.contact.fraction),
            extension,
        );
        let intervals = super::intervals(&mut hits, delta, max_fraction)?;
        Ok(Self {
            delta,
            max_fraction,
            hits,
            intervals,
        })
    }
}
pub struct Trace {
    pub end: [f32; 3],
    pub fraction: f32,
    pub start_solid: bool,
}
fn length(v: [f32; 3]) -> f32 {
    ((v[2] * v[2] + v[1] * v[1]) + v[0] * v[0]).sqrt()
}
fn scale(v: [f32; 3], s: f32) -> [f32; 3] {
    v.map(|x| x * s)
}
fn min(a: f32, b: f32) -> f32 {
    if a < b {
        a
    } else {
        b
    }
}
/// A670D0: call after BE5700, before merging; `delta` is the original query delta.
pub fn initial_max_fraction(delta: [f32; 3], first_hit: Option<f32>, extension: f32) -> f32 {
    if extension > 0. {
        if let Some(f) = first_hit {
            let n = length(delta);
            if n > 0. {
                return extension / n + f;
            }
        }
    }
    1.
}
/// A641E0 rescales both raw descriptors and merged interval coordinates, then trims counts.
fn clip(query: &mut Query, delta: [f32; 3], fraction: f32) -> Result<[f32; 3]> {
    let inverse = 1. / fraction;
    query.max_fraction = 1.;
    for hit in &mut query.hits {
        hit.contact.fraction = min(inverse * hit.contact.fraction, 1.);
    }
    let mut truncate = None;
    for (i, interval) in query.intervals.iter_mut().enumerate() {
        interval.segment.start = inverse * interval.segment.start;
        interval.segment.end = min(inverse * interval.segment.end, 1.);
        if truncate.is_none() && interval.segment.start > 1. {
            truncate = Some(i);
        }
    }
    if let Some(n) = truncate {
        query.intervals.truncate(n);
    }
    if let Some(last) = query.intervals.last() {
        let count = last.exit + 1;
        ensure!(
            count <= query.hits.len(),
            "clip requires additional descriptors"
        );
        query.hits.truncate(count);
    }
    let delta = scale(delta, fraction);
    query.delta = delta;
    Ok(delta)
}
/// `collect(delta, extension)` must use the same caller filter and origin.
/// extension=153 means A670D0: native all-hit query, set initial_max_fraction, merge.
/// extension=-1 means ordinary BE5700, set max_fraction=1, merge.
/// `trace(start,end)` must use the caller filter and exact native did-hit semantics.
pub fn adjust_range(
    origin: [f32; 3],
    original: [f32; 3],
    max_penetrations: i32,
    mut collect: impl FnMut([f32; 3], f32) -> Result<Query>,
    mut trace: impl FnMut([f32; 3], [f32; 3]) -> Result<Trace>,
) -> Result<(Query, [f32; 3])> {
    ensure!(
        origin.iter().chain(original.iter()).all(|x| x.is_finite()),
        "nonfinite range input"
    );
    // 1335CB0 normalization uses (y*y+z*z)+x*x, separately rounded, then reciprocal multiplication.
    let n = ((original[1] * original[1] + original[2] * original[2]) + original[0] * original[0])
        .sqrt();
    ensure!(
        (f32::from_bits(0x233877aa)..=f32::from_bits(0x5bb1a2bc)).contains(&n),
        "native normalization fallback not ported"
    );
    let direction = scale(original, 1. / n);
    let endpoint = std::array::from_fn(|i| origin[i] + original[i]);
    let mut delta = original;
    let mut query = collect(delta, 153.)?;
    let mut iterations = 0;
    let mut exits = 0;
    loop {
        let mut solid_fraction = 0.;
        for interval in &query.intervals {
            if interval.segment.solid {
                solid_fraction += min(interval.segment.end, query.max_fraction)
                    - min(interval.segment.start, query.max_fraction);
            }
        }
        if solid_fraction * length(query.delta) >= 90. {
            break;
        }
        iterations += 1;
        if iterations > 4 {
            break;
        }
        let ended_at_exit = query.hits.last().is_none_or(|hit| hit.contact.exit);
        if ended_at_exit {
            exits += 1;
            if exits > max_penetrations {
                break;
            }
            let Some(last) = query.hits.last() else {
                break;
            };
            let start = std::array::from_fn(|i| {
                (last.contact.fraction * delta[i] + origin[i]) + last.contact.normal[i] * 0.09375
            });
            let next = trace(start, endpoint)?;
            if !(next.fraction < 1. || next.start_solid) {
                delta = original;
            } else {
                let distance = length(std::array::from_fn(|i| next.end[i] - origin[i]));
                delta = scale(direction, distance + 90.);
            }
        } else {
            delta = scale(direction, length(delta) + 90.);
        }
        query = collect(delta, -1.)?;
    }
    if query.max_fraction < 1. {
        let fraction = query.max_fraction;
        delta = clip(&mut query, delta, fraction)?;
    }
    Ok((query, delta))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unobstructed_ray_keeps_original_delta_and_never_retraces() {
        let original = [8192., 0., 0.];
        let (query, delta) = adjust_range(
            [0.; 3],
            original,
            4,
            |delta, extension| {
                assert_eq!(extension, 153.);
                Ok(Query {
                    delta,
                    max_fraction: 1.,
                    hits: vec![],
                    intervals: vec![],
                })
            },
            |_, _| panic!("empty scene must not retrace"),
        )
        .unwrap();
        assert_eq!(delta, original);
        assert!(query.intervals.is_empty());
    }
}
