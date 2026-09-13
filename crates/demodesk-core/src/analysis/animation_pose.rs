//! Primary-skeleton recipe evaluation with explicit unknown bones for unimplemented tasks.
use super::animation_clip::{Clip, Transform};
use super::animation_recipe::{IkTarget, MaskTask, Parameters, Recipe};
use anyhow::{bail, ensure, Context, Result};
use serde::Deserialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

/// The custom-task write sets were traced in this exact installed client, not inferred from task names.
pub const CS2_WRITE_SET_CLIENT_SHA256: &str =
    "a0c195f0b6ec00915ef08c548200a010ebbe7982d3a4bc468cad939b67c8c4e3";

#[derive(Debug)]
pub struct Skeleton {
    pub resource_name: String,
    pub names: Vec<String>,
    pub parents: Vec<Option<usize>>,
    pub reference: Vec<Transform>,
    pub masks: BTreeMap<String, Vec<f32>>,
    cs2_write_set_validated: bool,
}

#[derive(Clone, Debug)]
pub struct Pose {
    pub local: Vec<Option<Transform>>,
    pub model: Vec<Option<Transform>>,
    pub is_additive: bool,
}

/// Caches are deliberately scoped to one recorded recipe. External cache clear/destroy
/// events are not decoded from the trailer, so an ID alone cannot authorize reuse
/// from a previous packet, player, or animation context.
#[derive(Default)]
struct State {
    cached: BTreeMap<u8, Pose>,
}

#[derive(Deserialize)]
struct Source {
    #[serde(rename = "m_ID")]
    name: String,
    #[serde(rename = "m_boneIDs")]
    names: Vec<String>,
    #[serde(rename = "m_parentIndices")]
    parents: Vec<i32>,
    #[serde(rename = "m_parentSpaceReferencePose")]
    reference: Vec<[f32; 8]>,
    #[serde(rename = "m_maskDefinitions", default)]
    masks: Vec<Mask>,
}

#[derive(Deserialize)]
struct Mask {
    #[serde(rename = "m_ID")]
    name: String,
    #[serde(rename = "m_primaryWeightList")]
    primary: Weights,
}

#[derive(Deserialize)]
struct Weights {
    #[serde(rename = "m_skeletonName")]
    skeleton: String,
    #[serde(rename = "m_boneIDs")]
    names: Vec<String>,
    #[serde(rename = "m_weights")]
    values: Vec<f32>,
}

impl Skeleton {
    pub fn from_value(value: Value) -> Result<Self> {
        let source: Source = serde_json::from_value(value).context("Invalid animation skeleton")?;
        let count = source.names.len();
        ensure!((1..=1024).contains(&count), "Invalid skeleton bone count");
        ensure!(
            source.parents.len() == count && source.reference.len() == count,
            "Incomplete skeleton"
        );
        let mut names = BTreeSet::new();
        let mut parents = Vec::with_capacity(count);
        for (index, (name, &parent)) in source.names.iter().zip(&source.parents).enumerate() {
            ensure!(
                !name.is_empty() && names.insert(name),
                "Duplicate or empty bone name"
            );
            ensure!(
                if index == 0 {
                    parent == -1
                } else {
                    parent >= 0 && (parent as usize) < index
                },
                "Invalid skeleton parent order"
            );
            parents.push(usize::try_from(parent).ok());
        }
        let reference = source
            .reference
            .iter()
            .map(|row| {
                ensure!(
                    row.iter().all(|v| v.is_finite()),
                    "Nonfinite skeleton reference transform"
                );
                let norm: f32 = row[4..8].iter().map(|v| v * v).sum();
                ensure!(
                    (norm - 1.).abs() < 0.002 && row[3] > 0.,
                    "Invalid skeleton reference transform"
                );
                Ok(Transform {
                    position: [row[0], row[1], row[2]],
                    scale: row[3],
                    rotation: [row[4], row[5], row[6], row[7]],
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let mut masks = BTreeMap::new();
        for mask in source.masks {
            ensure!(
                mask.primary.skeleton == source.name,
                "Mask belongs to another skeleton"
            );
            ensure!(
                mask.primary.names.len() == mask.primary.values.len(),
                "Incomplete bone mask"
            );
            let mut weights = vec![-1.; count];
            for (name, weight) in mask.primary.names.iter().zip(mask.primary.values) {
                let index = source
                    .names
                    .iter()
                    .position(|n| n == name)
                    .context("Mask references missing bone")?;
                ensure!(
                    weight.is_finite() && (0. ..=1.).contains(&weight) && weights[index] == -1.,
                    "Invalid or duplicate mask weight"
                );
                weights[index] = weight;
            }
            feather_weights(&parents, &mut weights);
            ensure!(
                masks.insert(mask.name, weights).is_none(),
                "Duplicate bone mask"
            );
        }
        Ok(Self {
            resource_name: source.name,
            names: source.names,
            parents,
            reference,
            masks,
            cs2_write_set_validated: false,
        })
    }

    /// Call only after hashing the actual source-compatible client; a matching skeleton name alone is insufficient.
    pub fn validate_cs2_write_set(&mut self, client_sha256: &str) -> Result<()> {
        ensure!(
            client_sha256 == CS2_WRITE_SET_CLIENT_SHA256,
            "Unsupported CS2 custom-task implementation"
        );
        ensure!(
            self.resource_name == "animation/skeletons/characters/worldmodel.vnmskel",
            "Unsupported custom-task skeleton"
        );
        for (bone, parent) in [
            ("pelvis", "root_motion"),
            ("spine_0", "pelvis"),
            ("spine_1", "spine_0"),
            ("spine_2", "spine_1"),
            ("spine_3", "spine_2"),
            ("neck_0", "spine_3"),
            ("head_0", "neck_0"),
            ("clavicle_L", "spine_3"),
            ("arm_upper_L", "clavicle_L"),
            ("arm_lower_L", "arm_upper_L"),
            ("hand_L", "arm_lower_L"),
            ("clavicle_R", "spine_3"),
            ("arm_upper_R", "clavicle_R"),
            ("arm_lower_R", "arm_upper_R"),
            ("hand_R", "arm_lower_R"),
            ("leg_upper_L", "pelvis"),
            ("leg_lower_L", "leg_upper_L"),
            ("ankle_L", "leg_lower_L"),
            ("leg_upper_R", "pelvis"),
            ("leg_lower_R", "leg_upper_R"),
            ("ankle_R", "leg_lower_R"),
        ] {
            let index = self.index(bone)?;
            let parent_index = self.parents[index].context("Missing custom-task parent")?;
            ensure!(
                self.names[parent_index] == parent,
                "Custom-task hierarchy mismatch for {bone}"
            );
        }
        for bone in ["wpnPivot", "wpn", "wpnHand_L", "wpnHand_R"] {
            self.index(bone)?;
        }
        self.cs2_write_set_validated = true;
        Ok(())
    }

    pub fn evaluate(
        &self,
        recipe: &Recipe,
        clips: &BTreeMap<String, Clip>,
        resource_paths: &[String],
        mask_names: &[String],
    ) -> Result<Pose> {
        ensure!(
            !recipe.tasks.is_empty() && recipe.tasks.len() == recipe.parameters.len(),
            "Incomplete pose recipe"
        );
        // Local-space tasks do not consume FK. Compute model transforms only for
        // dependencies of model-space tasks and the final shared pose.
        let mut needs_model = vec![false; recipe.tasks.len()];
        *needs_model.last_mut().expect("nonempty recipe") = true;
        for task in &recipe.tasks {
            if matches!(
                task.kind.as_str(),
                "CNmAimCSTask" | "CNmModelSpaceBlendTask"
            ) {
                for &dependency in &task.dependencies {
                    *needs_model
                        .get_mut(dependency)
                        .context("Invalid model-space dependency")? = true;
                }
            }
        }
        let mut state = State::default();
        let mut results: Vec<Pose> = Vec::with_capacity(recipe.tasks.len());
        for (index, (task, parameters)) in recipe.tasks.iter().zip(&recipe.parameters).enumerate() {
            ensure!(
                task.dependencies.iter().all(|&d| d < index),
                "Invalid pose dependency"
            );
            let dependency = |n: usize| -> Result<&Pose> {
                let id = *task
                    .dependencies
                    .get(n)
                    .context("Missing pose dependency")?;
                results.get(id).context("Missing dependency pose")
            };
            let (mut local, is_additive) = match (task.kind.as_str(), parameters) {
                (
                    "CNmSampleTask",
                    Parameters::Sample {
                        resource_index,
                        normalized_time,
                    },
                ) => {
                    ensure!(task.dependencies.is_empty(), "Unexpected sample dependency");
                    let path = resource_paths
                        .get(*resource_index as usize)
                        .context("Missing animation resource path")?;
                    let clip = clips
                        .get(path)
                        .with_context(|| format!("Animation resource not loaded: {path}"))?;
                    ensure!(
                        clip.skeleton == self.resource_name,
                        "Clip targets another skeleton"
                    );
                    let samples = clip.sample(f32::from(*normalized_time) / 65535.)?;
                    ensure!(
                        samples.len() == self.names.len(),
                        "Clip/skeleton track count mismatch"
                    );
                    (samples.into_iter().map(Some).collect(), clip.is_additive)
                }
                ("CNmCachedPoseReadTask", Parameters::CachedPoseRead { cache_id }) => {
                    ensure!(
                        task.dependencies.is_empty() && *cache_id < 64,
                        "Invalid cached-pose read"
                    );
                    // Missing cached-pose type is unknown too: do not invent a reference or additive pose.
                    let cached = state
                        .cached
                        .get(cache_id)
                        .context("Cached pose unavailable within recorded recipe")?;
                    (cached.local.clone(), cached.is_additive)
                }
                ("CNmCachedPoseWriteTask", Parameters::CachedPoseWrite { cache_id }) => {
                    ensure!(
                        task.dependencies.len() == 1 && *cache_id < 64,
                        "Invalid cached-pose write"
                    );
                    let pose = dependency(0)?.clone();
                    state.cached.insert(*cache_id, pose.clone());
                    (pose.local, pose.is_additive)
                }
                ("CNmScaleTask", Parameters::Scale { masks }) => {
                    ensure!(task.dependencies.len() == 1, "Invalid scale dependency");
                    ensure!(
                        masks.is_empty() || matches!(masks.as_slice(), [MaskTask::Mask(_) | MaskTask::Generate(_)]),
                        "Scale requires verified mask classification; compound mask metadata unsupported"
                    );
                    let pose = dependency(0)?;
                    let mut local = pose.local.clone();
                    let weights = self.mask(masks, mask_names)?;
                    // Native ScaleTask 13276c2 special-cases an all-zero mask by
                    // zeroing only the root scale; ordinary masks scale each local bone.
                    if weights.iter().all(|&weight| weight == 0.) {
                        if let Some(root) = &mut local[0] {
                            root.scale = 0.;
                        }
                    } else {
                        for (transform, weight) in local.iter_mut().zip(weights) {
                            if let Some(transform) = transform {
                                transform.scale *= weight;
                            }
                        }
                    }
                    (local, pose.is_additive)
                }
                ("CNmReferencePoseTask", Parameters::ReferencePose) => {
                    ensure!(
                        task.dependencies.is_empty(),
                        "Unexpected reference-pose dependency"
                    );
                    (self.reference.iter().copied().map(Some).collect(), false)
                }
                ("CNmZeroPoseTask", Parameters::ZeroPose) => {
                    ensure!(
                        task.dependencies.is_empty(),
                        "Unexpected zero-pose dependency"
                    );
                    (
                        vec![
                            Some(Transform {
                                scale: 0.,
                                ..Transform::IDENTITY
                            });
                            self.names.len()
                        ],
                        true,
                    )
                }
                (
                    kind @ ("CNmBlendTask" | "CNmOverlayBlendTask" | "CNmAdditiveBlendTask"),
                    Parameters::Blend {
                        normalized_weight,
                        masks,
                    },
                ) => {
                    ensure!(task.dependencies.len() == 2, "Invalid blend dependencies");
                    let source = dependency(0)?;
                    let target = dependency(1)?;
                    let additive = kind == "CNmAdditiveBlendTask";
                    ensure!(
                        if additive {
                            target.is_additive
                        } else {
                            source.is_additive == target.is_additive
                        },
                        "Mismatched animation pose types"
                    );
                    let weight = f32::from(*normalized_weight) / 255.;
                    let mask = self.mask(masks, mask_names)?;
                    let local = source
                        .local
                        .iter()
                        .zip(&target.local)
                        .zip(mask)
                        .map(|((&a, &b), m)| {
                            // An ordinary full-weight blend copies target before applying its mask; overlay remains masked.
                            let w = if kind == "CNmBlendTask" && weight == 1. {
                                1.
                            } else {
                                weight * m
                            };
                            blend(a, b, w, additive)
                        })
                        .collect::<Result<Vec<_>>>()?;
                    (local, source.is_additive && target.is_additive)
                }
                (
                    "CNmModelSpaceBlendTask",
                    Parameters::ModelSpaceBlend {
                        normalized_weight,
                        masks,
                    },
                ) => {
                    ensure!(
                        task.dependencies.len() == 2 && !masks.is_empty(),
                        "Invalid model-space blend"
                    );
                    let source = dependency(0)?;
                    let target = dependency(1)?;
                    ensure!(
                        !source.is_additive && !target.is_additive,
                        "Model-space blend requires parent-space poses"
                    );
                    let mask = self.mask(masks, mask_names)?;
                    (
                        self.model_space_blend(
                            source,
                            target,
                            &mask,
                            f32::from(*normalized_weight) / 255.,
                        )?,
                        false,
                    )
                }
                (
                    "CNmFootIKTask",
                    Parameters::FootIk {
                        effectors,
                        targets,
                        effector_blend,
                        normalized_weight,
                    },
                ) => {
                    ensure!(
                        task.dependencies.len() == 1 && self.cs2_write_set_validated,
                        "Unverified FootIK implementation"
                    );
                    let source = dependency(0)?;
                    ensure!(!source.is_additive, "FootIK requires parent-space pose");
                    let mut local = source.local.clone();
                    // Both targets are resolved before either chain is modified, as in the recorded task.
                    let mut resolved = Vec::with_capacity(2);
                    for target in targets {
                        resolved.push(match target {
                            IkTarget::Bone(name) => {
                                self.model_bone(&source.local, self.index(name)?)?
                            }
                            IkTarget::Transform {
                                rotation,
                                translation,
                            } => Some(Transform {
                                rotation: super::animation_clip::decode_rotation(*rotation)?,
                                position: translation.map(|v| f32::from(v) / 65535. * 400. - 200.),
                                scale: 1.,
                            }),
                        });
                    }
                    for (effector, target) in effectors.iter().zip(resolved) {
                        self.solve_foot(
                            &mut local,
                            self.index(effector)?,
                            target,
                            *effector_blend,
                            f32::from(*normalized_weight) / 255.,
                        )?;
                    }
                    (local, false)
                }
                (
                    "CNmAimCSTask",
                    Parameters::AimCs {
                        normalized16,
                        normalized8,
                        flags3,
                        flags5,
                    },
                ) => {
                    ensure!(
                        task.dependencies.len() == 1 && self.cs2_write_set_validated,
                        "Unverified AimCS write set"
                    );
                    let mut pose = dependency(0)?.clone();
                    ensure!(!pose.is_additive, "AimCS requires parent-space pose");
                    let head_known = super::animation_aim::apply(
                        self,
                        &mut pose,
                        *normalized16,
                        *normalized8,
                        *flags3,
                        *flags5,
                    )?;
                    let mut local = pose.local;
                    if !head_known {
                        self.invalidate_descendants(&mut local, &["head_0"])?;
                    }
                    (local, false)
                }
                ("CNmSnapWeaponTask", Parameters::SnapWeapon { flags2 }) => {
                    ensure!(
                        task.dependencies.len() == 1 && self.cs2_write_set_validated,
                        "Unverified SnapWeapon write set"
                    );
                    let mut pose = dependency(0)?.clone();
                    super::animation_aim::apply_snap(self, &mut pose, *flags2)?;
                    (pose.local, false)
                }
                _ => bail!("Unsupported pose task {}", task.kind),
            };
            // Even locally known descendants cannot be located when an ancestor was not evaluated.
            for bone in 0..local.len() {
                if self.parents[bone].is_some_and(|p| local[p].is_none()) {
                    local[bone] = None;
                }
            }
            let model = if !needs_model[index] {
                vec![]
            } else if is_additive {
                // Deltas are not physical model-space transforms before composition onto a base pose.
                vec![None; local.len()]
            } else {
                self.model(&local)?
            };
            results.push(Pose {
                local,
                model,
                is_additive,
            });
        }
        results.pop().context("Empty pose recipe")
    }

    // Esoterica IK/TwoBoneIK.cpp, confirmed by client RVAs 0x1278c70 and 0x12780e0.
    // FootIK supplies two independent model-space targets and chainRotationWeight = 0.
    pub(crate) fn solve_foot(
        &self,
        local: &mut [Option<Transform>],
        end: usize,
        target: Option<Transform>,
        effector_blend: bool,
        weight: f32,
    ) -> Result<()> {
        if weight == 0. {
            return Ok(());
        }
        ensure!(
            local.len() == self.names.len() && end < local.len(),
            "Invalid IK pose size"
        );
        let middle = self.parents[end].context("Missing IK middle bone")?;
        let start = self.parents[middle].context("Missing IK start bone")?;
        let indices = [start, middle, end];
        let parent = match self.parents[start] {
            Some(index) => self.model_bone(local, index)?,
            None => Some(Transform::IDENTITY),
        };
        let a = if self.parents[start].is_none() {
            local[start]
        } else {
            parent
                .zip(local[start])
                .map(|(p, t)| Transform::compose(p, t))
        };
        let b = a.zip(local[middle]).map(|(p, t)| Transform::compose(p, t));
        let c = b.zip(local[end]).map(|(p, t)| Transform::compose(p, t));
        for transform in [a, b, c].into_iter().flatten() {
            ensure!(
                transform
                    .position
                    .iter()
                    .chain(transform.rotation.iter())
                    .all(|v| v.is_finite())
                    && transform.scale.is_finite(),
                "Nonfinite model-space pose"
            );
        }
        let (Some(mut a), Some(mut b), Some(mut c), Some(parent), Some(mut target)) =
            (a, b, c, parent, target)
        else {
            for i in indices {
                local[i] = None;
            }
            return Ok(());
        };
        if effector_blend && weight != 1. {
            // Current client RVA 0x1279116 uses the same fast quaternion blend as parent-space poses.
            target =
                blend(Some(c), Some(target), weight, false)?.context("Missing IK target blend")?;
        }
        let v1 = sub(b.position, a.position);
        let v2 = sub(c.position, b.position);
        let desired = sub(target.position, a.position);
        let length1 = length(v1);
        let length2 = length(v2);
        if length1 <= 0.001 || length2 <= 0.001 {
            // The current client solver returns without writing a degenerate chain.
            return Ok(());
        }
        if length(desired) <= 1e-6 {
            // No stable bend plane or alignment direction can be recovered for this degenerate chain.
            for i in indices {
                local[i] = None;
            }
            return Ok(());
        }
        let v1n = v1.map(|v| v / length1);
        let v2n = v2.map(|v| v / length2);
        let cosine = (-dot(v1n, v2n)).clamp(-1., 1.);
        let wanted_cosine = ((length1 * length1 + length2 * length2 - dot(desired, desired))
            / (2. * length1 * length2))
            .clamp(-1., 1.);
        let delta = wanted_cosine.acos() - cosine.acos();
        let hinge = if (cosine + 1.).abs() <= 1e-5 {
            let references = indices.map(|i| self.reference_model(i));
            let cross = cross(
                sub(references[2].position, references[1].position),
                sub(references[1].position, references[0].position),
            );
            let local_axis = if dot(cross, cross) > 1e-6 {
                unit(rotate_vector(references[1].inverse()?.rotation, cross))?
            } else {
                [1., 0., 0.]
            };
            rotate_vector(b.rotation, local_axis)
        } else {
            unit(cross(v2n, v1n))?
        };
        let end_local = Transform::compose(b.inverse()?, c);
        b.rotation = rotate_rotation(axis_angle(hinge, delta)?, b.rotation);
        c = Transform::compose(b, end_local);
        let middle_local = Transform::compose(a.inverse()?, b);
        let from = unit(sub(c.position, a.position))?;
        let to = unit(desired)?;
        // client RVA 0x1630600: shortest-arc half-vector construction. The
        // antiparallel fallback differs from the older Esoterica implementation.
        let halfway = std::array::from_fn(|i| (from[i] + to[i]) * 0.5);
        let turn = if dot(halfway, halfway) > f32::EPSILON {
            let xyz = cross(from, halfway);
            normalize_quaternion([xyz[0], xyz[1], xyz[2], dot(from, halfway)])?
        } else if from[0].abs() > 0.5 {
            normalize_quaternion([from[1], -from[0], 0., 0.])?
        } else {
            normalize_quaternion([0., from[2], -from[1], 0.])?
        };
        a.rotation = rotate_rotation(turn, a.rotation);
        b = Transform::compose(a, middle_local);
        c = Transform::compose(b, end_local);
        c.rotation = target.rotation;
        let solved = [
            Transform::compose(parent.inverse()?, a),
            Transform::compose(a.inverse()?, b),
            Transform::compose(b.inverse()?, c),
        ];
        for (index, transform) in indices.into_iter().zip(solved) {
            local[index] = if !effector_blend && weight != 1. {
                blend(local[index], Some(transform), weight, false)?
            } else {
                Some(transform)
            };
        }
        Ok(())
    }

    fn reference_model(&self, mut index: usize) -> Transform {
        let mut transform = self.reference[index];
        while let Some(parent) = self.parents[index] {
            transform = Transform::compose(self.reference[parent], transform);
            index = parent;
        }
        transform
    }

    // Esoterica AnimationBlender.cpp, confirmed in the pinned client's RVA 0x1307fe0.
    // Mask-space rotation blending and the final task-weight blend are separate operations.
    fn model_space_blend(
        &self,
        source: &Pose,
        target: &Pose,
        mask: &[f32],
        weight: f32,
    ) -> Result<Vec<Option<Transform>>> {
        ensure!(
            source.local.len() == self.names.len()
                && target.local.len() == self.names.len()
                && mask.len() == self.names.len(),
            "Model-space blend size mismatch"
        );
        if weight == 0. || mask.iter().all(|w| *w == 0.) {
            return Ok(source.local.clone());
        }
        let mut intermediate = target.local.clone();
        let mut rotations: Vec<Option<[f32; 4]>> = vec![None; self.names.len()];
        rotations[0] = if mask[0] == 0. {
            source.local[0].map(|v| v.rotation)
        } else {
            match (source.local[0], target.local[0]) {
                (Some(a), Some(b)) => {
                    // The installed routine writes root translation/scale here, but retains
                    // the target's local root rotation until the final parent-space blend.
                    let mut root = b;
                    root.position = std::array::from_fn(|i| {
                        a.position[i] + (b.position[i] - a.position[i]) * mask[0]
                    });
                    root.scale = a.scale + (b.scale - a.scale) * mask[0];
                    intermediate[0] = Some(root);
                    Some(fast_slerp(a.rotation, b.rotation, mask[0])?)
                }
                _ => {
                    intermediate[0] = None;
                    None
                }
            }
        };
        for i in 1..self.names.len() {
            if mask[i] == 0. {
                intermediate[i] = source.local[i];
                rotations[i] = source.model[i].map(|v| v.rotation);
                continue;
            }
            let parent = self.parents[i].context("Missing model-space parent")?;
            let (Some(a), Some(b), Some(am), Some(bm), Some(parent_rotation)) = (
                source.local[i],
                target.local[i],
                source.model[i],
                target.model[i],
                rotations[parent],
            ) else {
                intermediate[i] = None;
                continue;
            };
            let global = fast_slerp(am.rotation, bm.rotation, mask[i])?;
            rotations[i] = Some(global);
            let inverse = [
                -parent_rotation[0],
                -parent_rotation[1],
                -parent_rotation[2],
                parent_rotation[3],
            ];
            let rotation = Transform::compose(
                Transform {
                    rotation: inverse,
                    ..Transform::IDENTITY
                },
                Transform {
                    rotation: global,
                    ..Transform::IDENTITY
                },
            )
            .rotation;
            let norm = rotation.iter().map(|v| v * v).sum::<f32>().sqrt();
            ensure!(
                norm.is_finite() && norm > 0.,
                "Invalid model-space rotation delta"
            );
            intermediate[i] = Some(Transform {
                position: std::array::from_fn(|axis| {
                    a.position[axis] + (b.position[axis] - a.position[axis]) * mask[i]
                }),
                scale: a.scale + (b.scale - a.scale) * mask[i],
                rotation: rotation.map(|v| v / norm),
            });
        }
        source
            .local
            .iter()
            .zip(intermediate)
            .map(|(&a, b)| blend(a, b, weight, false))
            .collect()
    }

    /// Computes only this bone's ancestor chain, with the same parent-first
    /// composition and unknown propagation as full FK.
    pub(crate) fn model_bone(
        &self,
        local: &[Option<Transform>],
        bone: usize,
    ) -> Result<Option<Transform>> {
        ensure!(
            local.len() == self.names.len() && bone < local.len(),
            "Pose/skeleton size mismatch"
        );
        let Some(transform) = local[bone] else {
            return Ok(None);
        };
        let model = match self.parents[bone] {
            None => Some(transform),
            Some(parent) => {
                ensure!(parent < bone, "Invalid animation parent order");
                self.model_bone(local, parent)?
                    .map(|p| Transform::compose(p, transform))
            }
        };
        if let Some(transform) = model {
            ensure!(
                transform
                    .position
                    .iter()
                    .chain(transform.rotation.iter())
                    .all(|v| v.is_finite())
                    && transform.scale.is_finite(),
                "Nonfinite model-space pose"
            );
        }
        Ok(model)
    }

    pub fn model(&self, local: &[Option<Transform>]) -> Result<Vec<Option<Transform>>> {
        ensure!(
            local.len() == self.names.len(),
            "Pose/skeleton size mismatch"
        );
        let mut result: Vec<Option<Transform>> = Vec::with_capacity(local.len());
        for (index, &transform) in local.iter().enumerate() {
            let model = match (transform, self.parents[index]) {
                (Some(t), None) => Some(t),
                (Some(t), Some(parent)) => result[parent].map(|p| Transform::compose(p, t)),
                _ => None,
            };
            if let Some(t) = model {
                ensure!(
                    t.position
                        .iter()
                        .chain(t.rotation.iter())
                        .all(|v| v.is_finite())
                        && t.scale.is_finite(),
                    "Nonfinite model-space pose"
                );
            }
            result.push(model);
        }
        Ok(result)
    }

    fn index(&self, name: &str) -> Result<usize> {
        self.names
            .iter()
            .position(|n| n == name)
            .with_context(|| format!("Missing required bone {name}"))
    }

    fn invalidate_descendants(
        &self,
        local: &mut [Option<Transform>],
        names: &[&str],
    ) -> Result<()> {
        for name in names {
            local[self.index(name)?] = None;
        }
        for bone in 0..local.len() {
            if self.parents[bone].is_some_and(|p| local[p].is_none()) {
                local[bone] = None;
            }
        }
        Ok(())
    }

    fn mask(&self, tasks: &[MaskTask], names: &[String]) -> Result<Vec<f32>> {
        if tasks.is_empty() {
            return Ok(vec![1.; self.names.len()]);
        }
        ensure!(tasks.len() <= 31, "Too many bone mask tasks");
        let mut results: Vec<Vec<f32>> = Vec::with_capacity(tasks.len());
        for task in tasks {
            let dependency = |id: usize| results.get(id).context("Invalid mask dependency");
            let mask = match *task {
                MaskTask::Mask(id) => self
                    .masks
                    .get(
                        names
                            .get(id as usize)
                            .context("Missing recorded mask name")?,
                    )
                    .context("Missing skeleton mask")?
                    .clone(),
                MaskTask::Generate(w) => vec![f32::from(w) / 255.; self.names.len()],
                MaskTask::Scale { source, weight } => dependency(source)?
                    .iter()
                    .map(|v| v * (f32::from(weight) / 255.))
                    .collect(),
                MaskTask::Blend {
                    source,
                    target,
                    weight,
                } => {
                    let w = f32::from(weight) / 255.;
                    dependency(source)?
                        .iter()
                        .zip(dependency(target)?)
                        .map(|(a, b)| a + (b - a) * w)
                        .collect()
                }
                MaskTask::Combine { source, target } => dependency(source)?
                    .iter()
                    .zip(dependency(target)?)
                    .map(|(a, b)| a * b)
                    .collect(),
            };
            results.push(mask);
        }
        results.pop().context("Empty mask")
    }
}

// BoneMask::ResetWeights feathers between explicit ancestors and descendants, processing leaves first.
fn feather_weights(parents: &[Option<usize>], weights: &mut [f32]) {
    let original = weights.to_vec();
    for bone in (1..weights.len()).rev() {
        let mut chain = vec![bone];
        if weights[bone] == -1. {
            let mut parent = parents[bone];
            let mut weight = 0.;
            while let Some(p) = parent {
                if original[p] != -1. {
                    weight = original[p];
                    break;
                }
                chain.push(p);
                parent = parents[p];
            }
            if parent.is_none() {
                chain.pop();
            }
            for index in chain {
                weights[index] = weight;
            }
        } else if parents[bone].is_some_and(|p| weights[p] == -1.) {
            let end = weights[bone];
            let mut start = -1.;
            let mut parent = parents[bone];
            while let Some(p) = parent {
                chain.push(p);
                if original[p] != -1. {
                    start = original[p];
                    break;
                }
                parent = parents[p];
            }
            for i in (1..chain.len() - 1).rev() {
                weights[chain[i]] = if start == -1. {
                    0.
                } else {
                    end + (start - end) * (i as f32 / (chain.len() - 1) as f32)
                };
            }
        }
    }
    if weights[0] == -1. {
        weights[0] = 0.;
    }
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|i| a[i] - b[i])
}
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a.iter().zip(b).map(|(a, b)| a * b).sum()
}
fn length(v: [f32; 3]) -> f32 {
    dot(v, v).sqrt()
}
fn unit(v: [f32; 3]) -> Result<[f32; 3]> {
    let n = length(v);
    ensure!(n.is_finite() && n > 0., "Invalid IK direction");
    Ok(v.map(|x| x / n))
}
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn rotate_vector(rotation: [f32; 4], position: [f32; 3]) -> [f32; 3] {
    Transform::compose(
        Transform {
            rotation,
            ..Transform::IDENTITY
        },
        Transform {
            position,
            ..Transform::IDENTITY
        },
    )
    .position
}
fn rotate_rotation(parent: [f32; 4], local: [f32; 4]) -> [f32; 4] {
    Transform::compose(
        Transform {
            rotation: parent,
            ..Transform::IDENTITY
        },
        Transform {
            rotation: local,
            ..Transform::IDENTITY
        },
    )
    .rotation
}
fn normalize_quaternion(q: [f32; 4]) -> Result<[f32; 4]> {
    let n = q.iter().map(|v| v * v).sum::<f32>().sqrt();
    ensure!(n.is_finite() && n > 0., "Invalid IK quaternion");
    Ok(q.map(|v| v / n))
}
fn axis_angle(axis: [f32; 3], angle: f32) -> Result<[f32; 4]> {
    let axis = unit(axis)?;
    let (s, c) = (angle * 0.5).sin_cos();
    Ok([axis[0] * s, axis[1] * s, axis[2] * s, c])
}

fn blend(
    a: Option<Transform>,
    b: Option<Transform>,
    w: f32,
    additive: bool,
) -> Result<Option<Transform>> {
    if w == 0. {
        return Ok(a);
    }
    if !additive && w == 1. {
        return Ok(b);
    }
    let (Some(a), Some(b)) = (a, b) else {
        return Ok(None);
    };
    let result = if additive {
        // Esoterica's child*parent convention reverses Hamilton multiplication: delta*base => base(delta).
        let target = Transform::compose(
            Transform {
                position: [0.; 3],
                scale: 1.,
                rotation: a.rotation,
            },
            Transform {
                position: [0.; 3],
                scale: 1.,
                rotation: b.rotation,
            },
        );
        let rotation = Transform::interpolate(
            Transform {
                position: [0.; 3],
                scale: 1.,
                rotation: a.rotation,
            },
            target,
            w,
        )?
        .rotation;
        Transform {
            position: std::array::from_fn(|i| b.position[i].mul_add(w, a.position[i])),
            scale: b.scale.mul_add(w, a.scale),
            rotation,
        }
    } else {
        let mut result = Transform::interpolate(a, b, w)?;
        result.rotation = fast_slerp(a.rotation, b.rotation, w)?;
        result
    };
    Ok(Some(result))
}

// Same approximation used by Esoterica's parent-space Blender, distinct from clip sampling SLerp.
pub(super) fn fast_slerp(a: [f32; 4], b: [f32; 4], t: f32) -> Result<[f32; 4]> {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let d = dot.abs();
    let a_factor = 1.0904 + d * (-3.2452 + d * (3.55645 - d * 1.43519));
    let b_factor = 0.848013 + d * (-1.06021 + d * 0.215638);
    let k = a_factor * (t - 0.5).powi(2) + b_factor;
    let adjusted = t + t * (t - 0.5) * (t - 1.) * k;
    let signed = if dot > 0. { adjusted } else { -adjusted };
    let mut q = std::array::from_fn(|i| b[i].mul_add(signed, a[i] * (1. - adjusted)));
    let length = q.iter().map(|v| v * v).sum::<f32>().sqrt();
    ensure!(
        length.is_finite() && length > 0.,
        "Invalid blended quaternion"
    );
    for v in &mut q {
        *v /= length;
    }
    Ok(q)
}

#[cfg(test)]
mod tests {
    #[test]
    fn selected_bone_fk_matches_full_fk_across_branches_scales_and_unknowns() {
        let skeleton = Skeleton::from_value(serde_json::json!({
            "m_ID":"test","m_boneIDs":["root","a","b","c","d","e","f","g"],
            "m_parentIndices":[-1,0,1,2,0,4,5,1],
            "m_parentSpaceReferencePose":[
                [1.,2.,3.,1.2,0.,0.,0.,1.],[3.,1.,2.,0.8,0.,0.,0.,1.],
                [2.,3.,1.,1.1,0.,0.,0.,1.],[1.,1.,2.,0.9,0.,0.,0.,1.],
                [2.,1.,3.,1.3,0.,0.,0.,1.],[3.,2.,1.,0.7,0.,0.,0.,1.],
                [1.,3.,2.,1.4,0.,0.,0.,1.],[1.,2.,1.,0.6,0.,0.,0.,1.]
            ]
        }))
        .unwrap();
        let mut initial: Vec<_> = skeleton.reference.iter().copied().map(Some).collect();
        for (i, transform) in initial.iter_mut().enumerate() {
            transform.as_mut().unwrap().rotation =
                axis_angle([0., 0., 1.], i as f32 * 0.23).unwrap();
        }
        for absent in [None, Some(0), Some(1), Some(2), Some(4), Some(7)] {
            let mut local = initial.clone();
            if let Some(index) = absent {
                local[index] = None;
            }
            let full = skeleton.model(&local).unwrap();
            for (index, expected) in full.iter().copied().enumerate() {
                assert_eq!(skeleton.model_bone(&local, index).unwrap(), expected);
            }
            // Cross-chain targets must both use the source pose, even when the
            // first solve changes the bone used as the second target.
            let selected_targets = [
                skeleton.model_bone(&local, 6).unwrap(),
                skeleton.model_bone(&local, 3).unwrap(),
            ];
            for effector_blend in [false, true] {
                for weight in [0.5, 1.] {
                    let mut selected = local.clone();
                    let mut complete = local.clone();
                    for (end, selected_target, full_target) in [
                        (3, selected_targets[0], full[6]),
                        (6, selected_targets[1], full[3]),
                    ] {
                        skeleton
                            .solve_foot(&mut selected, end, selected_target, effector_blend, weight)
                            .unwrap();
                        skeleton
                            .solve_foot(&mut complete, end, full_target, effector_blend, weight)
                            .unwrap();
                    }
                    assert_eq!(selected, complete);
                }
            }
        }
        initial[1].as_mut().unwrap().scale = f32::INFINITY;
        assert!(skeleton.model_bone(&initial, 3).is_err());
    }

    use super::*;
    #[test]
    fn foot_solver_reaches_model_space_target_and_preserves_other_bones() {
        let skeleton = Skeleton::from_value(serde_json::json!({"m_ID":"test",
            "m_boneIDs":["root","hip","knee","ankle","other"],"m_parentIndices":[-1,0,1,2,0],
            "m_parentSpaceReferencePose":[[0.,0.,0.,1.,0.,0.,0.,1.],[0.,0.,0.,1.,0.,0.,0.,1.],
                [0.,0.,-1.,1.,0.,0.,0.,1.],[0.,0.,-1.,1.,0.,0.,0.,1.],[5.,0.,0.,1.,0.,0.,0.,1.]]}))
        .unwrap();
        let initial = skeleton
            .reference
            .iter()
            .copied()
            .map(Some)
            .collect::<Vec<_>>();
        let mut local = initial.clone();
        let target = Transform {
            position: [0., 1., -1.],
            ..Transform::IDENTITY
        };
        skeleton
            .solve_foot(&mut local, 3, Some(target), false, 1.)
            .unwrap();
        let model = skeleton.model(&local).unwrap();
        assert!(length(sub(model[3].unwrap().position, target.position)) < 0.00001);
        assert!(
            (length(sub(model[2].unwrap().position, model[1].unwrap().position)) - 1.).abs()
                < 0.00001
        );
        assert!(
            (length(sub(model[3].unwrap().position, model[2].unwrap().position)) - 1.).abs()
                < 0.00001
        );
        assert_eq!(local[0], initial[0]);
        assert_eq!(local[4], initial[4]);
        let mut half = initial.clone();
        skeleton
            .solve_foot(&mut half, 3, Some(target), true, 0.5)
            .unwrap();
        assert!(
            length(sub(
                skeleton.model(&half).unwrap()[3].unwrap().position,
                [0., 0.5, -1.5]
            )) < 0.00001
        );
        // Antiparallel alignment uses the current client's deterministic orthogonal axis.
        let mut opposite = initial.clone();
        let opposite_target = Transform {
            position: [0., 0., 2.],
            ..Transform::IDENTITY
        };
        skeleton
            .solve_foot(&mut opposite, 3, Some(opposite_target), false, 1.)
            .unwrap();
        assert!(
            length(sub(
                skeleton.model(&opposite).unwrap()[3].unwrap().position,
                opposite_target.position
            )) < 0.00001
        );
        // Targets are in model space, including a translated, rotated chain parent.
        let mut moved = initial.clone();
        let parent = Transform {
            position: [4., 5., 6.],
            rotation: axis_angle([0., 0., 1.], 0.7).unwrap(),
            ..Transform::IDENTITY
        };
        moved[0] = Some(parent);
        let moved_target = Transform::compose(parent, target);
        skeleton
            .solve_foot(&mut moved, 3, Some(moved_target), false, 1.)
            .unwrap();
        assert!(
            length(sub(
                skeleton.model(&moved).unwrap()[3].unwrap().position,
                moved_target.position
            )) < 0.00001
        );
        assert_eq!(moved[0], Some(parent));
        let mut degenerate = initial.clone();
        degenerate[2].as_mut().unwrap().position = [0., 0., -0.0005];
        let before = degenerate.clone();
        skeleton
            .solve_foot(&mut degenerate, 3, Some(target), false, 1.)
            .unwrap();
        assert_eq!(degenerate, before);
        skeleton.solve_foot(&mut half, 3, None, false, 1.).unwrap();
        assert!(half[1..4].iter().all(Option::is_none));
        assert_eq!(half[4], initial[4]);
    }

    #[test]
    fn model_space_blend_uses_global_rotations_and_retains_recorded_root_behavior() {
        let skeleton = Skeleton::from_value(serde_json::json!({"m_ID":"test",
            "m_boneIDs":["root","child"],"m_parentIndices":[-1,0],
            "m_parentSpaceReferencePose":[[0.,0.,0.,1.,0.,0.,0.,1.],[1.,0.,0.,1.,0.,0.,0.,1.]]}))
        .unwrap();
        let base = skeleton
            .reference
            .iter()
            .copied()
            .map(Some)
            .collect::<Vec<_>>();
        let c = std::f32::consts::FRAC_1_SQRT_2;
        let target = vec![
            Some(Transform {
                rotation: [0., 0., c, c],
                ..Transform::IDENTITY
            }),
            Some(Transform {
                rotation: [0., 0., -c, c],
                position: [1., 0., 0.],
                ..Transform::IDENTITY
            }),
        ];
        let source = Pose {
            model: skeleton.model(&base).unwrap(),
            local: base,
            is_additive: false,
        };
        let target = Pose {
            model: skeleton.model(&target).unwrap(),
            local: target,
            is_additive: false,
        };
        let result = skeleton
            .model_space_blend(&source, &target, &[0., 1.], 1.)
            .unwrap();
        let model = skeleton.model(&result).unwrap();
        let child = model[1].unwrap();
        assert!(child.position[0].abs() < 0.00001 && (child.position[1] - 1.).abs() < 0.00001);
        assert!((result[1].unwrap().rotation[3].abs() - 1.).abs() < 0.00001);
        assert_eq!(
            skeleton
                .model_space_blend(&source, &target, &[0., 0.], 1.)
                .unwrap(),
            source.local
        );
        let unknown = Pose {
            local: vec![None, None],
            model: vec![None, None],
            is_additive: false,
        };
        assert_eq!(
            skeleton
                .model_space_blend(&source, &unknown, &[1., 1.], 0.)
                .unwrap(),
            source.local
        );
        assert!(skeleton
            .model_space_blend(&source, &unknown, &[1., 1.], 1.)
            .unwrap()
            .iter()
            .all(Option::is_none));
    }

    #[test]
    fn aim_recipe_uses_recorded_parameters_and_rebuilds_model_pose() -> Result<()> {
        use super::super::animation_recipe::Task;
        let fixtures: Vec<Value> = serde_json::from_str(include_str!("animation_aim_cases.json"))?;
        let fixture = &fixtures[0];
        let local: Vec<Transform> = serde_json::from_value(fixture["local"].clone())?;
        let parents: Vec<Option<usize>> = serde_json::from_value(fixture["parents"].clone())?;
        let mut skeleton = Skeleton::from_value(serde_json::json!({
            "m_ID":"animation/skeletons/characters/worldmodel.vnmskel",
            "m_boneIDs":fixture["names"],
            "m_parentIndices":parents.iter().map(|p|p.map_or(-1,|p|p as i32)).collect::<Vec<_>>(),
            "m_parentSpaceReferencePose":local.iter().map(|t|[t.position[0],t.position[1],t.position[2],t.scale,
                t.rotation[0],t.rotation[1],t.rotation[2],t.rotation[3]]).collect::<Vec<_>>()
        }))?;
        let recipe = Recipe {
            tasks: vec![
                Task {
                    kind: "CNmReferencePoseTask".into(),
                    dependencies: vec![],
                },
                Task {
                    kind: "CNmAimCSTask".into(),
                    dependencies: vec![0],
                },
            ],
            parameters: vec![
                Parameters::ReferencePose,
                Parameters::AimCs {
                    normalized16: serde_json::from_value(fixture["args"]["normalized16"].clone())?,
                    normalized8: serde_json::from_value(fixture["args"]["normalized8"].clone())?,
                    flags3: fixture["args"]["mode"].as_u64().unwrap() as u8,
                    flags5: fixture["args"]["flags5"].as_u64().unwrap() as u8,
                },
            ],
            network_tick: 0,
            topology_bits_consumed: 0,
            bits_consumed: 0,
            raw_dynamic: vec![],
            raw_topology: vec![],
        };
        let clips = BTreeMap::new();
        assert!(skeleton.evaluate(&recipe, &clips, &[], &[]).is_err());
        skeleton.validate_cs2_write_set(CS2_WRITE_SET_CLIENT_SHA256)?;
        let pose = skeleton.evaluate(&recipe, &clips, &[], &[])?;
        assert_eq!(pose.model, skeleton.model(&pose.local)?);
        let head = skeleton.index("head_0")?;
        let world: Transform = serde_json::from_value(fixture["world"].clone())?;
        let point = Transform::compose(
            Transform::compose(world, pose.model[head].context("missing head")?),
            Transform {
                position: [7., 6., 0.],
                ..Transform::IDENTITY
            },
        )
        .position;
        let expected: [f32; 3] = serde_json::from_value(fixture["expected"].clone())?;
        assert!(
            point
                .into_iter()
                .zip(expected)
                .map(|(a, b)| (a - b).powi(2))
                .sum::<f32>()
                .sqrt()
                < 0.001
        );
        Ok(())
    }

    #[test]
    fn cached_pose_is_recipe_scoped_and_scale_preserves_local_offsets() {
        use super::super::animation_recipe::Task;
        let skeleton = Skeleton::from_value(serde_json::json!({"m_ID":"test",
            "m_boneIDs":["root","child"],"m_parentIndices":[-1,0],
            "m_parentSpaceReferencePose":[[0.,0.,0.,1.,0.,0.,0.,1.],[2.,0.,0.,1.,0.,0.,0.,1.]]}))
        .unwrap();
        let mut recipe = Recipe {
            tasks: vec![
                Task {
                    kind: "CNmReferencePoseTask".into(),
                    dependencies: vec![],
                },
                Task {
                    kind: "CNmCachedPoseWriteTask".into(),
                    dependencies: vec![0],
                },
                Task {
                    kind: "CNmCachedPoseReadTask".into(),
                    dependencies: vec![],
                },
                Task {
                    kind: "CNmScaleTask".into(),
                    dependencies: vec![2],
                },
            ],
            parameters: vec![
                Parameters::ReferencePose,
                Parameters::CachedPoseWrite { cache_id: 7 },
                Parameters::CachedPoseRead { cache_id: 7 },
                Parameters::Scale {
                    masks: vec![MaskTask::Generate(128)],
                },
            ],
            network_tick: 0,
            topology_bits_consumed: 0,
            bits_consumed: 0,
            raw_dynamic: vec![],
            raw_topology: vec![],
        };
        let clips = BTreeMap::new();
        let pose = skeleton.evaluate(&recipe, &clips, &[], &[]).unwrap();
        assert_eq!(pose.local[1].unwrap().position, [2., 0., 0.]);
        assert_eq!(pose.local[0].unwrap().scale, 128. / 255.);
        assert_eq!(pose.model[1].unwrap().position, [2. * 128. / 255., 0., 0.]);
        recipe.parameters[3] = Parameters::Scale {
            masks: vec![MaskTask::Generate(0)],
        };
        let pose = skeleton.evaluate(&recipe, &clips, &[], &[]).unwrap();
        assert_eq!(pose.local[0].unwrap().scale, 0.);
        assert_eq!(pose.local[1].unwrap().scale, 1.);
        recipe.tasks = vec![recipe.tasks[2].clone()];
        recipe.parameters = vec![recipe.parameters[2].clone()];
        assert!(skeleton
            .evaluate(&recipe, &clips, &[], &[])
            .unwrap_err()
            .to_string()
            .contains("unavailable"));
    }

    #[test]
    fn masks_feather_and_zero_weight_preserves_known_source() {
        let parents = vec![None, Some(0), Some(1), Some(2), Some(0)];
        let mut weights = vec![0., -1., -1., 1., -1.];
        feather_weights(&parents, &mut weights);
        assert!((weights[1] - 1. / 3.).abs() < 1e-6);
        assert!((weights[2] - 2. / 3.).abs() < 1e-6);
        assert_eq!(weights[4], 0.);
        let source = Some(Transform::IDENTITY);
        assert_eq!(blend(source, None, 0., true).unwrap(), source);
        assert!(blend(source, None, 1., true).unwrap().is_none());
        assert_eq!(blend(None, source, 1., false).unwrap(), source);
    }

    #[test]
    fn additive_uses_parent_local_rotation_and_adds_scale_delta() {
        let half = std::f32::consts::FRAC_1_SQRT_2;
        let a = Transform {
            rotation: [half, 0., 0., half],
            position: [1., 2., 3.],
            scale: 1.,
        };
        let delta = Transform {
            rotation: [0., half, 0., half],
            position: [2., 0., 0.],
            scale: 0.,
        };
        let result = blend(Some(a), Some(delta), 1., true).unwrap().unwrap();
        assert_eq!(result.position, [3., 2., 3.]);
        assert_eq!(result.scale, 1.);
        for (v, w) in result.rotation.iter().zip([0.5, 0.5, 0.5, 0.5]) {
            assert!((v - w).abs() < 1e-6);
        }
    }

    #[test]
    fn invalid_skeletons_and_custom_write_sets_fail() {
        let source = serde_json::json!({"m_ID":"test","m_boneIDs":["root","child"],"m_parentIndices":[-1,0],
            "m_parentSpaceReferencePose":[[0.,0.,0.,1.,0.,0.,0.,1.],[1.,0.,0.,1.,0.,0.,0.,1.]]});
        let mut skeleton = Skeleton::from_value(source.clone()).unwrap();
        assert!(skeleton
            .validate_cs2_write_set(CS2_WRITE_SET_CLIENT_SHA256)
            .is_err());
        assert!(skeleton.model(&[None, Some(Transform::IDENTITY)]).unwrap()[1].is_none());
        let mut invalid = source;
        invalid["m_parentIndices"] = serde_json::json!([-1, 1]);
        assert!(Skeleton::from_value(invalid).is_err());
    }
}
