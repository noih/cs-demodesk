//! Qualified smoke point sampling with explicitly supplied texture addressing.
use super::{
    atlas::{Atlas, Parameters},
    disturbance::{self, BulletSample, HeContext},
    effects::EffectFrame,
};
use anyhow::{ensure, Result};
#[derive(Clone, Copy, Debug)]
pub enum Address {
    Repeat,
    Mirror,
    Clamp,
}
pub struct Noise<'a> {
    pixels: &'a [u8],
    address: [Address; 3],
}
impl<'a> Noise<'a> {
    pub fn new(pixels: &'a [u8], address: [Address; 3]) -> Result<Self> {
        ensure!(
            pixels.len() == 128 * 128 * 128 * 4,
            "invalid smoke noise volume"
        );
        Ok(Self { pixels, address })
    }
    fn texel(i: i32, address: Address) -> usize {
        match address {
            Address::Clamp => i.clamp(0, 127) as usize,
            Address::Repeat => i.rem_euclid(128) as usize,
            Address::Mirror => {
                let i = i.rem_euclid(256);
                (if i < 128 { i } else { 255 - i }) as usize
            }
        }
    }
    pub fn sample(&self, position: [f32; 3]) -> Result<[f32; 2]> {
        ensure!(
            position.iter().all(|v| v.is_finite() && v.abs() < 1e6),
            "invalid smoke noise coordinate"
        );
        let p = position.map(|v| v * 128. - 0.5);
        let base = p.map(|v| v.floor() as i32);
        let frac = std::array::from_fn::<_, 3, _>(|i| p[i] - base[i] as f32);
        let mut out = [0.; 2];
        for z in 0..2 {
            for y in 0..2 {
                for x in 0..2 {
                    let delta = [x, y, z];
                    let cell = std::array::from_fn::<_, 3, _>(|i| {
                        Self::texel(base[i] + delta[i], self.address[i])
                    });
                    let weight = (0..3)
                        .map(|i| if delta[i] == 0 { 1. - frac[i] } else { frac[i] })
                        .product::<f32>();
                    let at = (cell[2] * 128 * 128 + cell[1] * 128 + cell[0]) * 4;
                    for c in 0..2 {
                        out[c] += self.pixels[at + c] as f32 / 255. * weight;
                    }
                }
            }
        }
        Ok(out)
    }
}
fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|i| a[i] + b[i])
}
fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|i| a[i] - b[i])
}
fn scale(a: [f32; 3], s: f32) -> [f32; 3] {
    a.map(|v| v * s)
}
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn length(v: [f32; 3]) -> f32 {
    dot(v, v).sqrt()
}
fn normalize(v: [f32; 3]) -> [f32; 3] {
    scale(v, 1. / length(v))
}
fn smooth(x: f32) -> f32 {
    let x = x.clamp(0., 1.);
    x * x * (3. - 2. * x)
}
#[derive(Clone, Copy, Debug)]
pub struct Material {
    pub speed: f32,
    pub low_strength: f32,
    pub high_strength: f32,
    pub normal_strength: f32,
    pub offset: f32,
    pub low_scale: f32,
    pub high_scale: f32,
    pub power: f32,
    pub minimum: f32,
    pub maximum: f32,
    pub boil: f32,
    pub blend: f32,
}
impl Material {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            [
                self.speed,
                self.low_strength,
                self.high_strength,
                self.normal_strength,
                self.offset,
                self.low_scale,
                self.high_scale,
                self.power,
                self.minimum,
                self.maximum,
                self.boil,
                self.blend
            ]
            .iter()
            .all(|x| x.is_finite())
                && self.low_scale > 0.
                && self.high_scale > 0.
                && self.power > 0.,
            "invalid smoke material"
        );
        Ok(())
    }
}
#[derive(Clone, Copy)]
pub struct View {
    pub camera: [f32; 3],
    pub up: [f32; 3],
    pub forward: [f32; 3],
    pub basis: [[f32; 3]; 3],
    pub character_depth_separation: bool,
    pub endpoint: [f32; 3],
    pub endpoint_height: f32,
}
impl View {
    fn side(&self) -> [f32; 3] {
        let a = self.forward;
        let b = self.up;
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    }
}
pub struct Cloud<'a> {
    pub atlas: &'a Atlas,
    pub origin: [f32; 3],
    pub slot: u8,
    pub parameters: Parameters,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct Point {
    pub opacity_density: f32,
    pub absorption_density: f32,
}
fn noise_field(
    noise: &Noise<'_>,
    m: &Material,
    time: f32,
    grid: [f32; 3],
    normal: [f32; 3],
    warped: [f32; 3],
    view: &View,
    dot_forward: f32,
) -> Result<f32> {
    let t = m.speed * time;
    let local = sub(grid, [0.5; 3]);
    let mut high = scale(local, 7.);
    let angle = 0.2
        * ((local[2] * 35.).sin() + 0.5).mul_add(0.15, 0.2)
        * (t * 0.5 + 0.5).sin()
        * (t * 0.187 + 0.5).sin()
        + t * 0.04;
    let (s, c) = angle.sin_cos();
    let rotated_x = c * high[0] - s * high[1];
    let rotated_y = s * high[0] + c * high[1];
    let clock = t + (t * 0.5).sin() * 0.02;
    let low_x = rotated_x + (local[2] * 18.9 + clock).sin() * 0.05;
    let low_z = high[2] + (rotated_x * 2.7 + clock).cos() * 0.05;
    let low_w =
        low_z + ((low_x * 3. + t * 0.35).sin() + (rotated_y * 2.84 + t * 0.235).sin()) * 0.05;
    let low = [low_x, rotated_y, low_w];
    let up = scale(view.up, 0.2);
    let side = scale(view.side(), 0.2);
    let transform = |v: f32| {
        let p = v.powf(m.power);
        let ordinary = p * (m.maximum - m.minimum) + m.minimum;
        ordinary + m.blend * ((0.25 - 1.75 * p) - ordinary)
    };
    let pair = |values: [f32; 2]| transform(values[0]) + 0.95 * transform(values[1]);
    let low_sample = |offset: [f32; 3]| -> Result<f32> {
        let p = add(scale(low, m.low_scale), offset);
        let q = std::array::from_fn(|i| (p[i].abs() - t * [0.2, 0.2, 0.45][i]) * 0.07);
        Ok(pair(noise.sample(q)?))
    };
    let low_value = low_sample([0.; 3])?;
    let low_up = low_sample(up)?;
    let low_side = low_sample(side)?;
    let gradient = normalize([
        (low_value - low_side) * 4.6,
        (low_value - low_up) * 4.6,
        0.8 / m.low_scale,
    ]);
    let combined = add(gradient, normal);
    let world = view.basis.map(|v| dot(combined, v));
    high = add(
        high,
        scale(world, (low_value * 4.6).powf(0.1) * m.boil * 0.2),
    );
    let flow = (low_value * 4.6 - 1.) * m.boil * local[2];
    high = add(high, scale([0.4, 0.4, 0.9], flow));
    high[0] += (t * 0.25 + low_w).sin() * 0.05;
    let high_value = pair(noise.sample(scale(high, m.high_scale * 0.07))?) * 4.6;
    let distance = (length(sub(warped, view.camera)) * 0.005).min(1.);
    Ok(0.95
        + m.low_strength * ((low_value * 4.6).clamp(0., 1.) - 0.95)
        + distance * dot_forward * m.high_strength * (high_value.clamp(0., 1.) - 0.95)
        + m.offset
        + 0.95)
}
/// Full alpha-relevant point kernel; excludes lighting and final ray integration.
pub fn point(
    cloud: &Cloud<'_>,
    noise: &Noise<'_>,
    material: &Material,
    view: &View,
    frame: &EffectFrame,
    bullet: BulletSample,
    dot_forward: f32,
    depth_fade: f32,
) -> Result<Point> {
    material.validate()?;
    let world = bullet.position;
    let grid = add(scale(sub(world, cloud.origin), 0.05), [16.; 3]);
    let uv = grid.map(|v| (v / 32.).clamp(0., 1.));
    let endpoint = length(sub(world, view.endpoint));
    let raw = cloud
        .parameters
        .density(cloud.atlas.sample(grid, cloud.slot)?, endpoint)?;
    let density = bullet.density(raw)?;
    if density <= 0.01 {
        return Ok(Point::default());
    }
    let normal = if material.normal_strength > 0. {
        let sample = |offset: [f32; 3]| -> Result<f32> {
            let q = std::array::from_fn(|i| (uv[i] + offset[i]).clamp(0., 1.) * 32.);
            cloud
                .parameters
                .density(cloud.atlas.sample(q, cloud.slot)?, endpoint)
        };
        normalize([
            density - sample(scale(view.side(), 0.03))?,
            sample(scale(view.up, -0.03))? - density,
            0.06,
        ])
    } else {
        [0., 0., 1.]
    };
    let guard_distance =
        (endpoint - (2. * (view.endpoint_height - cloud.origin[2]).abs()).min(20.)).max(0.);
    let mut d = ((density - 0.01) * 1.010101).max(0.) * cloud.parameters.shape[0];
    d = (d * (2. - (length(sub(world, view.camera)) * 0.1).min(1.))).clamp(0., 1.);
    let he = disturbance::high_explosives(
        world,
        frame,
        HeContext {
            cloud_age: cloud.parameters.age,
            cloud_slot: cloud.slot,
            endpoint_guard_distance: guard_distance,
            character_depth_separation: view.character_depth_separation,
        },
    )?;
    d *= he.density_factor;
    let n = noise_field(
        noise,
        material,
        frame.now,
        uv,
        normal,
        he.warped_position,
        view,
        dot_forward,
    )?;
    let shape = cloud.parameters.shape;
    let w = shape[3].clamp(0.0001, 0.9999);
    let a = smooth((w * 1.25).min(1.));
    let b = smooth(((w - 0.2) * 1.25).max(0.));
    if shape[3] < 1. {
        let p = sub(uv, [0.5; 3]);
        let radius = length([p[0], p[1], p[2] * 1.2]);
        d *= if a == b {
            w
        } else {
            smooth((radius - a) / (b - a))
        };
    }
    let opacity = (shape[3] * shape[0] * 8.).clamp(0., 1.) * (2. * d + n - 1.);
    if opacity < 0.0001 {
        return Ok(Point::default());
    }
    let density_threshold = smooth(((opacity + 0.3) * 5.).clamp(0., 1.));
    let fade = ((depth_fade - guard_distance) / depth_fade).clamp(0., 1.);
    Ok(Point {
        opacity_density: opacity,
        absorption_density: d * fade * density_threshold,
    })
}

#[derive(Clone, Copy)]
pub struct March {
    pub direction: [f32; 3],
    pub bounds: [[f32; 3]; 2],
    pub max_distance: f32,
    pub step: f32,
    pub jitter: f32,
    pub density_scale: f32,
    pub absorption: f32,
    pub endpoint_sample: bool,
    pub depth_fade: f32,
}
/// A cloud and its renderer bounds, in ascending instance-mask bit order.
#[derive(Clone, Copy)]
pub struct Layer<'a> {
    pub cloud: &'a Cloud<'a>,
    pub bounds: [[f32; 3]; 2],
}

pub fn march(
    cloud: &Cloud<'_>,
    noise: &Noise<'_>,
    material: &Material,
    view: &View,
    frame: &EffectFrame,
    settings: March,
) -> Result<f32> {
    march_layers(
        &[Layer {
            cloud,
            bounds: settings.bounds,
        }],
        noise,
        material,
        view,
        frame,
        settings,
    )
}

fn interval(
    camera: [f32; 3],
    direction: [f32; 3],
    bounds: [[f32; 3]; 2],
) -> Result<Option<[f32; 2]>> {
    ensure!(
        bounds.iter().flatten().all(|x| x.is_finite()),
        "invalid smoke bounds"
    );
    let mut near = f32::NEG_INFINITY;
    let mut far = f32::INFINITY;
    for i in 0..3 {
        ensure!(bounds[0][i] <= bounds[1][i], "inverted smoke bounds");
        if direction[i] == 0. {
            if camera[i] < bounds[0][i] || camera[i] > bounds[1][i] {
                return Ok(None);
            }
        } else {
            let a = (bounds[0][i] - camera[i]) / direction[i];
            let b = (bounds[1][i] - camera[i]) / direction[i];
            near = near.max(a.min(b));
            far = far.min(a.max(b));
        }
    }
    Ok((far >= near).then_some([near, far]))
}

/// March all selected clouds on one sampling schedule, accumulating each cloud
/// at each position. Separately compositing whole-cloud alphas changes the result.
/// This is continuous canonical opacity, not exact native binary visibility.
pub fn march_layers(
    layers: &[Layer<'_>],
    noise: &Noise<'_>,
    material: &Material,
    view: &View,
    frame: &EffectFrame,
    settings: March,
) -> Result<f32> {
    ensure!(layers.len() <= 16, "too many smoke layers");
    if layers.is_empty() {
        return Ok(0.);
    }
    ensure!(
        settings
            .direction
            .iter()
            .chain(settings.bounds.iter().flatten())
            .chain(view.camera.iter())
            .all(|x| x.is_finite())
            && [
                settings.max_distance,
                settings.step,
                settings.jitter,
                settings.density_scale,
                settings.absorption,
                settings.depth_fade
            ]
            .iter()
            .all(|x| x.is_finite())
            && settings.step >= 1.
            && settings.step <= 64.
            && settings.max_distance > 2.
            && settings.max_distance <= 65536.
            && (0. ..=1.).contains(&settings.jitter)
            && settings.depth_fade > 0.
            && settings.density_scale >= 0.
            && settings.absorption >= 0.,
        "invalid smoke ray"
    );
    ensure!(length(settings.direction) > 0., "zero smoke ray");
    let direction = normalize(settings.direction);
    let Some([near, far]) = interval(view.camera, direction, settings.bounds)? else {
        return Ok(0.);
    };
    let mut intervals = [None; 16];
    for (i, layer) in layers.iter().enumerate() {
        intervals[i] = interval(view.camera, direction, layer.bounds)?;
    }
    let jitter_scale = ((near + 150.) * 0.05).clamp(0., 1.) * 0.7 + 0.1;
    let start = near.max(4.) + settings.jitter * settings.step * jitter_scale;
    let end = far.min(settings.max_distance - 2.);
    if start > end {
        return Ok(0.);
    }
    let mut local_view = *view;
    local_view.endpoint = add(view.camera, scale(direction, end));
    let dot_forward = dot(view.forward, direction);
    let budget = (((end - start) / settings.step).ceil() + 10.).clamp(1., 500.) as usize;
    let mut alpha = 0_f32;
    let mut absorption = 0_f32;
    let mut last_distance = start;
    let mut cached = None;
    let mut early = false;
    let accumulate = |p: Point, weight: f32, alpha: &mut f32, absorption: &mut f32| {
        if p.opacity_density < 0.0001 {
            return;
        }
        let sample = smooth(
            ((*alpha * 1.5 + 0.5) * settings.density_scale / 0.2 * p.opacity_density).clamp(0., 1.),
        );
        let integer = weight.floor() as usize;
        for _ in 0..integer {
            *alpha += (1. - *alpha) * sample;
        }
        *alpha += (1. - *alpha) * sample * (weight - integer as f32);
        *absorption += p.absorption_density * settings.absorption * 6. * weight;
    };
    for i in 0..budget {
        let distance = start + i as f32 * settings.step;
        last_distance = distance;
        let position = add(view.camera, scale(direction, distance));
        let refresh = i < 16 || i % 16 == 0;
        if refresh {
            cached = None;
        }
        for (layer, limits) in layers.iter().zip(intervals) {
            let Some([near, far]) = limits else {
                continue;
            };
            if distance < near || distance > far {
                continue;
            }
            if cached.is_none() {
                let b = disturbance::bullets(position, frame)?;
                cached = Some((sub(b.position, position), b.opening, b.glow));
            }
            let (offset, opening, glow) = cached.unwrap();
            let p = point(
                layer.cloud,
                noise,
                material,
                &local_view,
                frame,
                BulletSample {
                    position: add(position, offset),
                    opening,
                    glow,
                },
                dot_forward,
                settings.depth_fade,
            )?;
            accumulate(p, settings.step * 0.25, &mut alpha, &mut absorption);
            if alpha > 0.991 {
                early = true;
                break;
            }
        }
        if early {
            break;
        }
        if distance + settings.step >= end {
            break;
        }
    }
    if settings.endpoint_sample && !early {
        let position = add(view.camera, scale(direction, end));
        let (offset, opening, glow) = cached.unwrap_or(([0.; 3], 0., 0.));
        for (layer, limits) in layers.iter().zip(intervals) {
            let Some([near, far]) = limits else {
                continue;
            };
            if last_distance < near || last_distance > far {
                continue;
            }
            let p = point(
                layer.cloud,
                noise,
                material,
                &local_view,
                frame,
                BulletSample {
                    position: add(position, offset),
                    opening,
                    glow,
                },
                dot_forward,
                settings.depth_fade,
            )?;
            accumulate(
                p,
                (end - last_distance).max(0.) * 0.25,
                &mut alpha,
                &mut absorption,
            );
            if alpha > 0.991 {
                break;
            }
        }
    }

    let output = if settings.absorption == 0. {
        alpha
    } else {
        alpha + (absorption - alpha * 0.2).clamp(0., 1.)
    };
    Ok(if output < 0.00001 { 0. } else { output })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn noise_addressing_preserves_rg_and_half_texel_boundaries() {
        let mut pixels = vec![0; 128 * 128 * 128 * 4];
        for z in 0..128 {
            for y in 0..128 {
                for x in 0..128 {
                    let at = (z * 128 * 128 + y * 128 + x) * 4;
                    pixels[at] = x as u8;
                    pixels[at + 1] = 255 - x as u8;
                }
            }
        }
        let clamp = Noise::new(&pixels, [Address::Clamp; 3]).unwrap();
        let mirror = Noise::new(&pixels, [Address::Mirror; 3]).unwrap();
        let repeat = Noise::new(&pixels, [Address::Repeat; 3]).unwrap();
        assert_eq!(clamp.sample([0., 0.5, 0.5]).unwrap(), [0., 1.]);
        assert_eq!(mirror.sample([0., 0.5, 0.5]).unwrap(), [0., 1.]);
        let edge = repeat.sample([0., 0.5, 0.5]).unwrap();
        assert!((edge[0] - 63.5 / 255.).abs() < 1e-6);
        assert!((clamp.sample([1., 0.5, 0.5]).unwrap()[0] - 127. / 255.).abs() < 1e-6);
        assert!(clamp.sample([f32::NAN, 0., 0.]).is_err());
        assert!(Noise::new(&pixels[..100], [Address::Clamp; 3]).is_err());
    }
}
