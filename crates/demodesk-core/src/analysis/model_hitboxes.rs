//! Fixed points from actual mesh capsule geometry; never selected by aim proximity.
use super::animation_clip::Transform;
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Hitbox {
    pub index: u32,
    pub bone: String,
    pub center: [f32; 3],
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HitboxSet {
    pub name: String,
    pub hitboxes: Vec<Hitbox>,
}

/// Embedded mesh copies must agree. Empty meshes do not define a hitbox set.
pub fn parse_meshes(meshes: &[Value]) -> Result<Vec<HitboxSet>> {
    let mut canonical = None;
    for mesh in meshes {
        let Some(sets) = mesh.get("m_hitboxsets") else {
            continue;
        };
        let sets = sets.as_array().context("invalid model hitbox sets")?;
        if sets.is_empty() {
            continue;
        }
        ensure!(sets.len() <= 256, "oversized model hitbox sets");
        let mut parsed = Vec::new();
        let mut names = BTreeSet::new();
        for set in sets {
            let data = &set["value"];
            let name = data["m_name"].as_str().context("missing hitbox set name")?;
            ensure!(
                !name.is_empty() && names.insert(name),
                "duplicate hitbox set name"
            );
            ensure!(
                set["key"].as_str() == Some(name),
                "inconsistent hitbox set name"
            );
            let boxes = data["m_HitBoxes"]
                .as_array()
                .context("missing model hitboxes")?;
            ensure!(boxes.len() <= 1024, "oversized model hitbox set");
            let mut indices = BTreeSet::new();
            let mut hitboxes = Vec::new();
            for value in boxes {
                ensure!(
                    value["m_nShapeType"].as_u64() == Some(2)
                        && value["m_bTranslationOnly"].as_bool() == Some(false),
                    "unsupported model hitbox transform/shape"
                );
                let radius = value["m_flShapeRadius"]
                    .as_f64()
                    .context("missing capsule radius")?;
                ensure!(radius.is_finite() && radius > 0.0, "invalid capsule radius");
                let index = u32::try_from(
                    value["m_nHitBoxIndex"]
                        .as_u64()
                        .context("invalid hitbox index")?,
                )?;
                ensure!(indices.insert(index), "duplicate hitbox index");
                let bone = value["m_sBoneName"]
                    .as_str()
                    .context("missing hitbox bone")?;
                ensure!(!bone.is_empty(), "empty hitbox bone");
                let vector = |key: &str| -> Result<[f32; 3]> {
                    let a = value[key].as_array().context("missing hitbox endpoint")?;
                    ensure!(a.len() == 3, "invalid hitbox endpoint");
                    let mut out = [0.0; 3];
                    for i in 0..3 {
                        out[i] = a[i].as_f64().context("invalid hitbox endpoint")? as f32;
                        ensure!(out[i].is_finite(), "nonfinite hitbox endpoint");
                    }
                    Ok(out)
                };
                let min = vector("m_vMinBounds")?;
                let max = vector("m_vMaxBounds")?;
                let center = std::array::from_fn(|i| min[i] * 0.5 + max[i] * 0.5);
                hitboxes.push(Hitbox {
                    index,
                    bone: bone.into(),
                    center,
                });
            }
            hitboxes.sort_by_key(|h| h.index);
            parsed.push(HitboxSet {
                name: name.into(),
                hitboxes,
            });
        }
        if let Some(previous) = &canonical {
            ensure!(previous == &parsed, "conflicting mesh hitbox definitions");
        } else {
            canonical = Some(parsed);
        }
    }
    Ok(canonical.unwrap_or_default())
}

#[derive(Default)]
pub struct Points {
    sets: BTreeMap<(u64, u32), Vec<Point>>,
}
struct Point {
    output: usize,
    bone: Option<usize>,
    center: [f32; 3],
}
impl Points {
    pub fn new(
        models: &BTreeMap<u64, Vec<HitboxSet>>,
        bones: &[String],
        names: &mut Vec<String>,
    ) -> Result<Self> {
        let mut result = Self::default();
        for (&model, sets) in models {
            for (set, geometry) in sets.iter().enumerate() {
                let set = u32::try_from(set)?;
                let mut points = Vec::new();
                for hitbox in &geometry.hitboxes {
                    let matching: Vec<_> = bones
                        .iter()
                        .enumerate()
                        .filter(|(_, name)| name.eq_ignore_ascii_case(&hitbox.bone))
                        .map(|(i, _)| i)
                        .collect();
                    ensure!(matching.len() <= 1, "ambiguous hitbox bone name");
                    points.push(Point {
                        output: names.len(),
                        bone: matching.first().copied(),
                        center: hitbox.center,
                    });
                    names.push(format!("hitbox/{model}/{set}/{}", hitbox.index));
                }
                result.sets.insert((model, set), points);
            }
        }
        Ok(result)
    }
    /// Caller initializes the fixed output slots to None for every frame.
    /// Returns counts of measured and unavailable bone transforms.
    pub fn sample(
        &self,
        model: u64,
        set: u32,
        bones: &[Option<Transform>],
        root: Transform,
        output: &mut [Option<[f64; 3]>],
    ) -> Result<(u64, u64)> {
        let points = self
            .sets
            .get(&(model, set))
            .context("unknown model hitbox set")?;
        let mut measured = 0;
        let mut missing = 0;
        for point in points {
            let Some(bone) = point.bone.and_then(|i| bones.get(i)).copied().flatten() else {
                missing += 1;
                continue;
            };
            let local = Transform {
                position: point.center,
                rotation: [0.0, 0.0, 0.0, 1.0],
                scale: 1.0,
            };
            let world = Transform::compose(root, Transform::compose(bone, local));
            ensure!(
                world.position.iter().all(|v| v.is_finite()),
                "nonfinite hitbox center"
            );
            *output
                .get_mut(point.output)
                .context("missing hitbox output slot")? = Some(world.position.map(f64::from));
            measured += 1;
        }
        Ok((measured, missing))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixed_centers_use_bone_rotation_and_scale_without_model_or_set_leak() {
        let geometry = vec![HitboxSet {
            name: "cstrike".into(),
            hitboxes: vec![
                Hitbox {
                    index: 0,
                    bone: "head_0".into(),
                    center: [1.25, 1.0, 0.0],
                },
                Hitbox {
                    index: 1,
                    bone: "unavailable".into(),
                    center: [0.0; 3],
                },
            ],
        }];
        let mut names = vec!["head_0".into()];
        let compiled = Points::new(
            &BTreeMap::from([(10, geometry.clone()), (20, geometry)]),
            &names.clone(),
            &mut names,
        )
        .unwrap();
        let q = std::f32::consts::FRAC_1_SQRT_2;
        let bones = [Some(Transform {
            position: [10.0, 0.0, 0.0],
            rotation: [0.0, 0.0, q, q],
            scale: 2.0,
        })];
        let root = Transform {
            position: [100.0, 20.0, 30.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: 3.0,
        };
        let mut out = vec![None; names.len()];
        assert_eq!(
            compiled.sample(10, 0, &bones, root, &mut out).unwrap(),
            (1, 1)
        );
        for (actual, expected) in out[1].unwrap().into_iter().zip([124.0, 27.5, 30.0]) {
            assert!((actual - expected).abs() < 0.0001);
        }
        assert!(out[2..].iter().all(Option::is_none));
        out.fill(None);
        assert!(compiled.sample(10, 1, &bones, root, &mut out).is_err());
        assert!(compiled.sample(30, 0, &bones, root, &mut out).is_err());
        assert!(out.iter().all(Option::is_none));
        assert!(names.contains(&"hitbox/20/0/0".into()));
    }
    #[test]
    fn real_capsule_endpoints_keep_axis_and_mesh_definitions_must_agree() {
        let mesh = serde_json::json!({"m_hitboxsets":[{"key":"cstrike", "value":{"m_name":"cstrike", "m_HitBoxes":[{
            "m_nShapeType":2,"m_bTranslationOnly":false,"m_flShapeRadius":6.0,"m_nHitBoxIndex":3,"m_sBoneName":"spine_0",
            "m_vMinBounds":[1.4,0.8,3.1],"m_vMaxBounds":[1.4,0.8,-3.1]
        }]}}]});
        let parsed = parse_meshes(&[mesh.clone(), mesh.clone()]).unwrap();
        assert_eq!(parsed[0].hitboxes[0].center, [1.4, 0.8, 0.0]);
        let mut different = mesh.clone();
        different["m_hitboxsets"][0]["value"]["m_HitBoxes"][0]["m_vMinBounds"][0] =
            serde_json::json!(9.0);
        assert!(parse_meshes(&[mesh, different]).is_err());
        assert!(parse_meshes(&[]).unwrap().is_empty());
    }
}
