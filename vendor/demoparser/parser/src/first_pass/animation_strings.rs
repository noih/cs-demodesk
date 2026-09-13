//! Animation resource dictionaries required to interpret recorded pose recipes.
//! Kept separate from the legacy statistics parser's string-table bookkeeping.
use super::read_bits::DemoParserError;
use bitter::{BitReader, LittleEndianReader};
use csgoproto::{CDemoStringTables, CsvcMsgCreateStringTable, CsvcMsgUpdateStringTable};
use std::collections::BTreeMap;

fn read(reader: &mut LittleEndianReader<'_>, bits: u32) -> Result<u32, DemoParserError> {
    reader.read_bits(bits).map(|n| n as u32).ok_or(DemoParserError::OutOfBytesError)
}
fn varint(reader: &mut LittleEndianReader<'_>) -> Result<u32, DemoParserError> {
    let mut value = 0;
    for shift in (0..35).step_by(7) {
        let byte = read(reader, 8)?;
        if shift == 28 && byte > 15 {
            return Err(DemoParserError::MalformedMessage);
        }
        value |= (byte & 127) << shift;
        if byte & 128 == 0 {
            return Ok(value);
        }
    }
    Err(DemoParserError::MalformedMessage)
}

#[derive(Default)]
pub struct AnimationStrings {
    pub dirty: bool,
    pub tables: Vec<AnimationTable>,
}
#[derive(Default)]
pub struct AnimationTable {
    pub name: String,
    fixed: bool,
    bits: u32,
    flags: i32,
    var_bits: bool,
    pub entries: BTreeMap<i32, (String, Vec<u8>)>,
}
impl AnimationTable {
    fn relevant(&self) -> bool {
        matches!(self.name.as_str(), "AnimTaskTypes" | "AnimAssetData" | "instancebaseline")
    }
    fn decode(&mut self, data: &[u8], count: i32) -> Result<(), DemoParserError> {
        if !self.relevant() {
            return Ok(());
        }
        if !(0..=65536).contains(&count) {
            return Err(DemoParserError::MalformedMessage);
        }
        let mut reader = LittleEndianReader::new(data);
        let mut index = -1i32;
        let mut history = Vec::<String>::new();
        for _ in 0..count {
            let delta = if read(&mut reader, 1)? != 0 {
                1
            } else {
                varint(&mut reader)?.checked_add(2).ok_or(DemoParserError::MalformedMessage)?
            };
            index = index
                .checked_add(i32::try_from(delta).map_err(|_| DemoParserError::MalformedMessage)?)
                .ok_or(DemoParserError::MalformedMessage)?;
            if index > 65535 {
                return Err(DemoParserError::MalformedMessage);
            }
            let mut key = None;
            if read(&mut reader, 1)? != 0 {
                let prefix = if read(&mut reader, 1)? != 0 {
                    let position = read(&mut reader, 5)? as usize;
                    let length = read(&mut reader, 5)? as usize;
                    history
                        .get(position)
                        .and_then(|s| s.get(..length))
                        .ok_or(DemoParserError::MalformedMessage)?
                        .to_owned()
                } else {
                    String::new()
                };
                let mut suffix = Vec::new();
                loop {
                    let byte = read(&mut reader, 8)? as u8;
                    if byte == 0 {
                        break;
                    }
                    if suffix.len() >= 65536 {
                        return Err(DemoParserError::MalformedMessage);
                    }
                    suffix.push(byte);
                }
                let full = prefix + std::str::from_utf8(&suffix).map_err(|_| DemoParserError::MalformedMessage)?;
                key = Some(full);
            }
            let mut value = None;
            if read(&mut reader, 1)? != 0 {
                let compressed = !self.fixed && self.flags & 1 != 0 && (read(&mut reader, 1)? != 0);
                let bits = if self.fixed {
                    self.bits
                } else {
                    let size = if self.var_bits {
                        {
                            let prefix = read(&mut reader, 6)?;
                            let extra = match prefix & 48 {
                                16 => 4,
                                32 => 8,
                                48 => 28,
                                _ => 0,
                            };
                            if extra == 0 {
                                prefix
                            } else {
                                (prefix & 15) | (read(&mut reader, extra)? << 4)
                            }
                        }
                    } else {
                        read(&mut reader, 17)?
                    };
                    size.checked_mul(8).ok_or(DemoParserError::MalformedMessage)?
                };
                if bits > 16 * 1024 * 1024 * 8 {
                    return Err(DemoParserError::MalformedMessage);
                }
                let mut bytes = vec![0; bits.div_ceil(8) as usize];
                let whole = (bits / 8) as usize;
                if !reader.read_bytes(&mut bytes[..whole]) {
                    return Err(DemoParserError::OutOfBytesError);
                }
                if bits % 8 != 0 {
                    bytes[whole] = read(&mut reader, bits % 8)? as u8;
                }
                value = Some(if compressed {
                    let size = snap::raw::decompress_len(&bytes).map_err(|_| DemoParserError::MalformedMessage)?;
                    if size > 16 * 1024 * 1024 {
                        return Err(DemoParserError::MalformedMessage);
                    }
                    snap::raw::Decoder::new()
                        .decompress_vec(&bytes)
                        .map_err(|_| DemoParserError::MalformedMessage)?
                } else {
                    bytes
                });
            }
            let entry = self.entries.entry(index).or_default();
            if let Some(key) = key {
                entry.0 = key;
            }
            if let Some(value) = value {
                entry.1 = value;
            }
            if history.len() == 32 {
                history.remove(0);
            }
            history.push(entry.0.clone());
        }
        Ok(())
    }
}
impl AnimationStrings {
    /// Baselines decoded from the current wire table, including keyless updates.
    pub fn instance_baseline(&self, class_id: u32) -> Option<&[u8]> {
        self.tables.iter().find(|table| table.name == "instancebaseline")?
            .entries.values().find(|(key, _)| key.parse::<u32>().ok() == Some(class_id))
            .map(|(_, bytes)| bytes.as_slice())
    }
    pub fn create(&mut self, message: &CsvcMsgCreateStringTable) -> Result<(), DemoParserError> {
        let mut table = AnimationTable {
            name: message.name().into(),
            fixed: message.user_data_fixed_size(),
            bits: message.user_data_size_bits() as u32,
            flags: message.flags(),
            var_bits: message.using_varint_bitcounts(),
            ..Default::default()
        };
        if table.relevant() {
            let bytes = if message.data_compressed() {
                let size = snap::raw::decompress_len(message.string_data()).map_err(|_| DemoParserError::MalformedMessage)?;
                if size > 16 * 1024 * 1024 {
                    return Err(DemoParserError::MalformedMessage);
                }
                snap::raw::Decoder::new()
                    .decompress_vec(message.string_data())
                    .map_err(|_| DemoParserError::MalformedMessage)?
            } else {
                message.string_data().to_vec()
            };
            table.decode(&bytes, message.num_entries())?;
            self.dirty = true;
        }
        self.tables.push(table);
        Ok(())
    }
    pub fn update(&mut self, message: &CsvcMsgUpdateStringTable) -> Result<(), DemoParserError> {
        let table = self.tables.get_mut(message.table_id() as usize).ok_or(DemoParserError::StringTableNotFound)?;
        table.decode(message.string_data(), message.num_changed_entries())?;
        self.dirty |= table.relevant();
        Ok(())
    }
    pub fn snapshot(&mut self, snapshot: &CDemoStringTables) {
        // Full packets can contain only selected tables; absent dictionaries persist.
        for source in &snapshot.tables {
            let index = if let Some(index) = self.tables.iter().position(|t| t.name == source.table_name()) {
                index
            } else {
                self.tables.push(AnimationTable {
                    name: source.table_name().into(),
                    flags: source.table_flags(),
                    ..Default::default()
                });
                self.tables.len() - 1
            };
            let table = &mut self.tables[index];
            table.entries.clear();
            if table.relevant() {
                for (index, item) in source.items.iter().enumerate() {
                    table.entries.insert(index as i32, (item.str().into(), item.data().to_vec()));
                }
            }
        }
        self.dirty = true;
    }
    pub fn clear(&mut self) {
        self.tables.clear();
        self.dirty = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn update(gap: Option<u8>, key: Option<&str>, value: &[u8]) -> Vec<u8> {
        let mut bits = Vec::new();
        let mut write = |n: u32, count: u32| {
            for bit in 0..count {
                bits.push(((n >> bit) & 1) as u8);
            }
        };
        write(u32::from(gap.is_none()), 1);
        if let Some(gap) = gap {
            write(u32::from(gap), 8);
        }
        write(u32::from(key.is_some()), 1);
        if let Some(key) = key {
            write(0, 1);
            for byte in key.bytes().chain([0]) {
                write(u32::from(byte), 8);
            }
        }
        write(1, 1);
        write(value.len() as u32, 17);
        for byte in value {
            write(u32::from(*byte), 8);
        }
        let mut out = vec![0; bits.len().div_ceil(8)];
        for (i, bit) in bits.into_iter().enumerate() {
            out[i / 8] |= bit << (i % 8);
        }
        out
    }
    #[test]
    fn instance_baselines_follow_wire_slots_and_keyless_updates() {
        let mut tables = AnimationStrings::default();
        tables.create(&CsvcMsgCreateStringTable {
            name: Some("unrelated".into()), ..Default::default()
        }).unwrap();
        tables.create(&CsvcMsgCreateStringTable {
            name: Some("instancebaseline".into()), num_entries: Some(1),
            string_data: Some(update(None, Some("19"), &[1, 2, 3]).into()),
            ..Default::default()
        }).unwrap();
        assert_eq!(tables.instance_baseline(19), Some(&[1, 2, 3][..]));
        for bytes in [vec![4, 5], vec![6]] {
            tables.update(&CsvcMsgUpdateStringTable {
                table_id: Some(1), num_changed_entries: Some(1),
                string_data: Some(update(None, None, &bytes).into()),
                ..Default::default()
            }).unwrap();
            assert_eq!(tables.instance_baseline(19), Some(bytes.as_slice()));
            assert_eq!(tables.tables.len(), 2);
        }
        tables.update(&CsvcMsgUpdateStringTable {
            table_id: Some(1), num_changed_entries: Some(1),
            string_data: Some(update(Some(0), Some("188"), &[9]).into()),
            ..Default::default()
        }).unwrap();
        assert_eq!(tables.instance_baseline(19), Some(&[6][..]));
        assert_eq!(tables.instance_baseline(188), Some(&[9][..]));
        tables.snapshot(&CDemoStringTables::default());
        assert_eq!(tables.instance_baseline(19), Some(&[6][..]));
        tables.clear();
        assert_eq!(tables.instance_baseline(19), None);
    }
    #[test]
    fn keyless_updates_and_partial_snapshots_preserve_animation_context() {
        let mut fixed = AnimationTable {
            name: "AnimAssetData".into(),
            fixed: true,
            bits: 3,
            ..Default::default()
        };
        fixed.decode(&[0x6d, 0x07], 2).unwrap();
        assert_eq!(fixed.entries[&0].1, [5]);
        assert_eq!(fixed.entries[&1].1, [3]);
        let encoded = update(None, Some("graph"), &[1, 2, 3]);
        for end in 0..encoded.len() {
            let mut truncated = AnimationTable {
                name: "AnimAssetData".into(),
                ..Default::default()
            };
            assert!(truncated.decode(&encoded[..end], 1).is_err());
        }
        let mut tables = AnimationStrings::default();
        tables
            .create(&CsvcMsgCreateStringTable {
                name: Some("AnimAssetData".into()),
                num_entries: Some(1),
                string_data: Some(update(None, Some("graph"), &[1, 2, 3]).into()),
                ..Default::default()
            })
            .unwrap();
        tables
            .update(&CsvcMsgUpdateStringTable {
                table_id: Some(0),
                num_changed_entries: Some(1),
                string_data: Some(update(None, None, &[4, 5]).into()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(tables.tables[0].entries[&0], ("graph".into(), vec![4, 5]));
        let mut snapshot = CDemoStringTables::default();
        snapshot.tables.push(csgoproto::c_demo_string_tables::TableT {
            table_name: Some("userinfo".into()),
            ..Default::default()
        });
        tables.snapshot(&snapshot);
        assert_eq!(tables.tables[0].name, "AnimAssetData");
        assert_eq!(tables.tables[0].entries[&0].1, [4, 5]);
        tables
            .update(&CsvcMsgUpdateStringTable {
                table_id: Some(0),
                num_changed_entries: Some(1),
                string_data: Some(update(None, None, &[]).into()),
                ..Default::default()
            })
            .unwrap();
        assert!(tables.tables[0].entries[&0].1.is_empty());
        tables
            .update(&CsvcMsgUpdateStringTable {
                table_id: Some(0),
                num_changed_entries: Some(1),
                string_data: Some(update(Some(0), Some("second"), &[9]).into()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(tables.tables[0].entries[&1], ("second".into(), vec![9]));
        tables.clear();
        assert!(tables.tables.is_empty());
        assert!(tables.dirty);
    }
}
