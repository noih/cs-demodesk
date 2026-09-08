//! Check Source 2 framing without decoding or decompressing game messages.
use std::io::{self, BufReader, Read, Seek};

pub(crate) fn is_complete(file: &mut std::fs::File) -> io::Result<bool> {
    let length = file.metadata()?.len();
    let mut reader = BufReader::new(file);
    let result = check(&mut reader, length);
    match result {
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => Ok(false),
        result => result,
    }
}

fn varint(reader: &mut impl Read, position: &mut u64) -> io::Result<Option<u32>> {
    let mut value = 0;
    for shift in (0..35).step_by(7) {
        let mut byte = [0];
        reader.read_exact(&mut byte)?;
        *position += 1;
        if shift == 28 && byte[0] > 15 { return Ok(None); }
        value |= u32::from(byte[0] & 127) << shift;
        if byte[0] & 128 == 0 { return Ok(Some(value)); }
    }
    Ok(None)
}

fn check(reader: &mut BufReader<impl Read + Seek>, length: u64) -> io::Result<bool> {
    let mut header = [0; 16];
    reader.read_exact(&mut header)?;
    if &header[..8] != b"PBDEMS2\0" { return Ok(false); }
    let mut position = 16;
    while position < length {
        let Some(command) = varint(reader, &mut position)? else { return Ok(false); };
        let Some(_) = varint(reader, &mut position)? else { return Ok(false); };
        let Some(size) = varint(reader, &mut position)? else { return Ok(false); };
        position += u64::from(size);
        if position > length { return Ok(false); }
        // DEM_Stop terminates the gameplay stream; metadata may follow it.
        if command & !64 == 0 { return Ok(true); }
        reader.seek_relative(i64::from(size))?;
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn only_complete_frames_with_stop_are_ready() {
        let mut bytes = b"PBDEMS2\0".to_vec();
        bytes.extend_from_slice(&[0; 8]);
        // A compressed packet containing zeros must not be mistaken for Stop.
        bytes.extend_from_slice(&[71, 1, 3, 0, 0, 0]);
        bytes.extend_from_slice(&[0, 2, 0]);
        for end in 0..=bytes.len() {
            let mut file = tempfile::tempfile().unwrap();
            file.write_all(&bytes[..end]).unwrap();
            file.rewind().unwrap();
            assert_eq!(is_complete(&mut file).unwrap(), end == bytes.len(), "prefix {end}");
        }
        bytes.extend_from_slice(&[255, 255]); // Trailer is outside the gameplay stream.
        let mut file = tempfile::tempfile().unwrap();
        file.write_all(&bytes).unwrap();
        file.rewind().unwrap();
        assert!(is_complete(&mut file).unwrap());
    }
}
