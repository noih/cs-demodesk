//! Undo what versions before 2026-09-08 wrote into the game folder (a CS Demo
//! Manager style server plugin in `game/csgo/csdm/` and an extra search path in
//! `gameinfo.gi`). Nothing is written to the game folder any more; this only
//! restores a game that an old, interrupted run left patched — a patched
//! gameinfo.gi would break normal online play.

use anyhow::Result;
use std::fs;
use std::path::{Path, PathBuf};

const SEARCH_PATH_LINE: &str = "Game\tcsgo/csdm";

struct Paths {
    csdm_dir: PathBuf,
    gameinfo: PathBuf,
    backup: PathBuf,
}

fn paths(cs2_dir: &Path) -> Paths {
    let csgo = cs2_dir.join("game").join("csgo");
    Paths { csdm_dir: csgo.join("csdm"), gameinfo: csgo.join("gameinfo.gi"), backup: csgo.join("gameinfo.gi.backup") }
}

/// Plugin folder or patched gameinfo.gi still present.
pub(super) fn has_leftovers(cs2_dir: &Path) -> bool {
    let p = paths(cs2_dir);
    p.csdm_dir.exists() || fs::read_to_string(&p.gameinfo).map(|c| c.contains(SEARCH_PATH_LINE)).unwrap_or(false)
}

/// Remove the plugin folder and restore gameinfo.gi (from the backup when there is one).
pub(super) fn remove_leftovers(cs2_dir: &Path) -> Result<()> {
    let p = paths(cs2_dir);
    if p.csdm_dir.exists() {
        fs::remove_dir_all(&p.csdm_dir)?;
    }
    if p.backup.exists() {
        fs::copy(&p.backup, &p.gameinfo)?;
        fs::remove_file(&p.backup)?;
    } else if p.gameinfo.exists() {
        let content = fs::read_to_string(&p.gameinfo)?;
        if content.contains(SEARCH_PATH_LINE) {
            fs::write(&p.gameinfo, content.replacen(&format!("{SEARCH_PATH_LINE}\n\t\t\t"), "", 1))?;
        }
    }
    Ok(())
}
