//! Compact top-down coverage shared by replay consumers. These are estimates,
//! not a replacement for the view-dependent game renderer.
use super::{density, disturbance, effects::EffectFrame, timeline::Timeline};
use serde::{Deserialize, Serialize};

/// World-space horizontal runs: x, y, z, length in 20-HU cells, opacity (0..15).
pub type Run = [i32; 5];
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub t: i32,
    /// None means unavailable; an empty list means no smoke.
    pub cells: Option<Vec<Run>>,
}
/// Consumers depend on coverage, not the game's journal format.
pub trait Coverage {
    fn top_down(&self, now: f32, effects: &EffectFrame) -> Option<Vec<Run>>;
}
impl Coverage for Timeline {
    fn top_down(&self, now: f32, effects: &EffectFrame) -> Option<Vec<Run>> {
        if !now.is_finite() {
            return None;
        }
        let mut runs = Vec::new();
        for (_, volume) in self.volumes() {
            if volume.did_effect != Some(true) {
                continue;
            }
            let origin = volume.origin?;
            let age = now - volume.effect_tick? as f32 / 64.;
            if age < 0. {
                return None;
            }
            let weight = super::lifetime(age);
            if weight == 0. {
                continue;
            }
            let density = volume.density()?;
            let mut columns = [0_f32; 1024];
            let coordinates = density::lookup();
            // ponytail: max-density columns are a top-down estimate; use view raymarching for POV opacity.
            for (i, value) in density.values().enumerate() {
                if value <= 0. || density.blocked_cells()[i] {
                    continue;
                }
                let p = coordinates[i].0;
                let point =
                    std::array::from_fn(|axis| origin[axis] + (p[axis] as f32 - 16.) * 20. + 10.);
                let mut opacity = (value / 50.).clamp(0., 1.) * weight;
                if effects.he_count > 0 {
                    opacity *= disturbance::high_explosives(
                        point,
                        effects,
                        disturbance::HeContext {
                            cloud_age: age,
                            cloud_slot: 0,
                            endpoint_guard_distance: 1000.,
                            character_depth_separation: false,
                        },
                    )
                    .ok()?
                    .density_factor;
                }
                if effects.bullet_count > 0 {
                    opacity = disturbance::bullets(point, effects)
                        .ok()?
                        .density(opacity)
                        .ok()?;
                }
                let column = p[1] as usize * 32 + p[0] as usize;
                columns[column] = columns[column].max(opacity);
            }
            runs.extend(encode(origin, &columns));
        }
        Some(runs)
    }
}
fn encode(origin: [f32; 3], columns: &[f32; 1024]) -> Vec<Run> {
    let mut runs = Vec::new();
    for y in 0..32 {
        let mut x = 0;
        while x < 32 {
            let alpha = (columns[y * 32 + x].clamp(0., 1.) * 15.).round() as i32;
            let start = x;
            x += 1;
            while x < 32 && (columns[y * 32 + x].clamp(0., 1.) * 15.).round() as i32 == alpha {
                x += 1;
            }
            if alpha > 0 {
                runs.push([
                    (origin[0] - 320.).round() as i32 + start as i32 * 20,
                    (origin[1] - 320.).round() as i32 + y as i32 * 20,
                    origin[2].round() as i32,
                    (x - start) as i32,
                    alpha,
                ]);
            }
        }
    }
    runs
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn coverage_runs_preserve_gaps_and_density_without_repeating_cells() {
        let mut columns = [0.; 1024];
        columns[0..4].fill(1.);
        columns[5] = 0.5;
        assert_eq!(
            encode([320.; 3], &columns),
            vec![[0, 0, 320, 4, 15], [100, 0, 320, 1, 8]]
        );
        assert_eq!(
            Timeline::default().top_down(0., &EffectFrame::default()),
            Some(vec![])
        );
        assert_eq!(
            Timeline::default().top_down(f32::NAN, &EffectFrame::default()),
            None
        );
    }
}
