use anyhow::{ensure, Result};
use demodesk_core::{analysis::scene, parser::DemoParser};
use std::{io::Write, path::Path};
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    ensure!(
        args.len() == 5 || (args.len() == 6 && args[5] == "--tracking"),
        "expected DEMO OUTPUT FIRST_TICK LAST_TICK STEP_TICKS [--tracking]"
    );
    let bytes = std::fs::read(&args[0])?;
    let frames = if args.len() == 6 {
        DemoParser::new().player_tracking_frames(
            &bytes,
            args[2].parse()?,
            args[3].parse()?,
            args[4].parse()?,
        )?
    } else {
        scene::extract(
            &DemoParser::new(),
            &bytes,
            args[2].parse()?,
            args[3].parse()?,
            args[4].parse()?,
        )?
    };
    let path = Path::new(&args[1]);
    let mut tmp = tempfile::NamedTempFile::new_in(
        path.parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new(".")),
    )?;
    let result = demodesk_core::analysis::Artifact {
        contract: if args.len() == 6 {
            demodesk_core::analysis::Contract {
                module: "player-tracking-scene".into(),
                schema_version: 1,
                implementation_version: "0.1.0".into(),
            }
        } else {
            scene::contract()
        },
        source: demodesk_core::analysis::demo_source(&bytes)?,
        dependencies: vec![],
        data: &frames,
    };
    {
        let mut writer = std::io::BufWriter::new(&mut tmp);
        serde_json::to_writer(&mut writer, &result)?;
        writer.flush()?;
    }
    tmp.flush()?;
    tmp.as_file().sync_all()?;
    tmp.persist_noclobber(path)?;
    println!("{} scene frames", frames.len());
    Ok(())
}
