use super::physical;
use std::fs;
use std::path::{Path, PathBuf};

pub const OWNED: &[&str] = &[
    "parsed",
    "analysis",
    "behavior-analysis",
    "scoring",
    "radar",
    "clips",
    "renders",
    "tools",
    "settings.json",
    "settings.tmp",
    "registered-demos.json",
    "registered-demos.tmp",
    "analysis-jobs.json",
];

pub fn has_data(root: &Path) -> Result<bool, String> {
    for name in OWNED {
        let path = root.join(name);
        if let Some(meta) = physical::metadata(&path)? {
            if !meta.is_dir()
                || fs::read_dir(&path)
                    .map_err(|e| e.to_string())?
                    .next()
                    .transpose()
                    .map_err(|e| e.to_string())?
                    .is_some()
            {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

pub fn validate_root(root: &Path) -> Result<(), String> {
    if !root.is_absolute() || root.components().count() < 3 {
        return Err(format!("Unsafe app data root: {}", root.display()));
    }
    if physical::app_data_root(root)? {
        return Err("Cannot clear an AppData system folder.".into());
    }
    for name in [
        "USERPROFILE",
        "LOCALAPPDATA",
        "APPDATA",
        "SystemRoot",
        "ProgramFiles",
        "ProgramFiles(x86)",
    ] {
        if let Some(value) = std::env::var_os(name) {
            if physical::same(root, Path::new(&value)) {
                return Err(format!("Cannot clear {name}."));
            }
        }
    }
    if let Some(meta) = physical::metadata(root)? {
        if !meta.is_dir()
            || physical::reparse(&meta)
            || !physical::same(root, &physical::canonical(root)?)
        {
            return Err(format!(
                "Unsafe or redirected cleanup root: {}",
                root.display()
            ));
        }
    }
    Ok(())
}

pub fn validate_roots(roots: &[PathBuf]) -> Result<(), String> {
    for (i, root) in roots.iter().enumerate() {
        validate_root(root)?;
        for other in &roots[..i] {
            if root.starts_with(other) || other.starts_with(root) {
                return Err("App cleanup roots overlap.".into());
            }
        }
    }
    Ok(())
}

/// A configured custom folder must contain an actual App settings document before reset.
pub fn prove_owned(root: &Path, default: &Path) -> Result<(), String> {
    validate_root(root)?;
    if physical::same(root, default) {
        return Ok(());
    }
    let path = root.join("settings.json");
    let bytes = fs::read(&path)
        .map_err(|e| format!("Cannot establish app ownership of {}: {e}", root.display()))?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    if !value
        .get("scanGameReplays")
        .is_some_and(serde_json::Value::is_boolean)
        || !value
            .get("replayFolders")
            .is_some_and(serde_json::Value::is_array)
    {
        return Err(format!("Unrecognized app settings in {}", path.display()));
    }
    let settings: demodesk_core::store::Settings =
        serde_json::from_value(value).map_err(|e| e.to_string())?;
    for external in [settings.cs2_dir, settings.steam_dir].into_iter().flatten() {
        if let Ok(actual) = physical::canonical(Path::new(&external)) {
            if root.starts_with(&actual) {
                return Err("Cannot reset data inside a game or Steam installation.".into());
            }
        }
    }
    Ok(())
}

fn visit(path: &Path, root: &Path, delete: bool, roots: &[PathBuf]) -> Result<(), String> {
    let Some(meta) = physical::metadata(path)? else {
        return Ok(());
    };
    let actual = physical::canonical(path)?;
    // Before clearing the private layer, the logical layer may still resolve into it.
    let contained = if delete {
        actual.starts_with(root)
    } else {
        roots.iter().any(|r| actual.starts_with(r))
    };
    if physical::reparse(&meta) || !contained {
        return Err(format!(
            "Refusing to clear an external link or redirected file: {}",
            path.display()
        ));
    }
    if meta.is_dir() {
        for entry in
            fs::read_dir(path).map_err(|e| format!("Cannot list {}: {e}", path.display()))?
        {
            visit(
                &entry.map_err(|e| e.to_string())?.path(),
                root,
                delete,
                roots,
            )?;
        }
        if delete {
            fs::remove_dir(path).map_err(|e| format!("Cannot remove {}: {e}", path.display()))?;
        }
    } else if delete {
        fs::remove_file(path).map_err(|e| format!("Cannot remove {}: {e}", path.display()))?;
    }
    Ok(())
}

pub fn validate_cleanup_roots(roots: &[PathBuf], allowed: &[PathBuf]) -> Result<(), String> {
    for root in roots {
        if !physical::in_app_data(root)?
            || !allowed
                .iter()
                .any(|candidate| physical::same(root, candidate))
        {
            return Err(format!(
                "Refusing to clear a directory outside the App's fixed AppData workspace: {}",
                root.display()
            ));
        }
    }
    validate_roots(roots)
}

pub fn clean(roots: &[PathBuf], allowed: &[PathBuf]) -> Result<(), String> {
    validate_cleanup_roots(roots, allowed)?;
    for delete in [false, true] {
        for root in roots {
            let entries = match fs::read_dir(root) {
                Ok(entries) => entries,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error.to_string()),
            };
            for entry in entries {
                visit(
                    &entry.map_err(|e| e.to_string())?.path(),
                    root,
                    delete,
                    roots,
                )?;
            }
            if delete {
                fs::remove_dir(root).map_err(|e| e.to_string())?;
            }
        }
    }
    Ok(())
}
