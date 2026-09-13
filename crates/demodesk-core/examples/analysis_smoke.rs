//! Decode a versioned raw scene independently, preserving per-entity failures.
use anyhow::{ensure, Context, Result};
use demodesk_core::analysis::{smoke, Artifact};
use serde::Serialize;
use serde_json::Value;
use std::{io::Write, path::Path};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Journal {
    tick: i64,
    entity_id: i64,
    serial: u64,
    records: Option<Vec<smoke::Record>>,
    error: Option<String>,
}
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    ensure!(args.len() == 2, "expected SCENE.json OUTPUT.json");
    let bytes = std::fs::read(&args[0])?;
    let scene: Artifact<Value> = serde_json::from_slice(&bytes)?;
    let frames = scene
        .require("packet-scene", 1)?
        .as_array()
        .context("expected scene frames")?;
    let mut journals = Vec::new();
    for frame in frames {
        let tick = frame["tick"].as_i64().context("missing tick")?;
        for entity in frame["entities"].as_array().context("missing entities")? {
            if entity["className"] != "CSmokeGrenadeProjectile" {
                continue;
            }
            let result = (|| -> Result<_> {
                ensure!(
                    !entity["smokeVoxelBytes"].is_null(),
                    "smoke bytes unavailable"
                );
                let raw: Vec<Option<u8>> =
                    serde_json::from_value(entity["smokeVoxelBytes"].clone())?;
                smoke::decode(&raw)
            })();
            let (records, error) = match result {
                Ok(records) => (Some(records), None),
                Err(error) => (None, Some(error.to_string())),
            };
            journals.push(Journal {
                tick,
                entity_id: entity["entityId"].as_i64().context("missing entity ID")?,
                serial: entity["serial"].as_u64().context("missing serial")?,
                records,
                error,
            });
        }
    }
    let artifact = Artifact {
        contract: smoke::contract(),
        source: scene.source,
        dependencies: vec![format!(
            "sha1:{}",
            sha1_smol::Sha1::from(bytes.as_slice()).digest()
        )],
        data: journals,
    };
    let path = Path::new(&args[1]);
    let mut temp = tempfile::NamedTempFile::new_in(
        path.parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new(".")),
    )?;
    serde_json::to_writer(&mut temp, &artifact)?;
    temp.flush()?;
    temp.as_file().sync_all()?;
    temp.persist_noclobber(path)?;
    Ok(())
}
