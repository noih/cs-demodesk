//! Surface identities and game overrides from installed surface-properties assets.
use super::Surface;
use anyhow::{ensure, Context, Result};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};

fn list(value: &Value) -> Result<&[Value]> {
    value["SurfacePropertiesList"]
        .as_array()
        .map(Vec::as_slice)
        .context("missing surface properties list")
}
fn name(value: &Value) -> Result<String> {
    let name = value["surfacePropertyName"]
        .as_str()
        .context("missing surface name")?;
    ensure!(
        !name.is_empty() && name.is_ascii(),
        "unsupported surface name"
    );
    Ok(name.to_ascii_lowercase())
}
fn hash(value: &Value, key: &str) -> Result<u32> {
    u32::try_from(
        value[key]
            .as_u64()
            .with_context(|| format!("missing surface {key}"))?,
    )
    .context("surface hash out of range")
}

// Pinned server BA1D17 calls tier0!V_atoi, then BA1D1D..28 keeps values
// 1..15, otherwise the first signed byte. Only the asset's canonical ASCII
// character/decimal forms are supported; other conversion syntax is rejected.
fn material_kind(text: &str) -> Result<u16> {
    ensure!(
        !text.is_empty() && text.is_ascii(),
        "unsupported game material code"
    );
    if text.bytes().all(|b| b.is_ascii_digit()) {
        let number: i32 = text.parse().context("game material number out of range")?;
        if (1..=15).contains(&number) {
            return Ok(number as u16);
        }
    } else {
        ensure!(text.len() == 1, "unsupported game material syntax");
    }
    Ok(u16::from(text.as_bytes()[0]))
}

/// Resolve by recorded PHYS surface hash, never by matching numeric coefficients.
/// The game default applies even to physical roots without their own override.
pub fn resolve(physical: &Value, game: &Value) -> Result<BTreeMap<u32, Surface>> {
    let mut definitions = BTreeMap::new();
    for row in list(physical)? {
        let id = hash(row, "m_nameHash")?;
        ensure!(
            id != 0 && definitions.insert(id, row).is_none(),
            "duplicate/zero surface hash"
        );
        name(row)?;
    }
    let mut overrides = BTreeMap::new();
    for row in list(game)? {
        ensure!(
            overrides.insert(name(row)?, row).is_none(),
            "duplicate game surface name"
        );
    }
    let default = overrides
        .get("default")
        .context("missing default game surface")?;
    let mut output = BTreeMap::new();
    for (&id, &row) in &definitions {
        let mut chain = Vec::new();
        let mut seen = BTreeSet::new();
        let mut current = row;
        loop {
            ensure!(
                seen.insert(hash(current, "m_nameHash")?),
                "cyclic surface inheritance"
            );
            chain.push(current);
            let parent = hash(current, "m_baseNameHash")?;
            if parent == 0 {
                break;
            }
            current = definitions.get(&parent).context("missing parent surface")?;
        }
        let mut properties: Map<String, Value> =
            default.as_object().context("invalid game default")?.clone();
        for layer in chain.into_iter().rev() {
            if let Some(override_) = overrides.get(&name(layer)?) {
                properties.extend(
                    override_
                        .as_object()
                        .context("invalid game surface")?
                        .clone(),
                );
            }
        }
        let distance = properties
            .get("bulletPenetrationDistanceModifier")
            .and_then(Value::as_f64)
            .context("missing surface penetration distance")? as f32;
        ensure!(
            distance.is_finite() && distance >= 0.,
            "invalid surface penetration distance"
        );
        let kind = material_kind(
            properties
                .get("gamematerial")
                .and_then(Value::as_str)
                .context("missing surface game material")?,
        )?;
        output.insert(
            id,
            Surface {
                penetration_distance: distance,
                kind,
            },
        );
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn inherited_materials_use_source_identity_and_native_numeric_codes() {
        let physical = json!({"SurfacePropertiesList":[
            {"m_nameHash":1,"m_baseNameHash":0,"surfacePropertyName":"root"},
            {"m_nameHash":2,"m_baseNameHash":1,"surfacePropertyName":"metal_barrel"},
            {"m_nameHash":3,"m_baseNameHash":2,"surfacePropertyName":"child"}]});
        let game = json!({"SurfacePropertiesList":[
            {"surfacePropertyName":"default","gamematerial":"C","bulletPenetrationDistanceModifier":0.5},
            {"surfacePropertyName":"METAL_BARREL","gamematerial":"12","bulletPenetrationDistanceModifier":0.8},
            {"surfacePropertyName":"child","bulletPenetrationDistanceModifier":0.4}]});
        let result = resolve(&physical, &game).unwrap();
        assert_eq!(
            (result[&1].kind, result[&1].penetration_distance),
            (67, 0.5)
        );
        assert_eq!(
            (result[&2].kind, result[&2].penetration_distance),
            (12, 0.8)
        );
        assert_eq!(
            (result[&3].kind, result[&3].penetration_distance),
            (12, 0.4)
        );
        assert_eq!(material_kind("15").unwrap(), 15);
        assert_eq!(material_kind("16").unwrap(), u16::from(b'1'));
        let mut broken = physical.clone();
        broken["SurfacePropertiesList"][0]["m_baseNameHash"] = json!(3);
        assert!(resolve(&broken, &game).is_err());
        broken["SurfacePropertiesList"][0]["m_baseNameHash"] = json!(99);
        assert!(resolve(&broken, &game).is_err());
    }
}
