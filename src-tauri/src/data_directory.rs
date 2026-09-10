//! Bootstrap selection lives outside the selected data directory so clearing it
//! can always restore the application data directory.
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Selection {
    data_dir: Option<PathBuf>,
}

pub struct DataDirectory {
    config_file: PathBuf,
    pub default: PathBuf,
    pub active: PathBuf,
    selected: Mutex<Option<PathBuf>>,
}

impl DataDirectory {
    pub fn load(config_file: PathBuf, default: &Path) -> Result<Self, String> {
        let default = default.to_path_buf();
        let selected = match fs::read(&config_file) {
            Ok(bytes) => serde_json::from_slice::<Selection>(&bytes).map_err(|e| format!("Cannot read data directory preference: {e}"))?.data_dir,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(format!("Cannot read data directory preference: {e}")),
        };
        let active = selected.as_ref().unwrap_or(&default).clone();
        Ok(Self { config_file, default, active, selected: Mutex::new(selected) })
    }

    pub fn prepare(&self) -> Result<(), String> { prepare(&self.active) }

    pub fn selected(&self) -> Option<PathBuf> { self.selected.lock().unwrap().clone() }

    pub fn validate(&self, value: Option<String>) -> Result<Option<PathBuf>, String> {
        let selected = value.map(|p| p.trim().to_string()).filter(|p| !p.is_empty()).map(PathBuf::from);
        prepare(selected.as_ref().unwrap_or(&self.default))?;
        Ok(selected)
    }

    pub fn save(&self, selected: Option<PathBuf>) -> Result<(), String> {
        let mut current = self.selected.lock().unwrap();
        let parent = self.config_file.parent().ok_or("Cannot locate preference directory")?;
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let bytes = serde_json::to_vec_pretty(&Selection { data_dir: selected.clone() }).map_err(|e| e.to_string())?;
        let temporary = self.config_file.with_extension("tmp");
        fs::write(&temporary, bytes).map_err(|e| e.to_string())?;
        fs::rename(&temporary, &self.config_file).map_err(|e| e.to_string())?;
        *current = selected;
        Ok(())
    }
}

fn prepare(path: &Path) -> Result<(), String> {
    if !path.is_absolute() { return Err("Choose an absolute data directory path.".into()); }
    fs::create_dir_all(path).map_err(|e| format!("Cannot create data directory {}: {e}", path.display()))?;
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|e| e.to_string())?.as_nanos();
    let probe = path.join(format!(".demodesk-write-test-{}-{stamp}", std::process::id()));
    let file = fs::OpenOptions::new().write(true).create_new(true).open(&probe)
        .map_err(|e| format!("Data directory is not writable ({}): {e}", path.display()))?;
    drop(file);
    fs::remove_file(probe).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_default_survives_selection_reset() {
        let root = std::env::temp_dir().join(format!("demodesk-msix-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let config = root.join("preferences/data-directory.json");
        let default = root.join("local/demodesk-data");
        let directory = DataDirectory::load(config.clone(), &default).unwrap();
        directory.prepare().unwrap();
        assert_eq!(directory.active, default);
        let custom = root.join("custom");
        directory.save(directory.validate(Some(custom.to_string_lossy().into_owned())).unwrap()).unwrap();
        let changed = DataDirectory::load(config.clone(), &default).unwrap();
        assert_eq!(changed.active, custom);
        changed.save(changed.validate(None).unwrap()).unwrap();
        let cleared = DataDirectory::load(config, &default).unwrap();
        assert_eq!(cleared.active, default);
        cleared.prepare().unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn unavailable_selection_can_be_replaced_or_cleared() {
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let root = std::env::temp_dir().join(format!("demodesk-recovery-{}-{stamp}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let config = root.join("data-directory.json");
        let default = root.join("local/demodesk-data");
        let unavailable = root.join("blocked");
        fs::write(&unavailable, b"existing file").unwrap();
        fs::write(&config, serde_json::to_vec(&Selection { data_dir: Some(unavailable.clone()) }).unwrap()).unwrap();
        let directory = DataDirectory::load(config.clone(), &default).unwrap();
        assert!(directory.prepare().is_err());
        assert_eq!(directory.selected(), Some(unavailable.clone()));
        assert!(!directory.default.exists(), "must not silently open the default store");
        assert!(directory.validate(Some(unavailable.to_string_lossy().into_owned())).is_err());
        let replacement = root.join("replacement");
        directory.save(directory.validate(Some(replacement.to_string_lossy().into_owned())).unwrap()).unwrap();
        let recovered = DataDirectory::load(config.clone(), &default).unwrap();
        recovered.prepare().unwrap();
        assert_eq!(recovered.active, replacement);
        // The default action also remains usable directly from the failed selection.
        directory.save(directory.validate(None).unwrap()).unwrap();
        let cleared = DataDirectory::load(config, &default).unwrap();
        cleared.prepare().unwrap();
        assert_eq!(cleared.active, cleared.default);
        assert_eq!(fs::read(&unavailable).unwrap(), b"existing file");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn custom_selection_and_clear_apply_on_restart() {
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let root = std::env::temp_dir().join(format!("demodesk-directory-{}-{stamp}", std::process::id()));
        let default = root.join("local/demodesk-data");
        let config = root.join("preferences/data-directory.json");
        let initial = DataDirectory::load(config.clone(), &default).unwrap();
        initial.prepare().unwrap();
        assert_eq!(initial.active, root.join("local/demodesk-data"));
        let custom = root.join("chosen/demodesk-data");
        let selected = initial.validate(Some(custom.to_string_lossy().into_owned())).unwrap();
        initial.save(selected).unwrap();
        assert_eq!(initial.active, initial.default);
        let next = DataDirectory::load(config.clone(), &default).unwrap();
        assert_eq!(next.active, custom);
        next.save(next.validate(None).unwrap()).unwrap();
        let moved_default = root.join("moved/demodesk-data");
        let cleared = DataDirectory::load(config, &moved_default).unwrap();
        assert_eq!(cleared.active, root.join("moved/demodesk-data"));
        assert!(cleared.selected().is_none());
        assert!(cleared.validate(Some("relative/path".into())).is_err());
        let file = root.join("not-a-directory");
        fs::write(&file, b"file").unwrap();
        assert!(cleared.validate(Some(file.to_string_lossy().into_owned())).is_err());
        // Remove only the unique fixture directory created by this test.
        fs::remove_dir_all(root).unwrap();
    }
}
