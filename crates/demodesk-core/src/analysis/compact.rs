//! Lossless packet state changes, independent of rules and player selection.
//! Values retain native IEEE-754 bits. Missing fields are removed, never filled.
use super::{Artifact, Contract, Source};
use anyhow::{ensure, Context, Result};
use parser::second_pass::{parser_settings::SecondPassParser, variants::Variant};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap},
    io::{Read, Write},
};

const MAGIC: &[u8; 8] = b"DDSTATE7";
const LIMIT: usize = 16 * 1024 * 1024;
// ponytail: bounded cumulative definitions; recycle retired IDs if long matches outgrow this.
const MAX_FIELDS: u32 = 5_000_000;
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Metadata {
    pub tick_rate: f64,
    pub sample_step_ticks: u32,
    pub precision: String,
    pub body_bounds: String,
    pub static_geometry: String,
    pub smoke_opacity: String,
    pub pose_time: String,
}
pub fn contract() -> Contract {
    Contract {
        module: "match-state".into(),
        schema_version: 7,
        implementation_version: "0.24.0".into(),
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Field {
    pub entity: i32,
    pub serial: u32,
    pub class: String,
    pub name: String,
}
/// Current rule-independent source profile, also used by cross-version lossless audits.
pub fn retains_field(field: &Field) -> bool {
    field.class == "AnimationContext"
        || field.name == "$present"
        || field.name.starts_with("pose")
        || field.name.starts_with("smokeVoxel/")
        || super::scene::relevant_property(&field.name)
}
/// Native value envelope: 0 bool, 1 u32, 2 i32, 3 f32, 4 u64,
/// 5 UTF-8, 6 two f32, 7 three f32; 9 u32 array, 10 u64 array,
/// 14 byte-valued u32 array; 8/11/12/13 JSON for uncommon compound variants.
pub fn value(value: &Variant) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    match value {
        Variant::Bool(v) => out.extend([0, u8::from(*v)]),
        Variant::U32(v) => {
            out.push(1);
            out.extend(v.to_le_bytes());
        }
        Variant::I32(v) => {
            out.push(2);
            out.extend(v.to_le_bytes());
        }
        Variant::F32(v) => {
            out.push(3);
            out.extend(v.to_bits().to_le_bytes());
        }
        Variant::U64(v) => {
            out.push(4);
            out.extend(v.to_le_bytes());
        }
        Variant::String(v) => {
            out.push(5);
            out.extend(v.as_bytes());
        }
        Variant::VecXY(v) => {
            out.push(6);
            for n in v {
                out.extend(n.to_bits().to_le_bytes());
            }
        }
        Variant::VecXYZ(v) => {
            out.push(7);
            for n in v {
                out.extend(n.to_bits().to_le_bytes());
            }
        }
        Variant::StringVec(v) => {
            out.push(8);
            serde_json::to_writer(&mut out, v)?;
        }
        Variant::U32Vec(v) if v.iter().all(|n| *n <= 255) => {
            out.push(14);
            out.extend(v.iter().map(|n| *n as u8));
        }
        Variant::U32Vec(v) => {
            out.push(9);
            for n in v {
                out.extend(n.to_le_bytes());
            }
        }
        Variant::U64Vec(v) => {
            out.push(10);
            for n in v {
                out.extend(n.to_le_bytes());
            }
        }
        Variant::Stickers(v) => {
            out.push(11);
            serde_json::to_writer(&mut out, v)?;
        }
        Variant::InputHistory(v) => {
            out.push(12);
            serde_json::to_writer(&mut out, v)?;
        }
        Variant::UserCmdSubtickMoves(v) => {
            out.push(13);
            serde_json::to_writer(&mut out, v)?;
        }
    }
    Ok(out)
}
fn number(out: &mut impl Write, mut n: u32) -> Result<()> {
    while n >= 128 {
        out.write_all(&[(n as u8) | 128])?;
        n >>= 7;
    }
    out.write_all(&[n as u8])?;
    Ok(())
}
fn read_number(input: &mut impl Read) -> Result<u32> {
    let mut position = 0;
    crate::demo_readiness::varint(input, &mut position)?.context("invalid compact integer")
}
fn blob(out: &mut impl Write, bytes: &[u8]) -> Result<()> {
    ensure!(bytes.len() <= LIMIT, "oversized compact value");
    number(out, bytes.len() as u32)?;
    out.write_all(bytes)?;
    Ok(())
}
fn read_blob(input: &mut impl Read) -> Result<Vec<u8>> {
    let length = read_number(input)? as usize;
    ensure!(length <= LIMIT, "oversized compact value");
    let mut bytes = vec![0; length];
    input.read_exact(&mut bytes)?;
    Ok(bytes)
}
struct Previous {
    id: u32,
    bytes: Vec<u8>,
}
pub struct Writer<W: Write> {
    output: W,
    fields: HashMap<(i32, u32, String), Previous>,
    selected: HashMap<u32, String>,
    strings: HashMap<String, u32>,
    classes: Vec<bool>,
    frames: u32,
    last_tick: Option<i32>,
    next_id: u32,
    context_names: std::collections::BTreeSet<String>,
    changes: BTreeMap<u32, Vec<u8>>,
}
impl<W: Write> Writer<W> {
    fn animation_context(
        &mut self,
        context: &parser::first_pass::animation_strings::AnimationStrings,
    ) -> Result<()> {
        let mut current = std::collections::BTreeSet::new();
        for table in &context.tables {
            // Baselines are already applied to entity fields; only these dictionaries remain external.
            if !matches!(table.name.as_str(), "AnimTaskTypes" | "AnimAssetData") {
                continue;
            }
            for (index, (key, bytes)) in &table.entries {
                for (suffix, value) in [
                    ("name", Variant::String(key.clone())),
                    (
                        "data",
                        Variant::U32Vec(bytes.iter().map(|v| u32::from(*v)).collect()),
                    ),
                ] {
                    let name = format!("{}/{index}/{suffix}", table.name);
                    current.insert(name.clone());
                    self.field(
                        Field {
                            entity: i32::MAX,
                            serial: 0,
                            class: "AnimationContext".into(),
                            name,
                        },
                        &value,
                    )?;
                }
            }
        }
        for name in self
            .context_names
            .difference(&current)
            .cloned()
            .collect::<Vec<_>>()
        {
            self.remove(&(i32::MAX, 0, name))?;
        }
        self.context_names = current;
        Ok(())
    }
    pub fn new(mut output: W, source: Source, tick_rate: f64) -> Result<Self> {
        ensure!(
            tick_rate.is_finite() && tick_rate > 0.0,
            "invalid compact clock"
        );
        output.write_all(MAGIC)?;
        blob(
            &mut output,
            &serde_json::to_vec(&Artifact {
                contract: contract(),
                source,
                dependencies: vec![],
                data: Metadata {
                    tick_rate,
                    sample_step_ticks: 1,
                    precision: "Native f32/u32/u64 bits; no rounding or interpolation".into(),
                    body_bounds: "Network collision/pose fields only; server hitboxes unavailable"
                        .into(),
                    static_geometry: "Unavailable: no verified historical map content resource"
                        .into(),
                    smoke_opacity: "Unavailable: network voxel bytes are not visual opacity".into(),
                    pose_time: "Network pose fields; historical bone timing unqualified".into(),
                },
            })?,
        )?;
        Ok(Self {
            output,
            fields: HashMap::new(),
            selected: HashMap::new(),
            strings: HashMap::new(),
            classes: vec![],
            frames: 0,
            last_tick: None,
            next_id: 0,
            context_names: Default::default(),
            changes: BTreeMap::new(),
        })
    }
    fn field(&mut self, field: Field, raw: &Variant) -> Result<()> {
        let key = (field.entity, field.serial, field.name.clone());
        if let Some(previous) = self.fields.get_mut(&key) {
            let bytes = value(raw)?;
            if previous.bytes == bytes {
                return Ok(());
            }
            Self::change(&mut self.changes, previous.id, &previous.bytes, &bytes)?;
            previous.bytes = bytes;
        } else {
            ensure!(self.next_id < MAX_FIELDS, "too many compact fields");
            let id = self.next_id;
            self.next_id += 1;
            let mut definition = vec![];
            number(
                &mut definition,
                u32::try_from(field.entity).context("negative field entity")?,
            )?;
            number(&mut definition, field.serial)?;
            for value in [&field.class, &field.name] {
                let id = if let Some(id) = self.strings.get(value) {
                    *id
                } else {
                    let id = u32::try_from(self.strings.len()).context("too many field strings")?;
                    self.output.write_all(&[4])?;
                    blob(&mut self.output, value.as_bytes())?;
                    self.strings.insert(value.clone(), id);
                    id
                };
                number(&mut definition, id)?;
            }
            self.output.write_all(&[1])?;
            blob(&mut self.output, &definition)?;
            let bytes = value(raw)?;
            Self::change(&mut self.changes, id, &[], &bytes)?;
            self.fields.insert(key, Previous { id, bytes });
        }
        Ok(())
    }
    fn change(
        changes: &mut BTreeMap<u32, Vec<u8>>,
        id: u32,
        previous: &[u8],
        bytes: &[u8],
    ) -> Result<()> {
        let mut out = Vec::new();
        number(&mut out, bytes.len() as u32)?;
        let xor: Vec<_> = bytes
            .iter()
            .enumerate()
            .map(|(i, b)| b ^ previous.get(i).copied().unwrap_or(0))
            .collect();
        let begin = xor.iter().position(|b| *b != 0).unwrap_or(xor.len());
        let end = xor.iter().rposition(|b| *b != 0).map_or(begin, |i| i + 1);
        number(&mut out, begin as u32)?;
        blob(&mut out, &xor[begin..end])?;
        changes.insert(id, out);
        Ok(())
    }
    fn frame(&mut self, tick: i32, net_tick: u32) -> Result<()> {
        let mut changes = Vec::new();
        let mut previous = 0;
        for (id, value) in &self.changes {
            number(&mut changes, *id - previous)?;
            changes.extend_from_slice(value);
            previous = *id;
        }
        self.output.write_all(&[2])?;
        self.output.write_all(&tick.to_le_bytes())?;
        self.output.write_all(&net_tick.to_le_bytes())?;
        blob(&mut self.output, &changes)?;
        self.frames += 1;
        self.last_tick = Some(tick);
        Ok(())
    }
    fn pose(
        &mut self,
        entity: &parser::second_pass::entities::Entity,
        class: &str,
        path: &[i32],
        arrays: &mut std::collections::BTreeSet<Vec<i32>>,
    ) -> Result<()> {
        let Some((name, raw)) = entity.pose_fields.get(path) else {
            return Ok(());
        };
        // Group only byte-vector sendtable fields; keep every other pose field unchanged.
        if name.ends_with("m_SerializePoseRecipeAG2Dynamic") {
            let parent = path.get(..path.len().saturating_sub(1)).unwrap_or_default();
            let array = if entity.pose_array_lengths.contains_key(path) {
                Some(path)
            } else if entity.pose_array_lengths.contains_key(parent) {
                Some(parent)
            } else {
                None
            };
            if let Some(array) = array {
                arrays.insert(array.to_vec());
                return Ok(());
            }
        }
        self.field(
            Field {
                entity: entity.entity_id,
                serial: entity.serial,
                class: class.into(),
                name: format!("pose/{name}/{path:?}"),
            },
            raw,
        )
    }
    fn array(
        &mut self,
        entity: &parser::second_pass::entities::Entity,
        class: &str,
        path: &[i32],
    ) -> Result<()> {
        let (name, length) = entity
            .pose_array_lengths
            .get(path)
            .context("missing pose array length")?;
        ensure!(*length <= 65536, "oversized pose byte array");
        let bitmap = (*length as usize).div_ceil(8);
        let mut bytes = length.to_le_bytes().to_vec();
        bytes.resize(4 + bitmap + *length as usize, 0);
        let mut child = path.to_vec();
        child.push(0);
        for index in 0..*length {
            *child.last_mut().expect("appended above") = index as i32;
            if let Some((child_name, value)) = entity.pose_fields.get(&child) {
                ensure!(child_name == name, "pose array field identity mismatch");
                let Variant::U32(value) = value else {
                    anyhow::bail!("unsupported pose byte type")
                };
                ensure!(*value <= 255, "invalid pose byte");
                bytes[4 + index as usize / 8] |= 1 << (index % 8);
                bytes[4 + bitmap + index as usize] = *value as u8;
            }
        }
        self.field(
            Field {
                entity: entity.entity_id,
                serial: entity.serial,
                class: class.into(),
                name: format!("pose-bytes/{name}/{path:?}"),
            },
            &Variant::U32Vec(bytes.into_iter().map(u32::from).collect()),
        )
    }

    pub fn push(&mut self, parser: &SecondPassParser<'_>) -> Result<()> {
        if parser.tick < 0 {
            return Ok(());
        }
        ensure!(
            self.last_tick.is_none_or(|t| parser.tick >= t),
            "compact ticks decreased"
        );
        if self.classes.is_empty() {
            self.classes = parser
                .cls_by_id
                .iter()
                .map(|c| {
                    super::scene::relevant_class(&c.name)
                        || super::scene::animation_serializer(&c.serializer)
                })
                .collect();
            self.selected = parser
                .prop_controller
                .id_to_name
                .iter()
                .filter(|(_, name)| {
                    super::scene::relevant_property(name)
                        && !parser::second_pass::entities::is_pose_field(name)
                })
                .map(|(id, name)| (*id, name.clone()))
                .collect();
            // Byte-array slots carry independent presence; absent slots stay absent.
            use parser::first_pass::prop_controller::{SMOKE_VOXELS_ID, SMOKE_VOXELS_LIMIT};
            for i in 0..SMOKE_VOXELS_LIMIT {
                self.selected
                    .insert(SMOKE_VOXELS_ID + i, format!("smokeVoxel/{i}"));
            }
        }
        self.changes.clear();
        if parser.animation_strings.dirty || self.frames == 0 {
            self.animation_context(&parser.animation_strings)?;
        }
        let updates = parser
            .analysis_changes
            .as_ref()
            .context("missing analysis change capture")?;
        let mut lifecycle = updates.lifecycle.clone();
        if self.frames == 0 {
            lifecycle.extend(parser.entities.iter().flatten().map(|e| e.entity_id));
        }
        for id in &lifecycle {
            let current = parser.entities.get(*id as usize).and_then(|e| e.as_ref());
            let removed: Vec<_> = self
                .fields
                .keys()
                .filter(|(entity, _, _)| entity == id)
                .cloned()
                .collect();
            for key in removed {
                self.remove(&key)?;
            }
            if let Some(entity) = current.filter(|e| {
                self.classes
                    .get(e.cls_id as usize)
                    .copied()
                    .unwrap_or(false)
            }) {
                let class = &parser.cls_by_id[entity.cls_id as usize].name;
                self.field(
                    Field {
                        entity: *id,
                        serial: entity.serial,
                        class: class.clone(),
                        name: "$present".into(),
                    },
                    &Variant::Bool(true),
                )?;
                for (prop, raw) in &entity.props {
                    if let Some(name) = self.selected.get(prop).cloned() {
                        self.field(
                            Field {
                                entity: *id,
                                serial: entity.serial,
                                class: class.clone(),
                                name,
                            },
                            raw,
                        )?;
                    }
                }
                let mut arrays = std::collections::BTreeSet::new();
                for path in entity.pose_fields.keys() {
                    self.pose(entity, class, path, &mut arrays)?;
                }
                for path in arrays {
                    self.array(entity, class, &path)?;
                }
            }
        }
        for (id, prop) in &updates.properties {
            if lifecycle.contains(id) {
                continue;
            }
            let Some(entity) = parser.entities.get(*id as usize).and_then(|e| e.as_ref()) else {
                continue;
            };
            if !self
                .classes
                .get(entity.cls_id as usize)
                .copied()
                .unwrap_or(false)
            {
                continue;
            }
            if let (Some(name), Some(raw)) =
                (self.selected.get(prop).cloned(), entity.props.get(prop))
            {
                self.field(
                    Field {
                        entity: *id,
                        serial: entity.serial,
                        class: parser.cls_by_id[entity.cls_id as usize].name.clone(),
                        name,
                    },
                    raw,
                )?;
            }
        }
        let mut changed_arrays =
            std::collections::BTreeMap::<i32, std::collections::BTreeSet<Vec<i32>>>::new();
        for (id, path) in &updates.poses {
            if lifecycle.contains(id) {
                continue;
            }
            let Some(entity) = parser.entities.get(*id as usize).and_then(|e| e.as_ref()) else {
                continue;
            };
            if !self
                .classes
                .get(entity.cls_id as usize)
                .copied()
                .unwrap_or(false)
            {
                continue;
            }
            let class = &parser.cls_by_id[entity.cls_id as usize].name;
            self.pose(entity, class, path, changed_arrays.entry(*id).or_default())?;
        }
        for (id, path, name) in &updates.pose_removals {
            let Some(entity) = parser.entities.get(*id as usize).and_then(|e| e.as_ref()) else {
                continue;
            };
            if entity.pose_fields.contains_key(path) {
                continue;
            }
            let key = (*id, entity.serial, format!("pose/{name}/{path:?}"));
            if self.fields.contains_key(&key) {
                self.remove(&key)?;
            }
            let key = (*id, entity.serial, format!("pose-bytes/{name}/{path:?}"));
            if self.fields.contains_key(&key) {
                self.remove(&key)?;
            }
        }
        for (id, arrays) in changed_arrays {
            let entity = parser.entities[id as usize]
                .as_ref()
                .context("missing changed array entity")?;
            let class = &parser.cls_by_id[entity.cls_id as usize].name;
            for path in arrays {
                self.array(entity, class, &path)?;
            }
        }
        self.user_commands(parser.tick, parser.net_tick, &updates.user_cmds)?;
        self.frame(parser.tick, parser.net_tick)
    }
    fn user_commands(
        &mut self,
        tick: i32,
        net_tick: u32,
        commands: &[parser::second_pass::parser_settings::AnalysisUserCmd],
    ) -> Result<()> {
        use std::fmt::Write as _;
        for command in commands {
            let encoded = command.protobuf.as_ref().map(|bytes| {
                let mut hex = String::with_capacity(bytes.len() * 2);
                for byte in bytes {
                    write!(&mut hex, "{byte:02x}").unwrap();
                }
                hex
            });
            let event = serde_json::json!({
                "event_name": "analysis_user_cmd", "tick": tick, "net_tick": net_tick,
                "ordinal": command.ordinal, "player_slot": command.player_slot,
                "command_number": command.command_number,
                "server_tick_executed": command.server_tick_executed,
                "client_tick": command.client_tick, "protobuf_hex": encoded,
                "invalid": command.invalid,
            });
            self.output.write_all(&[3])?;
            blob(&mut self.output, &serde_json::to_vec(&event)?)?;
        }
        Ok(())
    }
    fn remove(&mut self, key: &(i32, u32, String)) -> Result<()> {
        let previous = self.fields.remove(key).context("undefined removed field")?;
        self.changes.insert(previous.id, vec![0]);
        Ok(())
    }
    pub fn events(&mut self, events: &[parser::second_pass::game_events::GameEvent]) -> Result<()> {
        for event in events {
            self.output.write_all(&[3])?;
            blob(&mut self.output, &serde_json::to_vec(event)?)?;
        }
        Ok(())
    }
    pub fn finish(mut self) -> Result<W> {
        self.output.write_all(&[0])?;
        number(&mut self.output, self.frames)?;
        self.output
            .write_all(&self.last_tick.unwrap_or(-1).to_le_bytes())?;
        self.output.flush()?;
        Ok(self.output)
    }
}
pub struct Frame<'a> {
    pub tick: i32,
    pub net_tick: u32,
    pub fields: &'a [Field],
    pub values: &'a BTreeMap<u32, Vec<u8>>,
    pub changed: &'a [u32],
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub packets: u32,
    pub first_tick: Option<i32>,
    pub last_tick: Option<i32>,
    pub events: u32,
    pub fields: usize,
    pub encoded_field_bytes: BTreeMap<String, u64>,
}
/// One decoded state shared by every consumer. Repeated packet ticks are explicit.
pub fn visit(
    mut input: impl Read,
    mut consume: impl FnMut(Frame<'_>) -> Result<()>,
    mut event: impl FnMut(serde_json::Value) -> Result<()>,
) -> Result<(Artifact<Metadata>, Summary)> {
    let mut magic = [0; 8];
    input.read_exact(&mut magic)?;
    let schema = if &magic == MAGIC {
        7
    } else if &magic == b"DDSTATE6" {
        6
    } else if &magic == b"DDSTATE5" {
        5
    } else if &magic == b"DDSTATE4" {
        4
    } else if &magic == b"DDSTATE3" {
        3
    } else if &magic == b"DDSTATE2" {
        2
    } else {
        anyhow::bail!("unsupported compact envelope")
    };
    let header: Artifact<Metadata> = serde_json::from_slice(&read_blob(&mut input)?)?;
    header.require("match-state", schema)?;
    ensure!(
        header.data.tick_rate.is_finite()
            && header.data.tick_rate > 0.0
            && header.data.sample_step_ticks == 1,
        "invalid compact clock"
    );
    let mut fields: Vec<Field> = vec![];
    let mut strings: Vec<String> = vec![];
    let mut values = BTreeMap::<u32, Vec<u8>>::new();
    let mut summary = Summary {
        packets: 0,
        first_tick: None,
        last_tick: None,
        events: 0,
        fields: 0,
        encoded_field_bytes: BTreeMap::new(),
    };
    loop {
        let mut tag = [0];
        input.read_exact(&mut tag)?;
        match tag[0] {
            0 => {
                let count = read_number(&mut input)?;
                let mut tick = [0; 4];
                input.read_exact(&mut tick)?;
                ensure!(
                    count == summary.packets
                        && i32::from_le_bytes(tick) == summary.last_tick.unwrap_or(-1),
                    "compact end mismatch"
                );
                ensure!(input.read(&mut tag)? == 0, "data after compact end");
                summary.fields = fields.len();
                return Ok((header, summary));
            }
            1 => {
                ensure!(
                    fields.len() < MAX_FIELDS as usize,
                    "too many compact fields"
                );
                let definition = read_blob(&mut input)?;
                let field: Field = if schema >= 7 {
                    let mut cursor = definition.as_slice();
                    let entity =
                        i32::try_from(read_number(&mut cursor)?).context("invalid field entity")?;
                    let serial = read_number(&mut cursor)?;
                    let class = strings
                        .get(read_number(&mut cursor)? as usize)
                        .context("undefined field class")?
                        .clone();
                    let name = strings
                        .get(read_number(&mut cursor)? as usize)
                        .context("undefined field name")?
                        .clone();
                    ensure!(cursor.is_empty(), "trailing field definition bytes");
                    Field {
                        entity,
                        serial,
                        class,
                        name,
                    }
                } else {
                    serde_json::from_slice(&definition)?
                };
                ensure!(
                    field.entity >= 0 && !field.class.is_empty() && !field.name.is_empty(),
                    "invalid compact field"
                );
                fields.push(field);
            }
            2 => {
                let mut tick = [0; 4];
                input.read_exact(&mut tick)?;
                let tick = i32::from_le_bytes(tick);
                let mut net = [0; 4];
                input.read_exact(&mut net)?;
                ensure!(
                    tick >= 0 && summary.last_tick.is_none_or(|t| tick >= t),
                    "compact ticks decreased"
                );
                let changes = read_blob(&mut input)?;
                let mut cursor = changes.as_slice();
                let mut changed = Vec::new();
                let mut previous_id = 0u32;
                while !cursor.is_empty() {
                    let before = cursor.len();
                    let encoded_id = read_number(&mut cursor)?;
                    let id = if schema >= 4 {
                        ensure!(
                            changed.is_empty() || encoded_id > 0,
                            "duplicate compact field update"
                        );
                        previous_id
                            .checked_add(encoded_id)
                            .context("compact field ID overflow")?
                    } else {
                        encoded_id
                    };
                    previous_id = id;
                    ensure!((id as usize) < fields.len(), "undefined compact field");
                    changed.push(id);
                    let length = read_number(&mut cursor)? as usize;
                    ensure!(length <= LIMIT, "oversized compact state");
                    if length == 0 {
                        ensure!(
                            values.remove(&id).is_some(),
                            "missing removed compact field"
                        );
                        continue;
                    }
                    let begin = read_number(&mut cursor)? as usize;
                    let xor = read_blob(&mut cursor)?;
                    let bytes = values.entry(id).or_default();
                    if begin == u32::MAX as usize {
                        ensure!(schema >= 3, "absolute values require compact schema 3");
                        ensure!(xor.len() == length, "invalid absolute compact value");
                        *bytes = xor;
                    } else {
                        ensure!(
                            begin <= length && xor.len() <= length - begin,
                            "invalid compact delta"
                        );
                        bytes.resize(length, 0);
                        for (a, b) in bytes[begin..].iter_mut().zip(xor) {
                            *a ^= b;
                        }
                    }
                    validate_value(bytes)?;
                    let field = &fields[id as usize];
                    if field.name.starts_with("pose-bytes/") {
                        ensure!(schema >= 6, "pose bytes require schema 6");
                        pose_layout(bytes)?;
                    }
                    if field.name.starts_with("pose-array/") {
                        ensure!(schema >= 5, "pose arrays require schema 5");
                        pose_array(bytes)?;
                    }
                    let name = if field.name.starts_with("smokeVoxel/") {
                        "smokeVoxel"
                    } else if field.name.starts_with("pose/") {
                        field.name.split('/').nth(1).unwrap_or("poseRecipe")
                    } else {
                        &field.name
                    };
                    *summary.encoded_field_bytes.entry(name.into()).or_default() +=
                        (before - cursor.len()) as u64;
                }
                consume(Frame {
                    tick,
                    net_tick: u32::from_le_bytes(net),
                    fields: &fields,
                    values: &values,
                    changed: &changed,
                })?;
                summary.first_tick.get_or_insert(tick);
                summary.last_tick = Some(tick);
                summary.packets += 1;
            }
            3 => {
                event(serde_json::from_slice(&read_blob(&mut input)?)?)?;
                summary.events += 1;
            }
            4 => {
                ensure!(
                    schema >= 7 && strings.len() < 1_000_000,
                    "invalid field string record"
                );
                let value = String::from_utf8(read_blob(&mut input)?)?;
                ensure!(!value.is_empty(), "empty field string");
                strings.push(value);
            }
            _ => anyhow::bail!("unknown compact record"),
        }
    }
}
/// Presence bitmap distinguishes missing slots from actual zero bytes.
pub fn pose_bytes(bytes: &[u8]) -> Result<(u32, BTreeMap<u32, u32>)> {
    let (bitmap, payload) = pose_layout(bytes)?;
    let values = payload
        .iter()
        .enumerate()
        .filter(|(i, _)| bitmap[i / 8] & (1 << (i % 8)) != 0)
        .map(|(i, v)| (i as u32, u32::from(*v)))
        .collect();
    Ok((payload.len() as u32, values))
}

/// Borrow an entirely received payload; never replace missing network bytes with zero.
pub fn complete_pose_bytes(bytes: &[u8]) -> Result<Option<&[u8]>> {
    let (bitmap, payload) = pose_layout(bytes)?;
    let present: usize = bitmap.iter().map(|v| v.count_ones() as usize).sum();
    Ok((present == payload.len()).then_some(payload))
}
fn pose_layout(bytes: &[u8]) -> Result<(&[u8], &[u8])> {
    ensure!(bytes.len() >= 5 && bytes[0] == 14, "invalid pose bytes");
    let length = u32::from_le_bytes(bytes[1..5].try_into().expect("four-byte length")) as usize;
    ensure!(length <= 65536, "oversized pose bytes");
    let bitmap_length = length.div_ceil(8);
    ensure!(
        bytes.len() == 5 + bitmap_length + length,
        "truncated pose bytes"
    );
    let bitmap = &bytes[5..5 + bitmap_length];
    let payload = &bytes[5 + bitmap_length..];
    for (i, value) in payload.iter().enumerate() {
        ensure!(
            bitmap[i / 8] & (1 << (i % 8)) != 0 || *value == 0,
            "payload in missing pose slot"
        );
    }
    if !length.is_multiple_of(8) {
        ensure!(
            bitmap[bitmap_length - 1] >> (length % 8) == 0,
            "invalid pose presence bitmap"
        );
    }
    Ok((bitmap, payload))
}

/// Decode a packed pose byte array while preserving absent indices.
pub fn pose_array(bytes: &[u8]) -> Result<(u32, BTreeMap<u32, u32>)> {
    let numbers: Vec<u32> = match bytes.first() {
        Some(14) => bytes[1..].iter().map(|n| u32::from(*n)).collect(),
        Some(9) if (bytes.len() - 1).is_multiple_of(4) => bytes[1..]
            .chunks_exact(4)
            .map(|n| u32::from_le_bytes(n.try_into().expect("four-byte chunks")))
            .collect(),
        _ => anyhow::bail!("invalid pose array encoding"),
    };
    ensure!(
        !numbers.is_empty() && numbers.len() % 2 == 1,
        "invalid pose array pairs"
    );
    let length = numbers[0];
    ensure!(length <= 65536, "oversized pose array");
    let mut values = BTreeMap::new();
    let mut last = None;
    for pair in numbers[1..].chunks_exact(2) {
        ensure!(
            pair[0] < length && pair[1] <= 255 && last.is_none_or(|n| pair[0] > n),
            "invalid pose array index/value"
        );
        values.insert(pair[0], pair[1]);
        last = Some(pair[0]);
    }
    Ok((length, values))
}
fn validate_value(bytes: &[u8]) -> Result<()> {
    let Some(tag) = bytes.first() else {
        anyhow::bail!("empty compact value")
    };
    let valid = match tag {
        0 => bytes.len() == 2 && bytes[1] <= 1,
        1..=3 => bytes.len() == 5,
        4 | 6 => bytes.len() == 9,
        7 => bytes.len() == 13,
        5 => std::str::from_utf8(&bytes[1..]).is_ok(),
        9 => (bytes.len() - 1).is_multiple_of(4),
        10 => (bytes.len() - 1).is_multiple_of(8),
        14 => true,
        8 | 11..=13 => serde_json::from_slice::<serde_json::Value>(&bytes[1..]).is_ok(),
        _ => false,
    };
    ensure!(valid, "invalid compact value");
    Ok(())
}
/// Content-addressed cache; only explicit match scoring calls this producer.
pub fn prepare(
    parser: &crate::parser::DemoParser,
    bytes: &[u8],
    root: &std::path::Path,
    fingerprint: &str,
) -> Result<(std::path::PathBuf, u64)> {
    let dir = root.join("analysis/match-state");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!(
        "{}-v{}-{}.gz",
        sha1_smol::Sha1::from(fingerprint).digest(),
        contract().schema_version,
        contract().implementation_version
    ));
    if !path.try_exists()? {
        let mut temp = tempfile::NamedTempFile::new_in(&dir)?;
        let mut encoder = flate2::write::GzEncoder::new(
            std::io::BufWriter::new(&mut temp),
            flate2::Compression::default(),
        );
        parser.write_match_state(bytes, &mut encoder)?;
        encoder.finish()?.flush()?;
        temp.as_file().sync_all()?;
        temp.persist_noclobber(&path)?;
    }
    let size = std::fs::metadata(&path)?.len();
    Ok((path, size))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_prefix(schema: u32) -> Vec<u8> {
        let writer = Writer::new(
            Vec::new(),
            Source {
                demo_fingerprint: Some("synthetic".into()),
                game_build: None,
                game_patch: None,
                map_content_fingerprint: None,
            },
            64.,
        )
        .unwrap();
        let mut cursor = &writer.output[8..];
        let mut header: Artifact<Metadata> =
            serde_json::from_slice(&read_blob(&mut cursor).unwrap()).unwrap();
        header.contract.schema_version = schema;
        header.contract.implementation_version =
            if schema == 6 { "0.13.0" } else { "0.14.0" }.into();
        let mut bytes = if schema == 6 {
            b"DDSTATE6".to_vec()
        } else {
            b"DDSTATE7".to_vec()
        };
        blob(&mut bytes, &serde_json::to_vec(&header).unwrap()).unwrap();
        bytes
    }

    #[test]
    fn reads_more_than_a_million_field_definitions() {
        let mut bytes = fixture_prefix(7);
        for text in ["CPhysicsPropMultiplayer", "$present"] {
            bytes.push(4);
            blob(&mut bytes, text.as_bytes()).unwrap();
        }
        let count = MAX_FIELDS;
        for serial in 0..count {
            let mut definition = Vec::new();
            for n in [1, serial, 0, 1] {
                number(&mut definition, n).unwrap();
            }
            bytes.push(1);
            blob(&mut bytes, &definition).unwrap();
        }
        let mut changes = Vec::new();
        number(&mut changes, count - 1).unwrap();
        number(&mut changes, 2).unwrap();
        number(&mut changes, 0).unwrap();
        blob(&mut changes, &[0, 1]).unwrap();
        bytes.push(2);
        bytes.extend(1i32.to_le_bytes());
        bytes.extend(1u32.to_le_bytes());
        blob(&mut bytes, &changes).unwrap();
        bytes.push(0);
        number(&mut bytes, 1).unwrap();
        bytes.extend(1i32.to_le_bytes());
        let (_, summary) = visit(
            bytes.as_slice(),
            |frame| {
                assert_eq!(frame.changed, &[count - 1]);
                assert_eq!(frame.fields[(count - 1) as usize].serial, count - 1);
                assert_eq!(frame.values[&(count - 1)], [0, 1]);
                Ok(())
            },
            |_| Ok(()),
        )
        .unwrap();
        assert_eq!(summary.fields, count as usize);
        assert_eq!(summary.packets, 1);
        bytes.truncate(bytes.len() - 6);
        bytes.push(1);
        blob(&mut bytes, &[1, 0, 0, 1]).unwrap();
        assert!(visit(bytes.as_slice(), |_| Ok(()), |_| Ok(()))
            .unwrap_err()
            .to_string()
            .contains("too many compact fields"));
    }

    #[test]
    fn writer_rejects_fields_beyond_reader_limit() {
        let mut writer = Writer::new(
            Vec::new(),
            Source {
                demo_fingerprint: Some("synthetic".into()),
                game_build: None,
                game_patch: None,
                map_content_fingerprint: None,
            },
            64.,
        )
        .unwrap();
        writer.next_id = MAX_FIELDS;
        let before = writer.output.len();
        let error = writer
            .field(
                Field {
                    entity: 1,
                    serial: 0,
                    class: "CCSPlayerPawn".into(),
                    name: "$present".into(),
                },
                &Variant::Bool(true),
            )
            .unwrap_err();
        assert!(error.to_string().contains("too many compact fields"));
        assert_eq!(writer.output.len(), before);
    }

    #[test]
    fn schema_seven_rejects_undefined_or_forward_string_and_field_references() {
        for (strings, class, name, expected) in [
            (vec![], 0, 0, "undefined field class"),
            (vec!["CCSPlayerPawn"], 0, 1, "undefined field name"),
            (vec!["CCSPlayerPawn"], 1, 0, "undefined field class"),
        ] {
            let mut bytes = fixture_prefix(7);
            for string in strings {
                bytes.push(4);
                blob(&mut bytes, string.as_bytes()).unwrap();
            }
            let mut definition = vec![];
            for value in [1, 0, class, name] {
                number(&mut definition, value).unwrap();
            }
            bytes.push(1);
            blob(&mut bytes, &definition).unwrap();
            // Definitions cannot refer forward, even if the string appears later.
            bytes.push(4);
            blob(&mut bytes, b"later").unwrap();
            let error = visit(bytes.as_slice(), |_| Ok(()), |_| Ok(())).unwrap_err();
            assert!(error.to_string().contains(expected), "{error}");
        }
        let mut bytes = fixture_prefix(7);
        bytes.push(2);
        bytes.extend(1i32.to_le_bytes());
        bytes.extend(1u32.to_le_bytes());
        blob(&mut bytes, &[0]).unwrap();
        assert!(visit(bytes.as_slice(), |_| Ok(()), |_| Ok(()))
            .unwrap_err()
            .to_string()
            .contains("undefined compact field"));
        for invalid in [&b""[..], &[0xff][..]] {
            let mut bytes = fixture_prefix(7);
            bytes.push(4);
            blob(&mut bytes, invalid).unwrap();
            assert!(visit(bytes.as_slice(), |_| Ok(()), |_| Ok(())).is_err());
        }
    }

    #[test]
    fn schema_six_json_fields_remain_readable_without_string_dictionary() {
        let mut bytes = fixture_prefix(6);
        let field = Field {
            entity: 17,
            serial: 4,
            class: "CCSPlayerPawn".into(),
            name: "CCSPlayerPawn.m_steamID".into(),
        };
        bytes.push(1);
        blob(&mut bytes, &serde_json::to_vec(&field).unwrap()).unwrap();
        let native = value(&Variant::U64(0xfedcba9876543210)).unwrap();
        let mut changes = vec![];
        number(&mut changes, 0).unwrap();
        number(&mut changes, native.len() as u32).unwrap();
        number(&mut changes, 0).unwrap();
        blob(&mut changes, &native).unwrap();
        bytes.push(2);
        bytes.extend(5i32.to_le_bytes());
        bytes.extend(9u32.to_le_bytes());
        blob(&mut bytes, &changes).unwrap();
        bytes.push(0);
        number(&mut bytes, 1).unwrap();
        bytes.extend(5i32.to_le_bytes());
        let mut frames = 0;
        let (header, summary) = visit(
            bytes.as_slice(),
            |frame| {
                frames += 1;
                assert_eq!((frame.tick, frame.net_tick), (5, 9));
                assert_eq!(frame.changed, &[0]);
                assert_eq!(frame.fields[0].entity, 17);
                assert_eq!(frame.fields[0].serial, 4);
                assert_eq!(frame.fields[0].class, field.class);
                assert_eq!(frame.fields[0].name, field.name);
                assert_eq!(frame.values[&0], native);
                Ok(())
            },
            |_| Ok(()),
        )
        .unwrap();
        assert_eq!(frames, 1);
        assert_eq!(header.contract.schema_version, 6);
        assert_eq!(summary.fields, 1);
        let mut invalid = fixture_prefix(6);
        invalid.push(4);
        blob(&mut invalid, b"not valid in schema six").unwrap();
        assert!(visit(invalid.as_slice(), |_| Ok(()), |_| Ok(()))
            .unwrap_err()
            .to_string()
            .contains("invalid field string record"));
    }

    #[test]
    fn complete_pose_payload_borrows_only_received_slots() {
        let mut value = vec![14, 2, 0, 0, 0, 3, 0, 7];
        assert_eq!(complete_pose_bytes(&value).unwrap(), Some(&value[6..]));
        value[5] = 2;
        assert!(complete_pose_bytes(&value).unwrap().is_none());
        value[6] = 9;
        assert!(complete_pose_bytes(&value).is_err());
    }
    #[test]
    fn animation_dictionaries_roundtrip_updates_and_removal() {
        use parser::first_pass::animation_strings::{AnimationStrings, AnimationTable};
        let mut context = AnimationStrings::default();
        let mut table = AnimationTable::default();
        table.name = "AnimAssetData".into();
        table
            .entries
            .insert(0, ("worldmodel".into(), vec![0, 128, 255]));
        context.tables.push(table);
        let mut baseline = AnimationTable::default();
        baseline.name = "instancebaseline".into();
        baseline.entries.insert(0, ("19".into(), vec![1, 2, 3]));
        context.tables.push(baseline);

        let mut writer = Writer::new(
            Vec::new(),
            Source {
                demo_fingerprint: Some("synthetic".into()),
                game_build: None,
                game_patch: None,
                map_content_fingerprint: None,
            },
            64.0,
        )
        .unwrap();
        writer.animation_context(&context).unwrap();
        writer.frame(0, 0).unwrap();
        writer.changes.clear();
        context.tables[0].entries.get_mut(&0).unwrap().1 = vec![];
        writer.animation_context(&context).unwrap();
        writer.frame(1, 1).unwrap();
        writer.changes.clear();
        context.clear();
        writer.animation_context(&context).unwrap();
        writer.frame(2, 2).unwrap();
        let bytes = writer.finish().unwrap();
        let mut frames = Vec::new();
        visit(
            bytes.as_slice(),
            |frame| {
                let current: BTreeMap<_, _> = frame
                    .values
                    .iter()
                    .map(|(id, value)| {
                        let field = &frame.fields[*id as usize];
                        assert!(retains_field(field));
                        assert_eq!(field.class, "AnimationContext");
                        (field.name.clone(), value.clone())
                    })
                    .collect();
                frames.push(current);
                Ok(())
            },
            |_| Ok(()),
        )
        .unwrap();
        assert_eq!(frames[0]["AnimAssetData/0/data"], vec![14, 0, 128, 255]);
        assert_eq!(frames[1]["AnimAssetData/0/data"], vec![14]);
        assert_eq!(
            frames[1]["AnimAssetData/0/name"],
            value(&Variant::String("worldmodel".into())).unwrap()
        );
        assert!(frames[2].is_empty());
    }

    #[test]
    fn animation_context_vectors_keep_distinct_indices_and_types() {
        use parser::second_pass::entities::{is_pose_field, Entity, EntityType};
        let names = [
            "m_vecExternalGraphIds",
            "m_vecExternalClipIds",
            "m_vecSecondarySkeletons",
            "m_vecSecondarySkeletonSlotIDs",
        ];
        let mut entity = Entity {
            entity_id: 1,
            serial: 1,
            cls_id: 0,
            entity_type: EntityType::Normal,
            pose_fields: Default::default(),
            pose_array_lengths: Default::default(),
            props: Default::default(),
        };
        for (index, leaf) in names.iter().enumerate() {
            let name = format!("CCSPlayerPawn.CBodyComponentBaseAnimGraph.{leaf}");
            assert!(is_pose_field(&name));
            for slot in 0..2 {
                let value = if index == 3 {
                    Variant::String(format!("slot{slot}"))
                } else {
                    Variant::U32(1000 + slot as u32)
                };
                entity
                    .pose_fields
                    .insert(vec![9, index as i32, slot], (name.clone(), value));
            }
        }
        assert!(!is_pose_field("CCSPlayerPawn.m_iHealth"));
        let mut writer = Writer::new(
            Vec::new(),
            Source {
                demo_fingerprint: Some("synthetic".into()),
                game_build: None,
                game_patch: None,
                map_content_fingerprint: None,
            },
            64.0,
        )
        .unwrap();
        let mut arrays = Default::default();
        for path in entity.pose_fields.keys() {
            writer
                .pose(&entity, "CCSPlayerPawn", path, &mut arrays)
                .unwrap();
        }
        writer.frame(1, 1).unwrap();
        let bytes = writer.finish().unwrap();
        visit(
            bytes.as_slice(),
            |frame| {
                assert_eq!(frame.values.len(), 8);
                for (path, (name, raw)) in &entity.pose_fields {
                    let field = frame
                        .fields
                        .iter()
                        .position(|f| f.name == format!("pose/{name}/{path:?}"))
                        .unwrap();
                    assert_eq!(frame.values[&(field as u32)], value(raw)?);
                }
                Ok(())
            },
            |_| Ok(()),
        )
        .unwrap();
    }

    #[test]
    fn packed_pose_producer_preserves_length_presence_and_zero_bytes() {
        use parser::second_pass::entities::{Entity, EntityType};
        let name = "CCSPlayerPawn.CBodyComponentBaseAnimGraph.m_SerializePoseRecipeAG2Dynamic";
        let path = vec![9, 33];
        let mut entity = Entity {
            entity_id: 1,
            serial: 2,
            cls_id: 0,
            entity_type: EntityType::Normal,
            pose_fields: Default::default(),
            pose_array_lengths: Default::default(),
            props: Default::default(),
        };
        entity
            .pose_array_lengths
            .insert(path.clone(), (name.into(), 4));
        entity
            .pose_fields
            .insert(vec![9, 33, 0], (name.into(), Variant::U32(0)));
        entity
            .pose_fields
            .insert(vec![9, 33, 2], (name.into(), Variant::U32(255)));
        let source = Source {
            demo_fingerprint: Some("synthetic".into()),
            game_build: None,
            game_patch: None,
            map_content_fingerprint: None,
        };
        let mut writer = Writer::new(Vec::new(), source, 64.0).unwrap();
        writer.array(&entity, "Player", &path).unwrap();
        writer.frame(1, 1).unwrap();
        let bytes = writer.finish().unwrap();
        visit(
            bytes.as_slice(),
            |frame| {
                let value = frame.values.values().next().unwrap();
                let (length, slots) = pose_bytes(value)?;
                assert_eq!(length, 4);
                assert_eq!(slots.get(&0), Some(&0));
                assert!(!slots.contains_key(&1));
                assert_eq!(slots.get(&2), Some(&255));
                assert!(pose_bytes(&value[..value.len() - 1]).is_err());
                Ok(())
            },
            |_| Ok(()),
        )
        .unwrap();
    }
    #[test]
    fn pose_arrays_distinguish_missing_zero_and_reject_invalid_indices() {
        let encoded = value(&Variant::U32Vec(vec![4, 0, 0, 2, 255])).unwrap();
        let (length, slots) = pose_array(&encoded).unwrap();
        assert_eq!(length, 4);
        assert_eq!(slots.get(&0), Some(&0));
        assert_eq!(slots.get(&1), None);
        assert_eq!(slots.get(&2), Some(&255));
        for bad in [
            vec![],
            vec![4, 0],
            vec![4, 4, 1],
            vec![4, 0, 256],
            vec![4, 2, 1, 2, 2],
        ] {
            assert!(pose_array(&value(&Variant::U32Vec(bad)).unwrap()).is_err());
        }
    }
    #[test]
    fn native_values_cover_signed_zero_large_integers_bytes_and_missing_recreation() {
        let source = Source {
            demo_fingerprint: Some("synthetic".into()),
            game_build: None,
            game_patch: None,
            map_content_fingerprint: None,
        };
        let mut writer = Writer::new(Vec::new(), source, 64.0).unwrap();
        let field = Field {
            entity: 1,
            serial: 2,
            class: "Player".into(),
            name: "value".into(),
        };
        let states = [
            Some(Variant::F32(-0.0)),
            Some(Variant::F32(0.0)),
            None,
            Some(Variant::U64(u64::MAX)),
            Some(Variant::U32Vec(vec![0, 128, 255])),
            Some(Variant::U32Vec(vec![0, 256, u32::MAX])),
        ];
        for (tick, raw) in states.iter().enumerate() {
            writer.changes.clear();
            if let Some(raw) = raw {
                writer.field(field.clone(), raw).unwrap();
            } else {
                writer
                    .remove(&(field.entity, field.serial, field.name.clone()))
                    .unwrap();
            }
            writer.frame(tick as i32 * 2, tick as u32 * 2).unwrap();
        }
        let bytes = writer.finish().unwrap();
        let mut decoded = Vec::new();
        let (_, summary) = visit(
            bytes.as_slice(),
            |frame| {
                decoded.push(frame.values.values().next().cloned());
                Ok(())
            },
            |_| Ok(()),
        )
        .unwrap();
        assert_eq!(
            decoded,
            states
                .iter()
                .map(|v| v.as_ref().map(|v| value(v).unwrap()))
                .collect::<Vec<_>>()
        );
        assert_eq!(summary.packets, 6);
        assert_eq!(summary.last_tick, Some(10));
        let mut unknown = bytes.clone();
        unknown[7] = b'9';
        assert!(visit(unknown.as_slice(), |_| Ok(()), |_| Ok(())).is_err());
        use std::io::Write;
        let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        gzip.write_all(&bytes).unwrap();
        let compressed = gzip.finish().unwrap();
        assert!(visit(
            flate2::read::MultiGzDecoder::new(&compressed[..compressed.len() - 4]),
            |_| Ok(()),
            |_| Ok(())
        )
        .is_err());
    }
    #[test]
    fn deltas_preserve_bits_missing_fields_recreation_and_reject_truncation() {
        let source = Source {
            demo_fingerprint: Some("synthetic".into()),
            game_build: None,
            game_patch: None,
            map_content_fingerprint: None,
        };
        let mut writer = Writer::new(Vec::new(), source, 64.0).unwrap();
        let field = Field {
            entity: 1,
            serial: 2,
            class: "Player".into(),
            name: "view".into(),
        };
        let states = [
            Variant::VecXY([-0.0, 0.00001]),
            Variant::VecXY([0.0, f32::from_bits(1)]),
        ];
        for (tick, raw) in states.iter().enumerate() {
            writer.changes.clear();
            writer.field(field.clone(), raw).unwrap();
            writer.frame(tick as i32, tick as u32).unwrap();
        }
        let bytes = writer.finish().unwrap();
        let mut decoded = Vec::new();
        let (_, summary) = visit(
            bytes.as_slice(),
            |frame| {
                decoded.push(frame.values[&0].clone());
                Ok(())
            },
            |_| Ok(()),
        )
        .unwrap();
        assert_eq!(
            decoded,
            states.iter().map(|v| value(v).unwrap()).collect::<Vec<_>>()
        );
        assert_eq!(summary.packets, 2);
        for end in 0..bytes.len() {
            assert!(visit(&bytes[..end], |_| Ok(()), |_| Ok(())).is_err());
        }
    }
}
