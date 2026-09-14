// SPDX-License-Identifier: GPL-3.0-only
// Rust translation (2026-09-14) of cs2parser smokeDensity.ts / smokeVoxel.ts:
// https://github.com/osztenkurden/cs2parser/tree/d68a5bdee48b68e41c9e64639075e35bc085797b
// See LICENSE in this directory. Raw CPU density, not visual opacity.
use anyhow::{ensure, Context, Result};
use std::sync::OnceLock;
const N: usize = 32768;
const AXES: [[i32; 3]; 6] = [
    [-1, 0, 0],
    [1, 0, 0],
    [0, -1, 0],
    [0, 1, 0],
    [0, 0, -1],
    [0, 0, 1],
];
fn index(p: [i32; 3]) -> usize {
    let mut out = 0;
    for bit in 0..5 {
        for axis in 0..3 {
            out |= (((p[axis] >> bit) & 1) as usize) << (bit * 3 + axis);
        }
    }
    out
}
fn coordinates(i: usize) -> [i32; 3] {
    std::array::from_fn(|axis| {
        (0..5)
            .map(|bit| (((i >> (bit * 3 + axis)) & 1) as i32) << bit)
            .sum()
    })
}
pub(super) fn lookup() -> &'static Vec<([i32; 3], [Option<usize>; 6])> {
    static LOOKUP: OnceLock<Vec<([i32; 3], [Option<usize>; 6])>> = OnceLock::new();
    LOOKUP.get_or_init(|| {
        (0..N)
            .map(|i| {
                let p = coordinates(i);
                let neighbours = AXES.map(|d| {
                    let q = std::array::from_fn(|a| p[a] + d[a]);
                    q.iter().all(|v| (0..32).contains(v)).then(|| index(q))
                });
                (p, neighbours)
            })
            .collect()
    })
}
#[derive(Clone, Copy, Debug)]
struct Auxiliary {
    remaining: u32,
    position: [f32; 3],
    radius_step: f32,
}
fn normalize(mut v: [f32; 3]) -> [f32; 3] {
    let length = ((v[1] * v[1] + v[2] * v[2]) + v[0] * v[0]).sqrt();
    if length == 0. {
        return [0.; 3];
    }
    if (1e-17..=1e17).contains(&length) {
        let inverse = 1. / length;
        for value in &mut v {
            *value *= inverse;
        }
    } else {
        let length = ((v[1] as f64).powi(2) + (v[0] as f64).powi(2) + (v[2] as f64).powi(2)).sqrt();
        let inverse = 1. / length;
        for value in &mut v {
            *value = (*value as f64 * inverse) as f32;
        }
    }
    v
}
pub struct Density {
    current: Vec<[f32; 4]>,
    next: Vec<[f32; 4]>,
    active: Vec<bool>,
    next_active: Vec<bool>,
    blocked: Vec<bool>,
    seeds: Vec<(usize, f32)>,
    auxiliary: Vec<Auxiliary>,
    sequence: Option<u16>,
    origin: [f32; 3],
}
impl Density {
    pub fn new(origin: [f32; 3]) -> Result<Self> {
        ensure!(
            origin.iter().all(|v| v.is_finite()),
            "non-finite smoke origin"
        );
        Ok(Self {
            current: vec![[0.; 4]; N],
            next: vec![[0.; 4]; N],
            active: vec![false; N],
            next_active: vec![false; N],
            blocked: vec![false; N],
            seeds: Vec::new(),
            auxiliary: Vec::new(),
            sequence: None,
            origin,
        })
    }
    /// Directional estimate only: sample occupied 20-HU cells every 10 HU.
    /// ponytail: thin edge crossings can be missed; exact voxel traversal is unnecessary for this estimate.
    pub fn estimated_entry(&self, start: [f32; 3], end: [f32; 3]) -> Option<[f32; 3]> {
        if self.sequence.is_none() || !start.iter().chain(&end).all(|v| v.is_finite()) {
            return None;
        }
        let bounds = crate::analysis::line_of_sight::Bounds {
            min: self.origin.map(|v| f64::from(v) - 320.),
            max: self.origin.map(|v| f64::from(v) + 320.),
        };
        let [lo, hi] = bounds.segment_interval(start.map(f64::from), end.map(f64::from))?;
        let delta: [f64; 3] = std::array::from_fn(|i| f64::from(end[i]) - f64::from(start[i]));
        let length = delta.iter().map(|v| v * v).sum::<f64>().sqrt();
        let steps = (((hi - lo) * length / 10.).ceil() as usize).max(1);
        for step in 0..=steps {
            let t = lo + (hi - lo) * step as f64 / steps as f64;
            let point: [f32; 3] =
                std::array::from_fn(|i| (f64::from(start[i]) + delta[i] * t) as f32);
            let cell =
                std::array::from_fn(|i| ((point[i] - self.origin[i] + 320.) / 20.).floor() as i32);
            if cell.iter().any(|v| !(0..32).contains(v)) {
                continue;
            }
            let id = index(cell);
            if !self.blocked[id] && self.current[id][0] >= 10. {
                return Some(point);
            }
        }
        None
    }
    /// Raw native CPU smoke-query value, before lifetime weighting and volume aggregation.
    /// This is not rendered opacity; renderer disturbances are not evaluated here.
    pub fn line_density(&self, start: [f32; 3], end: [f32; 3]) -> Result<f32> {
        ensure!(self.sequence.is_some(), "smoke density is not initialized");
        ensure!(
            start.iter().chain(&end).all(|v| v.is_finite()),
            "non-finite smoke ray"
        );
        let lo = self.origin.map(|v| v - 320.);
        let hi = self.origin.map(|v| v + 320.);
        let delta: [f32; 3] = std::array::from_fn(|i| end[i] - start[i]);
        let length = ((delta[1] * delta[1] + delta[2] * delta[2]) + delta[0] * delta[0]).sqrt();
        ensure!(
            length == 0. || (1e-17..=1e17).contains(&length),
            "unsupported smoke ray normalization"
        );
        let inverse = if length == 0. { 0. } else { 1. / length };
        let direction = delta.map(|v| v * inverse);
        let inside = |p: [f32; 3]| (0..3).all(|i| p[i] >= lo[i] && p[i] <= hi[i]);
        let mut point = start;
        if !inside(start) {
            let mut enter: f32 = -1.;
            let mut leave: f32 = 1.;
            let mut began_inside = true;
            for plane in 0..6 {
                let i = plane % 3;
                let a = if plane < 3 {
                    lo[i] - start[i]
                } else {
                    start[i] - hi[i]
                };
                let b = if plane < 3 {
                    a - delta[i]
                } else {
                    a + delta[i]
                };
                if a > 0. && b > 0. {
                    return Ok(0.);
                }
                if a <= 0. && b <= 0. {
                    continue;
                }
                if a > 0. {
                    began_inside = false;
                }
                let denominator = a - b;
                if a > b {
                    enter = enter.max(a.max(0.) / denominator);
                } else {
                    leave = leave.min(a / denominator);
                }
            }
            let fraction = if began_inside {
                0.
            } else if leave > enter && enter >= 0. {
                enter
            } else {
                return Ok(0.);
            };
            point = std::array::from_fn(|i| (start[i] + delta[i] * fraction) + direction[i]);
        }
        let packed = |p: [f32; 3]| -> Result<[u8; 3]> {
            let grid: [f32; 3] =
                std::array::from_fn(|i| ((p[i] - self.origin[i]) * 0.05 + 16.).trunc());
            ensure!(
                grid.iter()
                    .all(|v| v.is_finite() && *v >= i32::MIN as f32 && *v < i32::MAX as f32),
                "smoke coordinate out of range"
            );
            Ok(grid.map(|v| v as i32 as u8))
        };
        let mut cell = packed(point)?;
        let target = packed(end)?;
        let step = direction.map(|v| {
            if v > 0. {
                1i8
            } else if v < 0. {
                -1
            } else {
                0
            }
        });
        let distance: [f32; 3] = std::array::from_fn(|i| {
            if direction[i] == 0. {
                0.
            } else {
                (step[i] as f32 * 20.) / direction[i]
            }
        });
        let fraction: [f32; 3] = std::array::from_fn(|i| {
            let g = (point[i] - lo[i]) * 0.05;
            g - g.trunc()
        });
        let mut next: [f32; 3] = std::array::from_fn(|i| {
            if direction[i] == 0. {
                f32::MAX
            } else {
                (if direction[i] > 0. {
                    1. - fraction[i]
                } else {
                    fraction[i]
                }) * distance[i]
            }
        });
        let mut sum = 0.;
        // Preserve the engine's Y/X/Z tie order and float32 operation order.
        for _ in 0..128 {
            let center =
                std::array::from_fn(|i| ((cell[i] as f32 - 16.) * 20. + self.origin[i]) + 10.);
            if !inside(center) {
                return Ok(sum);
            }
            let i = index(cell.map(i32::from));
            if self.blocked[i] {
                return Ok(1.);
            }
            let value = (self.current[i][0] / 50.).clamp(0., 1.);
            if value >= 0.8 {
                return Ok(1.);
            }
            if value > 0.1 {
                sum += value;
                if sum >= 0.2 {
                    return Ok(1.);
                }
            }
            if cell == target {
                return Ok(sum);
            }
            let axis = if next[1] > next[0] {
                if next[0] > next[2] {
                    2
                } else {
                    0
                }
            } else if next[1] > next[2] {
                2
            } else {
                1
            };
            next[axis] += distance[axis];
            cell[axis] = cell[axis].wrapping_add_signed(step[axis]);
        }
        Ok(sum)
    }
    /// Morton-order native flags; zero density does not imply an inactive cell.
    pub fn active_cells(&self) -> &[bool] {
        &self.active
    }
    pub fn blocked_cells(&self) -> &[bool] {
        &self.blocked
    }
    /// Current decoded seeds in journal order. A renderer scene keeps the first
    /// nonempty result it observes; later journal replacements must not replace it.
    pub fn seed_centres(&self) -> impl Iterator<Item = [f32; 3]> + '_ {
        self.seeds.iter().map(|&(index, _)| {
            let cell = lookup()[index].0;
            std::array::from_fn(|i| ((cell[i] as f32 - 16.) * 20. + self.origin[i]) + 10.)
        })
    }
    pub fn sequence(&self) -> Option<u16> {
        self.sequence
    }
    pub fn values(&self) -> impl Iterator<Item = f32> + '_ {
        self.current.iter().map(|v| v[0])
    }
    pub fn step(&mut self, seq: u16, payload: &[u8]) -> Result<()> {
        ensure!(
            Some(seq) == self.sequence.map_or(Some(0), |s| s.checked_add(1)),
            "non-contiguous smoke sequence"
        );
        ensure!(payload.len() >= 3, "truncated smoke payload");
        let flags = payload[1];
        ensure!(flags & !3 == 0, "unsupported smoke flags");
        let stop = payload[0] != 0;
        let mut offset = 2;
        // Validate all sections before touching the live simulation.
        let seeds = if flags & 1 != 0 {
            let count = usize::from(payload[offset]);
            offset += 1;
            let bytes = payload
                .get(offset..offset + count * 8)
                .context("truncated smoke seeds")?;
            offset += count * 8;
            let mut result = Vec::with_capacity(count);
            for v in bytes.chunks_exact(8) {
                ensure!(
                    v[..3].iter().all(|v| *v < 32),
                    "invalid smoke seed coordinate"
                );
                let age = f32::from_le_bytes(v[4..8].try_into()?);
                ensure!(age.is_finite(), "invalid smoke seed age");
                result.push((index([v[0] as i32, v[1] as i32, v[2] as i32]), age));
            }
            Some(result)
        } else {
            None
        };
        let mut masks = Vec::new();
        if flags & 2 != 0 {
            let count = u16::from_le_bytes(
                payload
                    .get(offset..offset + 2)
                    .context("truncated smoke mask count")?
                    .try_into()?,
            ) as usize;
            offset += 2;
            let bytes = payload
                .get(offset..offset + count * 10)
                .context("truncated smoke masks")?;
            offset += count * 10;
            for v in bytes.chunks_exact(10) {
                let word = u16::from_le_bytes(v[..2].try_into()?) as usize;
                ensure!(word < 512, "invalid smoke mask index");
                masks.push((word, u64::from_le_bytes(v[2..].try_into()?)));
            }
        }
        let count = usize::from(
            *payload
                .get(offset)
                .context("missing auxiliary smoke count")?,
        );
        offset += 1;
        let bytes = payload
            .get(offset..offset + count * 20)
            .context("truncated auxiliary smoke records")?;
        offset += bytes.len();
        let mut auxiliary = Vec::with_capacity(count);
        for record in bytes.chunks_exact(20) {
            let remaining = u32::from_le_bytes(record[..4].try_into()?);
            let position = [
                f32::from_le_bytes(record[4..8].try_into()?),
                f32::from_le_bytes(record[8..12].try_into()?),
                f32::from_le_bytes(record[12..16].try_into()?),
            ];
            let radius_step = f32::from_le_bytes(record[16..20].try_into()?);
            ensure!(
                remaining > 0
                    && position.iter().all(|v| v.is_finite())
                    && radius_step.is_finite()
                    && radius_step >= 0.
                    && (remaining as f32 * radius_step).is_finite(),
                "invalid auxiliary smoke record"
            );
            auxiliary.push(Auxiliary {
                remaining,
                position,
                radius_step,
            });
        }
        ensure!(offset == payload.len(), "trailing smoke data");
        if let Some(seeds) = seeds {
            self.seeds = seeds;
        }
        for (word, mask) in masks {
            for bit in 0..64 {
                self.blocked[word * 64 + bit] = mask & (1 << bit) != 0;
            }
        }
        self.update_seeds(&auxiliary, stop);
        self.update_cells(&auxiliary, stop);
        let mut i = 0;
        while i < auxiliary.len() {
            auxiliary[i].remaining -= 1;
            if auxiliary[i].remaining == 0 {
                auxiliary.swap_remove(i);
            } else {
                i += 1;
            }
        }
        self.auxiliary = auxiliary;
        for i in 0..N {
            if self.active[i] {
                self.current[i] = self.next[i];
            }
            self.active[i] |= self.next_active[i];
            self.next_active[i] = self.active[i];
        }
        std::mem::swap(&mut self.current, &mut self.next);
        std::mem::swap(&mut self.active, &mut self.next_active);
        self.sequence = Some(seq);
        Ok(())
    }
    fn update_seeds(&mut self, auxiliary: &[Auxiliary], stop: bool) {
        let grid = lookup();
        if self.sequence.is_none() {
            for &(i, _) in &self.seeds {
                let p = grid[i].0;
                for dz in -1..=1 {
                    for dy in -1..=1 {
                        for dx in -1..=1 {
                            let q = [p[0] + dx, p[1] + dy, p[2] + dz];
                            if (dx == 0 && dy == 0 && dz == 0)
                                || q.iter().any(|v| !(0..32).contains(v))
                            {
                                continue;
                            }
                            let j = index(q);
                            if self.active[j] || self.blocked[j] {
                                continue;
                            }
                            self.current[j][0] += 30.;
                            self.next[j][0] = self.current[j][0];
                            self.active[j] = true;
                        }
                    }
                }
                self.active[i] = true;
            }
        } else {
            let fresh = self.seeds.iter().filter(|(_, age)| *age < 0.01).count();
            let cap = 50. * ((1. - fresh as f32 / 40.) * 3.5).max(1.);
            for (i, age) in &mut self.seeds {
                let mut cell = *i;
                let mut moved_world = None;
                if !auxiliary.is_empty() {
                    let world: [f32; 3] = std::array::from_fn(|a| {
                        ((grid[cell].0[a] - 16) as f32 * 20. + self.origin[a]) + 10.
                    });
                    let mut candidate = world;
                    let mut move_seed = false;
                    for record in auxiliary.iter().filter(|r| r.remaining >= 29) {
                        let delta: [f32; 3] =
                            std::array::from_fn(|a| world[a] - record.position[a]);
                        let distance = ((delta[1] * delta[1] + delta[2] * delta[2])
                            + delta[0] * delta[0])
                            .sqrt();
                        if record.remaining as f32 * record.radius_step > distance {
                            let direction = normalize(delta);
                            for a in 0..3 {
                                candidate[a] += direction[a] * 30.;
                            }
                            let r: [f32; 3] = std::array::from_fn(|a| world[a] - self.origin[a]);
                            if ((r[2] * r[2] + r[1] * r[1]) + r[0] * r[0]).sqrt() > 80. {
                                *age = 1.;
                                move_seed = true;
                            } else {
                                *age = 0.5;
                            }
                        }
                    }
                    if move_seed {
                        let q: [i32; 3] = std::array::from_fn(|a| {
                            ((candidate[a] - self.origin[a]) * 0.05 + 16.) as i32
                        });
                        if q.iter().all(|v| (0..32).contains(v)) && q != grid[cell].0 {
                            let target = index(q);
                            if !self.blocked[target] {
                                cell = target;
                                *i = target;
                                *age = 1.;
                                moved_world = Some(candidate);
                            }
                        }
                    }
                }
                let i = cell;
                if stop {
                    self.current[i][0] = 0.;
                } else {
                    let r: [f32; 3] = std::array::from_fn(|a| {
                        moved_world.map_or_else(
                            || {
                                ((grid[i].0[a] - 16) as f32 * 20. + self.origin[a]) + 10.
                                    - self.origin[a]
                            },
                            |world| world[a] - self.origin[a],
                        )
                    });
                    let distance = ((r[2] * r[2] + r[1] * r[1]) + r[0] * r[0]).sqrt();
                    if distance <= 80. {
                        *age *= 0.8;
                    } else {
                        let mut average = 0.;
                        for j in grid[i].1.into_iter().flatten() {
                            if self.active[j] {
                                average += (self.current[j][0] / 50.).clamp(0., 1.);
                            }
                        }
                        average /= 6.;
                        if average > 0.2 {
                            if *age > 0. {
                                *age *= 0.8;
                            }
                        } else {
                            *age = 1.;
                        }
                    }
                    self.current[i][0] = cap.min(self.current[i][0] + (1. - *age) * 60.);
                }
                self.active[i] = true;
                self.next[i] = self.current[i];
            }
        }
        if stop {
            self.seeds.clear();
        }
    }
    fn update_cells(&mut self, auxiliary: &[Auxiliary], stop: bool) {
        let grid = lookup();
        for (i, (p, neighbours)) in grid.iter().enumerate() {
            if !self.active[i] {
                continue;
            }
            if self.blocked[i] {
                self.next[i][0] = 0.;
                continue;
            }
            let saturation = (self.next[i][0] / 50.).clamp(0., 1.);
            let mut incoming = 0.;
            let mut blocked_axes = 0;
            let mut densities = [0.; 6];
            for k in 0..6 {
                let Some(j) = neighbours[k].filter(|j| !self.blocked[*j]) else {
                    blocked_axes |= 1 << (k >> 1);
                    continue;
                };
                let q = self.current[j];
                let delta = AXES[k];
                densities[k] = q[0];
                let direction = (-(delta[1] as f32) * q[2] - (delta[2] as f32) * q[3])
                    - (delta[0] as f32) * q[1];
                let weight = if k == 4 {
                    0.79
                } else if k == 5 {
                    1.2
                } else if direction > 0.2 {
                    1.25
                } else {
                    0.9
                };
                let transfer = ((1. - saturation) * weight) * (q[0] / 6.);
                self.next[j][0] = (self.next[j][0] - transfer).max(0.);
                incoming += transfer;
                if !self.active[j] && self.current[i][0] > 5. {
                    self.next_active[j] = true;
                }
            }
            if p.iter().any(|v| *v == 0 || *v == 31) {
                self.next[i][0] *= 0.3;
                continue;
            }
            self.next[i][0] += incoming;
            let mut vx = if blocked_axes & 1 != 0 {
                0.
            } else {
                densities[0] - densities[1]
            };
            let mut vy = if blocked_axes & 2 != 0 {
                0.
            } else {
                densities[2] - densities[3]
            };
            let mut vz = 0.;
            if !auxiliary.is_empty() {
                let world: [f32; 3] =
                    std::array::from_fn(|a| ((p[a] - 16) as f32 * 20. + self.origin[a]) + 10.);
                let mut force = [0.; 3];
                let mut count = 0;
                for record in auxiliary {
                    let delta: [f32; 3] = std::array::from_fn(|a| world[a] - record.position[a]);
                    let distance =
                        ((delta[1] * delta[1] + delta[2] * delta[2]) + delta[0] * delta[0]).sqrt();
                    if record.remaining as f32 * record.radius_step > distance {
                        let direction = normalize(delta);
                        for a in 0..3 {
                            force[a] += direction[a];
                        }
                        count += 1;
                    }
                }
                if count > 0 {
                    let inverse = 1. / count as f32;
                    force = normalize(force.map(|v| v * inverse));
                    let r: [f32; 3] = std::array::from_fn(|a| world[a] - self.origin[a]);
                    self.next[i][0] = if ((r[2] * r[2] + r[1] * r[1]) + r[0] * r[0]).sqrt() < 80. {
                        self.next[i][0].min(15.)
                    } else {
                        0.
                    };
                    for axis in 0..3 {
                        if blocked_axes & (1 << axis) != 0 {
                            force[axis] = 0.;
                        }
                    }
                    [vx, vy, vz] = force;
                }
            }
            let length = ((vy * vy + vz * vz) + vx * vx).sqrt();
            if length != 0. {
                let inv = 1. / length;
                vx *= inv;
                vy *= inv;
                vz *= inv;
            }
            self.next[i][1] = vx;
            self.next[i][2] = vy;
            self.next[i][3] = vz;
            self.next[i][0] = (self.next[i][0] - if stop { 0.25 } else { 0.5 }).max(0.);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn directional_estimate_samples_density_not_empty_or_blocked_bounds() {
        let mut sim = Density::new([0.; 3]).unwrap();
        sim.step(0, &[0, 0, 0]).unwrap();
        let ray = ([-1000., 10., 10.], [1000., 10., 10.]);
        assert_eq!(sim.estimated_entry(ray.0, ray.1), None);
        sim.current[index([16, 16, 16])][0] = 50.;
        assert!(sim.estimated_entry(ray.0, ray.1).is_some());
        assert_eq!(
            sim.estimated_entry([10., 10., 10.], ray.1),
            Some([10., 10., 10.])
        );
        assert_eq!(
            sim.estimated_entry([-1000., 50., 10.], [1000., 50., 10.]),
            None
        );
        sim.blocked[index([16, 16, 16])] = true;
        assert_eq!(sim.estimated_entry(ray.0, ray.1), None);
        assert_eq!(sim.estimated_entry([f32::NAN; 3], ray.1), None);
    }
    #[test]
    fn auxiliary_seed_displacement_preserves_native_age_and_blocker_order() {
        let mut sim = Density::new([0.; 3]).unwrap();
        sim.sequence = Some(0);
        sim.current.fill([50., 0., 0., 0.]);
        sim.next.clone_from(&sim.current);
        sim.active.fill(true);
        let original = index([12, 14, 12]);
        let target = index([11, 13, 11]);
        let record = Auxiliary {
            remaining: 30,
            position: [-20., 15., 0.],
            radius_step: 3.5,
        };
        sim.seeds.push((original, 0.5));
        sim.update_seeds(&[record], false);
        assert_eq!(sim.seeds, vec![(target, 0.8)]);
        assert_eq!(sim.current[target][0], 62.);
        sim.seeds = vec![(original, 0.5)];
        sim.blocked[target] = true;
        sim.update_seeds(&[record], false);
        assert_eq!(sim.seeds, vec![(original, 0.8)]);
        sim.seeds = vec![(original, 0.5)];
        sim.update_seeds(
            &[Auxiliary {
                remaining: 28,
                ..record
            }],
            false,
        );
        assert_eq!(sim.seeds, vec![(original, 0.4)]);
    }
    #[test]
    fn auxiliary_records_validate_atomically_and_expire_by_simulation_step() {
        let mut sim = Density::new([0.; 3]).unwrap();
        sim.step(0, &[0, 0, 0]).unwrap();
        let payload = |remaining: u32, position: [f32; 3], scale: f32| {
            let mut bytes = vec![0, 0, 1];
            bytes.extend(remaining.to_le_bytes());
            for value in position {
                bytes.extend(value.to_le_bytes());
            }
            bytes.extend(scale.to_le_bytes());
            bytes
        };
        for invalid in [
            payload(0, [0.; 3], 1.),
            payload(1, [f32::NAN, 0., 0.], 1.),
            payload(1, [0.; 3], -1.),
            payload(u32::MAX, [0.; 3], f32::MAX),
        ] {
            assert!(sim.step(1, &invalid).is_err());
            assert_eq!(sim.sequence, Some(0));
            assert!(sim.auxiliary.is_empty());
            assert!(sim.values().all(|v| v == 0.));
        }
        let mut bytes = vec![0, 0, 4];
        for (i, remaining) in [1, 2, 1, 3].into_iter().enumerate() {
            bytes.extend(&payload(remaining, [i as f32, 0., 0.], 1.)[3..]);
        }
        sim.step(1, &bytes).unwrap();
        assert_eq!(
            sim.auxiliary
                .iter()
                .map(|r| (r.remaining, r.position[0]))
                .collect::<Vec<_>>(),
            vec![(2, 3.), (1, 1.)]
        );
        sim.step(2, &[0, 0, 0]).unwrap();
        assert!(sim.auxiliary.is_empty());
    }
    #[test]
    fn cpu_line_retains_tie_order_thresholds_and_clipping() {
        let mut sim = Density::new([0.; 3]).unwrap();
        assert!(sim.line_density([0.; 3], [0.; 3]).is_err());
        sim.step(0, &[0, 0, 0]).unwrap();
        sim.current[index([16, 17, 16])][0] = 6.;
        assert_eq!(
            sim.line_density([10., 10., 10.], [30., 30., 30.]).unwrap(),
            6. / 50.
        );
        sim.current[index([17, 17, 16])][0] = 6.;
        assert_eq!(
            sim.line_density([10., 10., 10.], [30., 30., 30.]).unwrap(),
            1.
        );
        assert_eq!(
            sim.line_density([320., 10., 10.], [0., 10., 10.]).unwrap(),
            0.
        );
        assert_eq!(
            sim.line_density([-400., 400., 0.], [400., 400., 0.])
                .unwrap(),
            0.
        );
        sim.blocked[index([0, 16, 16])] = true;
        assert_eq!(
            sim.line_density([-400., 0., 0.], [-280., 0., 0.]).unwrap(),
            1.
        );
        assert!(sim.line_density([f32::NAN, 0., 0.], [0.; 3]).is_err());
    }
    #[test]
    fn advances_density_and_rejects_bad_frames_without_mutating_state() {
        let mut sim = Density::new([0.; 3]).unwrap();
        sim.step(0, &[0, 1, 1, 16, 16, 16, 0, 0, 0, 0, 0, 0])
            .unwrap();
        assert_eq!(sim.seed_centres().collect::<Vec<_>>(), vec![[10.; 3]]);
        let before: Vec<_> = sim.values().map(f32::to_bits).collect();
        assert!(sim.values().any(|v| v > 0.));
        for payload in [
            &[0, 0][..],
            &[0, 4, 0],
            &[0, 2, 1, 0],
            &[0, 0, 1],
            &[0, 0, 0, 0],
            &[0, 2, 1, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        ] {
            assert!(sim.step(1, payload).is_err());
            assert_eq!(sim.sequence, Some(0));
            assert!(sim.values().map(f32::to_bits).eq(before.iter().copied()));
        }
        assert!(sim.step(2, &[0, 0, 0]).is_err());
        sim.step(1, &[0, 0, 0]).unwrap();
        assert!(sim.values().all(f32::is_finite));
        assert!(sim.values().map(f32::to_bits).ne(before.iter().copied()));
    }
}
