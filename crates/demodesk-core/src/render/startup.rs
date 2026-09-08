//! Native hiding and cursor isolation for background recording.
use anyhow::Result;
#[cfg(not(windows))]
use anyhow::anyhow;
use std::path::{Path, PathBuf};

/// Extract the bundled native DLL; no shell, download, or game-file changes.
#[cfg(windows)]
pub(super) fn prepare_hook(cfg_dir: &Path) -> Result<PathBuf> {
    let dir = cfg_dir.join("window-hook");
    std::fs::create_dir_all(&dir)?;
    let dll = dir.join("demodesk-window-hook.dll");
    const BYTES: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/demodesk-window-hook.dll"));
    if !std::fs::read(&dll).is_ok_and(|current| current == BYTES) { std::fs::write(&dll, BYTES)?; }
    std::fs::write(dir.join("Detours.LICENSE.md"), include_bytes!("../../window-hook/Detours.LICENSE.md"))?;
    Ok(dll)
}

#[cfg(not(windows))]
pub(super) fn prepare_hook(_: &Path) -> Result<PathBuf> {
    Err(anyhow!("Synchronous window hiding requires Windows."))
}


#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn native_hook_blocks_real_win32_display_calls_before_they_show() {
        use std::{fs, process::Command};
        let dir = tempfile::tempdir().unwrap();
        let dll = prepare_hook(dir.path()).unwrap();
        let probe = dir.path().join("window-probe.exe");
        fs::write(&probe, include_bytes!(concat!(env!("OUT_DIR"), "/demodesk-window-probe.exe"))).unwrap();
        let log = dir.path().join("window-hook.log");
        let result = Command::new(&probe).arg(&dll).env("DEMODESK_WINDOW_HOOK_LOG", &log)
            .output().unwrap();
        assert!(result.status.success(), "{:?}: {} {}", result.status, String::from_utf8_lossy(&result.stdout), String::from_utf8_lossy(&result.stderr));
        let evidence = fs::read_to_string(log).unwrap();
        for event in ["installed", "CreateWindowExW", "CreateWindowExA", "ShowWindow", "ShowWindowAsync", "SetWindowPos", "SetCursorPos", "ClipCursor", "SetForegroundWindow", "SetFocus", "SetActiveWindow"] {
            assert!(evidence.contains(event), "missing {event}: {evidence}");
        }
    }

}
