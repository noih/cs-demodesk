//! Latest complete match assessment and its owned intermediate data.
use super::*;
use anyhow::{ensure, Context as _, Result};
use std::{
    fs,
    io::{BufRead, Read, Seek, Write},
    path::{Path, PathBuf},
};
fn key(demo_id: &str, player_id: &str) -> String {
    sha1_smol::Sha1::from(format!("{demo_id}\0{player_id}"))
        .digest()
        .to_string()
}
pub fn list(root: &Path, demo_id: &str, player_id: &str) -> Result<Vec<Assessment>> {
    Ok(list_match(root, demo_id)?
        .remove(player_id)
        .unwrap_or_default())
}
/// Read and decode the complete latest result once for all players.
pub fn list_match(root: &Path, demo_id: &str) -> Result<BTreeMap<String, Vec<Assessment>>> {
    let path = match_directory(root, demo_id).join("latest.json");
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(error) => return Err(error.into()),
    };
    let records: Vec<Assessment> = serde_json::from_slice(&bytes)?;
    let mut players = BTreeMap::new();
    for record in records {
        ensure!(
            record.schema_version == 2 && record.demo_id == demo_id,
            "invalid match history"
        );
        ensure!(
            !players.contains_key(&record.player_id),
            "duplicate player in match history"
        );
        players.insert(record.player_id.clone(), vec![record]);
    }
    Ok(players)
}
pub struct Inputs {
    pub values: BTreeMap<String, serde_json::Value>,
    pub body_journal: Option<fs::File>,
    pub provenance: serde_json::Value,
}
/// Generic producer inputs are keyed by rule ID. No rule owns shared map/pose reconstruction.
pub fn measurements(root: &Path, fingerprint: &str, player_id: &str) -> Result<Inputs> {
    let mut journal = root
        .join("analysis")
        .join("body-measurements")
        .join(format!(
            "{}.ndjson",
            sha1_smol::Sha1::from(fingerprint).digest()
        ));
    let compressed = journal.with_extension("ndjson.gz");
    if compressed.try_exists()? {
        journal = compressed;
    }
    if journal.try_exists()? {
        let mut options = fs::OpenOptions::new();
        options.read(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.share_mode(1);
        }
        let mut file = options.open(journal)?;
        let mut header_bytes = Vec::new();
        std::io::BufReader::new(
            crate::analysis::body_journal::reader(&file)?.take(1024 * 1024 + 1),
        )
        .read_until(b'\n', &mut header_bytes)?;
        ensure!(
            header_bytes.len() <= 1024 * 1024,
            "oversized body journal header"
        );
        let header: crate::analysis::Artifact<crate::analysis::body_journal::BodyClock> =
            serde_json::from_slice(&header_bytes)?;
        ensure!(
            matches!(header.contract.schema_version, 1 | 2),
            "unsupported body journal schema"
        );
        header.require("body-measurement-journal", header.contract.schema_version)?;
        ensure!(
            header.source.demo_fingerprint.as_deref() == Some(fingerprint),
            "analysis input belongs to a different demo"
        );
        file.rewind()?;
        let mut hash = sha1_smol::Sha1::new();
        let mut buffer = [0u8; 65536];
        loop {
            let n = file.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            hash.update(&buffer[..n]);
        }
        file.rewind()?;
        let provenance = serde_json::json!({"contract":header.contract,"source":header.source,"dependencies":header.dependencies,"contentFingerprint":format!("sha1:{}",hash.digest())});
        return Ok(Inputs {
            values: BTreeMap::new(),
            body_journal: Some(file),
            provenance,
        });
    }
    legacy_measurements(root, fingerprint, player_id)
}
pub fn legacy_measurements(root: &Path, fingerprint: &str, player_id: &str) -> Result<Inputs> {
    let path = root
        .join("analysis")
        .join("scoring-inputs")
        .join(format!("{}.json", key(fingerprint, player_id)));
    if !path.try_exists()? {
        return Ok(Inputs {
            values: BTreeMap::new(),
            body_journal: None,
            provenance: serde_json::Value::Null,
        });
    }
    let bytes = fs::read(path)?;
    let file: crate::analysis::Artifact<BTreeMap<String, serde_json::Value>> =
        serde_json::from_slice(&bytes)?;
    file.require("scoring-measurements", 1)?;
    ensure!(
        file.source.demo_fingerprint.as_deref() == Some(fingerprint),
        "analysis input belongs to a different demo"
    );
    Ok(Inputs {
        values: file.data,
        body_journal: None,
        provenance: serde_json::json!({"contract":file.contract,"source":file.source,"dependencies":file.dependencies,"sourceBytes":bytes.len(),"contentFingerprint":format!("sha1:{}",sha1_smol::Sha1::from(bytes.as_slice()).digest())}),
    })
}
fn match_directory(root: &Path, demo_id: &str) -> PathBuf {
    root.join("behavior-analysis")
        .join("matches")
        .join(sha1_smol::Sha1::from(demo_id).digest().to_string())
}
/// Publish every player's result together; an interrupted run cannot leave half a match.
pub fn save_match(root: &Path, records: &mut [Assessment]) -> Result<()> {
    let first = records.first().context("empty match assessment")?;
    let demo_id = first.demo_id.clone();
    let fingerprint = first.demo_fingerprint.clone();
    let mut players = std::collections::BTreeSet::new();
    ensure!(
        records.iter().all(|r| r.demo_id == demo_id
            && r.demo_fingerprint == fingerprint
            && players.insert(&r.player_id)),
        "inconsistent match assessment"
    );
    let dir = match_directory(root, &demo_id);
    fs::create_dir_all(&dir)?;
    let mut temp = tempfile::Builder::new()
        .prefix("match-")
        .suffix(".pending")
        .tempfile_in(&dir)?;
    let run = temp
        .path()
        .file_stem()
        .context("invalid match filename")?
        .to_string_lossy()
        .into_owned();
    for record in records.iter_mut() {
        record.id = format!(
            "{run}-{}",
            sha1_smol::Sha1::from(&record.player_id).digest()
        );
    }
    serde_json::to_writer(&mut temp, records)?;
    temp.flush()?;
    temp.as_file().sync_all()?;
    let latest = dir.join("latest.json");
    temp.persist(&latest)?;
    // Remove former append-only runs only after the complete replacement is durable.
    for entry in fs::read_dir(&dir)? {
        let path = entry?.path();
        if path != latest
            && path.extension().and_then(|s| s.to_str()) == Some("json")
            && path
                .file_name()
                .and_then(|s| s.to_str())
                .is_some_and(|s| s.starts_with("match-"))
        {
            fs::remove_file(path)?;
        }
    }
    Ok(())
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchResponse {
    pub source_fingerprint: String,
    pub players: BTreeMap<String, Vec<Assessment>>,
    pub preparation_seconds: f64,
    pub analysis_seconds: f64,
    pub shared_bytes: u64,
    pub generic_bytes: u64,
    pub diagnostic_bytes: u64,
}

/// Track ownership before conversion, including attempts that fail before saving results.
pub fn track_source(root: &Path, demo_id: &str, fingerprint: &str) -> Result<()> {
    let dir = match_directory(root, demo_id);
    fs::create_dir_all(&dir)?;
    let path = dir.join("sources.json");
    let mut sources: Vec<String> = match fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes)?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(e) => return Err(e.into()),
    };
    if !sources.iter().any(|s| s == fingerprint) {
        sources.push(fingerprint.into());
        crate::store::write_atomic(&path, &serde_json::to_vec(&sources)?)?;
    }
    Ok(())
}

pub fn delete_match(root: &Path, demo_id: &str) -> Result<()> {
    let dir = match_directory(root, demo_id);
    let mut sources: Vec<String> = match fs::read(dir.join("sources.json")) {
        Ok(bytes) => serde_json::from_slice(&bytes)?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(e) => return Err(e.into()),
    };
    for records in list_match(root, demo_id)?.values() {
        sources.extend(records.iter().map(|r| r.demo_fingerprint.clone()));
    }
    for source in sources {
        let prefix = format!("{}-", sha1_smol::Sha1::from(source.as_str()).digest());
        match fs::read_dir(root.join("analysis/match-state")) {
            Ok(entries) => {
                for entry in entries {
                    let entry = entry?;
                    if entry.file_name().to_string_lossy().starts_with(&prefix) {
                        fs::remove_file(entry.path())?;
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    match fs::remove_dir_all(dir) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}
