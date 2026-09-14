//! Pointwise displacement from the qualified new-visuals smoke shader.
//! Callers retain native ray-step refresh ordering; this is not final opacity.
use super::effects::EffectFrame;
use anyhow::{ensure, Result};
fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|i| a[i] - b[i])
}
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn length(v: [f32; 3]) -> f32 {
    dot(v, v).sqrt()
}
fn xyz(v: [f32; 4]) -> [f32; 3] {
    [v[0], v[1], v[2]]
}
fn smooth(x: f32) -> f32 {
    let x = x.clamp(0., 1.);
    x * x * (3. - 2. * x)
}
fn finite(v: [f32; 3]) -> bool {
    v.iter().all(|v| v.is_finite())
}
#[derive(Clone, Copy, Debug)]
pub struct BulletSample {
    pub position: [f32; 3],
    pub opening: f32,
    pub glow: f32,
}
impl BulletSample {
    pub fn density(&self, density: f32) -> Result<f32> {
        ensure!(
            density.is_finite() && self.opening.is_finite() && (0. ..=1.).contains(&self.opening),
            "invalid bullet density sample"
        );
        Ok((density + self.opening * (-density - 0.05)).clamp(0., 1.))
    }
}
/// Evaluate a fresh bullet influence at a point. The full marcher must cache
/// this influence using the shader's refresh schedule after its first16 steps.
pub fn bullets(point: [f32; 3], frame: &EffectFrame) -> Result<BulletSample> {
    ensure!(
        finite(point) && frame.bullet_count <= frame.bullets.len(),
        "invalid bullet sample input"
    );
    let mut direction = [0., 0., 0.01];
    let mut opening = 0_f32;
    let mut glow = 0_f32;
    for b in &frame.bullets[..frame.bullet_count] {
        ensure!(
            b.start_age
                .iter()
                .chain(b.end_flag.iter())
                .chain(b.width.iter())
                .all(|v| v.is_finite())
                && (0. ..1.).contains(&b.start_age[3])
                && b.width[0] > 0.,
            "invalid packed bullet"
        );
        let start = xyz(b.start_age);
        let end = xyz(b.end_flag);
        let axis = sub(end, start);
        let squared = dot(axis, axis);
        if squared == 0. {
            continue;
        }
        let from_start = sub(point, start);
        let projection = (dot(from_start, axis) / squared).clamp(0., 1.);
        let perpendicular = std::array::from_fn(|i| from_start[i] - projection * axis[i]);
        let radial = (length(perpendicular) * 0.05 * b.width[0]).clamp(0., 1.);
        let age = b.start_age[3];
        let envelope = smooth(age * 100.) * (1. - smooth((age - 0.01) * (1. / 0.19)));
        let mut hotspot = 0.;
        if radial < 1. {
            hotspot = envelope * (1. - radial).powf(64.) * 10.;
            let endpoint_distance = (length(sub(point, end)) * 0.01).min(1.);
            let a = (radial - endpoint_distance + 1.).min(1.);
            let influence = smooth(1. - (a + age).clamp(0., 1.));
            opening = opening.max(influence);
            let reverse = sub(start, end);
            let inverse = 1. / squared.sqrt();
            for i in 0..3 {
                direction[i] += opening * (reverse[i] * inverse - direction[i]);
            }
        }
        if b.end_flag[3] > 0. {
            let start_falloff = 1. - (length(from_start) * 0.01).min(1.);
            glow = glow.max(hotspot.max((envelope * start_falloff).powi(2)));
        }
    }
    let inverse = 1. / length(direction);
    let displacement = opening * opening * opening * 20.;
    Ok(BulletSample {
        position: std::array::from_fn(|i| point[i] + direction[i] * inverse * displacement),
        opening,
        glow,
    })
}
#[derive(Clone, Copy, Debug)]
pub struct HeContext {
    pub cloud_age: f32,
    pub cloud_slot: u8,
    /// max(0, distance(warped sample, ray endpoint) - vertical endpoint allowance).
    pub endpoint_guard_distance: f32,
    /// True when depth excluding characters lies >10HU behind scene depth.
    pub character_depth_separation: bool,
}
#[derive(Clone, Copy, Debug)]
pub struct HeSample {
    /// HE warps subsequent HE and distance/lighting inputs; atlas coordinates are retained.
    pub warped_position: [f32; 3],
    pub density_factor: f32,
    pub endpoint_guard: f32,
}
pub fn high_explosives(
    point: [f32; 3],
    frame: &EffectFrame,
    context: HeContext,
) -> Result<HeSample> {
    ensure!(
        finite(point)
            && frame.now.is_finite()
            && frame.he_count <= frame.he.len()
            && context.cloud_slot < 16
            && context.cloud_age.is_finite()
            && context.cloud_age >= 0.
            && context.endpoint_guard_distance.is_finite()
            && context.endpoint_guard_distance >= 0.,
        "invalid HE sample input"
    );
    let mut position = point;
    let mut factor = 1_f32;
    let mut guard = 0.;
    let endpoint = (1. - context.endpoint_guard_distance * (1. / 48.)).max(0.);
    for h in &frame.he[..frame.he_count] {
        ensure!(
            h.position_time.iter().all(|v| v.is_finite())
                && h.mask.is_finite()
                && h.mask >= 0.
                && h.mask <= 65535.
                && h.mask.fract() == 0.,
            "invalid packed HE"
        );
        if h.mask as u32 & (1_u32 << context.cloud_slot) == 0 {
            continue;
        }
        let age = frame.now - h.position_time[3];
        ensure!(age >= 0. && age < 5., "HE sample outside ring lifetime");
        if age >= context.cloud_age - 0.4 {
            continue;
        }
        let centre = xyz(h.position_time);
        let distance = length(sub(position, centre));
        if distance >= 250. {
            continue;
        }
        let initial = (1. - smooth(age * 0.5)).powf(128.);
        let remaining = 1. - smooth(age * (1. / 7.));
        let shock = (1. - smooth((distance - 100.) * (1. / 150.)))
            * (1. - initial)
            * if distance >= age * 1250. { 1. } else { 0. };
        for i in 0..3 {
            position[i] += shock * (centre[i] - position[i]);
        }
        let radial = smooth((distance + initial * 250. - 200.) * 0.025);
        let recovery = smooth((age - 0.5) * (1. / 4.5)).powf(1.8);
        if !context.character_depth_separation {
            guard = endpoint * remaining;
        }
        factor = factor.min(guard.max(radial + recovery));
    }
    Ok(HeSample {
        warped_position: position,
        density_factor: 0.02 + 0.98 * factor,
        endpoint_guard: guard,
    })
}

#[cfg(test)]
mod tests {
    use super::super::effects::{PackedBullet, PackedHe};
    use super::*;
    fn alpha(density: f32) -> f32 {
        let x = ((density - 0.01) * 1.010101).max(0.) * 0.5;
        smooth(x)
    }
    #[test]
    fn original_shader_bullet_age_and_endpoint_fixture() {
        let mut frame = EffectFrame::default();
        frame.bullet_count = 1;
        frame.bullets[0] = PackedBullet {
            start_age: [-100., 0., 0., 0.5],
            end_flag: [100., 0., 0., 0.],
            width: [1., 0., 0., 0.],
        };
        let sample = bullets([0.; 3], &frame).unwrap();
        assert!((alpha(sample.density(0.5).unwrap()) - 0.0328120142).abs() < 2e-6);
        frame.bullets[0].end_flag[0] = 0.;
        assert_eq!(bullets([0.; 3], &frame).unwrap().opening, 0.);
        frame.bullet_count = 17;
        assert!(bullets([0.; 3], &frame).is_err());
    }
    #[test]
    fn original_shader_he_guard_and_mask_fixture() {
        let mut frame = EffectFrame::default();
        frame.now = 10.;
        frame.he_count = 1;
        frame.he[0] = PackedHe {
            position_time: [0., 0., 0., 9.],
            mask: 1.,
        };
        let mut context = HeContext {
            cloud_age: 10.,
            cloud_slot: 0,
            endpoint_guard_distance: 1.,
            character_depth_separation: true,
        };
        let sample = high_explosives([0.; 3], &frame, context).unwrap();
        let density = ((0.5 - 0.01) * 1.010101) * sample.density_factor;
        assert!((smooth(density * 0.5) - 0.0000907270733).abs() < 2e-6);
        context.character_depth_separation = false;
        let guarded = high_explosives([0.; 3], &frame, context).unwrap();
        assert!(
            (smooth(((0.5 - 0.01) * 1.010101) * guarded.density_factor * 0.5) - 0.133588687).abs()
                < 2e-6
        );
        context.cloud_slot = 1;
        assert_eq!(
            high_explosives([0.; 3], &frame, context)
                .unwrap()
                .density_factor,
            1.
        );
        // The same atlas slot can be reused by a cloud born after this HE.
        context.cloud_slot = 0;
        context.cloud_age = 0.5;
        assert_eq!(
            high_explosives([0.; 3], &frame, context)
                .unwrap()
                .density_factor,
            1.
        );
    }
}
