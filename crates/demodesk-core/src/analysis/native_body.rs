//! Native packet pose preparation. Unknown task outputs remain absent.
use super::{
    animation_assets::{self, Assets},
    animation_clip::Transform,
    animation_recipe, compact,
};
use anyhow::{ensure, Context, Result};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fs::File,
    io::BufReader,
    path::Path,
};

pub struct Prepared {
    pub assets: Assets,
    pub events: super::event_context::Events,
    pub header: super::Artifact<compact::Metadata>,
    pub coverage: compact::Summary,
    pub skeleton: super::animation_pose::Skeleton,
    pub client_sha256: String,
    pub point_names: Vec<String>,
    hitbox_points: super::model_hitboxes::Points,
}
fn reader(path: &Path) -> Result<flate2::read::MultiGzDecoder<BufReader<File>>> {
    Ok(flate2::read::MultiGzDecoder::new(BufReader::new(
        File::open(path)?,
    )))
}
fn strings(data: &[u8], cursor: &mut usize) -> Result<Vec<String>> {
    let count = u32::from_le_bytes(
        data.get(*cursor..*cursor + 4)
            .context("truncated dictionary count")?
            .try_into()?,
    ) as usize;
    *cursor += 4;
    ensure!(count <= 65536, "oversized animation dictionary");
    (0..count)
        .map(|_| {
            let tail = data.get(*cursor..).context("truncated dictionary")?;
            let end = tail
                .iter()
                .position(|b| *b == 0)
                .context("unterminated dictionary name")?;
            let value = std::str::from_utf8(&tail[..end])?.to_owned();
            *cursor += end + 1;
            Ok(value)
        })
        .collect()
}
fn dictionary(data: &[u8]) -> Result<(Vec<String>, Vec<String>)> {
    ensure!(
        data.get(..4) == Some(&1u32.to_le_bytes()),
        "unsupported animation dictionary"
    );
    let mut cursor = 4;
    let names = strings(data, &mut cursor)?;
    let masks = if cursor < data.len() {
        strings(data, &mut cursor)?
    } else {
        vec![]
    };
    ensure!(cursor == data.len(), "unparsed animation dictionary bytes");
    Ok((names, masks))
}
pub fn prepare(path: &Path, root: &Path, game: &Path, vrf: &Path) -> Result<Prepared> {
    let mut raw_events = Vec::new();
    let mut seen = BTreeSet::new();
    let mut resources = BTreeSet::new();
    let mut models = BTreeMap::<(i32, u32), BTreeSet<u64>>::new();
    let mut secondary_pawns = BTreeMap::<(i32, u32), Entity>::new();
    let mut secondary_entities = BTreeSet::new();
    let mut pawn_models = BTreeSet::new();
    let (header, coverage) = compact::visit(
        reader(path)?,
        |frame| {
            let mut changed_secondary = BTreeSet::new();
            for id in frame.changed {
                let field = &frame.fields[*id as usize];
                let key = (field.entity, field.serial);
                if field.class == "CCSPlayerPawn"
                    && field.name.contains(".m_vecSecondarySkeletons/")
                {
                    secondary_pawns
                        .entry(key)
                        .or_default()
                        .update(&field.name, frame.values.get(id));
                    changed_secondary.insert(key);
                }
                if field.name.ends_with(".m_hModel") {
                    if let Some(value) = frame.values.get(id) {
                        if value.len() == 9 && value[0] == 4 {
                            let handle = u64::from_le_bytes(value[1..].try_into()?);
                            if handle != 0 && handle != u64::MAX {
                                models.entry(key).or_default().insert(handle);
                                if field.class == "CCSPlayerPawn" {
                                    pawn_models.insert(handle);
                                }
                            }
                        }
                    }
                }
                if field.class != "AnimationContext"
                    || !field.name.starts_with("AnimAssetData/")
                    || !field.name.ends_with("/data")
                {
                    continue;
                }
                let Some(value) = frame.values.get(id) else {
                    continue;
                };
                if !seen.insert(value.clone()) {
                    continue;
                }
                ensure!(
                    value.first() == Some(&14),
                    "invalid animation dictionary envelope"
                );
                let (names, _) = dictionary(&value[1..])?;
                resources.extend(
                    names
                        .into_iter()
                        .filter(|n| n.ends_with(".vnmclip") || n.ends_with(".vnmskel")),
                );
            }
            for key in changed_secondary {
                // An incomplete network array remains unavailable in visit; do not fabricate handles.
                if let Ok(handles) = secondary_pawns[&key].secondary_skeleton_handles() {
                    secondary_entities.extend(
                        handles
                            .into_iter()
                            .map(|handle| ((handle & 0x3fff) as i32, handle >> 14)),
                    );
                }
            }
            Ok(())
        },
        |event| {
            raw_events.push(event);
            Ok(())
        },
    )?;
    // Resolve animation dependencies, not unrelated map brush model resources.
    // The generic source still contains every recorded model and entity.
    let model_handles = secondary_entities
        .iter()
        .filter_map(|key| models.get(key))
        .flat_map(|models| models.iter().copied())
        .chain(pawn_models.iter().copied())
        .collect();
    ensure!(
        !resources.is_empty(),
        "missing recorded animation dependencies"
    );
    let assets = animation_assets::prepare_with_model_hitboxes(
        root,
        game,
        vrf,
        &resources.into_iter().collect::<Vec<_>>(),
        &model_handles,
        &pawn_models,
    )?;
    use sha2::Digest;
    let client = std::fs::read(game.join("game/csgo/bin/win64/client.dll"))?;
    let client_sha256 = format!("{:x}", sha2::Sha256::digest(client));
    let mut skeleton = super::animation_pose::Skeleton::from_value(assets.skeleton.clone())?;
    skeleton.validate_cs2_write_set(&client_sha256)?;
    let mut point_names = skeleton.names.clone();
    let hitbox_points = super::model_hitboxes::Points::new(
        &assets.model_hitboxes,
        &skeleton.names,
        &mut point_names,
    )?;
    Ok(Prepared {
        events: super::event_context::Events::from_raw(raw_events),
        point_names,
        hitbox_points,
        assets,
        header,
        coverage,
        skeleton,
        client_sha256,
    })
}

#[derive(Default)]
struct Entity {
    class: String,
    fields: HashMap<String, Vec<u8>>,
    body_fields: HashMap<String, String>,
    pose_version: Option<String>,
    pose_slot: Option<String>,
    simulation_time: Option<String>,
    dynamic: Option<String>,
    model: Option<String>,
    eye_offsets: [Option<String>; 3],
    velocities: [Option<String>; 3],
    movement_fields: HashMap<String, String>,
    topologies: HashMap<u32, String>,
}
impl Entity {
    fn remember_field(&mut self, name: &str) {
        if name.starts_with("pose/rawNetworkTime/CCSPlayerPawn.m_flSimulationTime/") {
            self.simulation_time = Some(name.into());
        }
        if name.ends_with(".m_hModel") {
            self.model = Some(name.into());
        }
        if let Some(leaf) = name.strip_prefix("CCSPlayerPawn.CBodyComponentBaseAnimGraph.") {
            self.body_fields.insert(leaf.into(), name.into());
        }
        let Some((property, path)) = name.strip_prefix("pose/").and_then(|n| n.split_once('/'))
        else {
            if name.starts_with("pose-bytes/") && name.contains(".m_SerializePoseRecipeAG2Dynamic/")
            {
                self.dynamic = Some(name.into());
            }
            return;
        };
        if property.starts_with("CCSPlayerPawn.") {
            let leaf = property.rsplit('.').next().unwrap_or(property);
            if matches!(
                leaf,
                "m_fFlags"
                    | "m_nLastJumpTick"
                    | "m_flLastJumpFrac"
                    | "m_MoveType"
                    | "m_nActualMoveType"
                    | "m_flWaterLevel"
                    | "m_nWaterLevel"
                    | "m_nLadderSurfacePropIndex"
            ) {
                self.movement_fields.insert(leaf.into(), name.into());
            }
        }
        if property.ends_with(".m_nSerializePoseRecipeVersionAG2") {
            self.pose_version = Some(name.into());
        }
        if property.ends_with(".m_nSerializePoseRecipeAG2ActiveSlot") {
            self.pose_slot = Some(name.into());
        }
        for (index, axis) in ["X", "Y", "Z"].iter().enumerate() {
            if property == format!("CCSPlayerPawn.m_vecVelocity.m_vec{axis}") {
                self.velocities[index] = Some(name.into());
            }
            if property == format!("CCSPlayerPawn.m_vecViewOffset.m_vec{axis}") {
                self.eye_offsets[index] = Some(name.into());
            }
        }
        if property.ends_with(".m_topology") {
            if let Ok(path) = serde_json::from_str::<Vec<u32>>(path) {
                if path.len() >= 2 && path.last() == Some(&0) {
                    self.topologies.insert(path[path.len() - 2], name.into());
                }
            }
        }
    }
    fn update(&mut self, name: &str, value: Option<&Vec<u8>>) {
        if let Some(value) = value {
            if let Some(existing) = self.fields.get_mut(name) {
                existing.clone_from(value);
            } else {
                self.remember_field(name);
                self.fields.insert(name.into(), value.clone());
            }
        } else {
            self.fields.remove(name);
        }
    }
    fn alias(&self, name: &Option<String>) -> Option<&[u8]> {
        self.get(name.as_deref()?)
    }
    fn get(&self, name: &str) -> Option<&[u8]> {
        self.fields.get(name).map(Vec::as_slice)
    }
    fn number(&self, name: &str) -> Option<f64> {
        number(self.get(name)?)
    }
    fn vector(&self, name: &str) -> Option<[f32; 3]> {
        let bytes = self.get(name)?;
        if bytes.len() != 13 || bytes[0] != 7 {
            return None;
        }
        Some([0, 1, 2].map(|i| f32::from_le_bytes(bytes[1 + i * 4..5 + i * 4].try_into().unwrap())))
    }
    fn body_number(&self, name: &str) -> Option<f64> {
        if let Some(key) = self.body_fields.get(name) {
            self.number(key)
        } else {
            self.number(&format!("CCSPlayerPawn.CBodyComponentBaseAnimGraph.{name}"))
        }
    }
    fn simulation_tick(&self) -> Option<i32> {
        let bytes = self.alias(&self.simulation_time)?;
        if bytes.len() != 5 || bytes[0] != 1 {
            return None;
        }
        i32::try_from(u32::from_le_bytes(bytes[1..].try_into().ok()?)).ok()
    }
    fn pose_number(&self, name: &str) -> Option<u32> {
        let field = match name {
            "m_nSerializePoseRecipeVersionAG2" => &self.pose_version,
            "m_nSerializePoseRecipeAG2ActiveSlot" => &self.pose_slot,
            _ => return None,
        };
        number(self.alias(field)?).map(|v| v as u32)
    }
    fn secondary_skeleton_handles(&self) -> Result<Vec<u32>> {
        let prefix = "pose/CCSPlayerPawn.CBodyComponentBaseAnimGraph.m_vecSecondarySkeletons/";
        let mut entries = self
            .fields
            .iter()
            .filter_map(|(name, value)| name.strip_prefix(prefix).map(|path| (path, value)))
            .map(|(path, value)| Ok((serde_json::from_str::<Vec<i32>>(path)?, value)))
            .collect::<Result<Vec<_>>>()?;
        entries.sort_by(|(a, _), (b, _)| a.len().cmp(&b.len()).then(a.cmp(b)));
        let (parent, length) = entries
            .first()
            .context("missing indexed secondary skeleton array")?;
        ensure!(
            length.len() == 5 && length[0] == 1,
            "invalid secondary skeleton array length"
        );
        let length = u32::from_le_bytes(length[1..].try_into()?) as usize;
        ensure!(
            length <= 1024 && entries.len() == length + 1,
            "incomplete indexed secondary skeleton array"
        );
        let mut ids = vec![None; length];
        for (path, value) in &entries[1..] {
            ensure!(
                path.len() == parent.len() + 1 && path.starts_with(parent),
                "ambiguous secondary skeleton array path"
            );
            let index = usize::try_from(*path.last().context("missing array index")?)?;
            ensure!(
                index < length && ids[index].is_none() && value.len() == 5 && value[0] == 1,
                "invalid secondary skeleton array element"
            );
            ids[index] = Some(u32::from_le_bytes(value[1..].try_into()?));
        }
        ids.into_iter()
            .map(|id| id.context("missing secondary skeleton array element"))
            .collect()
    }
    fn movement(&self, network_tick: u32) -> Option<Movement> {
        let field = |name: &str| self.get(self.movement_fields.get(name)?);
        let unsigned = |bytes: &[u8]| -> Option<u32> {
            match bytes.first()? {
                1 if bytes.len() == 5 => Some(u32::from_le_bytes(bytes[1..].try_into().ok()?)),
                4 if bytes.len() == 9 => {
                    u32::try_from(u64::from_le_bytes(bytes[1..].try_into().ok()?)).ok()
                }
                _ => None,
            }
        };
        let flags = unsigned(field("m_fFlags")?)?;
        let tick = field("m_nLastJumpTick")?;
        if tick.len() != 5 || tick[0] != 2 {
            return None;
        }
        let last_jump_tick = i32::from_le_bytes(tick[1..].try_into().ok()?);
        let last_jump_fraction = number(field("m_flLastJumpFrac")?)?;
        if !(0.0..1.0).contains(&last_jump_fraction) {
            return None;
        }
        let mut velocity = [0.0; 3];
        for (i, part) in velocity.iter_mut().enumerate() {
            *part = number(self.alias(&self.velocities[i])?)?;
        }
        let signed = |name| {
            let b = field(name)?;
            (b.len() == 5 && b[0] == 2).then(|| i32::from_le_bytes(b[1..].try_into().unwrap()))
        };
        Some(Movement {
            network_tick,
            flags,
            last_jump_tick,
            last_jump_fraction,
            velocity,
            origin: self.origin()?.map(f64::from),
            move_type: field("m_MoveType")
                .and_then(unsigned)
                .or_else(|| field("m_nActualMoveType").and_then(unsigned)),
            water_level: field("m_flWaterLevel")
                .and_then(number)
                .or_else(|| field("m_nWaterLevel").and_then(number)),
            ladder_surface: signed("m_nLadderSurfacePropIndex"),
        })
    }
    fn origin(&self) -> Option<[f32; 3]> {
        let mut out = [0.; 3];
        for (i, axis) in ["X", "Y", "Z"].into_iter().enumerate() {
            out[i] = (self.body_number(&format!("m_cell{axis}"))? * 512.
                + self.body_number(&format!("m_vec{axis}"))?
                - 16384.) as f32;
        }
        out.iter().all(|v| v.is_finite()).then_some(out)
    }
    fn transform(&self) -> Option<Transform> {
        let origin = self.origin()?;
        let angles = self.vector("CCSPlayerPawn.CBodyComponentBaseAnimGraph.m_angRotation")?;
        // Only the verified upright root convention is currently qualified.
        if !angles.iter().all(|v| v.is_finite()) || angles[0] != 0. || angles[2] != 0. {
            return None;
        }
        let half = angles[1].to_radians() * 0.5;
        let offsets = ["x", "y", "z"].map(|a| {
            self.body_number(&format!("m_flRootBoneOffset_{a}"))
                .map(|v| v as f32)
        });
        let offsets = [offsets[0]?, offsets[1]?, offsets[2]?];
        let scale = self.body_number("m_flScale")? as f32;
        // The captured attachment oracle qualifies upright unit-scale roots with
        // a recorded vertical offset only; horizontal offset space is not proven.
        if offsets[0] != 0.
            || offsets[1] != 0.
            || scale != 1.
            || !offsets.iter().all(|v| v.is_finite())
        {
            return None;
        }
        Some(Transform {
            position: [origin[0], origin[1], origin[2] + offsets[2]],
            rotation: [0., 0., half.sin(), half.cos()],
            scale,
        })
    }
}
fn number(bytes: &[u8]) -> Option<f64> {
    match bytes.first()? {
        0 if bytes.len() == 2 => Some(f64::from(bytes[1])),
        1 if bytes.len() == 5 => Some(u32::from_le_bytes(bytes[1..].try_into().ok()?) as f64),
        2 if bytes.len() == 5 => Some(i32::from_le_bytes(bytes[1..].try_into().ok()?) as f64),
        3 if bytes.len() == 5 => Some(f32::from_le_bytes(bytes[1..].try_into().ok()?) as f64),
        _ => None,
    }
    .filter(|v| v.is_finite())
}

#[derive(Default)]
struct RecordedContext {
    values: BTreeMap<String, Vec<u8>>,
}
type AssetDictionary = (String, Vec<String>, Vec<String>);
impl RecordedContext {
    fn tasks(&self) -> Result<Vec<&str>> {
        let mut tasks = vec![];
        for index in 0..256 {
            let Some(value) = self.values.get(&format!("AnimTaskTypes/{index}/name")) else {
                break;
            };
            ensure!(value.first() == Some(&5), "invalid task dictionary name");
            tasks.push(std::str::from_utf8(&value[1..])?);
        }
        ensure!(!tasks.is_empty(), "missing recorded task types");
        Ok(tasks)
    }
    fn assets(&self) -> Result<Vec<AssetDictionary>> {
        let mut out = vec![];
        for (key, value) in &self.values {
            if !key.starts_with("AnimAssetData/") || !key.ends_with("/name") {
                continue;
            }
            let data_key = format!("{}data", key.strip_suffix("name").unwrap());
            let Some(data) = self.values.get(&data_key) else {
                continue;
            };
            ensure!(
                value.first() == Some(&5) && data.first() == Some(&14),
                "invalid asset context"
            );
            let (names, masks) = dictionary(&data[1..])?;
            out.push((std::str::from_utf8(&value[1..])?.to_owned(), names, masks));
        }
        Ok(out)
    }
}

#[derive(Clone, Debug)]
pub struct Movement {
    pub network_tick: u32,
    pub flags: u32,
    pub last_jump_tick: i32,
    pub last_jump_fraction: f64,
    pub velocity: [f64; 3],
    pub origin: [f64; 3],
    pub move_type: Option<u32>,
    pub water_level: Option<f64>,
    pub ladder_surface: Option<i32>,
}
#[derive(Clone)]
pub struct PlayerFrame {
    /// Native simulation-time wire integer; rewind records use this same tick.
    /// Packet/pose phase eligibility still has to be established by the caller.
    pub simulation_tick: Option<i32>,
    pub movement: Option<Movement>,
    pub player_id: String,
    pub identity: String,
    pub identity_key: (i32, u32, u32),
    pub team: u32,
    pub eye: Option<[f64; 3]>,
    pub view: Option<[f64; 2]>,
    pub points: Vec<Option<[f64; 3]>>,
    pub capsules: Vec<super::line_of_sight::Capsule>,
    /// Capsule ordinal indexes this immutable model/set's native-order hitbox metadata.
    pub hitbox_set: Option<(u64, u32)>,
    /// Per-hitbox world transforms retained for server rewind interpolation.
    pub hitbox_transforms: Vec<Transform>,
}
#[derive(Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Coverage {
    /// Packets decoded and applied, but excluded from live-round measurement.
    pub skipped_nonlive_packets: u64,
    pub pawn_frames: u64,
    pub measured_frames: u64,
    pub measured_points: u64,
    pub body_attached_points: u64,
    pub hitbox_center_points: u64,
    pub hitbox_unavailable: BTreeMap<String, u64>,
    pub recipe_frames_with_unparsed_trailer: u64,
    pub unparsed_trailer_bits: u64,
    pub unavailable: BTreeMap<String, u64>,
}

fn bone_name_context(
    pawn: &Entity,
    entities: &BTreeMap<(i32, u32), Entity>,
    primary: &[String],
    assets: &[(String, Vec<String>, Vec<String>)],
    model_skeletons: &BTreeMap<u64, Vec<String>>,
) -> Result<Vec<String>> {
    // Original sendtable: CNetworkUtlVectorBase<CHandle<CBaseAnimGraph>>.
    // These are entity handles, not hashes or string-table resource indices.
    let handles = pawn.secondary_skeleton_handles()?;
    let mut secondary = vec![];
    for handle in handles {
        let entity = entities
            .get(&((handle & 0x3fff) as i32, handle >> 14))
            .filter(|entity| entity.get("$present").is_some())
            .context("missing referenced secondary skeleton entity")?;
        let bytes = entity
            .alias(&entity.model)
            .context("missing secondary entity model")?;
        ensure!(
            bytes.len() == 9 && bytes[0] == 4,
            "invalid secondary model resource handle"
        );
        let model = u64::from_le_bytes(bytes[1..].try_into()?);
        let paths = model_skeletons
            .get(&model)
            .with_context(|| format!("missing skeleton reference for secondary model {model}"))?;
        ensure!(
            paths.len() == 1,
            "ambiguous secondary model skeleton references"
        );
        let path = &paths[0];
        let matches = assets
            .iter()
            .filter(|(name, _, _)| name == path)
            .collect::<Vec<_>>();
        ensure!(
            matches.len() == 1,
            "missing or ambiguous recorded secondary skeleton"
        );
        secondary.push(matches[0]);
    }
    secondary.sort_by_key(|item| item.0.to_ascii_lowercase());
    let mut names = primary.to_vec();
    let mut unique: BTreeSet<_> = names.iter().cloned().collect();
    for (_, bones, _) in secondary {
        for bone in bones {
            if unique.insert(bone.clone()) {
                names.push(bone.clone());
            }
        }
    }
    Ok(names)
}

/// Anatomical bones, hand/foot attachments, and fixed actual-model hitbox centers.
/// Generic packet/pose data retains all bones, including world and weapon helpers.
pub fn is_body_attached_point(name: &str) -> bool {
    if name.starts_with("hitbox/") {
        return true;
    }
    if matches!(
        name,
        "pelvis" | "spine_0" | "spine_1" | "spine_2" | "spine_3" | "neck_0" | "head_0"
    ) {
        return true;
    }
    let Some(stem) = name.strip_suffix("_L").or_else(|| name.strip_suffix("_R")) else {
        return false;
    };
    if matches!(
        stem,
        "clavicle"
            | "arm_upper"
            | "arm_lower"
            | "hand"
            | "leg_upper"
            | "leg_lower"
            | "ankle"
            | "ball"
            | "attachHand"
            | "attachFoot"
    ) {
        return true;
    }
    let Some(finger) = stem.strip_prefix("finger_") else {
        return false;
    };
    let Some((digit, segment)) = finger.rsplit_once('_') else {
        return false;
    };
    matches!(digit, "middle" | "pinky" | "index" | "thumb" | "ring")
        && matches!(segment, "meta" | "0" | "1" | "2")
}

/// Decode each packet once and reconstruct each pawn once before sharing it with consumers.
pub fn visit(
    path: &Path,
    prepared: &Prepared,
    skeleton: &super::animation_pose::Skeleton,
    consume: impl FnMut(i32, &[PlayerFrame]) -> Result<()>,
) -> Result<Coverage> {
    visit_when(path, prepared, skeleton, |_| true, consume)
}

#[derive(Default)]
pub struct SceneOcclusion {
    /// Shared pre-smoke-system snapshot; only the CPU query API consumes it.
    pub cpu_smoke: super::smoke::timeline::Timeline,
    pub smoke_bounds: BTreeMap<i64, Option<super::line_of_sight::Bounds>>,
    pub uncertain: Vec<super::line_of_sight::Bounds>,
    pub unbounded: bool,
}
fn dynamic_occluder(class: &str) -> bool {
    [
        "Door",
        "Breakable",
        "DynamicProp",
        "PhysicsProp",
        "PhysProp",
        "FuncBrush",
        "MovingToggle",
    ]
    .iter()
    .any(|kind| class.contains(kind))
}
fn scene_occlusion(entities: &BTreeMap<(i32, u32), Entity>) -> SceneOcclusion {
    let mut scene = SceneOcclusion::default();
    for entity in entities
        .values()
        .filter(|e| dynamic_occluder(&e.class) && e.get("$present").is_some())
    {
        let field = |suffix: &str| -> Option<&str> {
            let mut values = entity.fields.keys().filter(|name| name.ends_with(suffix));
            let first = values.next()?;
            if values.next().is_some() {
                None
            } else {
                Some(first.as_str())
            }
        };
        if field(".m_nSolidType").and_then(|key| entity.number(key)) == Some(0.) {
            continue;
        }
        let bounds = (|| -> Option<super::line_of_sight::Bounds> {
            let model = field(".m_hModel")?;
            let prefix = model.strip_suffix("m_hModel")?;
            let mut origin = [0.; 3];
            for (i, axis) in ["X", "Y", "Z"].iter().enumerate() {
                origin[i] = entity.number(&format!("{prefix}m_cell{axis}"))? * 512. - 16384.
                    + entity.number(&format!("{prefix}m_vec{axis}"))?;
            }
            let min = entity.vector(field(".m_vecMins")?)?;
            let max = entity.vector(field(".m_vecMaxs")?)?;
            let scale = entity.number(&format!("{prefix}m_flScale"))?;
            if !scale.is_finite()
                || scale <= 0.
                || !(0..3).all(|i| min[i].is_finite() && max[i].is_finite() && min[i] <= max[i])
            {
                return None;
            }
            let radius = (0..3)
                .map(|i| f64::from(min[i].abs().max(max[i].abs())).powi(2))
                .sum::<f64>()
                .sqrt()
                * scale;
            if !radius.is_finite() || radius <= 0. || !origin.iter().all(|n| n.is_finite()) {
                return None;
            }
            Some(super::line_of_sight::Bounds {
                min: origin.map(|v| v - radius),
                max: origin.map(|v| v + radius),
            })
        })();
        if let Some(bounds) = bounds {
            scene.uncertain.push(bounds);
        } else {
            scene.unbounded = true;
        }
    }
    for ((id, _), entity) in entities
        .iter()
        .filter(|(_, e)| e.class == "CSmokeGrenadeProjectile" && e.get("$present").is_some())
    {
        let bounds = (entity.number("m_bDidSmokeEffect") == Some(1.))
            .then(|| {
                entity
                    .vector("m_vSmokeDetonationPos")
                    .and_then(super::smoke::bounds)
            })
            .flatten();
        scene
            .smoke_bounds
            .entry(i64::from(*id))
            .and_modify(|b| *b = None)
            .or_insert(bounds);
    }
    scene
}
pub fn visit_when(
    path: &Path,
    prepared: &Prepared,
    skeleton: &super::animation_pose::Skeleton,
    should_measure: impl FnMut(i32) -> bool,
    mut consume: impl FnMut(i32, &[PlayerFrame]) -> Result<()>,
) -> Result<Coverage> {
    visit_scene(
        path,
        prepared,
        skeleton,
        should_measure,
        |tick, players, _| consume(tick, players),
    )
}

/// Apply every source update; reconstruct poses only in the caller's shared measurement scope.
/// Excluded packets still reach consumers with an empty frame to break temporal continuity.
pub fn visit_scene(
    path: &Path,
    prepared: &Prepared,
    skeleton: &super::animation_pose::Skeleton,
    mut should_measure: impl FnMut(i32) -> bool,
    mut consume: impl FnMut(i32, &[PlayerFrame], &SceneOcclusion) -> Result<()>,
) -> Result<Coverage> {
    let mut entities: BTreeMap<(i32, u32), Entity> = BTreeMap::new();
    let mut context = RecordedContext::default();
    let mut bone_contexts: HashMap<(i32, u32), std::result::Result<Vec<String>, String>> =
        HashMap::new();
    let mut asset_context = vec![];
    let mut task_names: Vec<String> = vec![];
    let mut graphs: Vec<(u64, usize)> = vec![];
    let body_points: Vec<_> = prepared
        .point_names
        .iter()
        .map(|name| is_body_attached_point(name))
        .collect();
    let mut coverage = Coverage::default();
    let mut last_tick = None;
    let mut scene = SceneOcclusion::default();
    let mut scene_dirty = true;
    compact::visit(
        reader(path)?,
        |frame| {
            scene.cpu_smoke.begin_packet(&frame);
            let mut context_changed = false;
            for id in frame.changed {
                let field = &frame.fields[*id as usize];
                let smoke_changed = field.class == "CSmokeGrenadeProjectile"
                    && matches!(
                        field.name.as_str(),
                        "$present" | "m_bDidSmokeEffect" | "m_vSmokeDetonationPos"
                    );
                if smoke_changed {
                    scene_dirty = true;
                }
                if field.class != "CCSPlayerPawn"
                    && field.class != "AnimationContext"
                    && dynamic_occluder(&field.class)
                    && matches!(
                        field.name.rsplit('.').next(),
                        Some(
                            "$present"
                                | "m_hModel"
                                | "m_nSolidType"
                                | "m_vecMins"
                                | "m_vecMaxs"
                                | "m_flScale"
                                | "m_cellX"
                                | "m_cellY"
                                | "m_cellZ"
                                | "m_vecX"
                                | "m_vecY"
                                | "m_vecZ"
                        )
                    )
                {
                    scene_dirty = true;
                }
                if field.class == "AnimationContext" {
                    context_changed = true;
                    if let Some(value) = frame.values.get(id) {
                        context.values.insert(field.name.clone(), value.clone());
                    } else {
                        context.values.remove(&field.name);
                    }
                    continue;
                }
                if field.class != "CCSPlayerPawn"
                    && field.class != "CCSPlayerController"
                    && !dynamic_occluder(&field.class)
                    && !smoke_changed
                    && field.name != "$present"
                    && !field.name.ends_with(".m_hModel")
                {
                    continue;
                }
                if field.name.ends_with(".m_hModel") || field.name == "$present" {
                    bone_contexts.clear();
                }
                let key = (field.entity, field.serial);
                if field.name == "$present" && !frame.values.contains_key(id) {
                    entities.remove(&key);
                    bone_contexts.remove(&key);
                    continue;
                }
                if field.name.contains(".m_vecSecondarySkeletons/") {
                    bone_contexts.remove(&key);
                }
                let entity = entities.entry(key).or_default();
                entity.class.clone_from(&field.class);
                entity.update(&field.name, frame.values.get(id));
            }
            if context_changed {
                bone_contexts.clear();
                asset_context = context.assets()?;
                task_names = context.tasks()?.into_iter().map(str::to_owned).collect();
                graphs = asset_context
                    .iter()
                    .enumerate()
                    .filter(|(_, (n, _, _))| n.ends_with(".vnmgraph"))
                    .map(|(index, item)| Ok((animation_assets::resource_id(&item.0)?, index)))
                    .collect::<Result<_>>()?;
            }
            if last_tick == Some(frame.tick) {
                scene.cpu_smoke.update(&frame, |_, _| Ok(()))?;
                return Ok(());
            }
            last_tick = Some(frame.tick);
            // Entity values and animation dictionaries above must advance during freeze too.
            if !should_measure(frame.tick) {
                coverage.skipped_nonlive_packets += 1;
                consume(frame.tick, &[], &SceneOcclusion::default())?;
                scene.cpu_smoke.update(&frame, |_, _| Ok(()))?;
                return Ok(());
            }
            let skeleton_context = asset_context
                .iter()
                .find(|(n, _, _)| n == &skeleton.resource_name);
            let names: Vec<_> = task_names.iter().map(String::as_str).collect();
            let mut players = vec![];
            for (&(entity_id, serial), pawn) in &entities {
                if pawn.class != "CCSPlayerPawn" || pawn.get("$present").is_none() {
                    continue;
                }
                let alive = pawn.number("CCSPlayerPawn.m_lifeState") == Some(0.)
                    && pawn
                        .number("CCSPlayerPawn.m_iHealth")
                        .is_some_and(|h| h > 0.);
                if !alive {
                    continue;
                }
                let controller = pawn
                    .number("CCSPlayerPawn.m_hOriginalController")
                    .map(|n| n as u32);
                let Some(controller) = controller
                    .and_then(|h| entities.get(&((h & 0x3fff) as i32, h >> 14)))
                    .filter(|entity| {
                        entity.class == "CCSPlayerController" && entity.get("$present").is_some()
                    })
                else {
                    continue;
                };
                let Some(steam) = controller
                    .get("CCSPlayerController.m_steamID")
                    .filter(|v| v.len() == 9 && v[0] == 4)
                else {
                    continue;
                };
                let player_id = u64::from_le_bytes(steam[1..].try_into()?).to_string();
                if player_id == "0" {
                    continue;
                }
                let Some(team) = pawn
                    .number("CCSPlayerPawn.m_iTeamNum")
                    .filter(|n| *n == 2. || *n == 3.)
                else {
                    continue;
                };
                coverage.pawn_frames += 1;
                let view = pawn
                    .vector("CCSPlayerPawn.m_angEyeAngles")
                    .filter(|v| v.iter().all(|n| n.is_finite()) && v[0].abs() <= 90.)
                    .map(|v| [v[0] as f64, v[1] as f64]);
                let eye = pawn.origin().and_then(|origin| {
                    let mut point = [0.; 3];
                    for i in 0..3 {
                        let offset = number(pawn.alias(&pawn.eye_offsets[i])?)?;
                        point[i] = origin[i] as f64 + offset;
                    }
                    Some(point)
                });
                let mut capsules = vec![];
                let mut hitbox_transforms = vec![];
                let mut hitbox_set = None;
                let result = (|| -> Result<Vec<Option<[f64; 3]>>> {
                    let graph_handle = pawn
                        .get("CCSPlayerPawn.CBodyComponentBaseAnimGraph.m_hGraphDefinitionAG2")
                        .filter(|v| v.len() == 9 && v[0] == 4)
                        .context("missing graph resource handle")?;
                    let graph_id = u64::from_le_bytes(graph_handle[1..].try_into()?);
                    ensure!(
                        pawn.body_number("m_primaryGraphId") == Some((graph_id as u32) as f64),
                        "primary graph identity mismatch"
                    );
                    let matches: Vec<_> = graphs.iter().filter(|(id, _)| *id == graph_id).collect();
                    ensure!(
                        matches.len() == 1,
                        "missing or ambiguous primary graph dictionary"
                    );
                    let (_, resources, _) = &asset_context[matches[0].1];
                    let (_, bones, masks) =
                        skeleton_context.context("missing recorded skeleton dictionary")?;
                    ensure!(
                        *bones == skeleton.names,
                        "recorded skeleton bone order differs from installed skeleton"
                    );
                    let version = pawn
                        .pose_number("m_nSerializePoseRecipeVersionAG2")
                        .context("missing pose version")?;
                    let slot = pawn
                        .pose_number("m_nSerializePoseRecipeAG2ActiveSlot")
                        .context("missing pose slot")?;
                    let topology = pawn
                        .topologies
                        .get(&slot)
                        .and_then(|name| pawn.get(name))
                        .context("missing pose topology")?;
                    ensure!(topology.first() == Some(&14), "invalid topology bytes");
                    let dynamic = pawn.alias(&pawn.dynamic).context("missing pose payload")?;
                    let dynamic = compact::complete_pose_bytes(dynamic)?
                        .context("incomplete pose payload")?;
                    let bone_names =
                        bone_contexts.entry((entity_id, serial)).or_insert_with(|| {
                            bone_name_context(
                                pawn,
                                &entities,
                                &skeleton.names,
                                &asset_context,
                                &prepared.assets.model_skeletons,
                            )
                            .map_err(|e| e.to_string())
                        });
                    // Non-bone tasks do not require a secondary dictionary; FootIK rejects an empty context.
                    let bone_context_error = bone_names.as_ref().err().cloned();
                    let bone_names = bone_names.as_ref().map(Vec::as_slice).unwrap_or(&[]);
                    let recipe = animation_recipe::decode(
                        version,
                        &topology[1..],
                        dynamic,
                        &animation_recipe::Context {
                            task_names: &names,
                            bone_names,
                            resource_count: resources.len() as u32,
                            resource_bits: animation_recipe::index_bits(resources.len() as u32),
                            mask_count: masks.len() as u32,
                            mask_bits: animation_recipe::index_bits(masks.len() as u32),
                        },
                    )
                    .map_err(|error| {
                        if error.to_string() == "missing animation bone name context" {
                            anyhow::anyhow!(
                                "{}",
                                bone_context_error
                                    .as_deref()
                                    .unwrap_or("missing animation bone name context")
                            )
                        } else {
                            error
                        }
                    })?;
                    ensure!(
                        recipe.network_tick == frame.net_tick,
                        "pose timestamp differs from packet"
                    );
                    if recipe.remaining_bits() > 0 {
                        coverage.recipe_frames_with_unparsed_trailer += 1;
                        coverage.unparsed_trailer_bits += recipe.remaining_bits() as u64;
                    }
                    let pose =
                        skeleton.evaluate(&recipe, &prepared.assets.clips, resources, masks)?;
                    ensure!(!pose.is_additive, "final pose is additive");
                    let root = pawn
                        .transform()
                        .context("unsupported pawn root transform")?;
                    let mut points: Vec<_> = pose
                        .model
                        .iter()
                        .map(|bone| {
                            bone.map(|b| Transform::compose(root, b).position.map(f64::from))
                        })
                        .collect();
                    points.resize(prepared.point_names.len(), None);
                    let hitboxes =
                        (|| -> Result<(u64, u64)> {
                            let bytes = pawn
                                .alias(&pawn.model)
                                .context("missing pawn hitbox model")?;
                            ensure!(
                                bytes.len() == 9 && bytes[0] == 4,
                                "invalid pawn hitbox model"
                            );
                            let model = u64::from_le_bytes(bytes[1..].try_into()?);
                            let set = pawn
                                .body_number("m_nHitboxSet")
                                .context("missing pawn hitbox set")?;
                            ensure!(
                                set >= 0.0 && set <= u32::MAX as f64 && set.fract() == 0.0,
                                "invalid pawn hitbox set"
                            );
                            if let Ok((measured, transforms)) = prepared
                                .hitbox_points
                                .posed_capsules(model, set as u32, &pose.model, root)
                            {
                                capsules = measured;
                                hitbox_transforms = transforms;
                                hitbox_set = Some((model, set as u32));
                            }
                            prepared.hitbox_points.sample(
                                model,
                                set as u32,
                                &pose.model,
                                root,
                                &mut points,
                            )
                        })();
                    match hitboxes {
                        Ok((measured, missing)) => {
                            coverage.hitbox_center_points += measured;
                            if missing > 0 {
                                *coverage
                                    .hitbox_unavailable
                                    .entry("missing hitbox bone transform".into())
                                    .or_default() += missing;
                            }
                        }
                        Err(error) => {
                            *coverage
                                .hitbox_unavailable
                                .entry(error.to_string())
                                .or_default() += 1;
                        }
                    }
                    Ok(points)
                })();
                let points = match result {
                    Ok(points) => {
                        let count = points.iter().flatten().count();
                        coverage.body_attached_points += points
                            .iter()
                            .zip(&body_points)
                            .filter(|(point, attached)| point.is_some() && **attached)
                            .count()
                            as u64;
                        if count > 0 {
                            coverage.measured_frames += 1;
                            coverage.measured_points += count as u64;
                        }
                        points
                    }
                    Err(error) => {
                        *coverage.unavailable.entry(error.to_string()).or_default() += 1;
                        vec![]
                    }
                };
                let player = PlayerFrame {
                    simulation_tick: pawn.simulation_tick(),
                    movement: pawn.movement(frame.net_tick),
                    player_id,
                    identity: format!("{entity_id}:{serial}:{team}"),
                    team: team as u32,
                    identity_key: (entity_id, serial, team as u32),
                    eye,
                    view,
                    points,
                    capsules,
                    hitbox_set,
                    hitbox_transforms,
                };
                players.push(player);
            }
            if scene_dirty {
                let cpu_smoke = std::mem::take(&mut scene.cpu_smoke);
                scene = scene_occlusion(&entities);
                scene.cpu_smoke = cpu_smoke;
                scene_dirty = false;
            }
            consume(frame.tick, &players, &scene)?;
            // Server weapon simulation precedes the shared smoke-system journal step.
            scene.cpu_smoke.update(&frame, |_, _| Ok(()))?;
            Ok(())
        },
        |_| Ok(()),
    )?;
    Ok(coverage)
}

#[cfg(test)]
mod tests {
    #[test]
    fn rewind_tick_uses_raw_simulation_time_and_respects_removal() {
        let mut entity = Entity::default();
        let key = "pose/rawNetworkTime/CCSPlayerPawn.m_flSimulationTime/[1]";
        entity.update(
            "CCSPlayerPawn.m_flSimulationTime",
            Some(&vec![3, 0, 0, 0, 0]),
        );
        assert_eq!(entity.simulation_tick(), None);
        let mut raw = vec![1];
        raw.extend(103_520_u32.to_le_bytes());
        entity.update(key, Some(&raw));
        assert_eq!(entity.simulation_tick(), Some(103_520));
        entity.update(key, None);
        assert_eq!(entity.simulation_tick(), None);
        raw[1..].copy_from_slice(&u32::MAX.to_le_bytes());
        entity.update(key, Some(&raw));
        assert_eq!(entity.simulation_tick(), None);
    }

    #[test]
    fn smoke_bounds_require_recorded_initialization_and_follow_entity_removal() {
        let mut entity = Entity {
            class: "CSmokeGrenadeProjectile".into(),
            ..Default::default()
        };
        entity.update("$present", Some(&vec![0, 1]));
        let mut position = vec![7];
        for value in [-1473.8789_f32, 754.08203, -45.96875] {
            position.extend(value.to_le_bytes());
        }
        entity.update("m_vSmokeDetonationPos", Some(&position));
        let mut entities = BTreeMap::from([((7, 1), entity)]);
        assert!(scene_occlusion(&entities).smoke_bounds[&7].is_none());
        entities
            .get_mut(&(7, 1))
            .unwrap()
            .update("m_bDidSmokeEffect", Some(&vec![0, 1]));
        let scene = scene_occlusion(&entities);
        let bounds = scene.smoke_bounds[&7].as_ref().unwrap();
        assert!(bounds.intersects([-1473., 754., -46.], [-1470., 754., -46.]));
        assert!(!bounds.intersects([0.; 3], [10., 0., 0.]));
        entities
            .get_mut(&(7, 1))
            .unwrap()
            .update("m_vSmokeDetonationPos", None);
        assert!(scene_occlusion(&entities).smoke_bounds[&7].is_none());
        entities.get_mut(&(7, 1)).unwrap().update("$present", None);
        assert!(scene_occlusion(&entities).smoke_bounds.is_empty());
    }
    use super::*;
    #[test]
    fn movement_requires_signed_clock_and_qualified_velocity_without_filling_missing_values() {
        use parser::second_pass::variants::Variant;
        let mut entity = Entity::default();
        let mut put = |name: &str, value: Variant| {
            entity.update(name, Some(&compact::value(&value).unwrap()));
        };
        for axis in ["X", "Y", "Z"] {
            put(
                &format!("CCSPlayerPawn.CBodyComponentBaseAnimGraph.m_cell{axis}"),
                Variant::U32(32),
            );
            put(
                &format!("CCSPlayerPawn.CBodyComponentBaseAnimGraph.m_vec{axis}"),
                Variant::F32(0.0),
            );
        }
        put("pose/CCSPlayerPawn.m_fFlags/[6]", Variant::U32(65664));
        let jump = "pose/CCSPlayerPawn.CCSPlayer_MovementServices.m_nLastJumpTick/[1,34]";
        put(jump, Variant::I32(3634));
        put(
            "pose/CCSPlayerPawn.CCSPlayer_MovementServices.m_flLastJumpFrac/[1,35]",
            Variant::F32(0.171875),
        );
        for (axis, value) in [("X", 10.0), ("Y", -20.0), ("Z", 288.515625)] {
            put(
                &format!("pose/CCSPlayerPawn.m_vecVelocity.m_vec{axis}/[77]"),
                Variant::F32(value),
            );
        }
        put("pose/CCSPlayerPawn.m_MoveType/[80]", Variant::U64(2));
        put("pose/CCSPlayerPawn.m_flWaterLevel/[81]", Variant::F32(0.0));
        put(
            "pose/CCSPlayerPawn.CCSPlayer_MovementServices.m_nLadderSurfacePropIndex/[1,50]",
            Variant::I32(-1),
        );
        let m = entity.movement(3635).unwrap();
        assert_eq!(m.last_jump_tick, 3634);
        assert_eq!(m.network_tick, 3635);
        assert_eq!(m.last_jump_fraction, 0.171875);
        assert_eq!(m.velocity, [10.0, -20.0, 288.515625]);
        assert_eq!(m.move_type, Some(2));
        assert_eq!(m.ladder_surface, Some(-1));
        entity.update(jump, Some(&compact::value(&Variant::U32(7268)).unwrap()));
        assert!(
            entity.movement(3635).is_none(),
            "old unsigned clock must not be guessed"
        );
        entity.update(jump, Some(&compact::value(&Variant::I32(3634)).unwrap()));
        entity.update("pose/CCSPlayerPawn.m_vecVelocity.m_vecZ/[77]", None);
        assert!(
            entity.movement(3635).is_none(),
            "missing velocity is not zero"
        );
    }
    #[test]
    fn indexed_aliases_track_updates_removal_and_exact_eye_owner() {
        let mut entity = Entity::default();
        let version="pose/CCSPlayerPawn.CBodyComponentBaseAnimGraph.m_nSerializePoseRecipeVersionAG2/[9, 2]";
        let value = vec![1, 2, 0, 0, 0];
        entity.update(version, Some(&value));
        assert_eq!(
            entity.pose_number("m_nSerializePoseRecipeVersionAG2"),
            Some(2)
        );
        let pointer = entity.fields[version].as_ptr();
        entity.update(version, Some(&vec![1, 3, 0, 0, 0]));
        assert_eq!(entity.fields[version].as_ptr(), pointer);
        assert_eq!(
            entity.pose_number("m_nSerializePoseRecipeVersionAG2"),
            Some(3)
        );
        entity.update(version, None);
        assert_eq!(entity.pose_number("m_nSerializePoseRecipeVersionAG2"), None);
        let wrong = "pose/CCSPlayerPawn.CBodyComponentBaseAnimGraph.m_vecX/[9, 3]";
        entity.update(wrong, Some(&value));
        assert!(entity.alias(&entity.eye_offsets[0]).is_none());
        let eye = "pose/CCSPlayerPawn.m_vecViewOffset.m_vecX/[209]";
        entity.update(eye, Some(&value));
        assert_eq!(entity.alias(&entity.eye_offsets[0]), Some(value.as_slice()));
        let topology="pose/CCSPlayerPawn.CBodyComponentBaseAnimGraph.AnimGraph2SerializedPoseRecipeSlot_t.m_topology/[9, 32, 7, 0]";
        entity.update(topology, Some(&vec![14, 23]));
        assert_eq!(
            entity.topologies.get(&7).map(String::as_str),
            Some(topology)
        );
        assert!(!entity.topologies.contains_key(&6));
    }

    #[test]
    fn secondary_bone_dictionary_resolves_entity_handles_and_sorted_unique_names() {
        let assets = vec![
            (
                "z.vnmskel".into(),
                vec!["shared".into(), "z".into()],
                vec![],
            ),
            (
                "A.vnmskel".into(),
                vec!["a".into(), "shared".into()],
                vec![],
            ),
        ];
        let mut pawn = Entity::default();
        let mut put = |path: &str, value: u32| {
            let mut bytes = vec![1];
            bytes.extend(value.to_le_bytes());
            pawn.update(
                &format!(
                    "pose/CCSPlayerPawn.CBodyComponentBaseAnimGraph.m_vecSecondarySkeletons/{path}"
                ),
                Some(&bytes),
            );
        };
        put("[9, 29]", 2);
        put("[9, 29, 0]", (391 << 14) | 66);
        put("[9, 29, 1]", (660 << 14) | 276);
        let mut entities = BTreeMap::new();
        for (key, handle) in [
            ((66, 391), 7398392065489137260u64),
            ((276, 660), 10602312940084141493),
        ] {
            let mut entity = Entity::default();
            entity.update("$present", Some(&vec![0, 1]));
            let mut bytes = vec![4];
            bytes.extend(handle.to_le_bytes());
            entity.update("CBodyComponentBaseAnimGraph.m_hModel", Some(&bytes));
            entities.insert(key, entity);
        }
        let mapping = BTreeMap::from([
            (7398392065489137260, vec!["z.vnmskel".into()]),
            (10602312940084141493, vec!["A.vnmskel".into()]),
        ]);
        assert_eq!(
            bone_name_context(
                &pawn,
                &entities,
                &["root".into(), "shared".into()],
                &assets,
                &mapping
            )
            .unwrap(),
            ["root", "shared", "a", "z"]
        );
        entities.remove(&(66, 391));
        assert!(
            bone_name_context(&pawn, &entities, &["root".into()], &assets, &mapping)
                .unwrap_err()
                .to_string()
                .contains("referenced secondary")
        );
        pawn.fields.remove(
            "pose/CCSPlayerPawn.CBodyComponentBaseAnimGraph.m_vecSecondarySkeletons/[9, 29, 0]",
        );
        assert!(pawn.secondary_skeleton_handles().is_err());
        let mut plain = Entity::default();
        plain.fields.insert(
            "CCSPlayerPawn.CBodyComponentBaseAnimGraph.m_vecSecondarySkeletons".into(),
            vec![1, 0, 0, 0, 0],
        );
        assert!(plain.secondary_skeleton_handles().is_err());
    }

    #[test]
    fn body_point_semantics_exclude_world_and_weapon_helpers() {
        for name in [
            "pelvis",
            "head_0",
            "leg_upper_L",
            "ankle_R",
            "finger_index_1_L",
            "attachFoot_L",
        ] {
            assert!(is_body_attached_point(name), "{name}");
        }
        for name in [
            "root_motion",
            "attachWorld",
            "wpnAimIntent",
            "wpn",
            "wpnHand_L",
            "wpnPivot",
            "unknown_L",
        ] {
            assert!(!is_body_attached_point(name), "{name}");
        }
    }
    #[test]
    fn root_transform_requires_recorded_qualified_offsets_and_scale() {
        fn scalar(entity: &mut Entity, name: &str, value: f32) {
            let mut bytes = vec![3];
            bytes.extend(value.to_le_bytes());
            entity.fields.insert(
                format!("CCSPlayerPawn.CBodyComponentBaseAnimGraph.{name}"),
                bytes,
            );
        }
        let mut entity = Entity::default();
        for (axis, value) in [("X", 10.), ("Y", 20.), ("Z", 30.)] {
            scalar(&mut entity, &format!("m_cell{axis}"), 32.);
            scalar(&mut entity, &format!("m_vec{axis}"), value);
        }
        for (axis, value) in [("x", 0.), ("y", 0.), ("z", -0.8359909)] {
            scalar(&mut entity, &format!("m_flRootBoneOffset_{axis}"), value);
        }
        scalar(&mut entity, "m_flScale", 1.);
        let mut angles = vec![7];
        for value in [0f32, 90., 0.] {
            angles.extend(value.to_le_bytes());
        }
        entity.fields.insert(
            "CCSPlayerPawn.CBodyComponentBaseAnimGraph.m_angRotation".into(),
            angles,
        );
        let root = entity.transform().unwrap();
        let point = Transform::compose(
            root,
            Transform {
                position: [1., 2., 3.],
                ..Transform::IDENTITY
            },
        )
        .position;
        for (actual, expected) in point.into_iter().zip([8., 21., 32.164_01]) {
            assert!((actual - expected).abs() < 1e-5);
        }
        scalar(&mut entity, "m_flRootBoneOffset_x", 1.);
        assert!(entity.transform().is_none());
        scalar(&mut entity, "m_flRootBoneOffset_x", 0.);
        scalar(&mut entity, "m_flScale", 2.);
        assert!(entity.transform().is_none());
        scalar(&mut entity, "m_flScale", 1.);
        entity
            .fields
            .remove("CCSPlayerPawn.CBodyComponentBaseAnimGraph.m_flRootBoneOffset_z");
        assert!(entity.transform().is_none());
    }

    #[test]
    fn dictionary_rejects_truncation_and_keeps_masks_separate() {
        let mut data = 1u32.to_le_bytes().to_vec();
        data.extend(1u32.to_le_bytes());
        data.extend(b"pelvis\0");
        data.extend(1u32.to_le_bytes());
        data.extend(b"UpperBody\0");
        assert_eq!(
            dictionary(&data).unwrap(),
            (vec!["pelvis".into()], vec!["UpperBody".into()])
        );
        for end in 0..data.len() {
            if end != 15 {
                assert!(dictionary(&data[..end]).is_err());
            }
        }
    }
}
