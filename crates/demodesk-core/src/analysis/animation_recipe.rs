//! Checked, partial decoding of recorded AnimGraph2 task recipes.
//! This exposes task parameters, not measured bones or a complete animation runtime.
use anyhow::{bail, ensure, Context as _, Result};

pub const RECORDED_TASK_NAMES: [&str; 16] = [
    "CNmCachedPoseWriteTask",
    "CNmSampleTask",
    "CNmScaleTask",
    "CNmReferencePoseTask",
    "CNmTwoBoneIKTask",
    "CNmSnapWeaponTask",
    "CNmCachedPoseReadTask",
    "CNmBlendTask",
    "CNmAdditiveBlendTask",
    "CNmOverlayBlendTask",
    "CNmModelSpaceBlendTask",
    "CNmChainLookatTask",
    "CNmFollowBoneTask",
    "CNmZeroPoseTask",
    "CNmAimCSTask",
    "CNmFootIKTask",
];

pub struct Context<'a> {
    pub task_names: &'a [&'a str],
    pub resource_count: u32,
    pub resource_bits: u8,
    pub mask_count: u32,
    pub mask_bits: u8,
    /// Unique recorded bone names: primary first, then sorted secondary skeletons.
    pub bone_names: &'a [String],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    pub kind: String,
    pub dependencies: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MaskTask {
    Mask(u32),
    Generate(u8),
    Blend {
        source: usize,
        target: usize,
        weight: u8,
    },
    Scale {
        source: usize,
        weight: u8,
    },
    Combine {
        source: usize,
        target: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IkTarget {
    Bone(String),
    Transform {
        rotation: [u16; 3],
        translation: [u16; 3],
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Parameters {
    Sample {
        resource_index: u32,
        normalized_time: u16,
    },
    Blend {
        normalized_weight: u8,
        masks: Vec<MaskTask>,
    },
    ModelSpaceBlend {
        normalized_weight: u8,
        masks: Vec<MaskTask>,
    },
    AimCs {
        normalized16: [u16; 2],
        normalized8: [u8; 4],
        flags3: u8,
        flags5: u8,
    },
    FootIk {
        effectors: [String; 2],
        targets: [IkTarget; 2],
        effector_blend: bool,
        normalized_weight: u8,
    },
    SnapWeapon {
        flags2: u8,
    },
    CachedPoseRead {
        cache_id: u8,
    },
    CachedPoseWrite {
        cache_id: u8,
    },
    Scale {
        masks: Vec<MaskTask>,
    },
    ReferencePose,
    ZeroPose,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recipe {
    pub tasks: Vec<Task>,
    pub parameters: Vec<Parameters>,
    pub network_tick: u32,
    pub topology_bits_consumed: usize,
    pub bits_consumed: usize,
    /// Original bytes, including the unparsed trailer and its boundary byte.
    pub raw_dynamic: Vec<u8>,
    pub raw_topology: Vec<u8>,
}

impl Recipe {
    pub fn remaining_bits(&self) -> usize {
        self.raw_dynamic.len() * 8 - self.bits_consumed
    }
    /// The first byte may include consumed bits; use bits_consumed % 8 as its offset.
    pub fn remaining_bytes(&self) -> &[u8] {
        &self.raw_dynamic[self.bits_consumed / 8..]
    }
}

struct Bits<'a> {
    bytes: &'a [u8],
    position: usize,
}
impl<'a> Bits<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }
    fn read(&mut self, width: u8) -> Result<u32> {
        ensure!(width <= 32, "invalid recipe bit width");
        let end = self
            .position
            .checked_add(width as usize)
            .context("recipe bit overflow")?;
        ensure!(
            end <= self.bytes.len().saturating_mul(8),
            "truncated recipe at bit {}",
            self.position
        );
        let mut result = 0;
        for bit in 0..width {
            result |= u32::from((self.bytes[self.position / 8] >> (self.position % 8)) & 1) << bit;
            self.position += 1;
        }
        Ok(result)
    }
}

pub fn index_bits(count: u32) -> u8 {
    (u32::BITS - count.leading_zeros()) as u8
}

fn arity(name: &str) -> Result<usize> {
    Ok(match name {
        "CNmSampleTask" | "CNmReferencePoseTask" | "CNmCachedPoseReadTask" | "CNmZeroPoseTask" => 0,
        "CNmCachedPoseWriteTask"
        | "CNmScaleTask"
        | "CNmTwoBoneIKTask"
        | "CNmSnapWeaponTask"
        | "CNmChainLookatTask"
        | "CNmFollowBoneTask"
        | "CNmAimCSTask"
        | "CNmFootIKTask" => 1,
        "CNmBlendTask"
        | "CNmAdditiveBlendTask"
        | "CNmOverlayBlendTask"
        | "CNmModelSpaceBlendTask" => 2,
        _ => bail!("unsupported animation task type {name}"),
    })
}

fn dependency(bits: &mut Bits<'_>, width: u8, current: usize) -> Result<usize> {
    let index = bits.read(width)? as usize;
    ensure!(index < current, "recipe dependency is not an earlier task");
    Ok(index)
}

fn masks(bits: &mut Bits<'_>, context: &Context<'_>) -> Result<Vec<MaskTask>> {
    let count = bits.read(5)?;
    let width = index_bits(count);
    let mut tasks = Vec::with_capacity(count as usize);
    for i in 0..count as usize {
        tasks.push(match bits.read(3)? {
            0 => {
                let index = bits.read(context.mask_bits)?;
                ensure!(
                    index < context.mask_count,
                    "bone mask index outside recorded context"
                );
                MaskTask::Mask(index)
            }
            1 => MaskTask::Generate(bits.read(8)? as u8),
            2 => MaskTask::Blend {
                source: dependency(bits, width, i)?,
                target: dependency(bits, width, i)?,
                weight: bits.read(8)? as u8,
            },
            3 => MaskTask::Scale {
                source: dependency(bits, width, i)?,
                weight: bits.read(8)? as u8,
            },
            4 => MaskTask::Combine {
                source: dependency(bits, width, i)?,
                target: dependency(bits, width, i)?,
            },
            kind => bail!("unsupported bone mask task {kind}"),
        });
    }
    Ok(tasks)
}

fn bone_name(bits: &mut Bits<'_>, context: &Context<'_>) -> Result<String> {
    ensure!(
        !context.bone_names.is_empty(),
        "missing animation bone name context"
    );
    ensure!(
        context.bone_names.len() <= 65535,
        "oversized animation bone name context"
    );
    let index = bits.read(index_bits(context.bone_names.len() as u32))? as usize;
    let name = context
        .bone_names
        .get(index)
        .context("animation bone name index outside recorded context")?;
    ensure!(!name.is_empty(), "empty animation bone name");
    Ok(name.clone())
}

fn foot_target(bits: &mut Bits<'_>, context: &Context<'_>) -> Result<IkTarget> {
    Ok(if bits.read(1)? != 0 {
        IkTarget::Bone(bone_name(bits, context)?)
    } else {
        IkTarget::Transform {
            rotation: [
                bits.read(16)? as u16,
                bits.read(16)? as u16,
                bits.read(16)? as u16,
            ],
            translation: [
                bits.read(16)? as u16,
                bits.read(16)? as u16,
                bits.read(16)? as u16,
            ],
        }
    })
}

/// Decode the supported prefix, preserving all remaining bytes.
/// The network tick prefix and task payload layout are only supported for recipe version 2.
pub fn decode(
    version: u32,
    topology: &[u8],
    dynamic: &[u8],
    context: &Context<'_>,
) -> Result<Recipe> {
    ensure!(
        version == 2,
        "unsupported animation recipe version {version}"
    );
    ensure!(
        !context.task_names.is_empty() && context.task_names.len() <= 256,
        "invalid animation task dictionary"
    );
    ensure!(
        context.resource_count > 0 && context.resource_bits == index_bits(context.resource_count),
        "resource index width does not match recorded context"
    );
    ensure!(
        context.mask_bits == index_bits(context.mask_count),
        "mask index width does not match recorded context"
    );
    let mut topology_bits = Bits::new(topology);
    let dependency_width = topology_bits.read(4)? as u8;
    ensure!(dependency_width > 0, "empty recipe dependency width");
    let count = topology_bits.read(dependency_width)? as usize;
    ensure!(count > 0 && count <= 4096, "invalid recipe task count");
    ensure!(
        dependency_width == index_bits(count as u32),
        "noncanonical recipe dependency width"
    );
    let kind_width = index_bits(context.task_names.len() as u32);
    let mut tasks = Vec::with_capacity(count);
    for i in 0..count {
        let kind_index = topology_bits.read(kind_width)? as usize;
        let name = context
            .task_names
            .get(kind_index)
            .context("animation task index outside recorded dictionary")?;
        let mut dependencies = Vec::new();
        for _ in 0..arity(name)? {
            dependencies.push(dependency(&mut topology_bits, dependency_width, i)?);
        }
        tasks.push(Task {
            kind: (*name).to_owned(),
            dependencies,
        });
    }
    let mut bits = Bits::new(dynamic);
    let network_tick = bits.read(32)?;
    let mut parameters = Vec::with_capacity(count);
    for task in &tasks {
        parameters.push(match task.kind.as_str() {
            "CNmSampleTask" => {
                let resource_index = bits.read(context.resource_bits)?;
                ensure!(
                    resource_index < context.resource_count,
                    "animation resource index outside recorded context"
                );
                Parameters::Sample {
                    resource_index,
                    normalized_time: bits.read(16)? as u16,
                }
            }
            "CNmBlendTask"
            | "CNmAdditiveBlendTask"
            | "CNmOverlayBlendTask"
            | "CNmModelSpaceBlendTask" => {
                let normalized_weight = bits.read(8)? as u8;
                let masks = if bits.read(1)? != 0 {
                    masks(&mut bits, context)?
                } else {
                    vec![]
                };
                if task.kind == "CNmModelSpaceBlendTask" {
                    ensure!(!masks.is_empty(), "model-space blend requires a bone mask");
                    Parameters::ModelSpaceBlend {
                        normalized_weight,
                        masks,
                    }
                } else {
                    Parameters::Blend {
                        normalized_weight,
                        masks,
                    }
                }
            }
            "CNmAimCSTask" => Parameters::AimCs {
                normalized16: [bits.read(16)? as u16, bits.read(16)? as u16],
                normalized8: [
                    bits.read(8)? as u8,
                    bits.read(8)? as u8,
                    bits.read(8)? as u8,
                    bits.read(8)? as u8,
                ],
                flags3: bits.read(3)? as u8,
                flags5: bits.read(5)? as u8,
            },
            "CNmFootIKTask" => {
                let effectors = [
                    bone_name(&mut bits, context)?,
                    bone_name(&mut bits, context)?,
                ];
                let targets = [
                    foot_target(&mut bits, context)?,
                    foot_target(&mut bits, context)?,
                ];
                let (effector_blend, normalized_weight) = if bits.read(1)? != 0 {
                    (bits.read(1)? != 0, bits.read(8)? as u8)
                } else {
                    (false, 255)
                };
                Parameters::FootIk {
                    effectors,
                    targets,
                    effector_blend,
                    normalized_weight,
                }
            }
            "CNmSnapWeaponTask" => Parameters::SnapWeapon {
                flags2: bits.read(2)? as u8,
            },
            // Verified in client.dll 1322d00/1322e30: both IDs are six bits.
            "CNmCachedPoseReadTask" => Parameters::CachedPoseRead {
                cache_id: bits.read(6)? as u8,
            },
            "CNmCachedPoseWriteTask" => Parameters::CachedPoseWrite {
                cache_id: bits.read(6)? as u8,
            },
            // 1327870 calls the same mask-list serializer as blend; no presence bit.
            "CNmScaleTask" => Parameters::Scale {
                masks: masks(&mut bits, context)?,
            },
            "CNmReferencePoseTask" => Parameters::ReferencePose,
            "CNmZeroPoseTask" => Parameters::ZeroPose,
            kind => bail!(
                "unsupported animation task payload {kind} at bit {}",
                bits.position
            ),
        });
    }
    Ok(Recipe {
        tasks,
        parameters,
        network_tick,
        topology_bits_consumed: topology_bits.position,
        bits_consumed: bits.position,
        raw_dynamic: dynamic.to_vec(),
        raw_topology: topology.to_vec(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn hex(value: &str) -> Vec<u8> {
        value
            .as_bytes()
            .chunks_exact(2)
            .map(|b| u8::from_str_radix(std::str::from_utf8(b).unwrap(), 16).unwrap())
            .collect()
    }
    fn pack(parts: &[(u32, u8)]) -> Vec<u8> {
        let mut bytes = vec![];
        let mut position = 0usize;
        for &(value, width) in parts {
            for bit in 0..width {
                if position / 8 == bytes.len() {
                    bytes.push(0);
                }
                bytes[position / 8] |= (((value >> bit) & 1) as u8) << (position % 8);
                position += 1;
            }
        }
        bytes
    }

    #[test]
    fn foot_and_model_space_payloads_have_checked_boundaries() {
        let names = vec!["ankle_L".into(), "ankle_R".into(), "target".into()];
        let context = Context {
            task_names: &RECORDED_TASK_NAMES,
            resource_count: 1,
            resource_bits: 1,
            mask_count: 0,
            mask_bits: 0,
            bone_names: &names,
        };
        let topology = pack(&[(2, 4), (2, 2), (3, 5), (15, 5), (0, 2)]);
        let dynamic = pack(&[
            (17, 32),
            (0, 2),
            (1, 2),
            (0, 1),
            (1, 16),
            (2, 16),
            (3, 16),
            (4, 16),
            (5, 16),
            (6, 16),
            (1, 1),
            (2, 2),
            (1, 1),
            (1, 1),
            (128, 8),
            (0xab, 8),
        ]);
        let recipe = decode(2, &topology, &dynamic, &context).unwrap();
        assert_eq!(recipe.bits_consumed, 146);
        assert_eq!(
            recipe.parameters[1],
            Parameters::FootIk {
                effectors: ["ankle_L".into(), "ankle_R".into()],
                targets: [
                    IkTarget::Transform {
                        rotation: [1, 2, 3],
                        translation: [4, 5, 6]
                    },
                    IkTarget::Bone("target".into())
                ],
                effector_blend: true,
                normalized_weight: 128
            }
        );
        assert!(decode(2, &topology, &dynamic[..18], &context).is_err());
        assert!(decode(
            2,
            &topology,
            &dynamic,
            &Context {
                bone_names: &[],
                ..context
            }
        )
        .is_err());
        let mut invalid = dynamic.clone();
        invalid[4] |= 3;
        assert!(decode(2, &topology, &invalid, &context).is_err());
        let topology = pack(&[(2, 4), (3, 2), (3, 5), (3, 5), (10, 5), (0, 2), (1, 2)]);
        let dynamic = pack(&[(0, 32), (128, 8), (1, 1), (1, 5), (1, 3), (255, 8)]);
        let recipe = decode(2, &topology, &dynamic, &context).unwrap();
        assert_eq!(recipe.bits_consumed, 57);
        assert_eq!(
            recipe.parameters[2],
            Parameters::ModelSpaceBlend {
                normalized_weight: 128,
                masks: vec![MaskTask::Generate(255)]
            }
        );
    }

    #[test]
    fn cache_ids_and_scale_masks_use_verified_bit_boundaries() {
        // Reference, cache write(63), cache read(63), scale(generate 128).
        let topology = pack(&[
            (3, 4),
            (4, 3),
            (3, 5),
            (0, 5),
            (0, 3),
            (6, 5),
            (2, 5),
            (2, 3),
        ]);
        let dynamic = pack(&[
            (17, 32),
            (63, 6),
            (63, 6),
            (1, 5),
            (1, 3),
            (128, 8),
            (0xabc, 12),
        ]);
        let context = Context {
            task_names: &RECORDED_TASK_NAMES,
            resource_count: 1,
            resource_bits: 1,
            mask_count: 0,
            mask_bits: 0,
            bone_names: &[],
        };
        let recipe = decode(2, &topology, &dynamic, &context).unwrap();
        assert_eq!(recipe.bits_consumed, 60);
        assert_eq!(
            recipe.parameters[1],
            Parameters::CachedPoseWrite { cache_id: 63 }
        );
        assert_eq!(
            recipe.parameters[2],
            Parameters::CachedPoseRead { cache_id: 63 }
        );
        assert_eq!(
            recipe.parameters[3],
            Parameters::Scale {
                masks: vec![MaskTask::Generate(128)]
            }
        );
        assert!(decode(2, &topology, &dynamic[..7], &context).is_err());
    }

    #[test]
    fn recorded_recipe_preserves_unparsed_trailer_and_rejects_bad_inputs() {
        let topology = hex("f310101290e6580a");
        let dynamic = hex("8b340000270300e00e00f03f00c0796dfb1f300382f87bff0000800100010000030000600000000003005b86cb82d0a35c2bf122faee00fdd00501000200021c00000600010030001015e49d3d0300008615e49d3d0300009315e49d3d030000");
        let context = Context {
            task_names: &RECORDED_TASK_NAMES,
            resource_count: 975,
            resource_bits: 10,
            mask_count: 12,
            mask_bits: 4,
            bone_names: &[],
        };
        let recipe = decode(2, &topology, &dynamic, &context).unwrap();
        assert_eq!(recipe.network_tick, 13451);
        assert_eq!(recipe.tasks.len(), 7);
        assert_eq!(recipe.tasks[2].dependencies, [0, 1]);
        assert_eq!(recipe.tasks[4].dependencies, [2, 3]);
        assert_eq!(
            recipe.parameters[0],
            Parameters::Sample {
                resource_index: 807,
                normalized_time: 0
            }
        );
        assert_eq!(
            recipe.parameters[1],
            Parameters::Sample {
                resource_index: 952,
                normalized_time: 0
            }
        );
        assert_eq!(
            recipe.parameters[2],
            Parameters::Blend {
                normalized_weight: 255,
                masks: vec![MaskTask::Mask(0)]
            }
        );
        assert_eq!(
            recipe.parameters[4],
            Parameters::Blend {
                normalized_weight: 255,
                masks: vec![MaskTask::Mask(3)]
            }
        );
        assert_eq!(recipe.topology_bits_consumed, 60);
        assert_eq!(recipe.bits_consumed, 226);
        assert_eq!(recipe.remaining_bits(), dynamic.len() * 8 - 226);
        assert_eq!(recipe.raw_dynamic, dynamic);
        assert_eq!(recipe.remaining_bytes(), &dynamic[28..]);
        for end in 0..29 {
            assert!(decode(2, &topology, &dynamic[..end], &context).is_err());
        }
        for end in 0..topology.len() {
            assert!(decode(2, &topology[..end], &dynamic, &context).is_err());
        }
        assert!(decode(1, &topology, &dynamic, &context).is_err());
        let mut names = RECORDED_TASK_NAMES;
        names[1] = "UnknownTask";
        assert!(decode(
            2,
            &topology,
            &dynamic,
            &Context {
                task_names: &names,
                ..context
            }
        )
        .is_err());
        names = RECORDED_TASK_NAMES;
        names[14] = "CNmTwoBoneIKTask";
        assert!(decode(
            2,
            &topology,
            &dynamic,
            &Context {
                task_names: &names,
                ..context
            }
        )
        .unwrap_err()
        .to_string()
        .contains("unsupported animation task payload"));
        assert!(decode(
            2,
            &topology,
            &dynamic,
            &Context {
                resource_bits: 9,
                ..context
            }
        )
        .is_err());
        assert!(decode(
            2,
            &topology,
            &dynamic,
            &Context {
                mask_bits: 3,
                ..context
            }
        )
        .is_err());
        let context = Context {
            resource_count: 800,
            ..context
        };
        assert!(decode(2, &topology, &dynamic, &context).is_err());
    }
}
