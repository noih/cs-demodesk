use anyhow::{ensure, Result};
use demodesk_core::{
    analysis::{demo_source, journal::Journal},
    parser::DemoParser,
};
use std::{io::BufWriter, path::Path};
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    ensure!(
        args.len() == 5 || (args.len() == 6 && args[5] == "--tracking"),
        "expected DEMO OUTPUT FIRST LAST STEP [--tracking]"
    );
    let bytes = std::fs::read(&args[0])?;
    let path = Path::new(&args[1]);
    ensure!(!path.try_exists()?, "output already exists");
    let mut temp = tempfile::NamedTempFile::new_in(
        path.parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new(".")),
    )?;
    let count = {
        let mut writer = Journal::new(
            BufWriter::new(&mut temp),
            demo_source(&bytes)?,
            args.len() == 6,
        )?;
        DemoParser::new().visit_scene_frames(
            &bytes,
            args[2].parse()?,
            args[3].parse()?,
            args[4].parse()?,
            args.len() == 6,
            |frame| writer.push(frame),
        )?;
        writer.finish()?
    };
    temp.as_file().sync_all()?;
    temp.persist_noclobber(path)?;
    println!("{count} journal frames");
    Ok(())
}
