//! Ordered convar snapshots from a complete raw demo, independent of entity parsing.
use csgoproto::{CDemoFileHeader, CDemoFullPacket, CDemoPacket, CMsgTeFireBullets, CnetMsgSetConVar, CnetMsgSignonState};
use prost::Message;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug)]
pub struct FireConvars {
    pub tick: i32,
    pub player: u32,
    pub seed: u32,
    pub item_def_index: u32,
    pub values: BTreeMap<String, String>,
}
#[derive(Debug)]
pub struct VerifiedConvars {
    header: CDemoFileHeader,
    fires: Vec<FireConvars>,
}
impl VerifiedConvars {
    pub fn header(&self) -> &CDemoFileHeader {
        &self.header
    }
    pub fn fires(&self) -> &[FireConvars] {
        &self.fires
    }
}
fn varint(data: &[u8], pos: &mut usize) -> Result<u32, String> {
    let mut value = 0;
    for shift in (0..35).step_by(7) {
        let byte = *data.get(*pos).ok_or("truncated varint")?;
        *pos += 1;
        if shift == 28 && byte > 15 {
            return Err("oversized varint".into());
        }
        value |= u32::from(byte & 127) << shift;
        if byte < 128 {
            return Ok(value);
        }
    }
    Err("invalid varint".into())
}
struct Bits<'a> {
    data: &'a [u8],
    pos: usize,
}
impl Bits<'_> {
    fn read(&mut self, n: usize) -> Result<u32, String> {
        if n > 32 || self.pos + n > self.data.len() * 8 {
            return Err("truncated packet bits".into());
        }
        let mut v = 0;
        for i in 0..n {
            v |= u32::from((self.data[(self.pos + i) / 8] >> ((self.pos + i) % 8)) & 1) << i;
        }
        self.pos += n;
        Ok(v)
    }
    fn kind(&mut self) -> Result<u32, String> {
        let v = self.read(6)?;
        Ok(match v & 48 {
            16 => (v & 15) | self.read(4)? << 4,
            32 => (v & 15) | self.read(8)? << 4,
            48 => (v & 15) | self.read(28)? << 4,
            _ => v,
        })
    }
    fn size(&mut self) -> Result<usize, String> {
        let mut v = 0;
        for shift in (0..35).step_by(7) {
            let b = self.read(8)?;
            if shift == 28 && b > 15 {
                return Err("oversized message".into());
            }
            v |= (b & 127) << shift;
            if b < 128 {
                return Ok(v as usize);
            }
        }
        Err("invalid message size".into())
    }
}
/// No early exit or resumed parsing: completeness is derived from the wire sequence.
pub fn read(data: &[u8], selected: &[&str]) -> Result<VerifiedConvars, String> {
    if data.len() < 16 || &data[..8] != b"PBDEMS2\0" {
        return Err("invalid demo prefix".into());
    }
    let mut pos = 16;
    let mut header = None;
    let mut snapshot = false;
    let mut state = 0;
    let mut started = false;
    let mut values = BTreeMap::new();
    let mut fires = Vec::new();
    let mut keys = BTreeSet::new();
    loop {
        let cmd = varint(data, &mut pos)?;
        let tick = varint(data, &mut pos)? as i32;
        let size = varint(data, &mut pos)? as usize;
        let end = pos.checked_add(size).ok_or("oversized frame")?;
        let raw = data.get(pos..end).ok_or("truncated frame")?;
        pos = end;
        let kind = cmd & !64;
        if kind >= 18 {
            return Err("unsupported demo command or recovery".into());
        }
        if header.is_none() && kind != 1 {
            return Err("missing initial header".into());
        }
        if header.is_some() && !matches!(kind, 0 | 1 | 7 | 8 | 13) {
            continue;
        }
        let owned;
        let payload = if cmd & 64 != 0 {
            let size = snap::raw::decompress_len(raw).map_err(|e| e.to_string())?;
            if size > 256 * 1024 * 1024 {
                return Err("oversized decompressed frame".into());
            }
            owned = snap::raw::Decoder::new().decompress_vec(raw).map_err(|e| e.to_string())?;
            &owned[..]
        } else {
            raw
        };
        if header.is_none() {
            header = Some(CDemoFileHeader::decode(payload).map_err(|e| e.to_string())?);
            continue;
        }
        if kind == 1 {
            return Err("repeated demo header".into());
        }
        if kind == 0 {
            if !started || !snapshot || state < 5 {
                return Err("incomplete signon".into());
            }
            return Ok(VerifiedConvars {
                header: header.unwrap(),
                fires,
            });
        }
        if !matches!(kind, 7 | 8 | 13) {
            continue;
        }
        if kind != 8 {
            if !snapshot || state < 5 {
                return Err("packet precedes convar signon".into());
            }
            started = true;
        } else if started {
            return Err("late signon packet".into());
        }
        let packet = if kind == 13 {
            CDemoFullPacket::decode(payload)
                .map_err(|e| e.to_string())?
                .packet
                .ok_or("missing full packet")?
        } else {
            CDemoPacket::decode(payload).map_err(|e| e.to_string())?
        };
        let bytes = packet.data.ok_or("missing packet data")?;
        let mut bits = Bits { data: &bytes, pos: 0 };
        while bits.pos + 8 < bytes.len() * 8 {
            let typ = bits.kind()?;
            let size = bits.size()?;
            let end = bits
                .pos
                .checked_add(size.checked_mul(8).ok_or("oversized message")?)
                .ok_or("oversized message")?;
            if end > bytes.len() * 8 {
                return Err("truncated message".into());
            }
            if !matches!(typ, 6 | 7 | 452) {
                bits.pos = end;
                continue;
            }
            let msg = (0..size).map(|_| bits.read(8).map(|b| b as u8)).collect::<Result<Vec<_>, _>>()?;
            match typ {
                6 => {
                    let cvars = CnetMsgSetConVar::decode(&msg[..])
                        .map_err(|e| e.to_string())?
                        .convars
                        .ok_or("missing convars")?;
                    if !started && state == 0 {
                        snapshot = true;
                    }
                    for cv in cvars.cvars {
                        let name = cv.name.ok_or("missing convar name")?;
                        let value = cv.value.ok_or("missing convar value")?;
                        if selected.contains(&name.as_str()) {
                            values.insert(name, value);
                        }
                    }
                }
                7 => {
                    let next = CnetMsgSignonState::decode(&msg[..])
                        .map_err(|e| e.to_string())?
                        .signon_state
                        .ok_or("missing signon state")?;
                    if !started {
                        if !snapshot || next > 6 || (state == 0 && next != 3) || (state != 0 && next != state + 1) {
                            return Err("reordered convar signon".into());
                        }
                        state = next;
                    } else {
                        // FULL may arrive in the first normal packet. A reconnect or
                        // changelevel requires another baseline and is not supported here.
                        if next != 6 || state > 6 {
                            return Err("post-start signon reset".into());
                        }
                        state = next;
                    }
                }
                452 => {
                    if !started {
                        return Err("fire before signon completion".into());
                    }
                    let fire = CMsgTeFireBullets::decode(&msg[..]).map_err(|e| e.to_string())?;
                    let player = fire.player.ok_or("missing fire player")?;
                    let seed = fire.seed.ok_or("missing fire seed")?;
                    let item_def_index = fire.item_def_index.ok_or("missing fire item")?;
                    if !keys.insert((tick, player, seed, item_def_index)) {
                        return Err("ambiguous fire identity".into());
                    }
                    fires.push(FireConvars {
                        tick,
                        player,
                        seed,
                        item_def_index,
                        values: values.clone(),
                    });
                }
                _ => unreachable!(),
            }
        }
    }
}
