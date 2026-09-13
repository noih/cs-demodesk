//! Diagnostic smoke journal decoding, not rendered opacity or a visibility verdict.
//! Format evidence: https://github.com/osztenkurden/cs2parser/blob/master/docs/smoke-voxel-format.md
use anyhow::{ensure, Context, Result};
use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Record {
    pub sequence: u16,
    pub heartbeat: bool,
    pub active_flag: u8,
    pub section_flags: u8,
    pub seed_cells: Option<Vec<SeedCell>>,
    /// Undecoded density/palette data is preserved; seed cells are not the visible cloud.
    pub remaining_payload: Vec<u8>,
}
#[derive(Debug, Serialize)]
pub struct SeedCell {
    pub grid: [u8; 3],
    pub state: [u8; 5],
}

pub fn decode(bytes: &[Option<u8>]) -> Result<Vec<Record>> {
    let data: Vec<u8> = bytes
        .iter()
        .copied()
        .collect::<Option<_>>()
        .context("smoke byte slots were not received")?;
    let mut remaining = data.as_slice();
    let mut records: Vec<Record> = Vec::new();
    while !remaining.is_empty() {
        ensure!(remaining.len() >= 4, "truncated smoke record header");
        let sequence = u16::from_le_bytes([remaining[0], remaining[1]]);
        let length = u16::from_le_bytes([remaining[2], remaining[3]]) as usize;
        ensure!(
            length >= 2 && length <= remaining.len() - 4,
            "invalid smoke payload length"
        );
        if let Some(previous) = records.last() {
            ensure!(
                sequence == previous.sequence.wrapping_add(1),
                "non-contiguous smoke journal"
            );
        } else {
            ensure!(
                sequence == 0,
                "smoke journal does not start at sequence zero"
            );
        }
        let payload = &remaining[4..4 + length];
        let heartbeat = payload == [0, 0, 0];
        let section_flags = payload[1];
        ensure!(section_flags & !3 == 0, "unsupported smoke section flags");
        let mut consumed = if heartbeat { 3 } else { 2 };
        let seed_cells = if section_flags & 1 != 0 {
            let count = *payload.get(2).context("missing smoke cell count")? as usize;
            consumed = 3 + count * 8;
            ensure!(consumed <= payload.len(), "truncated smoke cell list");
            let mut cells = Vec::with_capacity(count);
            for entry in payload[3..consumed].chunks_exact(8) {
                ensure!(
                    entry[..3].iter().all(|v| *v < 32),
                    "invalid smoke grid coordinate"
                );
                cells.push(SeedCell {
                    grid: [entry[2], entry[1], entry[0]],
                    state: entry[3..8].try_into().unwrap(),
                });
            }
            Some(cells)
        } else {
            None
        };
        records.push(Record {
            sequence,
            heartbeat,
            active_flag: payload[0],
            section_flags,
            seed_cells,
            remaining_payload: payload[consumed..].to_vec(),
        });
        remaining = &remaining[4 + length..];
    }
    Ok(records)
}

/// Increment schema for incompatible data changes; implementation for algorithm changes.
pub fn contract() -> super::Contract {
    super::Contract {
        module: "smoke-journal".into(),
        schema_version: 1,
        implementation_version: "0.1.0".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn journal_preserves_seed_state_and_rejects_missing_or_truncated_data() {
        let bytes = [
            0, 0, 13, 0, 1, 3, 1, 2, 3, 4, 128, 255, 0, 6, 7, 90, 91, 1, 0, 3, 0, 0, 0, 0,
        ];
        let mut input: Vec<_> = bytes.into_iter().map(Some).collect();
        let decoded = decode(&input).unwrap();
        assert_eq!(decoded[0].seed_cells.as_ref().unwrap()[0].grid, [4, 3, 2]);
        assert_eq!(
            decoded[0].seed_cells.as_ref().unwrap()[0].state,
            [128, 255, 0, 6, 7]
        );
        assert_eq!(decoded[0].remaining_payload, [90, 91]);
        assert!(decoded[1].heartbeat);
        for end in [1, 3, 5, 16, 18, 23] {
            assert!(decode(&input[..end]).is_err());
        }
        input[10] = None;
        assert!(decode(&input).is_err());
    }
}
