//! Server C61660 interpolation of recorded per-hitbox transforms.
//! Caller-qualified history and interpolation; clocks and pose construction are external.
use super::animation_clip::Transform;
use anyhow::{ensure, Result};
use std::collections::VecDeque;

fn dot_pairwise(a: [f32; 4], b: [f32; 4]) -> f32 {
    (a[0] * b[0] + a[1] * b[1]) + (a[2] * b[2] + a[3] * b[3])
}
fn rotation(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    let dot = dot_pairwise(a, b);
    let d = dot.abs();
    // The coefficients match the client approximation, but C61660 uses DPPS
    // pairwise sums and separate arithmetic, so client fast_slerp is not equal.
    let polynomial = ((3.55645 - d * 1.43519) * d - 3.2452) * d + 1.0904;
    let quadratic = (d * 0.215638 - 1.06021) * d + 0.848013;
    let centered = t - 0.5;
    let correction = polynomial * (centered * centered) + quadratic;
    let adjusted = correction * ((centered * t) * (t - 1.)) + t;
    let other = 1. - adjusted;
    let signed = if dot > 0. { adjusted } else { -adjusted };
    let q = std::array::from_fn(|i| b[i] * signed + a[i] * other);
    let length = dot_pairwise(q, q).sqrt();
    if length == 0. {
        [0., 0., 0., 1.]
    } else {
        q.map(|v| v / length)
    }
}
/// Blend corresponding native-order hitbox transforms into preallocated storage.
/// This accepts finite native interpolation weights, including extrapolation;
/// choosing or clamping a rewind weight is the caller's responsibility.
pub fn blend_into(
    output: &mut [Transform],
    a: &[Transform],
    b: &[Transform],
    t: f32,
) -> Result<()> {
    ensure!(
        output.len() == a.len() && a.len() == b.len(),
        "rewind transform count mismatch"
    );
    ensure!(
        t.is_finite()
            && a.iter().chain(b).all(|v| v
                .position
                .iter()
                .chain(&v.rotation)
                .all(|x| x.is_finite())
                && v.scale.is_finite()),
        "nonfinite rewind transform input"
    );
    let other = 1. - t;
    for ((out, a), b) in output.iter_mut().zip(a).zip(b) {
        *out = Transform {
            position: std::array::from_fn(|i| other * a.position[i] + t * b.position[i]),
            rotation: rotation(a.rotation, b.rotation, t),
            scale: (b.scale - a.scale) * t + a.scale,
        };
    }
    Ok(())
}
/// Exact recorded interpolation endpoints; no nearest-tick fallback.
#[derive(Clone, Copy, Debug)]
pub struct Triple {
    pub src_tick: i32,
    pub dst_tick: i32,
    pub fraction: f32,
}
struct Record {
    tick: i32,
    origin: [f32; 3],
    transforms: Vec<Transform>,
}
/// One full entity handle, including serial. The caller must use a separate
/// history for each identity and supply the qualified native expiry cutoff.
pub struct History {
    full_handle: u32,
    records: VecDeque<Record>,
}
impl History {
    pub fn new(full_handle: u32) -> Self {
        Self {
            full_handle,
            records: VecDeque::new(),
        }
    }
    pub fn full_handle(&self) -> u32 {
        self.full_handle
    }
    /// Call for deletion or another known discontinuity, never just dormancy.
    pub fn clear(&mut self) {
        self.records.clear();
    }
    /// Snapshot only after the complete source state update. False means no
    /// new record (dead, expired, or a non-increasing simulation tick).
    pub fn push(
        &mut self,
        alive: bool,
        tick: i32,
        cutoff_tick: i32,
        origin: [f32; 3],
        transforms: &[Transform],
    ) -> Result<bool> {
        if !alive {
            self.clear();
            return Ok(false);
        }
        ensure!(
            origin.iter().all(|v| v.is_finite())
                && !transforms.is_empty()
                && transforms.iter().all(|v| v
                    .position
                    .iter()
                    .chain(&v.rotation)
                    .all(|x| x.is_finite())
                    && v.scale.is_finite()),
            "invalid rewind history snapshot"
        );
        while self.records.front().is_some_and(|r| r.tick < cutoff_tick) {
            self.records.pop_front();
        }
        if tick < cutoff_tick || self.records.back().is_some_and(|r| r.tick >= tick) {
            return Ok(false);
        }
        if self.records.back().is_some_and(|last| {
            let d: [f32; 3] = std::array::from_fn(|i| origin[i] - last.origin[i]);
            // DEE88D..8D5: source sum order and strict 64-HU threshold.
            (d[1] * d[1] + d[0] * d[0]) + d[2] * d[2] > 4096.
                || transforms.len() != last.transforms.len()
        }) {
            self.clear();
        }
        self.records.push_back(Record {
            tick,
            origin,
            transforms: transforms.to_vec(),
        });
        Ok(true)
    }
    fn group(&self, triple: Triple) -> Result<Option<Vec<Transform>>> {
        ensure!(
            triple.fraction.is_finite(),
            "nonfinite rewind interpolation"
        );
        let Some(src) = self
            .records
            .iter()
            .rev()
            .find(|r| r.tick == triple.src_tick)
        else {
            return Ok(None);
        };
        // DD775E and DD7E6C: a nonpositive fraction copies without normalizing.
        if triple.fraction <= 0. {
            return Ok(Some(src.transforms.clone()));
        }
        // The native descending walk stops at src; an older dst is not found.
        if triple.dst_tick < triple.src_tick {
            return Ok(None);
        }
        let Some(dst) = self
            .records
            .iter()
            .rev()
            .find(|r| r.tick == triple.dst_tick)
        else {
            return Ok(None);
        };
        let mut output = src.transforms.clone();
        blend_into(
            &mut output,
            &src.transforms,
            &dst.transforms,
            triple.fraction,
        )?;
        Ok(Some(output))
    }
    /// DF4335..4568: both server groups must materialize successfully even if
    /// the final client fraction is zero. Only then combine their arrays.
    pub fn interpolate(
        &self,
        first: Triple,
        second: Triple,
        client_fraction: f32,
    ) -> Result<Option<Vec<Transform>>> {
        ensure!(
            client_fraction.is_finite(),
            "nonfinite client interpolation"
        );
        let Some(a) = self.group(first)? else {
            return Ok(None);
        };
        let Some(b) = self.group(second)? else {
            return Ok(None);
        };
        if client_fraction <= 0. {
            return Ok(Some(a));
        }
        let mut output = a.clone();
        blend_into(&mut output, &a, &b, client_fraction)?;
        Ok(Some(output))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn history_lifecycle_exact_groups_and_copy_semantics() {
        let shape = |x| Transform {
            position: [x, 0., 0.],
            rotation: [0., 0., 0., 2.],
            scale: 1.,
        };
        let triple = |src_tick, dst_tick, fraction| Triple {
            src_tick,
            dst_tick,
            fraction,
        };
        let mut h = History::new(0x12340042);
        assert_eq!(h.full_handle(), 0x12340042);
        assert!(h.push(true, 10, 0, [0.; 3], &[shape(0.)]).unwrap());
        assert!(!h.push(true, 10, 0, [0.; 3], &[shape(99.)]).unwrap());
        h.push(true, 11, 0, [1., 0., 0.], &[shape(8.)]).unwrap();
        h.push(true, 12, 0, [2., 0., 0.], &[shape(16.)]).unwrap();
        let copy = h
            .interpolate(triple(10, 999, 0.), triple(11, 999, -1.), 0.)
            .unwrap()
            .unwrap();
        assert_eq!(copy[0].position, [0.; 3]);
        assert_eq!(copy[0].rotation[3], 2.); // copy, not normalized blend
        let mixed = h
            .interpolate(triple(10, 11, 0.5), triple(11, 12, 0.5), 0.25)
            .unwrap()
            .unwrap();
        assert_eq!(mixed[0].position[0], 6.);
        assert!(h
            .interpolate(triple(10, 11, 0.), triple(11, 999, 0.5), 0.)
            .unwrap()
            .is_none());
        assert!(h
            .interpolate(triple(9, 10, 0.), triple(10, 11, 0.), 0.)
            .unwrap()
            .is_none());
        h.push(true, 13, 11, [3., 0., 0.], &[shape(24.)]).unwrap();
        assert_eq!(h.records.front().unwrap().tick, 11);
        // Exactly 64HU retains history; strictly farther clears it.
        h.push(true, 14, 11, [67., 0., 0.], &[shape(32.)]).unwrap();
        assert_eq!(h.records.len(), 4);
        h.push(true, 15, 11, [132., 0., 0.], &[shape(40.)]).unwrap();
        assert_eq!(h.records.len(), 1);
        h.push(true, 16, 11, [132., 0., 0.], &[shape(40.); 2])
            .unwrap();
        assert_eq!(h.records.len(), 1);
        h.push(false, 17, 11, [0.; 3], &[]).unwrap();
        assert!(h.records.is_empty());
        h.push(true, 18, 11, [0.; 3], &[shape(0.)]).unwrap();
        h.clear();
        assert!(h.records.is_empty());
        assert!(History::new(0x22340042).records.is_empty());
    }
    #[test]
    fn corresponding_hitbox_arrays_preserve_metadata_order_and_reject_mismatch() {
        let a = Transform {
            position: [2., 4., 8.],
            rotation: [0., 0., 0., 1.],
            scale: 2.,
        };
        let b = Transform {
            position: [6., 8., 12.],
            rotation: [0., 0., 0., -1.],
            scale: 4.,
        };
        let mut out = [a; 2];
        blend_into(&mut out, &[a, b], &[b, a], 0.25).unwrap();
        assert_eq!(out[0].position, [3., 5., 9.]);
        assert_eq!(out[1].position, [5., 7., 11.]);
        assert_eq!(out[0].scale, 2.5);
        assert_eq!(out[1].scale, 3.5);
        assert_eq!(out[0].rotation, [0., 0., 0., 1.]);
        assert_eq!(out[1].rotation, [0., 0., 0., -1.]);
        let before = out;
        assert!(blend_into(&mut out, &[a], &[b], 0.5).is_err());
        assert_eq!(out, before);
        let zero = Transform {
            rotation: [0.; 4],
            ..a
        };
        blend_into(&mut out, &[zero; 2], &[zero; 2], 0.5).unwrap();
        assert_eq!(out[0].rotation, [0., 0., 0., 1.]);
    }
}
