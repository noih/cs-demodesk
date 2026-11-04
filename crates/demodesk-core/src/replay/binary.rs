//! Disposable binary transport cache; the JSON replay remains the source of truth.
use anyhow::{anyhow, ensure, Context, Result};
use flate2::{read::GzDecoder, write::GzEncoder, Compression};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

const MAGIC: &[u8; 8] = b"DDRPLY01";
const HEADER: usize = 80;

fn stamp(path: &Path) -> Result<(u64, u128)> {
    let metadata = fs::metadata(path)?;
    Ok((
        metadata.len(),
        metadata.modified()?.duration_since(UNIX_EPOCH)?.as_nanos(),
    ))
}

fn current(path: &Path, source: (u64, u128)) -> bool {
    let Ok(file) = fs::File::open(path) else {
        return false;
    };
    // Validate only the header here; the worker verifies the complete payload.
    let mut file = GzDecoder::new(file);
    let mut h = [0u8; HEADER];
    if file.read_exact(&mut h).is_err() || &h[..8] != MAGIC {
        return false;
    }
    let length = u64::from_le_bytes(h[8..16].try_into().unwrap());
    let modified = u128::from_le_bytes(h[16..32].try_into().unwrap());
    let meta = u64::from_le_bytes(h[32..40].try_into().unwrap());
    let rows = u64::from_le_bytes(h[40..48].try_into().unwrap());
    let expected = meta
        .checked_add(7)
        .map(|n| n & !7)
        .and_then(|n| n.checked_add(HEADER as u64))
        .and_then(|n| rows.checked_mul(8).and_then(|bytes| n.checked_add(bytes)));
    expected.is_some() && source == (length, modified)
}

fn take_rows(frame: &mut Value, key: &str, values: &mut Vec<f64>) -> Result<()> {
    let group = frame
        .as_object_mut()
        .ok_or_else(|| anyhow!("Invalid replay frame"))?
        .remove(key);
    let Some(group) = group.filter(|v| !v.is_null()) else {
        values.push(-1.0);
        return Ok(());
    };
    let rows = group.as_array().context("Invalid replay group")?;
    values.push(rows.len() as f64);
    for row in rows {
        let row = row
            .as_array()
            .ok_or_else(|| anyhow!("Invalid replay row"))?;
        values.push(row.len() as f64);
        for number in row {
            values.push(
                number
                    .as_f64()
                    .ok_or_else(|| anyhow!("Invalid replay number"))?,
            );
        }
    }
    Ok(())
}

pub fn ensure(source: &Path) -> Result<PathBuf> {
    let target = source.with_extension("bin");
    let source_stamp = stamp(source)?;
    if current(&target, source_stamp) {
        return Ok(target);
    }
    let mut meta: Value =
        serde_json::from_reader(std::io::BufReader::new(fs::File::open(source)?))?;
    ensure!(
        meta["schemaVersion"] == super::REPLAY_SCHEMA_VERSION,
        "Unsupported replay schema"
    );
    let object = meta.as_object_mut().context("Invalid replay object")?;
    let mut frames = object.remove("frames").context("Missing replay frames")?;
    let mut smoke = object.remove("smoke");
    let mut rows = Vec::new();
    for frame in frames.as_array_mut().context("Invalid replay frames")? {
        for field in ["p", "g", "d", "f"] {
            take_rows(frame, field, &mut rows)?;
        }
    }
    if let Some(smoke) = &mut smoke {
        for frame in smoke.as_array_mut().context("Invalid smoke frames")? {
            take_rows(frame, "cells", &mut rows)?;
        }
    }
    let mut packed = json!({"meta":meta,"frames":frames});
    if let Some(smoke) = smoke {
        packed["smoke"] = smoke;
    }
    let metadata = serde_json::to_vec(&packed)?;
    let metadata_len = metadata.len();
    let mut payload = metadata;
    payload.resize((payload.len() + 7) & !7, 0);
    for value in &rows {
        payload.extend_from_slice(&value.to_le_bytes());
    }
    let mut header = Vec::with_capacity(HEADER);
    header.extend_from_slice(MAGIC);
    header.extend_from_slice(&source_stamp.0.to_le_bytes());
    header.extend_from_slice(&source_stamp.1.to_le_bytes());
    header.extend_from_slice(&(metadata_len as u64).to_le_bytes());
    header.extend_from_slice(&(rows.len() as u64).to_le_bytes());
    header.extend_from_slice(&Sha256::digest(&payload));
    let mut staged =
        tempfile::NamedTempFile::new_in(source.parent().context("Missing replay directory")?)?;
    let mut compressed = GzEncoder::new(&mut staged, Compression::fast());
    compressed.write_all(&header)?;
    compressed.write_all(&payload)?;
    compressed.finish()?;
    ensure!(
        stamp(source)? == source_stamp,
        "Replay changed while caching"
    );
    staged.persist(&target)?;
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failed_cache_creation_preserves_json() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("fixture.replay.json");
        let data =
            json!({"schemaVersion":super::super::REPLAY_SCHEMA_VERSION,"frames":[]}).to_string();
        fs::write(&source, &data).unwrap();
        fs::create_dir(source.with_extension("bin")).unwrap();
        assert!(ensure(&source).is_err());
        assert_eq!(fs::read_to_string(&source).unwrap(), data);
        fs::remove_dir(source.with_extension("bin")).unwrap();
        fs::write(&source, r#"{"schemaVersion":0,"frames":[]}"#).unwrap();
        assert!(ensure(&source).is_err());
        assert!(!source.with_extension("bin").exists());
        fs::write(
            &source,
            json!({"schemaVersion":super::super::REPLAY_SCHEMA_VERSION,"frames":[{"p":3}]})
                .to_string(),
        )
        .unwrap();
        assert!(ensure(&source).is_err());
    }

    #[test]
    fn binary_cache_is_reused_and_rebuilt_when_stale_or_truncated() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("fixture.replay.json");
        let data = json!({"schemaVersion":super::super::REPLAY_SCHEMA_VERSION,"frames":[{"t":1,"p":[[1,2.5,-3]],"d":[]}],"smoke":[{"t":1,"cells":null}]});
        fs::write(&source, data.to_string()).unwrap();
        let path = ensure(&source).unwrap();
        let before = stamp(&path).unwrap();
        assert_eq!(ensure(&source).unwrap(), path);
        assert_eq!(stamp(&path).unwrap(), before);
        let bytes = fs::read(&path).unwrap();
        let mut decoded = Vec::new();
        GzDecoder::new(bytes.as_slice())
            .read_to_end(&mut decoded)
            .unwrap();
        assert_eq!(&decoded[48..80], Sha256::digest(&decoded[80..]).as_slice());
        fs::write(&path, b"partial").unwrap();
        ensure(&source).unwrap();
        assert_eq!(fs::read(&path).unwrap(), bytes);
        fs::write(&source, format!("{} ", data)).unwrap();
        ensure(&source).unwrap();
        assert!(current(&path, stamp(&source).unwrap()));
        assert_ne!(fs::read(&path).unwrap(), bytes);
        fs::write(&source, b"broken JSON").unwrap();
        assert!(ensure(&source).is_err());
    }
}
