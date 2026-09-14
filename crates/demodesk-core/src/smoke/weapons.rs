//! Ballistic parameters from the installed, expanded weapons.vdata resource.
use anyhow::{ensure, Context, Result};
use serde_json::Value;

#[derive(Clone, Copy, Debug)]
pub struct Weapon {
    pub pellets: u32,
    pub pattern_seed: u32,
    pub damage: i32,
    pub penetration: f32,
    pub range: f32,
    pub range_modifier: f32,
}
impl Weapon {
    pub fn from_resource(resource: &Value, item_definition: u32) -> Result<Self> {
        let entry = resource
            .get(item_definition.to_string())
            .context("weapon definition missing")?;
        let unsigned = |name: &str| -> Result<u32> {
            u32::try_from(
                entry[name]
                    .as_u64()
                    .with_context(|| format!("invalid weapon {name}"))?,
            )
            .with_context(|| format!("oversized weapon {name}"))
        };
        let number = |name: &str| -> Result<f32> {
            let value = entry[name]
                .as_f64()
                .with_context(|| format!("missing weapon {name}"))? as f32;
            ensure!(value.is_finite(), "nonfinite weapon {name}");
            Ok(value)
        };
        let weapon = Self {
            pellets: unsigned("m_nNumBullets")?,
            pattern_seed: unsigned("m_nSpreadSeed")?,
            damage: i32::try_from(unsigned("m_nDamage")?)?,
            penetration: number("m_flPenetration")?,
            range: number("m_flRange")?,
            range_modifier: number("m_flRangeModifier")?,
        };
        ensure!(
            weapon.pellets > 0 && weapon.pellets <= 64,
            "unsupported pellet count"
        );
        ensure!(
            weapon.damage > 0
                && weapon.penetration >= 0.
                && weapon.range > 0.
                && weapon.range_modifier > 0.
                && weapon.range_modifier <= 1.,
            "invalid ballistic parameters"
        );
        Ok(weapon)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn expanded_item_definition_is_required_without_defaults() {
        let mut resource = serde_json::json!({"25":{
            "m_nNumBullets":6,"m_nSpreadSeed":817955,"m_nDamage":20,
            "m_flPenetration":1.,"m_flRange":3000.,"m_flRangeModifier":0.7
        }});
        let w = Weapon::from_resource(&resource, 25).unwrap();
        assert_eq!(
            (w.pellets, w.pattern_seed, w.damage, w.range),
            (6, 817955, 20, 3000.)
        );
        assert!(Weapon::from_resource(&resource, 7).is_err());
        resource["25"]
            .as_object_mut()
            .unwrap()
            .remove("m_flPenetration");
        assert!(Weapon::from_resource(&resource, 25).is_err());
    }
}
