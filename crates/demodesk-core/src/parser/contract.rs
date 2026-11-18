//! Application-owned parser results. No third-party types cross this boundary.
use serde::{
    ser::{SerializeMap, SerializeSeq},
    Serialize,
};
#[derive(Debug, Clone, PartialEq)]
pub enum Variant {
    Bool(bool),
    U32(u32),
    I32(i32),
    F32(f32),
    U64(u64),
    String(String),
    VecXY([f32; 2]),
    VecXYZ([f32; 3]),
    // Todo change to Vec<T>
    StringVec(Vec<String>),
    U32Vec(Vec<u32>),
    U64Vec(Vec<u64>),
    Stickers(Vec<Sticker>),
    InputHistory(Vec<InputHistory>),
    UserCmdSubtickMoves(Vec<UserCmdSubtickMove>),
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Sticker {
    pub name: String,
    pub wear: f32,
    pub id: u32,
    pub x: f32,
    pub y: f32,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct InputHistory {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub render_tick_count: i32,
    pub render_tick_fraction: f32,
    pub player_tick_count: i32,
    pub player_tick_fraction: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UserCmdSubtickMove {
    pub when: f32,
    pub button: u64,
    pub pressed: bool,
    pub analog_forward: f32,
    pub analog_left: f32,
    pub pitch_delta: f32,
    pub yaw_delta: f32,
}

impl Serialize for Variant {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Variant::Bool(b) => serializer.serialize_bool(*b),
            Variant::F32(f) => serializer.serialize_f32(*f),
            Variant::I32(i) => serializer.serialize_i32(*i),
            Variant::String(s) => serializer.serialize_str(s),
            Variant::U32(u) => serializer.serialize_u32(*u),
            Variant::U64(u) => serializer.serialize_str(&u.to_string()),
            Variant::StringVec(v) => {
                let mut s = serializer.serialize_seq(Some(v.len()))?;
                for item in v {
                    s.serialize_element(item)?;
                }
                s.end()
            }
            Variant::VecXY(v) => {
                let mut s = serializer.serialize_seq(Some(v.len()))?;
                for item in v {
                    s.serialize_element(item)?;
                }
                s.end()
            }
            Variant::VecXYZ(v) => {
                let mut s = serializer.serialize_seq(Some(v.len()))?;
                for item in v {
                    s.serialize_element(item)?;
                }
                s.end()
            }
            Variant::U32Vec(v) => {
                let mut s = serializer.serialize_seq(Some(v.len()))?;
                for item in v {
                    s.serialize_element(item)?;
                }
                s.end()
            }
            Variant::U64Vec(v) => {
                let mut s = serializer.serialize_seq(Some(v.len()))?;
                for item in v {
                    s.serialize_element(&item.to_string())?;
                }
                s.end()
            }
            Variant::Stickers(v) => {
                let mut s = serializer.serialize_seq(Some(v.len()))?;
                for item in v {
                    s.serialize_element(&item)?;
                }
                s.end()
            }
            Variant::InputHistory(v) => {
                let mut s = serializer.serialize_seq(Some(v.len()))?;
                for item in v {
                    s.serialize_element(&item)?;
                }
                s.end()
            }
            Variant::UserCmdSubtickMoves(v) => {
                let mut s = serializer.serialize_seq(Some(v.len()))?;
                for item in v {
                    s.serialize_element(&item)?;
                }
                s.end()
            }
        }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct EventField {
    pub name: String,
    pub data: Option<Variant>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct GameEvent {
    pub name: String,
    pub fields: Vec<EventField>,
    pub tick: i32,
}

impl Serialize for GameEvent {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut map = serializer.serialize_map(Some(2))?;
        map.serialize_entry(&"tick", &self.tick)?;
        map.serialize_entry(&"event_name", &self.name)?;
        for field in &self.fields {
            map.serialize_entry(&field.name, &field.data)?;
        }
        map.end()
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ServerInfo {
    pub tick_interval: Option<f32>,
    pub protocol: Option<i32>,
    pub map_name: Option<String>,
    pub game_session_manifest: Option<Vec<u8>>,
}
#[derive(Debug, Clone)]
pub struct PlayerMetadata {
    pub steamid: Option<u64>,
    pub name: Option<String>,
    pub team_number: Option<i32>,
}
pub struct EventBatch {
    pub game_events: Vec<GameEvent>,
    pub server_infos: Vec<ServerInfo>,
    pub players: Vec<PlayerMetadata>,
}

#[derive(Debug)]
pub struct FireConvars {
    pub tick: i32,
    pub player: u32,
    pub seed: u32,
    pub item_def_index: u32,
    pub values: std::collections::BTreeMap<String, String>,
}
#[derive(Debug)]
pub struct ReplicatedConvars {
    pub patch_version: Option<i32>,
    pub demo_version_name: Option<String>,
    pub fires: Vec<FireConvars>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntitySnapshot {
    pub entity_id: i32,
    pub serial: u32,
    pub pose_fields: Vec<PoseField>,
    pub class_name: String,
    pub properties: std::collections::BTreeMap<String, Variant>,
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
