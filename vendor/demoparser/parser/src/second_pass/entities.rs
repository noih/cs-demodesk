use crate::first_pass::prop_controller::is_grenade_or_weapon;
use crate::first_pass::prop_controller::ITEM_PURCHASE_DEF_IDX;
use crate::first_pass::read_bits::Bitreader;
use crate::first_pass::read_bits::DemoParserError;
use crate::first_pass::sendtables::find_field;
use crate::first_pass::sendtables::get_decoder_from_field;
use crate::first_pass::sendtables::get_propinfo;
use crate::first_pass::sendtables::Field;
use crate::first_pass::sendtables::FieldInfo;
use crate::second_pass::game_events::GameEventInfo;
use crate::second_pass::other_netmessages::Class;
use crate::second_pass::parser_settings::SecondPassParser;
use crate::second_pass::path_ops::*;
use crate::second_pass::variants::Variant;
use ahash::AHashMap;
use csgoproto::CsvcMsgPacketEntities;
use prost::Message;

const NSERIALBITS: u32 = 17;
const STOP_READING_SYMBOL: u8 = 39;
const HUFFMAN_CODE_MAXLEN: u32 = 17;

/// These arrays need their sendtable indices; the statistics property map keeps one scalar per ID.
pub fn is_pose_field(name: &str) -> bool {
    (name.starts_with("CCSPlayerPawn.")
        && matches!(
            name.rsplit('.').next(),
            Some(
                "m_fFlags"
                    | "m_nLastJumpTick"
                    | "m_flLastJumpFrac"
                    | "m_MoveType"
                    | "m_nActualMoveType"
                    | "m_flWaterLevel"
                    | "m_nWaterLevel"
                    | "m_nLadderSurfacePropIndex"
            )
        ))
        || name.contains("PoseRecipe")
        || matches!(
            name.rsplit('.').next(),
            Some("m_vecExternalGraphIds" | "m_vecExternalClipIds" | "m_vecSecondarySkeletons" | "m_vecSecondarySkeletonSlotIDs"
                | "m_nNextPrimaryAttackTick" | "m_flNextPrimaryAttackTickRatio"
                | "m_nNextSecondaryAttackTick" | "m_flNextSecondaryAttackTickRatio"
                | "m_nPostponeFireReadyTicks" | "m_flPostponeFireReadyFrac")
        )
}

/// Decode only the sendtable-qualified GameTick_t representation; never alter legacy properties.
fn analysis_game_tick(raw: u32) -> i32 {
    ((raw >> 1) as i32) ^ -((raw & 1) as i32)
}

#[derive(Debug, Clone)]
pub struct Entity {
    pub cls_id: u32,
    pub entity_id: i32,
    pub serial: u32,
    pub pose_fields: AHashMap<Vec<i32>, (String, Variant)>,
    pub pose_array_lengths: AHashMap<Vec<i32>, (String, u32)>,
    pub props: AHashMap<u32, Variant>,
    pub entity_type: EntityType,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerMetaData {
    pub player_entity_id: Option<i32>,
    pub steamid: Option<u64>,
    pub controller_entid: Option<i32>,
    pub name: Option<String>,
    pub team_num: Option<u32>,
}
#[derive(Debug, Clone, PartialEq)]
pub enum EntityType {
    PlayerController,
    Rules,
    Projectile,
    Team,
    Normal,
    C4,
}
enum EntityCmd {
    Delete,
    CreateAndUpdate,
    Update,
}

impl<'a> SecondPassParser<'a> {
    pub fn parse_packet_ents(&mut self, bytes: &[u8], is_fullpacket: bool) -> Result<(), DemoParserError> {
        if !self.parse_entities {
            return Ok(());
        }
        let msg = match CsvcMsgPacketEntities::decode(bytes) {
            Err(_) => return Err(DemoParserError::MalformedMessage),
            Ok(msg) => msg,
        };

        let mut bitreader = Bitreader::new(msg.entity_data());
        let mut entity_id: i32 = -1;
        let mut events_to_emit = vec![];
        for _ in 0..msg.updated_entries() {
            entity_id += 1 + (bitreader.read_u_bit_var()? as i32);
            // Read 2 bits to know which operation should be done to the entity.
            let cmd = match bitreader.read_nbits(2)? {
                0b01 => EntityCmd::Delete,
                0b11 => EntityCmd::Delete,
                0b10 => EntityCmd::CreateAndUpdate,
                0b00 => EntityCmd::Update,
                _ => return Err(DemoParserError::ImpossibleCmd),
            };

            match cmd {
                EntityCmd::Delete => {
                    if let Some(changes) = &mut self.analysis_changes {
                        changes.lifecycle.insert(entity_id);
                    }
                    self.projectiles.remove(&entity_id);
                    if let Some(entry) = self.entities.get_mut(entity_id as usize) {
                        *entry = None;
                    }
                }
                EntityCmd::CreateAndUpdate => {
                    if let Some(changes) = &mut self.analysis_changes {
                        changes.lifecycle.insert(entity_id);
                    }
                    self.create_new_entity(&mut bitreader, &entity_id, &mut events_to_emit)?;
                    self.update_entity(&mut bitreader, entity_id, false, &mut events_to_emit, is_fullpacket)?;
                }
                EntityCmd::Update => {
                    if msg.has_pvs_vis_bits_deprecated() != 0 {
                        // Most entities pass trough here. Seems like entities that are not updated.
                        if bitreader.read_nbits(2)? & 0x01 == 1 {
                            continue;
                        }
                    }
                    self.update_entity(&mut bitreader, entity_id, false, &mut events_to_emit, is_fullpacket)?;
                }
            }
        }
        if !events_to_emit.is_empty() {
            self.emit_events(events_to_emit)?;
        }
        Ok(())
    }

    pub fn update_entity(
        &mut self,
        bitreader: &mut Bitreader,
        entity_id: i32,
        is_baseline: bool,
        events_to_emit: &mut Vec<GameEventInfo>,
        is_fullpacket: bool,
    ) -> Result<(), DemoParserError> {
        let _pp = crate::second_pass::parser::prof_on().then(std::time::Instant::now);
        let n_updates = self.parse_paths(bitreader)?;
        if let Some(t) = _pp {
            crate::second_pass::parser::PROF_PATHS_NS.with(|c| c.set(c.get() + t.elapsed().as_nanos() as u64));
        }
        let _pd = crate::second_pass::parser::prof_on().then(std::time::Instant::now);
        let n_updated_values = self.decode_entity_update(bitreader, entity_id, n_updates, is_fullpacket, is_baseline, events_to_emit)?;
        if let Some(t) = _pd {
            crate::second_pass::parser::PROF_DECODE_NS.with(|c| c.set(c.get() + t.elapsed().as_nanos() as u64));
        }
        if n_updated_values > 0 {
            self.gather_extra_info(&entity_id, is_baseline)?;
        }
        Ok(())
    }
    pub fn parse_paths(&mut self, bitreader: &mut Bitreader) -> Result<usize, DemoParserError> {
        /*
        Create a field path by decoding using a Huffman tree.
        The huffman tree can be found at the bottom of entities_utils.rs

        A field path is a "path trough a struct" where
        the struct can have normal fields but also pointers
        to other (nested) structs.

        Example:

        The array will be filled with these:

        Struct Field{
            wanted_information: Option<T>,
            Pointer: bool,
            fields: Option<Vec<Field>>
        },

        (struct is simplified for this example. In reality it also includes field name etc.)


        Path to each of the fields in the below fields list: [
            [0], [1, 0], [1, 1], [2]
        ]
        and they would map to:
        [0] => FloatDecoder,
        [1, 0] => IntegerDecoder,
        [1, 1] => StringDecoder,
        [2] => VectorDecoder,

        fields = [
            Field{
                wanted_information: FloatDecoder,
                pointer: false,
                fields: None,
            },
            Field{
                wanted_information: None,
                pointer: true,
                fields: Some(
                    [
                        Field{
                            wanted_information: IntegerDecoder,
                            pointer: false,
                            fields: Some(
                        },
                        Field{
                            wanted_information: StringDecoder,
                            pointer: flase,
                            fields: Some(
                        }
                    ]
                ),
            },
            Field{
                wanted_information: VectorDecoder,
                pointer: false,
                fields: None,
            },
        ]
        Not sure what the maximum depth of these structs are, but others seem to use
        7 as the max length of field path so maybe that?

        Personally I find this path idea horribly complicated. Why is this chosen over
        the way it was done in source 1 demos?
        */

        // Create an "empty" path ([-1, 0, 0, 0, 0, 0, 0])
        // For perfomance reasons have them always the same len
        let mut fp = generate_fp();
        let mut idx = 0;
        // Do huffman decoding with a lookup table instead of reading one bit at a time
        // and traversing a tree.
        // Here we peek ("HUFFMAN_CODE_MAXLEN" == 17) amount of bits and see from a table which
        // symbol it maps to and how many bits should be consumed from the stream.
        // The symbol is then mapped into an op for filling the field path.
        loop {
            if bitreader.bits_left < HUFFMAN_CODE_MAXLEN {
                bitreader.refill();
            }

            let peeked_bits = bitreader.peek(HUFFMAN_CODE_MAXLEN);
            // SAFETY: peek(17) yields a value in [0, 2^17-1] (it masks with (1<<17)-1), and the
            // huffman table is built with exactly 2^17 entries (huf.b = 131071 pairs + 1 sentinel,
            // see create_huffman_lookup_table). So `peeked_bits` is always a valid index. Eliding
            // the bounds check removes a per-symbol branch in the hottest decode loop.
            let (symbol, code_len) = unsafe { *self.huffman_lookup_table.get_unchecked(peeked_bits as usize) };
            bitreader.consume(code_len as u32);
            if symbol == STOP_READING_SYMBOL {
                break;
            }
            do_op(symbol, bitreader, &mut fp)?;
            self.write_fp(&mut fp, idx)?;
            idx += 1;
        }
        Ok(idx)
    }

    pub fn decode_entity_update(
        &mut self,
        bitreader: &mut Bitreader,
        entity_id: i32,
        n_updates: usize,
        is_fullpacket: bool,
        is_baseline: bool,
        events_to_emit: &mut Vec<GameEventInfo>,
    ) -> Result<usize, DemoParserError> {
        let entity = match self.entities.get_mut(entity_id as usize) {
            Some(Some(entity)) => entity,
            _ => return Err(DemoParserError::EntityNotFound),
        };
        let class = match self.cls_by_id.get(entity.cls_id as usize) {
            Some(cls) => cls,
            None => return Err(DemoParserError::ClassNotFound),
        };

        for path in self.paths.iter().take(n_updates) {
            let field = find_field(&path, &class.serializer)?;
            let field_info = get_propinfo(&field, path);
            let decoder = get_decoder_from_field(field)?;
            let (result, raw_network_time) = if self.capture_pose_fields
                && decoder == crate::second_pass::decoder::Decoder::FloatSimulationTimeDecoder
            {
                let (legacy, raw) = decode_analysis_network_time(bitreader)?;
                (legacy, Some(raw))
            } else {
                (bitreader.decode(&decoder, self.qf_mapper)?, None)
            };
            if let (Some(raw), Field::Value(value)) = (raw_network_time, field) {
                // Preserve the encoded integer before the legacy time conversion rounds it.
                // simulationTimeSerializer encodes trunc(f32(seconds * 64) + 0.5);
                // server rewind records use that same integer, independently of packet time.
                capture_analysis_value(
                    entity, entity_id, path.path[..=path.last as usize].to_vec(),
                    &format!("rawNetworkTime/{}", value.full_name), &Variant::U32(raw),
                    self.analysis_changes.as_mut(),
                );
            }


            // listen_to_props()
            if self.list_props {
                if let Field::Value(_v) = field {
                    if should_emit_prop_to_listen(&_v.full_name) {
                        self.uniq_prop_names.insert(convert_weapon_prefix_to_general(&_v.full_name));
                    }
                }
            }
            // Custom events
            if !is_baseline {
                SecondPassParser::listen_for_events(
                    entity,
                    &result,
                    field,
                    field_info,
                    &self.prop_controller,
                    &self.prop_controller.special_ids,
                    is_fullpacket,
                    events_to_emit,
                );
            }
            // Debug
            if self.is_debug_mode {
                SecondPassParser::debug_inspect(
                    &result,
                    field,
                    self.tick,
                    field_info,
                    path,
                    is_fullpacket,
                    is_baseline,
                    class,
                    &entity.cls_id,
                    &entity_id,
                );
            }
            if self.capture_pose_fields {
                let pose_value = match field {
                    Field::Value(value) => Some(value),
                    Field::Vector(vector) => match vector.field_enum.as_ref() {
                        Field::Value(value) => Some(value),
                        _ => None,
                    },
                    _ => None,
                };
                if let Some(value) = pose_value.filter(|value| value.analysis_name.is_some() || is_pose_field(&value.full_name)) {
                    let analysis_name = value.analysis_name.as_deref().unwrap_or(&value.full_name);
                    let indices = path.path[..=path.last as usize].to_vec();
                    if matches!(field, Field::Vector(_)) {
                        if let Variant::U32(length) = &result {
                            entity.pose_array_lengths.insert(indices.clone(), (analysis_name.to_owned(), *length));
                            entity.pose_array_lengths.retain(|key, _| {
                                !key.starts_with(&indices) || key.len() <= indices.len() || key[indices.len()] >= 0 && (key[indices.len()] as u32) < *length
                            });
                            entity.pose_fields.retain(|key, (name, _)| {
                                let keep = !key.starts_with(&indices)
                                    || key.len() <= indices.len()
                                    || key[indices.len()] >= 0 && (key[indices.len()] as u32) < *length;
                                if !keep {
                                    if let Some(changes) = &mut self.analysis_changes {
                                        changes.pose_removals.push((entity_id, key.clone(), name.clone()));
                                    }
                                }
                                keep
                            });
                        }
                    }
                    let signed_tick = match (&result, value.analysis_signed_tick) {
                        (Variant::U32(raw), true) => Some(Variant::I32(analysis_game_tick(*raw))),
                        _ => None,
                    };
                    capture_analysis_value(
                        entity,
                        entity_id,
                        indices,
                        analysis_name,
                        signed_tick.as_ref().unwrap_or(&result),
                        self.analysis_changes.as_mut(),
                    );
                }
            }
            if let (Some(changes), Some(fi)) = (&mut self.analysis_changes, field_info) {
                if fi.should_parse {
                    changes.properties.insert((entity_id, fi.prop_id));
                }
            }
            SecondPassParser::insert_field(entity, result, field_info);
        }
        Ok(n_updates)
    }

    pub fn debug_inspect(
        _result: &Variant,
        field: &Field,
        _tick: i32,
        field_info: Option<FieldInfo>,
        _path: &FieldPath,
        _is_fullpacket: bool,
        _is_baseline: bool,
        _cls: &Class,
        _cls_id: &u32,
        _entity_id: &i32,
    ) {
        if let Field::Value(_v) = field {
            println!("{:?} {:?} {:?} {:?} {:?}", _path, field_info, _v.full_name, _result, _cls.name);
        }
    }

    pub fn insert_field(entity: &mut Entity, result: Variant, field_info: Option<FieldInfo>) {
        if let Some(fi) = field_info {
            if fi.should_parse {
                entity.props.insert(fi.prop_id, result);
            }
        }
    }

    #[inline]
    fn write_fp(&mut self, fp_src: &mut FieldPath, idx: usize) -> Result<(), DemoParserError> {
        match self.paths.get_mut(idx) {
            Some(entry) => *entry = *fp_src,
            // need to extend vec (rare)
            None => {
                // If we have over 100k fields for an entity then something definitely went wrong. Do this to avoid infinite loop/oom
                if idx > 100_000 {
                    return Err(DemoParserError::VectorResizeFailure);
                }
                self.paths.resize(idx + 1, generate_fp());
                match self.paths.get_mut(idx) {
                    Some(entry) => *entry = *fp_src,
                    None => return Err(DemoParserError::VectorResizeFailure),
                }
            }
        }
        Ok(())
    }
    fn create_new_entity(&mut self, bitreader: &mut Bitreader, entity_id: &i32, _events_to_emit: &mut Vec<GameEventInfo>) -> Result<(), DemoParserError> {
        // Class id width is dynamic: ceil(log2(num_classes + 1)). Hardcoded 8 bits
        // capped at 256 classes and broke on patches with more (14154+), causing
        // bitstream desync and cascading EntityNotFound errors. cls_by_id.len()
        // already equals num_classes + 1 (see first_pass::parser::parse_class_info).
        let cls_bits = (self.cls_by_id.len() as f32).log2().ceil() as u32;
        let cls_id: u32 = bitreader.read_nbits(cls_bits)?;
        // The serial distinguishes entity-index reuse across lifetimes.
        let serial = bitreader.read_nbits(NSERIALBITS)?;
        let _unknown = bitreader.read_varint();
        let entity_type = self.check_entity_type(&cls_id)?;
        match entity_type {
            EntityType::Projectile => {
                self.projectiles.insert(*entity_id);
            }
            EntityType::Rules => self.rules_entity_id = Some(*entity_id),
            EntityType::C4 => self.c4_entity_id = Some(*entity_id),
            _ => {}
        };
        let entity = Entity {
            entity_id: *entity_id,
            serial,
            pose_fields: AHashMap::default(),
            pose_array_lengths: AHashMap::default(),
            cls_id,
            props: AHashMap::with_capacity(0),
            entity_type,
        };
        if self.entities.len() as i32 <= *entity_id {
            // if corrupt, this can cause oom allocations
            if *entity_id > 100000 {
                return Err(DemoParserError::EntityNotFound);
            }
            self.entities.resize(*entity_id as usize + 1, None);
        }
        match self.entities.get_mut(*entity_id as usize) {
            Some(entry) => *entry = Some(entity),
            None => return Err(DemoParserError::VectorResizeFailure),
        };
        // Analysis uses the current wire dictionary: legacy string-table updates
        // can omit baselines for the first instance of a newly introduced class.
        // Preserve the existing statistics parser when generic capture is disabled.
        let baseline = if self.analysis_changes.is_some() {
            self.animation_strings.instance_baseline(cls_id)
        } else {
            self.baselines.get(&cls_id).map(Vec::as_slice)
        };
        if let Some(baseline_bytes) = baseline {
            let b = baseline_bytes.to_vec();
            let mut br = Bitreader::new(&b);
            self.update_entity(&mut br, *entity_id, true, &mut vec![], false)?;
        }
        Ok(())
    }

    pub fn check_entity_type(&self, cls_id: &u32) -> Result<EntityType, DemoParserError> {
        let class = match self.cls_by_id.get(*cls_id as usize) {
            Some(cls) => cls,
            None => {
                return Err(DemoParserError::ClassNotFound);
            }
        };
        match class.name.as_str() {
            "CCSPlayerController" => return Ok(EntityType::PlayerController),
            "CCSGameRulesProxy" => return Ok(EntityType::Rules),
            "CCSTeam" => return Ok(EntityType::Team),
            "CC4" => return Ok(EntityType::C4),
            _ => {}
        }
        let is_projectile_prop =
            (class.name == "CInferno" || class.name.contains("Projectile") || class.name.contains("Grenade") || class.name.contains("Flash"))
                && !class.name.contains("Player");
        if is_projectile_prop {
            return Ok(EntityType::Projectile);
        }
        return Ok(EntityType::Normal);
    }
}

fn should_emit_prop_to_listen(prop_name: &str) -> bool {
    match prop_name.split(".").next() {
        Some("CCSGameRulesProxy") => return true,
        Some("CCSTeam") => return true,
        Some("CCSPlayerPawn") => return true,
        Some("CCSPlayerController") => return true,
        _ => {}
    };
    if is_weapon_prop(prop_name) || is_grenade_prop(prop_name) {
        return true;
    }
    false
}
fn convert_weapon_prefix_to_general(full_name: &str) -> String {
    let split_at_dot: Vec<&str> = full_name.split(".").collect();
    let grenade_or_weapon = is_grenade_or_weapon(full_name);
    // Strip first part of name from grenades and weapons.
    // if weapon prop: CAK47.m_iClip1 => m_iClip1
    // if grenade: CSmokeGrenadeProjectile.CBodyComponentBaseAnimGraph.m_cellX => CBodyComponentBaseAnimGraph.m_cellX
    if is_grenade_prop(full_name) {
        return "Grenade.".to_owned() + &split_at_dot[1..].join(".");
    }
    match grenade_or_weapon {
        true => "Weapon.".to_owned() + &split_at_dot[1..].join("."),
        false => full_name.to_string(),
    }
}
fn is_weapon_prop(full_name: &str) -> bool {
    let split_at_dot: Vec<&str> = full_name.split(".").collect();
    let is_weapon_prop =
        (split_at_dot[0].contains("Weapon") || split_at_dot[0].contains("AK")) && !split_at_dot[0].contains("Player") || split_at_dot[0].contains("CDEagle");
    is_weapon_prop
}
fn is_grenade_prop(full_name: &str) -> bool {
    if full_name.contains("CCSPlayer") {
        return false;
    }
    let parts = vec!["Molo", "Inc", "Infer", "Projectile", "Grenade", "Flash"];
    for part in parts {
        if full_name.contains(part) {
            return true;
        }
    }
    false
}

fn decode_analysis_network_time(reader: &mut Bitreader<'_>) -> Result<(Variant, u32), DemoParserError> {
    let (legacy, raw) = reader.decode_simul_time_with_raw()?;
    Ok((Variant::F32(legacy), raw))
}

// Capture before insert_field: distinct wire paths can share one legacy statistics ID.
fn capture_analysis_value(
    entity: &mut Entity,
    entity_id: i32,
    indices: Vec<i32>,
    name: &str,
    result: &Variant,
    changes: Option<&mut crate::second_pass::parser_settings::AnalysisChanges>,
) {
    if let Some(changes) = changes {
        changes.poses.insert((entity_id, indices.clone()));
    }
    entity.pose_fields.insert(indices, (name.to_owned(), result.clone()));
}

#[cfg(test)]
mod analysis_owner_tests {
    #[test]
    fn raw_network_time_survives_legacy_float_rounding() {
        let bytes=[0x81,0x80,0x80,0x10];
        let (legacy,raw)=super::decode_analysis_network_time(&mut crate::first_pass::read_bits::Bitreader::new(&bytes)).unwrap();
        assert_eq!(raw,33_554_433);
        assert_eq!(legacy,crate::second_pass::variants::Variant::F32(raw as f32*(1.0/30.0)));
        assert_ne!(raw as f32 as u32,raw);
    }
    use super::*;
    use crate::first_pass::{prop_controller::PropController, sendtables::ValueField};
    use crate::second_pass::{decoder::Decoder, parser_settings::AnalysisChanges};

    #[test]
    fn wire_owned_vectors_survive_legacy_property_collision() {
        let mut controller = PropController::new(vec![], vec![], Default::default(), Default::default(), false, &[], false);
        let mut velocity = ValueField::new(Decoder::NoscaleDecoder, "m_vecZ");
        velocity.send_node = "m_vecVelocity".into();
        controller.handle_prop("CCSPlayerPawn.m_vecZ", &mut velocity, vec![186]);
        let mut view = ValueField::new(Decoder::NoscaleDecoder, "m_vecZ");
        view.send_node = "m_vecViewOffset".into();
        controller.handle_prop("CCSPlayerPawn.m_vecZ", &mut view, vec![211]);
        assert_eq!(velocity.prop_id, view.prop_id);
        assert_eq!(velocity.full_name, view.full_name);
        assert_eq!(velocity.analysis_name.as_deref(), Some("CCSPlayerPawn.m_vecVelocity.m_vecZ"));
        assert_eq!(view.analysis_name.as_deref(), Some("CCSPlayerPawn.m_vecViewOffset.m_vecZ"));
        let mut entity = Entity {
            cls_id: 0,
            entity_id: 7,
            serial: 1,
            entity_type: EntityType::Normal,
            pose_fields: Default::default(),
            pose_array_lengths: Default::default(),
            props: Default::default(),
        };
        let mut changes = AnalysisChanges::default();
        for (field, path, number) in [(&view, 211, 64.0), (&velocity, 186, -120.0)] {
            let result = Variant::F32(number);
            capture_analysis_value(&mut entity, 7, vec![path], field.analysis_name.as_deref().unwrap(), &result, Some(&mut changes));
            SecondPassParser::insert_field(
                &mut entity,
                result,
                Some(FieldInfo {
                    decoder: field.decoder,
                    should_parse: true,
                    prop_id: field.prop_id,
                }),
            );
        }
        assert_eq!(entity.props.get(&view.prop_id), Some(&Variant::F32(-120.0)));
        assert_eq!(entity.pose_fields.get(&vec![211]).unwrap().1, Variant::F32(64.0));
        assert_eq!(entity.pose_fields.get(&vec![186]).unwrap().1, Variant::F32(-120.0));
        assert_eq!(changes.poses.len(), 2);
    }
}

#[cfg(test)]
mod movement_wire_tests {
    use super::*;
    #[test]
    fn analysis_movement_preserves_signed_tick_and_exact_field_ownership() {
        assert_eq!(analysis_game_tick(7268), 3634);
        assert_eq!(analysis_game_tick(1), -1);
        assert_eq!(analysis_game_tick(0), 0);
        assert_eq!(analysis_game_tick(u32::MAX), i32::MIN);
        assert!(is_pose_field("CCSPlayerPawn.CCSPlayer_MovementServices.m_nLastJumpTick"));
        assert!(is_pose_field("CCSPlayerPawn.m_fFlags"));
        assert!(!is_pose_field("CCSPlayerController.m_fFlags"));
    }
}
