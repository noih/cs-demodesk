//! Export unrounded per-tick player state and raw smoke/blind events from a real demo.
use anyhow::{ensure, Context, Result};
use demodesk_core::{analysis::measurements as data, parser::DemoParser};
use std::{io::Write, path::Path};
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    ensure!(
        args.len() == 5,
        "expected DEMO OUTPUT.json FIRST_TICK LAST_TICK VERIFIED_TICK_RATE"
    );
    let bytes = std::fs::read(&args[0]).context("reading demo")?;
    let result = data::extract(
        &DemoParser::new(),
        &bytes,
        args[2].parse()?,
        args[3].parse()?,
        args[4].parse()?,
    )?;
    let output = Path::new(&args[1]);
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    let artifact = demodesk_core::analysis::Artifact {
        contract: data::contract(),
        source: demodesk_core::analysis::demo_source(&bytes)?,
        dependencies: vec![],
        data: &result,
    };
    {
        let mut writer = std::io::BufWriter::new(&mut temp);
        serde_json::to_writer(&mut writer, &artifact)?;
        writer.flush()?;
    }
    temp.flush()?;
    temp.as_file().sync_all()?;
    temp.persist_noclobber(output)
        .context("saving measurements without replacing existing evidence")?;
    println!(
        "{} player samples; decoded counts: {:?}",
        result.samples.len(),
        result.decoded_counts
    );
    Ok(())
}
