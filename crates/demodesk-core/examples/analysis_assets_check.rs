//! End-to-end generic recorded animation dependency preparation check.
use anyhow::{ensure, Context, Result};
use demodesk_core::analysis::{animation_assets, compact};
use std::{collections::BTreeSet, fs::File, io::BufReader, path::Path, time::Instant};

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    ensure!(
        (4..=5).contains(&args.len()),
        "expected MATCH_STATE.gz GAME_DIRECTORY VRF_EXECUTABLE NEW_OR_EXISTING_CACHE_ROOT [MODEL_HANDLES_COMMA_SEPARATED]"
    );
    let model_handles: BTreeSet<u64> = args
        .get(4)
        .map(|s| s.split(',').map(str::parse).collect::<Result<_, _>>())
        .transpose()?
        .unwrap_or_default();
    let started = Instant::now();
    let mut seen = BTreeSet::new();
    let mut resources = BTreeSet::new();
    let mut all_names = BTreeSet::new();
    compact::visit(
        flate2::read::MultiGzDecoder::new(BufReader::new(File::open(&args[0])?)),
        |frame| {
            for id in frame.changed {
                let field = &frame.fields[*id as usize];
                if field.class != "AnimationContext"
                    || !field.name.starts_with("AnimAssetData/")
                    || !field.name.ends_with("/data")
                {
                    continue;
                }
                let Some(value) = frame.values.get(id) else {
                    continue;
                };
                if !seen.insert(value.clone()) {
                    continue;
                }
                ensure!(
                    value.first() == Some(&14) && value.len() >= 9,
                    "invalid animation asset dictionary bytes"
                );
                let data = &value[1..];
                ensure!(
                    u32::from_le_bytes(data[..4].try_into()?) == 1,
                    "unsupported animation asset dictionary version"
                );
                let count = u32::from_le_bytes(data[4..8].try_into()?) as usize;
                ensure!(count <= 65536, "oversized animation asset dictionary");
                let mut cursor = 8;
                for _ in 0..count {
                    let end = cursor
                        + data[cursor..]
                            .iter()
                            .position(|b| *b == 0)
                            .context("truncated resource string")?;
                    let name = std::str::from_utf8(&data[cursor..end])?;
                    all_names.insert(name.to_owned());
                    if name.ends_with(".vnmclip") || name.ends_with(".vnmskel") {
                        resources.insert(name.to_owned());
                    }
                    cursor = end + 1;
                }
            }
            Ok(())
        },
        |_| Ok(()),
    )?;
    let source_seconds = started.elapsed().as_secs_f64();
    let resources: Vec<_> = resources.into_iter().collect();
    ensure!(!resources.is_empty(), "no recorded animation resources");
    println!(
        "{}",
        serde_json::json!({"phase":"source","seconds":source_seconds,"dictionaries":seen.len(),"dictionaryNames":all_names.len(),"requestedResources":resources.len()})
    );
    for phase in ["prepare", "warm"] {
        let started = Instant::now();
        let assets = animation_assets::prepare_with_models(
            Path::new(&args[3]),
            Path::new(&args[1]),
            Path::new(&args[2]),
            &resources,
            &model_handles,
        )?;
        println!(
            "{}",
            serde_json::json!({
                "phase":phase,"seconds":started.elapsed().as_secs_f64(),"clips":assets.clips.len(),
                "totalBytes":assets.total_bytes,"contentId":assets.resource_content_id,
                "skeletonPresent":assets.skeleton.is_object(), "modelSkeletons":assets.model_skeletons
            })
        );
    }
    Ok(())
}
