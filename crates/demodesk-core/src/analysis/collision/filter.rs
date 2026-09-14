//! Qualified static-world layer filtering; dynamic body overrides are separate.
use anyhow::{ensure, Context, Result};
use serde_json::Value;

#[derive(Clone, Copy)]
pub enum WorldQuery {
    Bullet,
    SmokeBlast,
}

/// Tokens and layer indices are from the qualified Source 2 registry. Unknown
/// tokens or body-specific detail membership cannot silently become non-solid.
pub fn world_attributes(attributes: &[Value], query: WorldQuery) -> Result<Vec<bool>> {
    let with = match query {
        WorldQuery::Bullet => 0x1c3009,
        WorldQuery::SmokeBlast => 0x2001,
    };
    attributes
        .iter()
        .map(|attribute| {
            ensure!(
                attribute["m_DetailLayers"]
                    .as_array()
                    .context("missing detail layers")?
                    .is_empty(),
                "unsupported world detail membership"
            );
            let group = attribute["m_CollisionGroupString"]
                .as_str()
                .context("missing collision group")?;
            let default = if group.eq_ignore_ascii_case("default") {
                true
            } else {
                ensure!(
                    group.eq_ignore_ascii_case("conditionallysolid"),
                    "unsupported world collision group"
                );
                false
            };
            let mut as_mask = 0_u64;
            for token in attribute["m_InteractAs"]
                .as_array()
                .context("missing world interaction layers")?
            {
                let bit = match token.as_u64().context("invalid collision layer token")? {
                    286764971 => 5,
                    457985020 => 12,
                    1319015392 => 7,
                    1514777866 => 6,
                    1882493596 => 10,
                    2233579705 => 18,
                    2516355760 => 33,
                    3421025643 => 13,
                    3432399841 => 0,
                    3783272137 => 39,
                    3802511415 => 8,
                    3903581251 => 4,
                    4140083888 => 3,
                    _ => anyhow::bail!("unresolved collision layer token"),
                };
                as_mask |= 1_u64 << bit;
            }
            if default && as_mask == 0 {
                as_mask = 0x4c1;
            }
            // Both queries have As=Exclude=0, so With/Exclude on the shape drop
            // out. Qualified group rows 3 and 4 both return solid response 21
            // for the two supported world groups above.
            Ok(as_mask & with != 0)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn world_defaults_and_unknown_membership_are_distinct() {
        let shape = |group, tokens| json!({"m_CollisionGroupString":group,"m_DetailLayers":[],"m_InteractAs":tokens});
        let rows = [
            shape("Default", vec![]),
            shape("ConditionallySolid", vec![3903581251_u64]),
            shape("ConditionallySolid", vec![3421025643]),
        ];
        assert_eq!(
            world_attributes(&rows, WorldQuery::Bullet).unwrap(),
            [true, false, true]
        );
        assert_eq!(
            world_attributes(&rows, WorldQuery::SmokeBlast).unwrap(),
            [true, false, true]
        );
        assert!(world_attributes(&[shape("Default", vec![1])], WorldQuery::Bullet).is_err());
        let mut detailed = rows[0].clone();
        detailed["m_DetailLayers"] = json!([1]);
        assert!(world_attributes(&[detailed], WorldQuery::SmokeBlast).is_err());
    }
}
