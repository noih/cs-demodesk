//! Check Source 2 framing without decoding or decompressing game messages.
use std::io::{self, BufReader, Read, Seek};

pub(crate) fn is_complete(file: &mut std::fs::File) -> io::Result<bool> {
    let length = file.metadata()?.len();
    let mut reader = BufReader::new(file);
    match check(&mut reader, length) {
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => Ok(false),
        result => result.map(|tick| tick.is_some()),
    }
}

pub(crate) fn is_complete_bytes(bytes: &[u8]) -> io::Result<bool> {
    end_tick_bytes(bytes).map(|tick| tick.is_some())
}

/// Last gameplay packet tick, only when the stream has a complete Stop frame.
pub(crate) fn end_tick_bytes(bytes: &[u8]) -> io::Result<Option<i32>> {
    let mut reader = BufReader::new(std::io::Cursor::new(bytes));
    let result = check(&mut reader, bytes.len() as u64);
    match result {
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => Ok(None),
        result => result,
    }
}

pub(crate) fn varint(reader: &mut impl Read, position: &mut u64) -> io::Result<Option<u32>> {
    let mut value = 0;
    for shift in (0..35).step_by(7) {
        let mut byte = [0];
        reader.read_exact(&mut byte)?;
        *position += 1;
        if shift == 28 && byte[0] > 15 {
            return Ok(None);
        }
        value |= u32::from(byte[0] & 127) << shift;
        if byte[0] & 128 == 0 {
            return Ok(Some(value));
        }
    }
    Ok(None)
}

fn check(reader: &mut BufReader<impl Read + Seek>, length: u64) -> io::Result<Option<i32>> {
    let mut header = [0; 16];
    reader.read_exact(&mut header)?;
    if &header[..8] != b"PBDEMS2\0" {
        return Ok(None);
    }
    let mut position = 16;
    let mut end_tick = 0;
    while position < length {
        let Some(command) = varint(reader, &mut position)? else {
            return Ok(None);
        };
        let Some(tick) = varint(reader, &mut position)? else {
            return Ok(None);
        };
        let Some(size) = varint(reader, &mut position)? else {
            return Ok(None);
        };
        position += u64::from(size);
        if position > length {
            return Ok(None);
        }
        // DEM_Stop terminates the gameplay stream; metadata may follow it.
        if command & !64 == 0 {
            return Ok(Some(end_tick));
        }
        // DEM_Packet / DEM_FullPacket contain gameplay; headers can use tick -1.
        if matches!(command & !64, 7 | 13) {
            end_tick = end_tick.max(tick as i32);
        }
        reader.seek_relative(i64::from(size))?;
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_complete_frames_with_stop_are_ready() {
        let mut bytes = b"PBDEMS2\0".to_vec();
        bytes.extend_from_slice(&[0; 8]);
        // A compressed packet containing zeros must not be mistaken for Stop.
        bytes.extend_from_slice(&[71, 1, 3, 0, 0, 0]);
        bytes.extend_from_slice(&[0, 2, 0]);
        for end in 0..=bytes.len() {
            assert_eq!(
                is_complete_bytes(&bytes[..end]).unwrap(),
                end == bytes.len(),
                "prefix {end}"
            );
        }
        bytes.extend_from_slice(&[255, 255]); // Trailer is outside the gameplay stream.
        assert!(is_complete_bytes(&bytes).unwrap());
        assert_eq!(end_tick_bytes(&bytes).unwrap(), Some(1));
    }

    #[test]
    fn end_tick_uses_gameplay_not_negative_headers_stop_or_trailer() {
        let mut bytes = b"PBDEMS2\0".to_vec();
        bytes.extend_from_slice(&[0; 8]);
        bytes.extend_from_slice(&[1, 255, 255, 255, 255, 15, 0]); // Header at -1.
        bytes.extend_from_slice(&[71, 10, 0, 77, 20, 0]); // Compressed packet and full packet.
        assert_eq!(end_tick_bytes(&bytes).unwrap(), None);
        bytes.extend_from_slice(&[0, 30, 0, 7, 40, 0]); // Stop, then unrelated trailer.
        assert_eq!(end_tick_bytes(&bytes).unwrap(), Some(20));
    }
}
