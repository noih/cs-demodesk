// SPDX-License-Identifier: GPL-3.0-only
//! Renderer atlas state. This is density sampling, not final smoke opacity.
use super::density::Density;
use anyhow::{ensure, Context, Result};
const N: usize = 32 * 32 * 32;
fn linear(p: [i32; 3]) -> usize {
    (p[2] * 1024 + p[1] * 32 + p[0]) as usize
}
/// RG is previous/current density extended into adjacent blocked cells;
/// BA is previous/current unextended density. Byte values are UNORM.
pub struct Atlas {
    cells: Vec<[u8; 4]>,
    touched: Vec<bool>,
    fills: Vec<(usize, u8)>,
    sequence: Option<u16>,
}
impl Default for Atlas {
    fn default() -> Self {
        Self {
            cells: vec![[0; 4]; N],
            touched: vec![false; N],
            fills: Vec::new(),
            sequence: None,
        }
    }
}
impl Atlas {
    /// Consume each completed density step once, beginning with sequence zero.
    pub fn update(&mut self, density: &Density) -> Result<()> {
        let sequence = density
            .sequence()
            .context("smoke density has not started")?;
        ensure!(
            Some(sequence) == self.sequence.map_or(Some(0), |s| s.checked_add(1)),
            "non-contiguous smoke atlas sequence"
        );
        self.update_cells(
            density.values(),
            density.active_cells(),
            density.blocked_cells(),
        );
        self.sequence = Some(sequence);
        Ok(())
    }
    fn update_cells(
        &mut self,
        values: impl Iterator<Item = f32>,
        active: &[bool],
        blocked: &[bool],
    ) {
        let table = super::density::lookup();
        self.fills.clear();
        // Native upload traverses active Morton bits. The first neighbour wins,
        // including an active cell whose quantized density is zero.
        for (i, density) in values.enumerate() {
            if !active[i] {
                continue;
            }
            let value = ((density / 50.).clamp(0., 1.) * 255.) as u8;
            self.write(linear(table[i].0), value, value);
            for neighbour in table[i].1.into_iter().flatten() {
                if blocked[neighbour] && !self.touched[neighbour] {
                    self.touched[neighbour] = true;
                    self.fills.push((neighbour, value));
                }
            }
        }
        for index in 0..self.fills.len() {
            let (i, value) = self.fills[index];
            self.write(linear(table[i].0), value, 0);
            self.touched[i] = false;
        }
    }
    fn write(&mut self, index: usize, extended: u8, raw: u8) {
        let old = self.cells[index];
        self.cells[index] = [old[1], extended, old[3], raw];
    }
    pub fn cells(&self) -> &[[u8; 4]] {
        &self.cells
    }
    /// Trilinear UNORM lookup using the native 542 x 32 x 32 atlas layout.
    /// The bound density sampler clamps X/Y and wraps Z.
    pub fn sample(&self, grid: [f32; 3], slot: u8) -> Result<[f32; 4]> {
        ensure!(
            slot < 16 && grid.iter().all(|x| x.is_finite()),
            "invalid smoke atlas coordinate"
        );
        let position = grid.map(|x| x.clamp(0., 32.) - 0.5);
        let base = position.map(|x| x.floor() as i32);
        let fraction = std::array::from_fn::<_, 3, _>(|a| position[a] - base[a] as f32);
        let mut result = [0.; 4];
        for z in 0..2 {
            for y in 0..2 {
                for x in 0..2 {
                    let cell = [base[0] + x, base[1] + y, base[2] + z];
                    let global_x = (i32::from(slot) * 34 + cell[0]).clamp(0, 541);
                    let local_x = global_x - i32::from(slot) * 34;
                    if !(0..32).contains(&local_x) {
                        continue;
                    }
                    let index = (cell[2].rem_euclid(32) * 1024
                        + cell[1].clamp(0, 31) * 32
                        + local_x) as usize;
                    let weight = [x, y, z]
                        .into_iter()
                        .enumerate()
                        .map(|(a, b)| {
                            if b == 0 {
                                1. - fraction[a]
                            } else {
                                fraction[a]
                            }
                        })
                        .product::<f32>();
                    for (out, value) in result.iter_mut().zip(self.cells[index]) {
                        *out += f32::from(value) / 255. * weight;
                    }
                }
            }
        }
        Ok(result)
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Parameters {
    pub age: f32,
    pub interpolation: f32,
    pub expansion: f32,
    pub shape: [f32; 4],
}
fn smooth(value: f32) -> f32 {
    let x = value.clamp(0., 1.);
    (3. - (x + x)) * (x * x)
}
impl Parameters {
    /// Apply the shader's temporal and endpoint channel blend before noise/effects.
    pub fn density(&self, channels: [f32; 4], endpoint_distance: f32) -> Result<f32> {
        ensure!(
            self.interpolation.is_finite()
                && (0. ..=1.).contains(&self.interpolation)
                && channels
                    .iter()
                    .all(|v| v.is_finite() && (0. ..=1.).contains(v))
                && endpoint_distance.is_finite()
                && endpoint_distance >= 0.,
            "invalid smoke atlas sample"
        );
        let extended = channels[0] + self.interpolation * (channels[1] - channels[0]);
        let raw = channels[2] + self.interpolation * (channels[3] - channels[2]);
        Ok(if raw < extended {
            raw + smooth((endpoint_distance - 10.) / 30.) * (extended - raw)
        } else {
            raw
        })
    }
    /// Times must describe the same renderer update generation.
    pub fn new(now: f32, started: f32, updated: f32) -> Result<Self> {
        ensure!(
            [now, started, updated].iter().all(|x| x.is_finite()),
            "non-finite smoke atlas clock"
        );
        let age = now - started;
        ensure!(age >= 0., "smoke atlas clock precedes creation");
        Ok(Self {
            age,
            interpolation: ((now - updated) / 0.1).clamp(0., 1.),
            expansion: 0.25 * smooth((age - 6.) / -11.),
            shape: [
                smooth((age - 22.) / -5.),
                smooth((age - 0.1) / 1.4),
                smooth((age - 4.) / 14.),
                smooth((age - 0.1) / 1.9),
            ],
        })
    }
}
#[cfg(test)]
mod tests {
    use super::super::density;
    use super::*;
    #[test]
    fn blocked_fill_keeps_first_active_zero_and_history() {
        let mut atlas = Atlas::default();
        let mut density = vec![0.; N];
        let mut active = vec![false; N];
        let mut blocked = vec![false; N];
        let first = density::lookup()
            .iter()
            .position(|v| v.0 == [15, 16, 16])
            .unwrap();
        let later = density::lookup()
            .iter()
            .position(|v| v.0 == [17, 16, 16])
            .unwrap();
        let wall = density::lookup()
            .iter()
            .position(|v| v.0 == [16, 16, 16])
            .unwrap();
        assert!(first < later);
        active[first] = true;
        active[later] = true;
        density[later] = 50.;
        blocked[wall] = true;
        atlas.update_cells(density.iter().copied(), &active, &blocked);
        assert_eq!(atlas.cells[linear(density::lookup()[wall].0)], [0, 0, 0, 0]);
        density[first] = 25.;
        atlas.update_cells(density.iter().copied(), &active, &blocked);
        assert_eq!(
            atlas.cells[linear(density::lookup()[wall].0)],
            [0, 127, 0, 0]
        );
        density[first] = 50.;
        atlas.update_cells(density.iter().copied(), &active, &blocked);
        assert_eq!(
            atlas.cells[linear(density::lookup()[wall].0)],
            [127, 255, 0, 0]
        );
    }
    #[test]
    fn atlas_gutter_and_endpoint_transition() {
        let mut atlas = Atlas::default();
        atlas.cells.fill([255, 255, 0, 0]);
        assert_eq!(atlas.sample([0., 16., 16.], 1).unwrap(), [0.5, 0.5, 0., 0.]);
        assert_eq!(atlas.sample([0., 16., 16.], 0).unwrap(), [1., 1., 0., 0.]);
        let p = Parameters::new(10., 0., 10.).unwrap();
        assert_eq!(p.density([1., 1., 0., 0.], 10.).unwrap(), 0.);
        assert_eq!(p.density([1., 1., 0., 0.], 25.).unwrap(), 0.5);
        assert_eq!(p.density([1., 1., 0., 0.], 40.).unwrap(), 1.);
    }
    #[test]
    fn density_sampler_wraps_z_and_joint_march_shares_opacity_feedback() {
        use super::super::{effects::EffectFrame, sampling::*};
        let mut atlas = Atlas::default();
        atlas.cells[..1024].fill([255; 4]);
        assert_eq!(atlas.sample([16., 16., 0.], 0).unwrap(), [0.5; 4]);
        assert_eq!(atlas.sample([16., 16., 32.], 0).unwrap(), [0.5; 4]);
        atlas.cells.fill([255; 4]);
        let cloud = Cloud {
            atlas: &atlas,
            origin: [0.; 3],
            slot: 0,
            parameters: Parameters::new(10., 0., 10.).unwrap(),
        };
        let pixels = vec![0; 128 * 128 * 128 * 4];
        let noise = Noise::new(&pixels, [Address::Repeat; 3]).unwrap();
        let material = Material {
            speed: 1.,
            low_strength: 0.,
            high_strength: 0.,
            normal_strength: 0.,
            offset: -1.8,
            low_scale: 1.,
            high_scale: 1.,
            power: 1.,
            minimum: 0.,
            maximum: 1.,
            boil: 0.,
            blend: 0.,
        };
        let view = View {
            camera: [-400., 0., 0.],
            up: [0., 0., 1.],
            forward: [1., 0., 0.],
            basis: [[1., 0., 0.], [0., 1., 0.], [0., 0., 1.]],
            character_depth_separation: false,
            endpoint: [0.; 3],
            endpoint_height: 0.,
        };
        let frame = EffectFrame {
            now: 10.,
            ..EffectFrame::default()
        };
        let settings = March {
            direction: [1., 0., 0.],
            bounds: [[-10., -20., -20.], [10., 20., 20.]],
            max_distance: 1000.,
            step: 4.,
            jitter: 0.,
            density_scale: 0.02,
            absorption: 0.,
            endpoint_sample: true,
            depth_fade: 16.,
        };
        let one = march(&cloud, &noise, &material, &view, &frame, settings).unwrap();
        let layer = Layer {
            cloud: &cloud,
            bounds: settings.bounds,
        };
        let joint =
            march_layers(&[layer, layer], &noise, &material, &view, &frame, settings).unwrap();
        assert!(one > 0. && joint > one && joint < 1.);
        assert!((joint - (one + (1. - one) * one)).abs() > 0.001);
    }
    #[test]
    fn captured_renderer_shape_parameters() {
        let p = Parameters::new(577.5625, 558.15625, 577.53125).unwrap();
        assert_eq!(p.interpolation, 0.3125);
        assert_eq!(
            p.shape.map(f32::to_bits),
            [0.5281118154525757_f32, 1., 1., 1.].map(f32::to_bits)
        );
        assert!(Parameters::new(f32::NAN, 0., 0.).is_err());
    }
}
