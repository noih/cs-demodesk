//! Export source-bound round boundaries and player identities for analysis adapters.
use anyhow::{ensure, Result};
use demodesk_core::{
    analysis::{demo_source, Artifact, Contract},
    parser::DemoParser,
};
use std::{io::Write, path::Path};
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    ensure!(args.len() == 2, "expected DEMO OUTPUT.json");
    let bytes = std::fs::read(&args[0])?;
    let demo = DemoParser::new().load_demo_bytes(Path::new(&args[0]), &bytes)?;
    let server = DemoParser::new().server_info(&bytes)?;
    let server = server.iter().map(|info| serde_json::json!({"tickInterval":info.tick_interval,"protocol":info.protocol,"mapName":info.map_name,"gameSessionManifest":info.game_session_manifest.as_ref().map(|b| b.to_vec())})).collect::<Vec<_>>();
    let result = Artifact {
        contract: Contract {
            module: "match-context".into(),
            schema_version: 1,
            implementation_version: "0.1.0".into(),
        },
        source: demo_source(&bytes)?,
        dependencies: vec![],
        data: serde_json::json!({"rounds":demo.rounds,"players":demo.info.players,"tickRate":demo.info.tick_rate,"serverInfo":server}),
    };
    let path = Path::new(&args[1]);
    let mut temp = tempfile::NamedTempFile::new_in(
        path.parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new(".")),
    )?;
    serde_json::to_writer(&mut temp, &result)?;
    temp.flush()?;
    temp.as_file().sync_all()?;
    temp.persist_noclobber(path)?;
    Ok(())
}
