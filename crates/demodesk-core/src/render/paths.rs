//! Locate Steam, the CS2 install, HLAE and FFmpeg.
//! Everything can be overridden by the user; auto-detection is the fallback.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolPaths {
    pub tools_dir: PathBuf,
    pub steam_dir: Option<PathBuf>,
    pub cs2_dir: Option<PathBuf>,
    pub cs2_exe: Option<PathBuf>,
    /// PatchVersion from game/csgo/steam.inf, e.g. 14178
    pub cs2_patch_version: Option<u32>,
    pub hlae_exe: Option<PathBuf>,
    pub hlae_dll: Option<PathBuf>,
    pub ffmpeg_exe: Option<PathBuf>,
    /// Source2Viewer-CLI, used by the 2D replay to pull radar images out of the game files
    pub vrf_exe: Option<PathBuf>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PathOverrides {
    pub steam_dir: Option<PathBuf>,
    pub cs2_dir: Option<PathBuf>,
    pub hlae_exe: Option<PathBuf>,
    pub ffmpeg_exe: Option<PathBuf>,
    pub vrf_exe: Option<PathBuf>,
}

pub const IS_WINDOWS: bool = cfg!(windows);

#[cfg(windows)]
pub fn steam_dir_from_registry() -> Option<PathBuf> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;
    let key = RegKey::predef(HKEY_CURRENT_USER).open_subkey("Software\\Valve\\Steam").ok()?;
    let value: String = key.get_value("SteamPath").ok()?;
    Some(PathBuf::from(value.replace('/', "\\")))
}

#[cfg(not(windows))]
pub fn steam_dir_from_registry() -> Option<PathBuf> {
    None
}

pub fn find_steam_dir() -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = vec![];
    if let Some(p) = steam_dir_from_registry() {
        candidates.push(p);
    }
    candidates.push(PathBuf::from("C:/Program Files (x86)/Steam"));
    candidates.push(PathBuf::from("C:/Program Files/Steam"));
    candidates.into_iter().find(|c| c.join("steam.exe").is_file())
}

/// Every Steam library root listed in libraryfolders.vdf (plus the main one).
pub fn find_steam_libraries(steam_dir: &Path) -> Vec<PathBuf> {
    let mut libs = vec![steam_dir.to_path_buf()];
    let vdf = steam_dir.join("steamapps").join("libraryfolders.vdf");
    if let Ok(text) = std::fs::read_to_string(&vdf) {
        for line in text.lines() {
            let line = line.trim();
            if let Some(rest) = line.strip_prefix("\"path\"") {
                let value = rest.trim().trim_matches('"').replace("\\\\", "\\");
                let p = PathBuf::from(value);
                if !libs.contains(&p) {
                    libs.push(p);
                }
            }
        }
    }
    libs
}

pub fn cs2_exe_in(cs2_dir: &Path) -> PathBuf {
    cs2_dir.join("game").join("bin").join("win64").join("cs2.exe")
}

pub fn find_cs2_dir(steam_dir: Option<&Path>) -> Option<PathBuf> {
    let steam_dir = steam_dir?;
    find_steam_libraries(steam_dir)
        .into_iter()
        .map(|lib| lib.join("steamapps").join("common").join("Counter-Strike Global Offensive"))
        .find(|dir| cs2_exe_in(dir).is_file())
}

pub fn replays_dir(cs2_dir: &Path) -> PathBuf {
    cs2_dir.join("game").join("csgo").join("replays")
}

/// "1.41.7.8" → 14178
pub fn read_patch_version(cs2_dir: &Path) -> Option<u32> {
    let inf = std::fs::read_to_string(cs2_dir.join("game").join("csgo").join("steam.inf")).ok()?;
    let line = inf.lines().find(|l| l.starts_with("PatchVersion="))?;
    line.trim_start_matches("PatchVersion=").trim().replace('.', "").parse().ok()
}

pub(super) fn find_file(dir: &Path, name: &str, depth: usize) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    let mut dirs = vec![];
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() && path.file_name().map(|f| f.to_string_lossy().eq_ignore_ascii_case(name)).unwrap_or(false) {
            return Some(path);
        }
        if path.is_dir() {
            dirs.push(path);
        }
    }
    if depth == 0 {
        return None;
    }
    dirs.into_iter().find_map(|d| find_file(&d, name, depth - 1))
}

fn find_on_path(exe: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).map(|p| p.join(exe)).find(|p| p.is_file())
}

pub fn resolve_tool_paths(tools_dir: &Path, o: &PathOverrides) -> ToolPaths {
    let tools_dir = tools_dir.to_path_buf();
    let steam_dir = o.steam_dir.clone().or_else(find_steam_dir).filter(|dir| dir.join("steam.exe").is_file());
    let cs2_dir = o.cs2_dir.clone().or_else(|| find_cs2_dir(steam_dir.as_deref()));
    let cs2_exe = cs2_dir.as_ref().map(|d| cs2_exe_in(d)).filter(|p| p.is_file());
    let hlae_exe = o.hlae_exe.clone().or_else(|| find_file(&tools_dir.join("hlae"), "HLAE.exe", 2)).filter(|p| p.is_file());
    let hlae_dll = hlae_exe.as_ref().map(|e| e.parent().unwrap().join("x64").join("AfxHookSource2.dll")).filter(|p| p.is_file());
    let ffmpeg_name = if IS_WINDOWS { "ffmpeg.exe" } else { "ffmpeg" };
    let ffmpeg_exe = o
        .ffmpeg_exe
        .clone()
        .or_else(|| find_file(&tools_dir.join("ffmpeg"), ffmpeg_name, 4))
        .or_else(|| find_on_path(ffmpeg_name))
        .filter(|p| p.is_file());
    let vrf_exe = Some(o.vrf_exe.clone().unwrap_or_else(|| tools_dir.join("vrf").join(super::setup::vrf_exe_name()))).filter(|p| p.is_file());
    ToolPaths {
        tools_dir,
        steam_dir,
        vrf_exe,
        cs2_patch_version: cs2_dir.as_deref().and_then(read_patch_version),
        cs2_dir,
        cs2_exe,
        hlae_exe,
        hlae_dll,
        ffmpeg_exe,
    }
}

pub fn to_forward_slashes(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    #[test]
    fn custom_steam_detects_cs2_and_rejects_missing_executables() {
        let dir = tempfile::tempdir().unwrap();
        let steam = dir.path().join("Steam");
        let cs2 = steam.join("steamapps/common/Counter-Strike Global Offensive");
        let exe = super::cs2_exe_in(&cs2);
        std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
        std::fs::write(&exe, []).unwrap();
        std::fs::write(steam.join("steam.exe"), []).unwrap();
        let options = super::PathOverrides { steam_dir: Some(steam.clone()), ..Default::default() };
        let paths = super::resolve_tool_paths(dir.path(), &options);
        assert_eq!(paths.steam_dir, Some(steam.clone()));
        assert_eq!(paths.cs2_exe, Some(exe));
        std::fs::remove_file(steam.join("steam.exe")).unwrap();
        let paths = super::resolve_tool_paths(dir.path(), &options);
        assert!(paths.steam_dir.is_none());
        assert!(paths.cs2_exe.is_none());
    }

    use super::*;

    #[test]
    fn vrf_override_and_default() {
        let dir = tempfile::tempdir().unwrap();
        let custom = dir.path().join("custom.exe");
        std::fs::write(&custom, []).unwrap();
        let options = PathOverrides { vrf_exe: Some(custom.clone()), ..Default::default() };
        assert_eq!(resolve_tool_paths(dir.path(), &options).vrf_exe, Some(custom.clone()));
        std::fs::remove_file(&custom).unwrap();
        assert_eq!(resolve_tool_paths(dir.path(), &options).vrf_exe, None);
        let default = dir.path().join("vrf").join(super::super::setup::vrf_exe_name());
        std::fs::create_dir_all(default.parent().unwrap()).unwrap();
        std::fs::write(&default, []).unwrap();
        assert_eq!(resolve_tool_paths(dir.path(), &PathOverrides::default()).vrf_exe, Some(default));
    }
}
