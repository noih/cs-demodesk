//! Bootstrap selection lives outside the selected data directory so clearing it
//! can always restore the application data directory.
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

mod physical;
mod reset;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PendingReset {
    target: PathBuf,
    roots: Vec<PathBuf>,
}

#[derive(Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Selection {
    data_dir: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pending_reset: Option<PendingReset>,
    #[serde(default)]
    reset_preferences: bool,
}

pub struct DataDirectory {
    config_file: PathBuf,
    pub default: PathBuf,
    legacy_default: PathBuf,
    pub active: PathBuf,
    selected: Mutex<Selection>,
    locks: Mutex<Vec<physical::StorageLock>>,
}

impl DataDirectory {
    pub fn recovery(config_file: PathBuf, default: PathBuf) -> Self {
        Self {
            config_file: physical::redirected(&config_file)
                .ok()
                .flatten()
                .unwrap_or(config_file),
            active: default.clone(),
            legacy_default: default.clone(),
            default,
            selected: Mutex::new(Selection::default()),
            locks: Mutex::new(Vec::new()),
        }
    }

    pub fn load(config_file: PathBuf, default: &Path) -> Result<Self, String> {
        let legacy_config = config_file;
        let config_file = physical::redirected(&legacy_config)?.unwrap_or(legacy_config.clone());
        let source = if physical::metadata(&config_file)?.is_some() {
            &config_file
        } else {
            &legacy_config
        };
        let selected = match fs::read(source) {
            Ok(bytes) => serde_json::from_slice::<Selection>(&bytes)
                .map_err(|e| format!("Cannot read data directory preference: {e}"))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Selection::default(),
            Err(e) => return Err(format!("Cannot read data directory preference: {e}")),
        };
        let active = selected
            .data_dir
            .as_deref()
            .unwrap_or(default)
            .to_path_buf();
        let legacy_default = default.to_path_buf();
        let default = physical::redirected(default)?.unwrap_or(default.to_path_buf());
        Ok(Self {
            config_file,
            default,
            legacy_default,
            active,
            selected: Mutex::new(selected),
            locks: Mutex::new(Vec::new()),
        })
    }

    pub fn load_outside(
        config: PathBuf,
        default: &Path,
        legacy_config: PathBuf,
        legacy_default: PathBuf,
    ) -> Result<Self, String> {
        let mut directory = Self::load(config.clone(), default)?;
        if !config.exists() {
            let legacy = Self::load(legacy_config, &legacy_default)?;
            if legacy.selected().is_some() || physical::metadata(&legacy.active)?.is_some() {
                directory.active = legacy.active;
            }
        }
        directory.legacy_default = legacy_default;
        Ok(directory)
    }

    #[cfg(test)]
    pub fn prepare(&self) -> Result<(), String> {
        prepare(&self.active)
    }

    pub fn selected(&self) -> Option<PathBuf> {
        self.selected
            .lock()
            .unwrap()
            .data_dir
            .clone()
            .filter(|path| !physical::same(path, &self.default))
    }

    pub fn validate(&self, value: Option<String>) -> Result<Option<PathBuf>, String> {
        let selected = value
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty())
            .map(PathBuf::from);
        let requested = selected.as_ref().unwrap_or(&self.default);
        if physical::in_app_data(requested)? {
            return Err("Choose a data directory outside AppData.".into());
        }
        let (actual, roots) = inspect(requested)?;
        if physical::in_app_data(&actual)? {
            return Err("Choose a data directory outside AppData.".into());
        }
        if !roots.is_empty() {
            return Err(
                "This folder contains redirected app data. Choose a physical data directory."
                    .into(),
            );
        }
        prepare(&actual)?;
        verify_external(&actual)?;
        Ok(Some(actual))
    }

    pub fn validate_tool_directory(path: &Path) -> Result<PathBuf, String> {
        if physical::in_app_data(path)? {
            return Err("Choose a tool directory outside AppData.".into());
        }
        let (actual, roots) = inspect(path)?;
        if !roots.is_empty() || physical::in_app_data(&actual)? {
            return Err("Choose a tool directory outside AppData.".into());
        }
        verify_external(&actual)?;
        Ok(actual)
    }

    pub fn save(&self, selected: Option<PathBuf>) -> Result<(), String> {
        let mut current = self.selected.lock().unwrap();
        if current.pending_reset.is_some() {
            return Err(
                "An interrupted data repair must finish before choosing another directory.".into(),
            );
        }
        let mut next = current.clone();
        next.data_dir = selected.filter(|path| !physical::same(path, &self.default));
        self.write_selection(&next)?;
        *current = next;
        Ok(())
    }

    fn write_selection(&self, selection: &Selection) -> Result<(), String> {
        let parent = self
            .config_file
            .parent()
            .ok_or("Cannot locate preference directory")?;
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let bytes = serde_json::to_vec_pretty(selection).map_err(|e| e.to_string())?;
        let temporary = self.config_file.with_extension("tmp");
        fs::write(&temporary, bytes).map_err(|e| e.to_string())?;
        fs::rename(&temporary, &self.config_file).map_err(|e| e.to_string())?;
        Ok(())
    }

    fn allowed_legacy_roots(&self) -> Result<Vec<PathBuf>, String> {
        if !physical::in_app_data(&self.legacy_default)? {
            return Ok(Vec::new());
        }
        let mut roots = Vec::new();
        if let Some(private) = physical::redirected(&self.legacy_default)? {
            roots.push(private);
        }
        if !roots
            .iter()
            .any(|root| physical::same(root, &self.legacy_default))
        {
            roots.push(self.legacy_default.clone());
        }
        Ok(roots)
    }

    pub fn settings_use_app_data(
        settings: &demodesk_core::store::Settings,
    ) -> Result<bool, String> {
        for path in [
            &settings.steam_dir,
            &settings.cs2_dir,
            &settings.hlae_exe,
            &settings.ffmpeg_exe,
            &settings.vrf_exe,
        ]
        .into_iter()
        .flatten()
        .chain(settings.replay_folders.iter())
        {
            if path_uses_app_data(Path::new(path))? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn needs_reset(&self) -> Result<bool, String> {
        if path_uses_app_data(&self.active)? {
            return Ok(true);
        }
        let settings =
            demodesk_core::store::Store::settings_at(&self.active).map_err(|e| e.to_string())?;
        Self::settings_use_app_data(&settings)
    }

    pub fn initialize(&mut self) -> Result<(), String> {
        let mut selection = self.selected.lock().unwrap().clone();
        let allowed = self.allowed_legacy_roots()?;
        if selection.pending_reset.is_none() {
            if self.needs_reset()? {
                let mut roots = Vec::new();
                // Only the app's fixed legacy directory and its Windows private layer are deletable.
                // Never expand the boundary from a user-selected data or executable path.
                for root in &allowed {
                    if let Some(metadata) = physical::metadata(root)? {
                        if physical::reparse(&metadata) {
                            return Err("The old App workspace must not be a link.".into());
                        }
                        let actual = physical::canonical(root)?;
                        if !physical::same(root, &actual) {
                            if allowed
                                .iter()
                                .any(|candidate| physical::same(candidate, &actual))
                            {
                                continue;
                            }
                            return Err(
                                "The old App workspace resolves outside its allowed directory."
                                    .into(),
                            );
                        }
                        roots.push(root.clone());
                    }
                }
                let target = self.reset_target()?;
                let mut locked = roots.clone();
                locked.push(target.clone());
                reset::validate_roots(&locked)?;
                self.lock_roots(&locked)?;
                selection.pending_reset = Some(PendingReset { target, roots });
                selection.reset_preferences = true;
                self.write_selection(&selection)?;
                *self.selected.lock().unwrap() = selection.clone();
            } else {
                let (actual, redirected) = inspect(&self.active)?;
                if !redirected.is_empty() || physical::in_app_data(&actual)? {
                    return Err("Choose a physical data directory outside AppData.".into());
                }
                self.active = actual;
            }
        }
        if let Some(pending) = &selection.pending_reset {
            if !physical::same(&pending.target, &self.default) {
                return Err("The interrupted reset has an invalid target.".into());
            }
            reset::validate_root(&pending.target)?;
            reset::validate_cleanup_roots(&pending.roots, &allowed)?;
            if self.locks.lock().unwrap().is_empty() {
                let mut locked = pending.roots.clone();
                locked.push(pending.target.clone());
                reset::validate_roots(&locked)?;
                self.lock_roots(&locked)?;
            }
            reset::clean(&pending.roots, &allowed)?;
            self.active = pending.target.clone();
            selection.pending_reset = None;
        }
        prepare(&self.active)?;
        verify_external(&self.active)?;
        if self.locks.lock().unwrap().is_empty() {
            self.lock_roots(std::slice::from_ref(&self.active))?;
        }
        selection.data_dir =
            (!physical::same(&self.active, &self.default)).then(|| self.active.clone());
        self.write_selection(&selection)?;
        *self.selected.lock().unwrap() = selection;
        Ok(())
    }

    fn reset_target(&self) -> Result<PathBuf, String> {
        let target = Self::validate_tool_directory(&self.default)?;
        if !physical::same(&target, &self.default)
            || fs::read_dir(&target)
                .map_err(|e| e.to_string())?
                .next()
                .is_some()
        {
            return Err(
                "The new default data folder must be an empty physical directory before reset."
                    .into(),
            );
        }
        Ok(target)
    }

    pub fn reset_preferences(&self) -> bool {
        self.selected.lock().unwrap().reset_preferences
    }

    pub fn acknowledge_preferences(&self) -> Result<(), String> {
        let mut current = self.selected.lock().unwrap();
        let mut next = current.clone();
        next.reset_preferences = false;
        self.write_selection(&next)?;
        *current = next;
        Ok(())
    }

    pub fn changes(&self, target: &Path) -> bool {
        !physical::same(target, &self.active)
    }

    pub fn target_settings(
        &self,
        target: &Path,
        original: &demodesk_core::store::Settings,
        edited: &demodesk_core::store::Settings,
    ) -> Result<demodesk_core::store::Settings, String> {
        use demodesk_core::store::Store;
        if !self.changes(target) {
            return Ok(edited.clone().normalized());
        }
        reset::validate_root(target)?;
        if target.starts_with(&self.active) || self.active.starts_with(target) {
            return Err("The old and new data directories must not contain one another.".into());
        }
        if reset::has_data(target)? {
            reset::prove_owned(target, &self.default)?;
        } else {
            for entry in fs::read_dir(target).map_err(|e| e.to_string())? {
                let entry = entry.map_err(|e| e.to_string())?;
                if !["tools", "parsed", "clips"]
                    .iter()
                    .any(|name| entry.file_name() == *name)
                {
                    return Err(
                        "Choose an empty folder or an existing DemoDesk data folder.".into(),
                    );
                }
            }
        }
        let mut merged =
            serde_json::to_value(Store::settings_at(target).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        let original =
            serde_json::to_value(original.clone().normalized()).map_err(|e| e.to_string())?;
        let edited =
            serde_json::to_value(edited.clone().normalized()).map_err(|e| e.to_string())?;
        for (key, value) in edited.as_object().ok_or("Invalid settings")? {
            if original.get(key) != Some(value) {
                merged[key] = value.clone();
            }
        }
        serde_json::from_value(merged).map_err(|e| e.to_string())
    }

    pub fn commit_switch(
        &self,
        target: PathBuf,
        settings: &demodesk_core::store::Settings,
    ) -> Result<(), String> {
        let lock_count = self.locks.lock().unwrap().len();
        let result = (|| {
            self.lock_roots(std::slice::from_ref(&target))?;
            let path = target.join("settings.json");
            let previous = match fs::read(&path) {
                Ok(bytes) => Some(bytes),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                Err(e) => return Err(e.to_string()),
            };
            demodesk_core::store::Store::save_settings_at(&target, settings)
                .map_err(|e| e.to_string())?;
            if let Err(error) = self.save(Some(target)) {
                let restored = match previous {
                    Some(bytes) => fs::write(&path, bytes),
                    None => fs::remove_file(&path),
                };
                return Err(match restored {
                    Ok(()) => error,
                    Err(e) => format!("{error}; cannot restore target settings: {e}"),
                });
            }
            Ok(())
        })();
        self.locks.lock().unwrap().truncate(lock_count);
        result
    }

    pub fn release_locks(&self) {
        self.locks.lock().unwrap().clear();
    }

    fn lock_roots(&self, roots: &[PathBuf]) -> Result<(), String> {
        #[cfg(not(test))]
        physical::check_other_instance()?;
        let mut locks = Vec::new();
        for root in roots {
            locks.push(physical::lock_root(root)?);
        }
        self.locks.lock().unwrap().extend(locks);
        Ok(())
    }
}

fn path_uses_app_data(path: &Path) -> Result<bool, String> {
    if physical::in_app_data(path)? {
        return Ok(true);
    }
    // A not-yet-created child can still be below a junction into AppData.
    for ancestor in path.ancestors() {
        if physical::metadata(ancestor)?.is_some() {
            return physical::in_app_data(&physical::canonical(ancestor)?);
        }
    }
    Ok(false)
}

pub fn probe_command() -> Option<bool> {
    let mut args = std::env::args_os().skip(1);
    if args.next().as_deref() != Some(std::ffi::OsStr::new("--demodesk-verify-data-directory")) {
        return None;
    }
    Some(args.next().is_some_and(|path| {
        let root = PathBuf::from(path);
        root.is_absolute()
            && args.next().is_none()
            && verify_tree_roots(&root).is_ok()
            && physical::canonical(&root).is_ok_and(|actual| physical::same(&actual, &root))
    }))
}

fn verify_external(root: &Path) -> Result<(), String> {
    #[cfg(not(test))]
    {
        demodesk_core::render::verify_data_directory(
            &std::env::current_exe().map_err(|e| e.to_string())?,
            root,
        )
        .map_err(|e| format!("{e:#}"))
    }
    #[cfg(test)]
    {
        verify_tree_roots(root)
    } // libtest is not the GUI binary; installed probe tests exercise the child.
}

/// Detect redirection before prepare() creates managed subdirectories in a merged view.
fn inspect(path: &Path) -> Result<(PathBuf, Vec<PathBuf>), String> {
    if !path.is_absolute() {
        return Err("Choose an absolute data directory path.".into());
    }
    let redirected = physical::redirected(path)?;
    inspect_with_mapping(path, redirected)
}

fn inspect_with_mapping(
    path: &Path,
    redirected: Option<PathBuf>,
) -> Result<(PathBuf, Vec<PathBuf>), String> {
    let had_data = reset::has_data(path)?;
    let written = write_probe(path)?;
    let actual = physical::canonical(path)?;
    let mut roots = Vec::new();
    if let Some(private) = redirected {
        let private_has_data = reset::has_data(&private)?;
        // A normal physical directory wins if there is no private overlay data.
        if private_has_data
            || physical::same(&private, &actual)
            || physical::same(&private, &written)
        {
            fs::create_dir_all(&private).map_err(|e| e.to_string())?;
            let target = physical::canonical(&private)?;
            if had_data || private_has_data {
                roots.push(target.clone());
                if !physical::same(&actual, &target) {
                    roots.push(actual);
                }
            }
            verify_tree_roots(&target)?;
            return Ok((target, roots));
        }
    }
    if !physical::same(&actual, &written) {
        return Err("Cannot confirm the data directory's redirection target.".into());
    }
    verify_tree_roots(&actual)?;
    Ok((actual, roots))
}

fn verify_tree_roots(root: &Path) -> Result<(), String> {
    check_access(root)?;
    for name in reset::OWNED {
        let path = root.join(name);
        if let Some(meta) = physical::metadata(&path)? {
            if physical::reparse(&meta) || !physical::canonical(&path)?.starts_with(root) {
                return Err(format!(
                    "App data points outside its directory: {}",
                    path.display()
                ));
            }
            if meta.is_dir() {
                check_access(&path)?;
                verify_representative(&path, root, 0)?;
            }
        }
    }
    Ok(())
}

fn verify_representative(directory: &Path, root: &Path, depth: usize) -> Result<(), String> {
    if depth >= 16 {
        return Err("App data contains an unexpectedly deep directory.".into());
    }
    if let Some(entry) = fs::read_dir(directory).map_err(|e| e.to_string())?.next() {
        let path = entry.map_err(|e| e.to_string())?.path();
        let meta = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
        if physical::reparse(&meta) || !physical::canonical(&path)?.starts_with(root) {
            return Err(format!(
                "Existing app data points outside its directory: {}",
                path.display()
            ));
        }
        if meta.is_dir() {
            verify_representative(&path, root, depth + 1)?;
        }
    }
    Ok(())
}

fn prepare(path: &Path) -> Result<(), String> {
    if !path.is_absolute() {
        return Err("Choose an absolute data directory path.".into());
    }
    check_access(path)?;
    for child in ["tools", "parsed", "clips"] {
        check_access(&path.join(child))?;
    }
    for tool in ["hlae", "ffmpeg", "vrf"] {
        let dir = path.join("tools").join(tool);
        match fs::metadata(&dir) {
            Ok(_) => check_access(&dir)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                return Err(format!(
                    "Cannot access tool directory {}: {e}",
                    dir.display()
                ))
            }
        }
    }
    Ok(())
}

fn check_access(path: &Path) -> Result<(), String> {
    let written = write_probe(path)?;
    if !physical::same(&written, &physical::canonical(path)?) {
        return Err(format!(
            "New files are redirected outside {}",
            path.display()
        ));
    }
    Ok(())
}

fn write_probe(path: &Path) -> Result<PathBuf, String> {
    use std::io::Write;
    fs::create_dir_all(path)
        .map_err(|e| format!("Cannot create data directory {}: {e}", path.display()))?;
    fs::read_dir(path)
        .map_err(|e| format!("Data directory is not readable ({}): {e}", path.display()))?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let probe = path.join(format!(
        ".demodesk-write-test-{}-{stamp}",
        std::process::id()
    ));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
        .map_err(|e| format!("Data directory is not writable ({}): {e}", path.display()))?;
    let result = (|| -> Result<PathBuf, String> {
        file.write_all(b"DemoDesk access check")
            .map_err(|e| e.to_string())?;
        drop(file);
        if fs::read(&probe).map_err(|e| e.to_string())? != b"DemoDesk access check" {
            return Err("Data directory read-back failed".into());
        }
        physical::canonical(&probe)?
            .parent()
            .map(Path::to_path_buf)
            .ok_or("Invalid probe path".into())
    })();
    let cleanup = fs::remove_file(&probe);
    cleanup.map_err(|e| {
        format!(
            "Cannot remove data directory probe {}: {e}",
            probe.display()
        )
    })?;
    result.map_err(|e| {
        format!(
            "Cannot read or write data directory {}: {e}",
            path.display()
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_base() -> PathBuf {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("target/path-tests");
        fs::create_dir_all(&path).unwrap();
        physical::canonical(&path).unwrap()
    }

    fn scratch() -> PathBuf {
        let path = test_base().join(format!(
            "{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn seed(root: &Path) {
        for dir in ["parsed", "tools", "clips"] {
            fs::create_dir_all(root.join(dir)).unwrap();
        }
        fs::write(
            root.join("settings.json"),
            serde_json::to_vec(&demodesk_core::store::Settings::default()).unwrap(),
        )
        .unwrap();
        fs::write(root.join("parsed/test.error.json"), b"failed").unwrap();
        fs::write(root.join("clips/test.mp4"), b"synthetic video").unwrap();
        fs::write(root.join("tools/tool.exe"), b"synthetic tool").unwrap();
        fs::write(root.join("original.dem"), b"synthetic original").unwrap();
        fs::write(root.join("unknown.txt"), b"keep").unwrap();
    }

    /// Run only with a generated fixture name, first in Store, then neutral, then Store.
    #[test]
    #[ignore = "requires an installed package and an isolated Explorer-launched control"]
    fn packaged_reset_roundtrip() {
        let name = std::env::var("DEMODESK_PATH_TEST_NAME").expect("isolated fixture name");
        let suffix = name.strip_prefix("DemoDesk-CompatibilityTest-").unwrap();
        assert_eq!(suffix.len(), 32);
        assert!(suffix.bytes().all(|c| c.is_ascii_hexdigit()));
        let root = PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap()).join(&name);
        let report = test_base().join(format!("{name}.json"));
        let mode = std::env::var("DEMODESK_PATH_TEST_MODE").unwrap();
        if mode == "probe" {
            let outside = test_base().join(format!("{name} space"));
            fs::create_dir(&outside).unwrap();
            let exe = Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .unwrap()
                .join("target/debug/demodesk.exe");
            demodesk_core::render::verify_data_directory(&exe, &outside).unwrap();
            assert_eq!(outside.parent(), Some(test_base().as_path()));
            fs::remove_dir(outside).unwrap();
            return;
        }
        if mode == "scope" {
            seed(&root);
            let actual = physical::canonical(&root).unwrap();
            let allowed = vec![actual.clone()];
            assert!(reset::clean(&[actual.parent().unwrap().to_path_buf()], &allowed).is_err());
            let shared = actual.with_file_name(format!("{name}-shared"));
            seed(&shared);
            assert!(reset::clean(std::slice::from_ref(&shared), &allowed).is_err());
            assert!(shared.join("settings.json").exists());
            assert!(actual.join("original.dem").exists());
            reset::clean(&allowed, &allowed).unwrap();
            assert!(!actual.exists());
            assert!(shared.join("unknown.txt").exists());
            assert!(shared.join("original.dem").exists());
            assert_eq!(
                shared.file_name().unwrap(),
                format!("{name}-shared").as_str()
            );
            reset::validate_root(&shared).unwrap();
            fs::remove_dir_all(shared).unwrap();
            return;
        }
        if mode == "hold" {
            seed(&root);
            let actual = physical::canonical(&root).unwrap();
            let held = physical::lock_root(&actual).unwrap();
            fs::write(report.with_extension("ready"), b"locked").unwrap();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(40);
            while !report.with_extension("release").exists() && std::time::Instant::now() < deadline
            {
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            drop(held);
            fs::remove_dir_all(actual).unwrap();
            return;
        }
        if mode == "locked" {
            let mut directory =
                DataDirectory::load(test_base().join(format!("{name}-config.json")), &root)
                    .unwrap();
            let result = directory.initialize();
            fs::write(report.with_extension("release"), b"done").unwrap();
            assert!(result.unwrap_err().contains("Another DemoDesk instance"));
            return;
        }
        if mode == "seed" || mode == "neutral" {
            seed(&root);
            fs::write(
                test_base().join(format!("{name}-original.dem")),
                b"original outside app data",
            )
            .unwrap();
            fs::remove_file(root.join("original.dem")).unwrap();
            let actual = physical::canonical(&root.join("settings.json")).unwrap();
            fs::write(&report, serde_json::to_vec_pretty(&serde_json::json!({"mode":mode,"actual":actual,"mapped":physical::redirected(&root).unwrap()})).unwrap()).unwrap();
            return;
        }
        assert!(matches!(mode.as_str(), "repair" | "physical" | "tools"));
        let (old_target, mut roots) = inspect(&root).unwrap();
        if mode == "repair" {
            assert!(!roots.is_empty(), "fixture must actually be virtualized");
        } else if mode == "physical" {
            assert!(roots.is_empty(), "neutral-first fixture must be physical");
        }
        let config = test_base().join(format!("{name}-config.json"));
        let target = test_base().join(format!("{name}-outside"));
        let legacy_config = test_base().join(format!("{name}-legacy.json"));
        let mut directory = DataDirectory::load_outside(
            config.clone(),
            &target,
            legacy_config.clone(),
            root.clone(),
        )
        .unwrap();
        let external = test_base().join(format!("{name}-custom"));
        if mode == "tools" {
            seed(&external);
            demodesk_core::store::Store::save_settings_at(
                &external,
                &demodesk_core::store::Settings {
                    hlae_exe: Some(root.join("tools/tool.exe").to_string_lossy().into_owned()),
                    ..Default::default()
                },
            )
            .unwrap();
            directory.active = external.clone();
        }
        directory.initialize().unwrap();
        assert_eq!(directory.active, target);
        if mode == "tools" {
            assert!(
                external.join("settings.json").exists(),
                "do not delete external settings"
            );
            assert!(external.join("original.dem").exists());
            let clean = demodesk_core::store::Store::settings_at(&target).unwrap();
            assert!(
                clean.hlae_exe.is_none() && clean.ffmpeg_exe.is_none() && clean.vrf_exe.is_none()
            );
            assert_eq!(external.parent(), Some(test_base().as_path()));
            fs::remove_dir_all(external).unwrap();
        }
        assert!(!target.join("clips/test.mp4").exists());
        assert!(
            !old_target.exists(),
            "the old AppData root is removed completely"
        );
        assert!(test_base().join(format!("{name}-original.dem")).exists());
        assert!(directory.reset_preferences());
        if roots.is_empty() {
            roots.push(old_target);
        }
        assert!(
            roots.iter().all(|root| !root.exists()),
            "all old layers must be removed"
        );
        fs::write(target.join("clips/after.mp4"), b"new video").unwrap();
        directory.release_locks();
        let mut next =
            DataDirectory::load_outside(config.clone(), &target, legacy_config, root.clone())
                .unwrap();
        next.initialize().unwrap();
        assert!(target.join("clips/after.mp4").exists());
        next.release_locks();
        fs::write(
            report,
            serde_json::to_vec_pretty(
                &serde_json::json!({"mode":mode,"target":target,"roots":roots,"passed":true}),
            )
            .unwrap(),
        )
        .unwrap();
        // Only this test's uniquely named, synthetically seeded roots are removed.
        for old in &roots {
            assert_eq!(old.file_name(), root.file_name());
            reset::validate_root(old).unwrap();
            if old.exists() {
                fs::remove_dir_all(old).unwrap();
            }
        }
        fs::remove_file(test_base().join(format!("{name}-original.dem"))).unwrap();
        assert_eq!(target.parent(), Some(test_base().as_path()));
        fs::remove_dir_all(&target).unwrap();
        fs::remove_file(config).unwrap();
    }

    #[test]
    fn failed_bootstrap_save_restores_target_settings_and_releases_lock() {
        let root = scratch();
        let target = root.join("target");
        seed(&target);
        let previous = fs::read(target.join("settings.json")).unwrap();
        let blocked = root.join("not-a-directory");
        fs::write(&blocked, b"blocked").unwrap();
        let directory =
            DataDirectory::recovery(blocked.join("selection.json"), root.join("active"));
        let settings = demodesk_core::store::Settings {
            scan_game_replays: false,
            ..Default::default()
        };
        assert!(directory.commit_switch(target.clone(), &settings).is_err());
        assert_eq!(fs::read(target.join("settings.json")).unwrap(), previous);
        assert!(directory.selected().is_none());
        let lock = physical::lock_root(&target).unwrap();
        drop(lock);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[cfg(windows)]
    fn every_settings_directory_is_checked_for_app_data() {
        let path =
            PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap()).join("DemoDesk-path-check");
        for key in [
            "steamDir",
            "cs2Dir",
            "hlaeExe",
            "ffmpegExe",
            "vrfExe",
            "replayFolders",
        ] {
            let mut settings =
                serde_json::to_value(demodesk_core::store::Settings::default()).unwrap();
            settings[key] = if key == "replayFolders" {
                serde_json::json!([path])
            } else {
                serde_json::json!(path)
            };
            let settings = serde_json::from_value(settings).unwrap();
            assert!(
                DataDirectory::settings_use_app_data(&settings).unwrap(),
                "{key}"
            );
        }
        assert!(!DataDirectory::settings_use_app_data(&Default::default()).unwrap());
    }

    #[test]
    fn outside_upgrade_preserves_custom_selection_and_new_bootstrap_wins() {
        let root = scratch();
        let custom = root.join("custom");
        seed(&custom);
        let legacy_config = root.join("legacy.json");
        let legacy_default = root.join("old-default");
        let legacy = DataDirectory::load(legacy_config.clone(), &legacy_default).unwrap();
        legacy.save(Some(custom.clone())).unwrap();
        let config = root.join("new.json");
        let default = root.join("new-default");
        let mut directory = DataDirectory::load_outside(
            config.clone(),
            &default,
            legacy_config.clone(),
            legacy_default.clone(),
        )
        .unwrap();
        directory.initialize().unwrap();
        assert_eq!(directory.active, custom);
        assert!(custom.join("clips/test.mp4").exists());
        assert!(!directory.reset_preferences());
        directory.release_locks();
        legacy.save(Some(root.join("stale"))).unwrap();
        let next =
            DataDirectory::load_outside(config, &default, legacy_config, legacy_default).unwrap();
        assert_eq!(next.active, custom);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn physical_directory_is_preserved_across_startups() {
        let root = scratch();
        let data = root.join("data");
        seed(&data);
        let config = root.join("preferences/selection.json");
        for _ in 0..2 {
            let mut directory = DataDirectory::load(config.clone(), &data).unwrap();
            directory.initialize().unwrap();
            assert!(!directory.reset_preferences());
            assert!(data.join("clips/test.mp4").exists());
            assert!(data.join("parsed/test.error.json").exists());
            directory.release_locks();
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn mixed_old_data_is_detected_even_when_new_writes_are_physical() {
        let root = scratch();
        let real = root.join("real");
        let private = root.join("private");
        seed(&real);
        seed(&private);
        let (target, roots) = inspect_with_mapping(&real, Some(private.clone())).unwrap();
        assert_eq!(target, private);
        assert_eq!(roots, vec![private.clone(), real.clone()]);
        let config = root.join("preferences/selection.json");
        let mut directory = DataDirectory::load(config, &real).unwrap();
        let state = Selection {
            data_dir: Some(real.clone()),
            pending_reset: Some(PendingReset {
                target: real.clone(),
                roots,
            }),
            reset_preferences: true,
        };
        directory.write_selection(&state).unwrap();
        *directory.selected.lock().unwrap() = state;
        assert!(
            directory.initialize().is_err(),
            "an interrupted reset cannot authorize arbitrary roots"
        );
        for path in [&real, &private] {
            assert!(path.join("clips/test.mp4").exists());
            assert!(path.join("settings.json").exists());
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn unsafe_cleanup_stops_before_deleting_any_app_files() {
        let root = scratch();
        seed(&root);
        fs::write(root.join("tools/source.dem"), b"original").unwrap();
        assert!(reset::clean(&[root.clone()], &[root.clone()]).is_err());
        assert!(root.join("parsed/test.error.json").exists());
        assert!(root.join("settings.json").exists());
        assert!(reset::validate_roots(&[root.clone(), root.join("clips")]).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn directory_switch_applies_only_edited_settings_and_keeps_both_data_sets() {
        use demodesk_core::store::{Settings, Store};
        let root = scratch();
        let old = root.join("old");
        let target = root.join("new");
        seed(&old);
        seed(&target);
        let original = Settings {
            language: Some("en".into()),
            hlae_exe: Some("old.exe".into()),
            ..Settings::default()
        };
        let edited = Settings {
            language: Some("ja".into()),
            hlae_exe: None,
            ..original.clone()
        };
        Store::save_settings_at(
            &target,
            &Settings {
                scan_game_replays: false,
                vrf_exe: Some("external.exe".into()),
                ..Settings::default()
            },
        )
        .unwrap();
        let mut directory = DataDirectory::load(root.join("config/selection.json"), &old).unwrap();
        directory.initialize().unwrap();
        let merged = directory
            .target_settings(&target, &original, &edited)
            .unwrap();
        assert!(!merged.scan_game_replays);
        assert_eq!(merged.language.as_deref(), Some("ja"));
        assert_eq!(merged.hlae_exe, None);
        directory.commit_switch(target.clone(), &merged).unwrap();
        assert_eq!(directory.selected(), Some(target.clone()));
        assert_eq!(
            directory.active, old,
            "saving must not switch the live data directory"
        );
        let target_lock = physical::lock_root(&target).unwrap();
        drop(target_lock);
        assert!(old.join("clips/test.mp4").exists());
        assert!(target.join("clips/test.mp4").exists());
        directory.release_locks();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_inaccessible_tool_directory_before_use() {
        let root = test_base().join(format!(
            "demodesk-access-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        prepare(&root).unwrap();
        let blocked = root.join("tools").join("vrf");
        fs::write(&blocked, b"not a directory").unwrap();
        let error = prepare(&root).unwrap_err();
        assert!(error.contains(&blocked.display().to_string()));
        assert_eq!(fs::read(&blocked).unwrap(), b"not a directory");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn store_default_survives_selection_reset() {
        let root = test_base().join(format!(
            "demodesk-msix-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let config = root.join("preferences/data-directory.json");
        let default = root.join("local/demodesk-data");
        let directory = DataDirectory::load(config.clone(), &default).unwrap();
        directory.prepare().unwrap();
        assert_eq!(directory.active, default);
        let custom = root.join("custom");
        directory
            .save(
                directory
                    .validate(Some(custom.to_string_lossy().into_owned()))
                    .unwrap(),
            )
            .unwrap();
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
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = test_base().join(format!("demodesk-recovery-{}-{stamp}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let config = root.join("data-directory.json");
        let default = root.join("local/demodesk-data");
        let unavailable = root.join("blocked");
        fs::write(&unavailable, b"existing file").unwrap();
        fs::write(
            &config,
            serde_json::to_vec(&Selection {
                data_dir: Some(unavailable.clone()),
                ..Selection::default()
            })
            .unwrap(),
        )
        .unwrap();
        let directory = DataDirectory::load(config.clone(), &default).unwrap();
        assert!(directory.prepare().is_err());
        assert_eq!(directory.selected(), Some(unavailable.clone()));
        assert!(
            !directory.default.exists(),
            "must not silently open the default store"
        );
        assert!(directory
            .validate(Some(unavailable.to_string_lossy().into_owned()))
            .is_err());
        let replacement = root.join("replacement");
        directory
            .save(
                directory
                    .validate(Some(replacement.to_string_lossy().into_owned()))
                    .unwrap(),
            )
            .unwrap();
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
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = test_base().join(format!("demodesk-directory-{}-{stamp}", std::process::id()));
        let default = root.join("local/demodesk-data");
        let config = root.join("preferences/data-directory.json");
        let initial = DataDirectory::load(config.clone(), &default).unwrap();
        initial.prepare().unwrap();
        assert_eq!(initial.active, root.join("local/demodesk-data"));
        let custom = root.join("chosen/demodesk-data");
        let selected = initial
            .validate(Some(custom.to_string_lossy().into_owned()))
            .unwrap();
        initial.save(selected).unwrap();
        assert_eq!(initial.active, initial.default);
        let next = DataDirectory::load(config.clone(), &default).unwrap();
        assert_eq!(next.active, custom);
        next.save(next.validate(None).unwrap()).unwrap();
        let moved_default = root.join("moved/demodesk-data");
        let cleared = DataDirectory::load(config, &moved_default).unwrap();
        // Clearing stores no override and follows the default on the next launch.
        assert_eq!(cleared.active, moved_default);
        assert_eq!(cleared.selected(), None);
        let stored: serde_json::Value =
            serde_json::from_slice(&fs::read(&cleared.config_file).unwrap()).unwrap();
        assert!(stored["dataDir"].is_null());
        assert!(cleared.validate(Some("relative/path".into())).is_err());
        let file = root.join("not-a-directory");
        fs::write(&file, b"file").unwrap();
        assert!(cleared
            .validate(Some(file.to_string_lossy().into_owned()))
            .is_err());
        // Remove only the unique fixture directory created by this test.
        fs::remove_dir_all(root).unwrap();
    }
}
