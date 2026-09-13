//! Packet-aligned raw entity snapshots for body pose, smoke and dynamic geometry work.
use crate::parser::DemoParser;
use anyhow::{ensure, Result};
use parser::second_pass::{parser_settings::SecondPassParser, variants::Variant};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntitySnapshot {
    pub entity_id: i32,
    pub serial: u32,
    pub pose_fields: Vec<PoseField>,
    pub class_name: String,
    pub properties: BTreeMap<String, Variant>,
    /// Network byte slots never received remain null, not fabricated zeroes.
    pub smoke_voxel_bytes: Option<Vec<Option<u8>>>,
}
#[derive(Debug, Serialize)]
pub struct PoseField {
    pub name: String,
    pub path: Vec<i32>,
    pub value: Variant,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneFrame {
    pub tick: i32,
    pub net_tick: u32,
    pub entities: Vec<EntitySnapshot>,
}

fn tracking_property(name: &str) -> bool {
    matches!(
        name.rsplit('.').next(),
        Some(
            "m_cellX"
                | "m_cellY"
                | "m_cellZ"
                | "m_vecX"
                | "m_vecY"
                | "m_vecZ"
                | "m_angEyeAngles"
                | "m_hModel"
        )
    )
}

pub(crate) fn capture(parser: &SecondPassParser<'_>, tracking_only: bool) -> SceneFrame {
    let mut entities = Vec::new();
    for entity in parser.entities.iter().flatten() {
        let Some(class) = parser.cls_by_id.get(entity.cls_id as usize) else {
            continue;
        };
        if !(relevant_class(&class.name) || animation_serializer(&class.serializer)) || (tracking_only && class.name != "CCSPlayerPawn") {
            continue;
        }
        let mut properties: BTreeMap<String, Variant> = entity
            .props
            .iter()
            .filter_map(|(id, value)| {
                let name = parser.prop_controller.id_to_name.get(id)?;
                (if tracking_only {
                    tracking_property(name)
                } else {
                    relevant_property(name)
                })
                .then(|| (name.clone(), value.clone()))
            })
            .collect();
        let mut smoke_voxel_bytes = None;
        if class.name == "CSmokeGrenadeProjectile" {
            use parser::first_pass::prop_controller::{SMOKE_VOXELS_ID, SMOKE_VOXELS_LIMIT};
            let size = match properties.get("m_nVoxelFrameDataSize") {
                Some(Variant::U32(n)) if *n <= SMOKE_VOXELS_LIMIT => Some(*n),
                Some(Variant::I32(n)) if (0..=SMOKE_VOXELS_LIMIT as i32).contains(n) => {
                    Some(*n as u32)
                }
                _ => None,
            };
            properties.remove("m_VoxelFrameData");
            if let Some(size) = size {
                smoke_voxel_bytes = Some(
                    (0..size)
                        .map(|i| match entity.props.get(&(SMOKE_VOXELS_ID + i)) {
                            Some(Variant::U32(n)) => u8::try_from(*n).ok(),
                            _ => None,
                        })
                        .collect(),
                );
            }
        }
        let mut pose_fields: Vec<_> = entity
            .pose_fields
            .iter()
            .filter(|_| !tracking_only)
            .map(|(path, (name, value))| PoseField {
                name: name.clone(),
                path: path.clone(),
                value: value.clone(),
            })
            .collect();
        pose_fields.sort_by(|a, b| a.path.cmp(&b.path));
        properties.retain(|name, _| !name.contains("PoseRecipe"));
        entities.push(EntitySnapshot {
            entity_id: entity.entity_id,
            serial: entity.serial,
            pose_fields,
            class_name: class.name.clone(),
            properties,
            smoke_voxel_bytes,
        });
    }
    entities.sort_by_key(|e| e.entity_id);
    SceneFrame {
        tick: parser.tick,
        net_tick: parser.net_tick,
        entities,
    }
}
/// Follow the recorded schema, including animated weapons and secondary graph entities.
pub(crate) fn animation_serializer(serializer: &parser::first_pass::sendtables::Serializer) -> bool {
    use parser::first_pass::sendtables::Field;
    fn visit_field(field: &Field) -> bool {
        match field {
            Field::Serializer(value) => animation_serializer(&value.serializer),
            Field::Pointer(value) => animation_serializer(&value.serializer),
            Field::Array(value) => visit_field(&value.field_enum),
            Field::Vector(value) => visit_field(&value.field_enum),
            _ => false,
        }
    }
    serializer.name == "CBodyComponentBaseAnimGraph" || serializer.fields.iter().any(visit_field)
}

pub(crate) fn relevant_class(name: &str) -> bool {
    [
        "PlayerPawn",
        "PlayerController",
        "Projectile",
        "Inferno",
        "GameRules",
        "CCSTeam",
        "SmokeGrenadeProjectile",
        "Door",
        "Breakable",
        "DynamicProp",
        "PhysicsProp",
        "PhysProp",
        "FuncBrush",
        "MovingToggle",
    ]
    .iter()
    .any(|part| name.contains(part))
}
pub(crate) fn relevant_property(name: &str) -> bool {
    // Legacy IDs conflate velocity and view offset. Their distinct wire-owned
    // values are retained by generic pose capture before the statistics overwrite.
    if matches!(name, "CCSPlayerPawn.m_vecX" | "CCSPlayerPawn.m_vecY" | "CCSPlayerPawn.m_vecZ") {
        return false;
    }
    let n = name.rsplit('.').next().unwrap_or(name).to_ascii_lowercase();
    if n.rsplit('.').next() == Some("m_hplayerpawn") { return true; }
    [
        "graph",
        "serialization",
        "mesh",
        "bodygroup",
        "scale",
        "material",
        "time",
        "warmup",
        "match",
        "paused",
        "origin",
        "m_vec",
        "steamid",
        "teamnum",
        "flash",
        "blind",
        "duck",
        "velocity",
        "round",
        "freeze",
        "fire",
        "rotation",
        "angles",
        "cell",
        "offset",
        "model",
        "hitbox",
        "bone",
        "skeleton",
        "anim",
        "sequence",
        "cycle",
        "pose",
        "voxel",
        "smoke",
        "door",
        "solid",
        "collision",
        "health",
        "lifestate",
        "effects",
        "render",
        "parent",
        "alpha",
        "simtime",
    ]
    .iter()
    .any(|part| n.contains(part))
}

pub fn extract(
    parser: &DemoParser,
    bytes: &[u8],
    first: i32,
    last: i32,
    step: i32,
) -> Result<Vec<SceneFrame>> {
    ensure!(
        first >= 0 && last >= first && step > 0,
        "invalid scene tick range"
    );
    parser.scene_frames(bytes, first, last, step)
}

/// Increment schema for incompatible data changes; implementation for algorithm changes.
pub fn contract() -> super::Contract {
    super::Contract {
        module: "packet-scene".into(),
        schema_version: 1,
        implementation_version: "0.3.0".into(),
    }
}

#[cfg(test)]
mod tests {
    use parser::first_pass::{
        prop_controller::SMOKE_VOXELS_ID,
        read_bits::Bitreader,
        sendtables::{get_propinfo, Field, ValueField},
    };
    use parser::second_pass::{decoder::Decoder, path_ops::FieldPath};

    #[test]
    fn secondary_animation_entities_are_selected_by_schema() {
        use parser::first_pass::sendtables::{Field, Serializer, SerializerField};
        let mut serializer = Serializer { name: "UnlistedAnimatedEntity".into(), fields: vec![] };
        assert!(!super::animation_serializer(&serializer));
        serializer.fields.push(Field::Serializer(SerializerField { serializer: Serializer {
            name:"CBodyComponentBaseAnimGraph".into(),fields:vec![],
        }}));
        assert!(super::animation_serializer(&serializer));
    }

    #[test]
    fn controller_pawn_handle_does_not_select_every_player_internal() {
        assert!(super::relevant_property("CCSPlayerController.m_hPlayerPawn"));
        assert!(super::relevant_property("CCSPlayerController.m_steamID"));
        assert!(super::relevant_property("CCSPlayerPawn.m_iHealth"));
        assert!(!super::relevant_property("CCSPlayerPawn.m_vecZ"));
        assert!(super::relevant_property("CCSPlayerPawn.m_vecViewOffset.m_vecZ"));
        assert!(super::relevant_property("CCSPlayerPawn.m_vecVelocity.m_vecZ"));
        assert!(super::relevant_property("CBodyComponentBaseAnimGraph.m_nSecondarySkeletonMasterCount"));
        assert!(super::relevant_property("CCSPlayerPawn.m_angEyeAngles"));
        assert!(super::relevant_property("CCSPlayerPawn.m_flFlashDuration"));
        assert!(!super::relevant_property("CCSPlayerPawn.CCSPlayer_MovementServices.m_flFrictionStashedSpeed"));
        assert!(!super::relevant_property("CBodyComponentBaseAnimGraph.m_internalCounter"));
        assert!(super::relevant_property("CBodyComponentBaseAnimGraph.m_nServerSerializationContextIteration"));
    }
    #[test]
    fn binary_and_smoke_fields_preserve_bytes_and_indices() {
        assert_eq!(
            Bitreader::new(&[3, 0, 128, 255])
                .decode_binary_block()
                .unwrap(),
            [0, 128, 255]
        );
        assert!(Bitreader::new(&[3, 0, 128]).decode_binary_block().is_err());
        let field = Field::Value(ValueField {
            decoder: Decoder::UnsignedDecoder,
            name: "m_VoxelFrameData".into(),
            full_name: "CSmokeGrenadeProjectile.m_VoxelFrameData".into(),
            send_node: String::new(),
            analysis_name: None,
            analysis_signed_tick: false,
            should_parse: true,
            prop_id: SMOKE_VOXELS_ID,
        });
        let mut path = FieldPath {
            last: 1,
            path: [94, 0, 0, 0, 0, 0, 0],
        };
        assert_eq!(
            get_propinfo(&field, &path).unwrap().prop_id,
            SMOKE_VOXELS_ID
        );
        path.path[1] = 65535;
        assert_eq!(
            get_propinfo(&field, &path).unwrap().prop_id,
            SMOKE_VOXELS_ID + 65535
        );
        path.path[1] = 65536;
        assert!(get_propinfo(&field, &path).is_none());
        path.path[1] = -1;
        assert!(get_propinfo(&field, &path).is_none());
        path.last = 0;
        assert!(get_propinfo(&field, &path).is_none());
    }
}
