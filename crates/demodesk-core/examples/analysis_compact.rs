use anyhow::{ensure, Result};
use demodesk_core::{analysis::compact, parser::DemoParser};
use std::{
    io::{BufReader, BufWriter, Write},
    path::Path,
    time::Instant,
};
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    ensure!(args.len() == 2, "expected DEMO NEW_OUTPUT");
    if args[0] == "--digest" {
        let file = BufReader::new(std::fs::File::open(&args[1])?);
        let mut timelines = std::collections::BTreeMap::new();
        let mut arrays =
            std::collections::BTreeMap::<u32, std::collections::BTreeMap<String, Vec<u8>>>::new();
        let mut clock = sha1_smol::Sha1::new();
        let mut events = sha1_smol::Sha1::new();
        compact::visit(
            flate2::read::MultiGzDecoder::new(file),
            |frame| {
                clock.update(&frame.tick.to_le_bytes());
                clock.update(&frame.net_tick.to_le_bytes());
                for id in frame.changed {
                    let f = &frame.fields[*id as usize];
                    if !compact::retains_field(f) {
                        continue;
                    }
                    if let Some(array) = f
                        .name
                        .strip_prefix("pose-array/")
                        .or_else(|| f.name.strip_prefix("pose-bytes/"))
                    {
                        let (name, path) = array
                            .rsplit_once('/')
                            .ok_or_else(|| anyhow::anyhow!("invalid array path"))?;
                        let path: Vec<i32> = serde_json::from_str(path)?;
                        let mut current = std::collections::BTreeMap::new();
                        if let Some(bytes) = frame.values.get(id) {
                            let (length, slots) = if f.name.starts_with("pose-bytes/") {
                                compact::pose_bytes(bytes)?
                            } else {
                                compact::pose_array(bytes)?
                            };
                            current.insert(
                                format!("pose/{name}/{path:?}"),
                                compact::value(&parser::second_pass::variants::Variant::U32(
                                    length,
                                ))?,
                            );
                            for (index, value) in slots {
                                let mut child = path.clone();
                                child.push(index as i32);
                                current.insert(
                                    format!("pose/{name}/{child:?}"),
                                    compact::value(&parser::second_pass::variants::Variant::U32(
                                        value,
                                    ))?,
                                );
                            }
                        }
                        let previous = arrays.entry(*id).or_default();
                        let names: std::collections::BTreeSet<_> =
                            previous.keys().chain(current.keys()).cloned().collect();
                        for name in names {
                            if previous.get(&name) == current.get(&name) {
                                continue;
                            }
                            let key = (f.entity, f.serial, f.class.clone(), name.clone());
                            let hash = timelines.entry(key).or_insert_with(sha1_smol::Sha1::new);
                            let value = current.get(&name).map(Vec::as_slice).unwrap_or_default();
                            hash.update(&frame.tick.to_le_bytes());
                            hash.update(&(value.len() as u64).to_le_bytes());
                            hash.update(value);
                        }
                        *previous = current;
                        continue;
                    }
                    let key = (f.entity, f.serial, f.class.clone(), f.name.clone());
                    let hash = timelines.entry(key).or_insert_with(sha1_smol::Sha1::new);
                    hash.update(&frame.tick.to_le_bytes());
                    let value = frame.values.get(id).map(Vec::as_slice).unwrap_or_default();
                    hash.update(&(value.len() as u64).to_le_bytes());
                    hash.update(value);
                }
                Ok(())
            },
            |event| {
                events.update(&serde_json::to_vec(&event)?);
                Ok(())
            },
        )?;
        let mut digest = sha1_smol::Sha1::new();
        for (key, hash) in &timelines {
            digest.update(&serde_json::to_vec(key)?);
            digest.update(&hash.digest().bytes());
        }
        println!(
            "{}",
            serde_json::json!({"fields":timelines.len(),"stateChanges":digest.digest().to_string(),
            "clock":clock.digest().to_string(),"events":events.digest().to_string()})
        );
        return Ok(());
    }
    if args[0] == "--pose-budget" {
        let file = BufReader::new(std::fs::File::open(&args[1])?);
        let mut together =
            flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        let mut columns = std::collections::BTreeMap::new();
        let mut raw = 0u64;
        let mut unique = std::collections::HashSet::new();
        let mut unique_encoder =
            flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        let mut tags = std::collections::BTreeMap::<u8, (usize, usize)>::new();
        compact::visit(
            flate2::read::MultiGzDecoder::new(file),
            |frame| {
                for id in frame.changed {
                    let field = &frame.fields[*id as usize];
                    if !field.name.contains("m_SerializePoseRecipeAG2Dynamic") {
                        continue;
                    }
                    if let Some(bytes) = frame.values.get(id) {
                        raw += bytes.len() as u64;
                        let counts = tags.entry(bytes[0]).or_default();
                        counts.0 += 1;
                        counts.1 += bytes.len();
                        if unique.insert(bytes.clone()) {
                            unique_encoder.write_all(bytes)?;
                        }
                        together.write_all(bytes)?;
                        columns
                            .entry(*id)
                            .or_insert_with(|| {
                                flate2::write::GzEncoder::new(
                                    Vec::new(),
                                    flate2::Compression::default(),
                                )
                            })
                            .write_all(bytes)?;
                    }
                }
                Ok(())
            },
            |_| Ok(()),
        )?;
        let mut column_bytes = 0;
        for column in columns.into_values() {
            column_bytes += column.finish()?.len();
        }
        println!(
            "{}",
            serde_json::json!({"rawPoseBytes":raw,"gzipTogether":together.finish()?.len(),"gzipColumns":column_bytes,"tags":tags,"uniqueValues":unique.len(),"uniqueGzip":unique_encoder.finish()?.len()})
        );
        return Ok(());
    }
    if args[0] == "--inspect" {
        let file = BufReader::new(std::fs::File::open(&args[1])?);
        let mut context = std::collections::BTreeMap::new();
        let (_, summary) = compact::visit(
            flate2::read::MultiGzDecoder::new(file),
            |frame| {
                for id in frame.changed {
                    let field = &frame.fields[*id as usize];
                    if field.class != "AnimationContext" {
                        continue;
                    }
                    if let Some(value) = frame.values.get(id) {
                        context.insert(field.name.clone(), value.clone());
                    } else {
                        context.remove(&field.name);
                    }
                }
                Ok(())
            },
            |_| Ok(()),
        )?;
        let context_digest = sha1_smol::Sha1::from(serde_json::to_vec(&context)?)
            .digest()
            .to_string();
        println!(
            "{}",
            serde_json::json!({"summary":summary,"contextFields":context.len(),"contextDigest":context_digest})
        );
        return Ok(());
    }
    let started = Instant::now();
    let bytes = std::fs::read(&args[0])?;
    let parser = DemoParser::new();
    let path = Path::new(&args[1]);
    ensure!(!path.try_exists()?, "output already exists");
    let mut temp = tempfile::NamedTempFile::new_in(path.parent().unwrap_or(Path::new(".")))?;
    let mut gzip =
        flate2::write::GzEncoder::new(BufWriter::new(&mut temp), flate2::Compression::default());
    parser.write_match_state(&bytes, &mut gzip)?;
    gzip.finish()?.flush()?;
    temp.as_file().sync_all()?;
    temp.persist_noclobber(path)?;
    let conversion = started.elapsed().as_secs_f64();
    let started = Instant::now();
    let file = BufReader::new(std::fs::File::open(path)?);
    let (_, summary) = compact::visit(
        flate2::read::MultiGzDecoder::new(file),
        |_| Ok(()),
        |_| Ok(()),
    )?;
    println!(
        "{}",
        serde_json::json!({"conversionSeconds":conversion,"decodeSeconds":started.elapsed().as_secs_f64(),
        "bytes":std::fs::metadata(path)?.len(),"summary":summary,"mode":if cfg!(debug_assertions) {"debug"} else {"release"}})
    );
    Ok(())
}
