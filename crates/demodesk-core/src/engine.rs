//! Background work: demo parsing and the sequential render queue. The engine
//! owns the [`Store`] (config, parse results and render jobs), scans demos from
//! disk, and loads saved analysis into memory on demand. Changes are reported
//! through a [`Notify`] sink so the Tauri layer can forward them as window events.

use crate::parser::DemoParser;
use crate::render::paths::{find_cs2_dir, find_steam_dir, replays_dir, resolve_tool_paths, PathOverrides, ToolPaths};
use crate::radar::{ensure_map_assets, MapAssets};
use crate::replay::build_replay;
use crate::render::{clean_leftovers, doctor, render_highlights, run_setup, SetupTool, DoctorReport, RenderJobInput, RenderOptions};
use crate::stats::build_parsed_demo;
use crate::stats::ParsedDemo;
use crate::store::{now, DemoMeta, DemoStatus, DemoSummary, JobOutput, JobStatus, RenderJob, Settings, Store};
use anyhow::{anyhow, Result};
use serde::Serialize;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// Events the UI cares about. Payloads are already JSON-serializable.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Event {
    DemoChanged { demo: DemoMeta },
    JobChanged { job: RenderJob },
    SetupLog { line: String },
    SetupFinished { tool: SetupTool, ok: bool, error: Option<String> },
}

pub trait Notify: Send + Sync + 'static {
    fn notify(&self, event: Event);
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Detected {
    pub steam_dir: Option<PathBuf>,
    pub cs2_dir: Option<PathBuf>,
    pub replays_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupState {
    pub running: bool,
    pub log: Vec<String>,
}

/// In-memory state of one demo: what the last scan saw plus the parse result.
#[derive(Clone)]
struct DemoEntry {
    meta: DemoMeta,
    parsed: Option<Arc<ParsedDemo>>,
    auto_complete: Option<bool>,
}

impl DemoEntry {
    /// Back to "not parsed": no result in memory, nothing derived in the meta.
    fn reset(&mut self) {
        self.parsed = None;
        self.meta.status = DemoStatus::New;
        self.meta.error = None;
        self.meta.summary = None;
        self.meta.map_name = None;
        self.meta.parsed_at = None;
    }
}

pub struct Engine {
    store: Store,
    data_dir: PathBuf,
    notify: Arc<dyn Notify>,
    demos: Mutex<HashMap<String, DemoEntry>>,
    registered_demos: Mutex<Vec<PathBuf>>,
    parsing: Mutex<HashSet<String>>,
    parser: Arc<DemoParser>,
    render_queue: Mutex<VecDeque<String>>,
    /// Serializes replay builds (each reads the whole demo again).
    replay_lock: Mutex<()>,
    /// Serializes radar extraction (one Source2Viewer-CLI at a time).
    radar_lock: Mutex<()>,
    active_job: Mutex<Option<(String, Arc<AtomicBool>)>>,
    render_worker_running: AtomicBool,
    setup_running: AtomicBool,
    setup_log: Mutex<Vec<String>>,
    settings_lock: Mutex<()>,
}

impl Engine {
    pub fn new(data_dir: PathBuf, notify: Arc<dyn Notify>) -> Result<Arc<Self>> {
        let store = Store::open(data_dir.clone())?;
        // Jobs that were running when the app died are not running any more.
        for mut job in store.list_jobs() {
            if matches!(job.status, JobStatus::Running | JobStatus::Queued) {
                job.status = JobStatus::Error;
                job.error = Some("app was closed while the job was running".into());
                job.finished_at = Some(now());
                let _ = store.save_job(&job);
            }
        }
        let registered_demos = store.registered_demos()?;
        let engine = Arc::new(Self {
            store,
            data_dir,
            notify,
            demos: Mutex::new(HashMap::new()),
            registered_demos: Mutex::new(registered_demos),
            parsing: Mutex::new(HashSet::new()),
            parser: Arc::new(DemoParser::new()),
            render_queue: Mutex::new(VecDeque::new()),
            replay_lock: Mutex::new(()),
            radar_lock: Mutex::new(()),
            active_job: Mutex::new(None),
            render_worker_running: AtomicBool::new(false),
            setup_running: AtomicBool::new(false),
            setup_log: Mutex::new(vec![]),
            settings_lock: Mutex::new(()),
        });
        // Nothing can be recording yet, so an old plugin install is a leftover.
        engine.clean_leftovers();
        Ok(engine)
    }

    // ---- paths / settings ----
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }
    pub fn settings(&self) -> Settings {
        self.store.settings()
    }
    /// Validate, then write. Returns the problems instead of writing when there are any.
    pub fn save_settings(&self, settings: Settings) -> Result<(), Vec<String>> {
        let _guard = self.settings_lock.lock().unwrap();
        let clean = settings.normalized();
        let problems = self.validate_settings(&clean);
        if !problems.is_empty() {
            return Err(problems);
        }
        self.store.save_settings(&clean).map_err(|e| vec![format!("{e:#}")])?;
        self.clean_leftovers();
        Ok(())
    }
    /// Sizes of the disposable folders: (parsed, clips, radar).
    pub fn storage_bytes(&self) -> (u64, u64, u64) {
        (self.store.parsed_bytes(), self.store.clips_bytes(), self.store.radar_bytes())
    }
    pub fn clear_radar(&self) -> u64 {
        self.store.clear_radar()
    }
    pub fn list_jobs(&self) -> Result<Vec<RenderJob>> {
        let mut jobs = self.store.list_jobs();
        for job in &mut jobs {
            let mut available = Vec::with_capacity(job.outputs.len());
            for output in std::mem::take(&mut job.outputs) {
                match Path::new(&output.file).try_exists() {
                    Ok(true) => available.push(output),
                    Ok(false) => {},
                    Err(error) => return Err(anyhow!("Cannot check video {}: {error}", output.file)),
                }
            }
            job.outputs = available;
        }
        Ok(jobs)
    }
    pub fn tools_dir(&self) -> PathBuf {
        self.data_dir.join("tools")
    }
    pub fn overrides(&self, s: &Settings) -> PathOverrides {
        let p = |v: &Option<String>| v.as_ref().map(PathBuf::from);
        PathOverrides { steam_dir: p(&s.steam_dir), cs2_dir: p(&s.cs2_dir), hlae_exe: p(&s.hlae_exe), ffmpeg_exe: p(&s.ffmpeg_exe), vrf_exe: p(&s.vrf_exe) }
    }
    pub fn tool_paths(&self) -> ToolPaths {
        resolve_tool_paths(&self.tools_dir(), &self.overrides(&self.store.settings()))
    }
    pub fn doctor(&self) -> DoctorReport {
        doctor(&self.tools_dir(), &self.overrides(&self.store.settings()))
    }
    /// Undo an interrupted run's plugin install — skipped while a render is active.
    pub fn clean_leftovers(&self) -> bool {
        if self.active_job_id().is_some() {
            return false;
        }
        clean_leftovers(&self.tools_dir(), &self.overrides(&self.store.settings()))
    }
    pub fn detected(&self) -> Detected {
        let steam_dir = find_steam_dir();
        let cs2_dir = find_cs2_dir(steam_dir.as_deref());
        Detected { replays_dir: cs2_dir.as_deref().map(replays_dir), steam_dir, cs2_dir }
    }
    pub fn replay_folders(&self) -> Vec<PathBuf> {
        let s = self.store.settings();
        let mut folders: Vec<PathBuf> = vec![];
        if s.scan_game_replays {
            let cs2 = resolve_tool_paths(&self.tools_dir(), &self.overrides(&s)).cs2_dir;
            if let Some(cs2) = cs2 {
                folders.push(replays_dir(&cs2));
            }
        }
        folders.extend(s.replay_folders.iter().map(PathBuf::from));
        folders.dedup();
        folders.into_iter().filter(|f| f.is_dir()).collect()
    }

    pub fn validate_settings(&self, s: &Settings) -> Vec<String> {
        let mut problems = vec![];
        let o = self.overrides(s);
        if let Some(steam) = &o.steam_dir {
            if !steam.join("steam.exe").is_file() {
                problems.push(format!("steam.exe not found under {}", steam.display()));
            }
        }
        if let Some(cs2) = &o.cs2_dir {
            if !crate::render::paths::cs2_exe_in(cs2).is_file() {
                problems.push(format!("game\\bin\\win64\\cs2.exe not found under {}", cs2.display()));
            }
        }
        if let Some(p) = &o.hlae_exe {
            if !p.is_file() {
                problems.push(format!("HLAE not found: {}", p.display()));
            }
        }
        if let Some(p) = &o.ffmpeg_exe {
            if !p.is_file() {
                problems.push(format!("FFmpeg not found: {}", p.display()));
            }
        }
        if let Some(p) = &o.vrf_exe {
            if !p.is_file() {
                problems.push(format!("Source 2 Viewer CLI not found: {}", p.display()));
            }
        }
        for f in &s.replay_folders {
            if !Path::new(f).is_dir() {
                problems.push(format!("folder not found: {f}"));
            }
        }
        problems
    }

    // ---- demos ----
    /// Stable id for a path (case-insensitive, so D:\x and d:\x match).
    pub fn demo_id(path: &Path) -> String {
        let key = path.to_string_lossy().to_lowercase().replace('/', "\\");
        sha1_smol::Sha1::from(key.as_bytes()).digest().to_string()[..12].to_string()
    }

    fn is_dem(path: &Path) -> bool {
        path.extension().map(|e| e.to_string_lossy().eq_ignore_ascii_case("dem")).unwrap_or(false)
    }

    /// Scan configured folders plus individually registered files in their original locations.
    fn demo_paths(&self) -> Vec<PathBuf> {
        let mut out: Vec<PathBuf> = self.registered_demos.lock().unwrap().iter()
            .filter(|path| Self::is_dem(path) && path.is_file()).cloned().collect();
        for folder in self.replay_folders() {
            let Ok(rd) = std::fs::read_dir(&folder) else { continue };
            for entry in rd.flatten() {
                let path = entry.path();
                if Self::is_dem(&path) && path.is_file() {
                    out.push(path);
                }
            }
        }
        let mut seen = HashSet::new();
        out.retain(|p| seen.insert(Self::demo_id(p)));
        out
    }

    /// Rescan the file system and merge with the in-memory parse state.
    pub fn list_demos(self: &Arc<Self>) -> Vec<DemoMeta> {
        // Everything that touches the disk happens before the lock.
        let scanned: Vec<(PathBuf, u64, f64, f64, Option<f64>)> = self
            .demo_paths()
            .into_iter()
            .filter_map(|path| {
                let st = std::fs::metadata(&path).ok()?;
                let mtime_ms = st.modified().ok().and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis() as f64).unwrap_or(0.0);
                let created_ms = st.created().ok().and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis() as f64).unwrap_or(mtime_ms);
                let match_time_ms = crate::store::match_time_ms(&path);
                Some((path, st.len(), mtime_ms, created_ms, match_time_ms))
            })
            .collect();
        let keep: HashSet<String> = scanned.iter().map(|(p, _, _, _, _)| Self::demo_id(p)).collect();
        let list = {
            let mut demos = self.demos.lock().unwrap();
            let mut stale: Vec<String> = vec![];
            for (path, bytes, mtime_ms, created_ms, match_time_ms) in scanned {
                let id = Self::demo_id(&path);
                let entry = demos.entry(id.clone()).or_insert_with(|| {
                    // A result stored by an earlier session counts as parsed (loaded lazily).
                    let failure = self.store.parse_error(&id);
                    let stored = self.store.read_summary(&id, bytes, mtime_ms);
                    DemoEntry {
                        meta: DemoMeta {
                            id,
                            name: path.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default(),
                            path: path.to_string_lossy().to_string(),
                            bytes,
                            mtime_ms,
                            created_ms,
                            match_time_ms,
                            status: if failure.is_some() { DemoStatus::Error } else if stored.is_some() { DemoStatus::Parsed } else { DemoStatus::New },
                            error: failure,
                            map_name: stored.as_ref().map(|s| s.map_name.clone()),
                            parsed_at: stored.as_ref().map(|s| s.parsed_at.clone()),
                            summary: stored.map(|s| s.summary),
                        },
                        parsed: None,
                        auto_complete: None,
                    }
                });
                entry.meta.path = path.to_string_lossy().to_string();
                if !entry.meta.same_file(bytes, mtime_ms) {
                    entry.auto_complete = None;
                }
                // File changed under us: the parse result no longer describes it.
                if entry.meta.status == DemoStatus::Parsed && !entry.meta.same_file(bytes, mtime_ms) {
                    entry.reset();
                    stale.push(entry.meta.id.clone());
                }
                entry.meta.bytes = bytes;
                entry.meta.mtime_ms = mtime_ms;
                entry.meta.created_ms = created_ms;
                entry.meta.match_time_ms = match_time_ms;
            }
            let parsing = self.parsing.lock().unwrap();
            demos.retain(|id, _| keep.contains(id) || parsing.contains(id));
            for id in &stale {
                self.store.delete_parsed(id);
            }
            let mut list: Vec<DemoMeta> = demos.values().map(|e| e.meta.clone()).collect();
            list.sort_by(|a, b| b.date_ms().total_cmp(&a.date_ms()).then_with(|| a.id.cmp(&b.id)));
            list
        };
        self.store.prune_parsed(&keep);
        self.auto_parse_next();
        let demos = self.demos.lock().unwrap();
        list.into_iter().map(|meta| demos.get(&meta.id).map(|e| e.meta.clone()).unwrap_or(meta)).collect()
    }

    /// Meta + parse result; a result stored on disk is loaded on first use.
    pub fn get_demo(&self, id: &str) -> Option<(DemoMeta, Option<Arc<ParsedDemo>>)> {
        let (meta, parsed) = self.demos.lock().unwrap().get(id).map(|e| (e.meta.clone(), e.parsed.clone()))?;
        if parsed.is_some() || meta.status != DemoStatus::Parsed {
            return Some((meta, parsed));
        }
        match self.store.read_parsed(id).map(Arc::new) {
            Some(p) => {
                let mut demos = self.demos.lock().unwrap();
                if let Some(e) = demos.get_mut(id) {
                    // A cache read begun before clearing must not restore the old result.
                    if e.meta.status != DemoStatus::Parsed || e.meta.parsed_at != meta.parsed_at {
                        return Some((e.meta.clone(), e.parsed.clone()));
                    }
                    if e.parsed.is_none() {
                        e.parsed = Some(p.clone());
                    }
                }
                Some((meta, Some(p)))
            }
            None => {
                // Stored result unreadable: fall back to "not parsed".
                self.store.delete_parsed(id);
                self.update_meta(id, |e| { if e.meta.status != DemoStatus::Error { e.reset(); } }).map(|m| (m, None))
            }
        }
    }
    pub fn parsed(&self, id: &str) -> Option<Arc<ParsedDemo>> {
        self.get_demo(id).and_then(|(_, p)| p)
    }

    /// Register the selected file in place. Only derived data belongs to the store.
    pub fn add_demo(self: &Arc<Self>, path: &Path) -> Result<DemoMeta> {
        if !path.is_file() || !Self::is_dem(path) {
            return Err(anyhow!("not a .dem file: {}", path.display()));
        }
        let path = std::path::absolute(path)?;
        let id = Self::demo_id(&path);
        {
            let mut registered = self.registered_demos.lock().unwrap();
            if !registered.iter().any(|p| Self::demo_id(p) == id) {
                let mut updated = registered.clone();
                updated.push(path);
                self.store.save_registered_demos(&updated)?;
                *registered = updated;
            }
        }
        self.list_demos().into_iter().find(|m| m.id == id).ok_or_else(|| anyhow!("demo no longer available after registering"))
    }

    pub fn is_parsing(&self, id: &str) -> bool {
        self.parsing.lock().unwrap().contains(id)
    }

    fn update_meta(&self, id: &str, f: impl FnOnce(&mut DemoEntry)) -> Option<DemoMeta> {
        let mut demos = self.demos.lock().unwrap();
        let entry = demos.get_mut(id)?;
        f(entry);
        Some(entry.meta.clone())
    }

    fn auto_parse_next(self: &Arc<Self>) {
        let mut candidates: Vec<_> = self.demos.lock().unwrap().values()
            .filter(|e| e.meta.status == DemoStatus::New && e.auto_complete != Some(false))
            .map(|e| (e.meta.id.clone(), e.meta.mtime_ms)).collect();
        candidates.sort_by(|a, b| b.1.total_cmp(&a.1));
        for (id, _) in candidates {
            if let Err(error) = self.start_parse(&id, true) {
                eprintln!("could not auto-parse {id}: {error:#}");
            }
            if !self.parsing.lock().unwrap().is_empty() { break; }
        }
    }

    pub fn parse_demo(self: &Arc<Self>, id: &str) -> Result<()> {
        self.start_parse(id, false)
    }

    fn start_parse(self: &Arc<Self>, id: &str, automatic: bool) -> Result<()> {
        let source_guard = if automatic {
            let (meta, complete) = {
                let demos = self.demos.lock().unwrap();
                let entry = demos.get(id).ok_or_else(|| anyhow!("demo not found"))?;
                if entry.meta.status != DemoStatus::New || entry.auto_complete == Some(false) { return Ok(()); }
                (entry.meta.clone(), entry.auto_complete)
            };
            let mut options = std::fs::OpenOptions::new();
            options.read(true);
            // Refuse active writers and keep the source unchanged throughout parsing.
            #[cfg(windows)]
            {
                use std::os::windows::fs::OpenOptionsExt;
                options.share_mode(1); // FILE_SHARE_READ
            }
            let Ok(mut file) = options.open(&meta.path) else { return Ok(()); };
            let metadata = file.metadata()?;
            let modified = metadata.modified()?.duration_since(std::time::UNIX_EPOCH)?.as_millis() as f64;
            if !meta.same_file(metadata.len(), modified) { return Ok(()); }
            let complete = match complete {
                Some(ready) => ready,
                None => crate::demo_readiness::is_complete(&mut file)?,
            };
            if let Some(entry) = self.demos.lock().unwrap().get_mut(id) {
                if entry.meta.same_file(metadata.len(), modified) { entry.auto_complete = Some(complete); }
            }
            if !complete { return Ok(()); }
            Some(file)
        } else { None };
        // Finish any in-flight replay write before invalidating its cache.
        let _replay_guard = self.replay_lock.lock().unwrap();
        let meta = {
            // Same lock order as scanning; claim the demo and the automatic slot together.
            let mut demos = self.demos.lock().unwrap();
            let entry = demos.get_mut(id).ok_or_else(|| anyhow!("demo not found"))?;
            let mut parsing = self.parsing.lock().unwrap();
            if parsing.contains(id) || (automatic && (!parsing.is_empty() || entry.meta.status != DemoStatus::New)) {
                return Ok(());
            }
            parsing.insert(id.to_string());
            entry.reset();
            entry.meta.status = DemoStatus::Parsing;
            entry.meta.error = None;
            entry.meta.clone()
        };
        self.store.delete_parsed(id);
        // Also covers app termination during parsing: retry then requires an explicit click.
        if let Err(error) = self.store.write_parse_error(id, "Parsing was interrupted; parse manually to retry.") {
            self.finish_parse_error(id, format!("Cannot save parse state: {error:#}"));
            return Err(error);
        }
        self.notify.notify(Event::DemoChanged { demo: meta.clone() });
        let engine = self.clone();
        let id = id.to_string();
        let thread_id = id.clone();
        let spawned = std::thread::Builder::new().name(format!("parse-{id}")).spawn(move || {
            let _source_guard = source_guard;
            let id = thread_id;
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| engine.parser.load_demo(Path::new(&meta.path)).map(build_parsed_demo)));
            let outcome = match result {
                Ok(Ok(parsed)) => engine.store.write_parsed(&id, &meta, &parsed)
                    .and_then(|_| engine.store.clear_parse_error(&id))
                    .map(|_| parsed).map_err(|e| format!("Cannot save parse result: {e:#}")),
                Ok(Err(err)) => Err(format!("{err:#}")),
                Err(_) => Err("parser crashed (unsupported or corrupt demo?)".to_string()),
            };
            match outcome {
                Ok(parsed) => {
                    let fresh = engine.update_meta(&id, |e| {
                        e.meta.status = DemoStatus::Parsed;
                        e.meta.error = None;
                        e.meta.map_name = Some(parsed.info.map_name.clone());
                        e.meta.parsed_at = Some(parsed.parsed_at.clone());
                        e.meta.summary = Some(DemoSummary::of(&parsed));
                        e.parsed = if automatic { None } else { Some(Arc::new(parsed)) };
                    });
                    engine.parsing.lock().unwrap().remove(&id);
                    if let Some(demo) = fresh { engine.notify.notify(Event::DemoChanged { demo }); }
                }
                Err(error) => engine.finish_parse_error(&id, error),
            }
            engine.auto_parse_next();
        });
        if let Err(error) = spawned {
            let message = format!("could not start the parse thread: {error}");
            self.finish_parse_error(&id, message.clone());
            return Err(anyhow!(message));
        }
        Ok(())
    }

    fn finish_parse_error(&self, id: &str, message: String) {
        if let Err(error) = self.store.write_parse_error(id, &message) {
            eprintln!("could not store parse failure for {id}: {error:#}");
        }
        let fresh = self.update_meta(id, |e| {
            e.reset();
            e.meta.status = DemoStatus::Error;
            e.meta.error = Some(message);
        });
        self.parsing.lock().unwrap().remove(id);
        if let Some(demo) = fresh { self.notify.notify(Event::DemoChanged { demo }); }
    }

    /// Path of the replay stream for a parsed demo, building it on first use
    /// (a few seconds: the demo is read again for positions and projectiles).
    pub fn replay_file(&self, id: &str) -> Result<PathBuf> {
        let _guard = self.replay_lock.lock().unwrap();
        let (meta, parsed) = self.get_demo(id).ok_or_else(|| anyhow!("demo not found"))?;
        let parsed = parsed.filter(|_| meta.status == DemoStatus::Parsed).ok_or_else(|| anyhow!("demo not parsed"))?;
        let path = self.store.replay_path(id);
        if path.is_file() && self.store.replay_is_current(id) {
            return Ok(path);
        }
        let bytes = std::fs::read(&meta.path).map_err(|e| anyhow!("reading {}: {e}", meta.path))?;
        let replay = build_replay(&self.parser, &parsed.info, &parsed.rounds, &bytes)?;
        self.store.write_replay(id, &replay)
    }

    /// Radar image(s) + world mapping for a map, extracted from the game files
    /// on first use (or again after a game update).
    pub fn map_assets(&self, map: &str) -> Result<MapAssets> {
        if map.is_empty() || !map.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err(anyhow!("unsupported map name {map:?}"));
        }
        let _guard = self.radar_lock.lock().unwrap();
        let tools = self.tool_paths();
        let radar_dir = self.store.radar_dir();
        if let Some(a) = crate::radar::read_map_assets(&radar_dir, map, tools.cs2_patch_version) {
            return Ok(a);
        }
        let cs2 = tools.cs2_dir.ok_or_else(|| anyhow!("CS2 folder not set"))?;
        let vrf = tools.vrf_exe.ok_or_else(|| anyhow!("Source 2 Viewer CLI not installed — use \"Download tools\" in settings"))?;
        ensure_map_assets(&radar_dir, &cs2, &vrf, map, tools.cs2_patch_version)
    }

    pub fn active_job_id(&self) -> Option<String> {
        self.active_job.lock().unwrap().as_ref().map(|(id, _)| id.clone())
    }

    /// Drop the parse result (memory + disk); the demo goes back to "new".
    pub fn clear_analysis(&self, id: &str) -> Result<()> {
        let _replay_guard = self.replay_lock.lock().unwrap();
        self.ensure_idle(id)?;
        self.store.delete_parsed(id);
        let meta = self.update_meta(id, DemoEntry::reset).ok_or_else(|| anyhow!("demo not found"))?;
        self.notify.notify(Event::DemoChanged { demo: meta });
        Ok(())
    }

    /// Delete every parse result (disk + memory). Refused while something is parsing or rendering.
    pub fn clear_all_analysis(&self) -> Result<u64> {
        let _replay_guard = self.replay_lock.lock().unwrap();
        if !self.parsing.lock().unwrap().is_empty() {
            return Err(anyhow!("a demo is being parsed"));
        }
        if self.active_job_id().is_some() {
            return Err(anyhow!("a render is running"));
        }
        let freed = self.store.clear_all_parsed();
        let metas: Vec<DemoMeta> = {
            let mut demos = self.demos.lock().unwrap();
            for e in demos.values_mut() {
                if e.meta.status != DemoStatus::Error { e.reset(); }
            }
            demos.values().map(|e| e.meta.clone()).collect()
        };
        for meta in metas {
            self.notify.notify(Event::DemoChanged { demo: meta });
        }
        Ok(freed)
    }

    /// Delete the .dem file from disk. Rendered videos are kept.
    pub fn remove_demo(&self, id: &str) -> Result<()> {
        self.ensure_idle(id)?;
        let (meta, _) = self.get_demo(id).ok_or_else(|| anyhow!("demo not found"))?;
        let path = Path::new(&meta.path);
        if path.exists() {
            std::fs::remove_file(path).map_err(|e| anyhow!("cannot delete {}: {e}", meta.path))?;
        }
        self.store.delete_parsed(id);
        self.store.clear_parse_error(id)?;
        {
            let mut registered = self.registered_demos.lock().unwrap();
            let updated: Vec<_> = registered.iter().filter(|p| Self::demo_id(p) != id).cloned().collect();
            self.store.save_registered_demos(&updated)?;
            *registered = updated;
        }
        self.demos.lock().unwrap().remove(id);
        Ok(())
    }
    fn ensure_idle(&self, demo_id: &str) -> Result<()> {
        if self.is_parsing(demo_id) {
            return Err(anyhow!("demo is being parsed"));
        }
        if let Some(active) = self.active_job_id() {
            if self.store.get_job(&active).map(|j| j.demo_id == demo_id).unwrap_or(false) {
                return Err(anyhow!("a render for this demo is running"));
            }
        }
        Ok(())
    }

    // ---- setup ----
    pub fn setup_state(&self) -> SetupState {
        SetupState { running: self.setup_running.load(Ordering::Relaxed), log: self.setup_log.lock().unwrap().clone() }
    }
    pub fn start_setup(self: &Arc<Self>, tool: SetupTool, force: bool) -> bool {
        if self.setup_running.swap(true, Ordering::SeqCst) {
            return false;
        }
        self.setup_log.lock().unwrap().clear();
        let engine = self.clone();
        std::thread::spawn(move || {
            let settings = engine.store.settings();
            let overrides = engine.overrides(&settings);
            let mut log = |line: String| {
                engine.setup_log.lock().unwrap().push(line.clone());
                engine.notify.notify(Event::SetupLog { line });
            };
            let result = run_setup(&engine.tools_dir(), &overrides, tool, force, &mut log).and_then(|_| {
                let _guard = engine.settings_lock.lock().unwrap();
                let mut settings = engine.store.settings();
                match tool {
                    SetupTool::Hlae => settings.hlae_exe = None,
                    SetupTool::Ffmpeg => settings.ffmpeg_exe = None,
                    SetupTool::Vrf => settings.vrf_exe = None,
                }
                engine.store.save_settings(&settings)
            });
            engine.setup_running.store(false, Ordering::SeqCst);
            match result {
                Ok(_) => engine.notify.notify(Event::SetupFinished { tool, ok: true, error: None }),
                Err(e) => {
                    engine.setup_log.lock().unwrap().push(format!("error: {e:#}"));
                    engine.notify.notify(Event::SetupFinished { tool, ok: false, error: Some(format!("{e:#}")) })
                }
            }
        });
        true
    }

    // ---- render queue ----
    pub fn enqueue_render(self: &Arc<Self>, demo_id: &str, highlight_ids: Vec<String>, options: RenderOptions) -> Result<RenderJob> {
        let (meta, parsed) = self.get_demo(demo_id).ok_or_else(|| anyhow!("demo not found"))?;
        if meta.status != DemoStatus::Parsed || parsed.is_none() {
            return Err(anyhow!("demo not parsed"));
        }
        if highlight_ids.is_empty() {
            return Err(anyhow!("pick at least one highlight"));
        }
        check_ascii_path(Path::new(&meta.path))?;
        let job = self.store.new_job(demo_id, highlight_ids, options)?;
        self.render_queue.lock().unwrap().push_back(job.id.clone());
        self.notify.notify(Event::JobChanged { job: job.clone() });
        self.pump();
        Ok(job)
    }

    pub fn cancel_job(&self, id: &str) -> bool {
        {
            let mut q = self.render_queue.lock().unwrap();
            if let Some(pos) = q.iter().position(|j| j == id) {
                q.remove(pos);
                if let Some(mut job) = self.store.get_job(id) {
                    job.status = JobStatus::Cancelled;
                    job.finished_at = Some(now());
                    self.persist(&job);
                }
                return true;
            }
        }
        if let Some((active, flag)) = self.active_job.lock().unwrap().as_ref() {
            if active == id {
                flag.store(true, Ordering::SeqCst);
                return true;
            }
        }
        false
    }

    pub fn delete_job(&self, id: &str) -> Result<()> {
        if self.active_job_id().as_deref() == Some(id) {
            return Err(anyhow!("job is running"));
        }
        self.cancel_job(id);
        self.store.delete_job(id)
    }

    /// Delete every render job and its videos. Refused while a render is running or queued.
    pub fn clear_all_clips(&self) -> Result<u64> {
        if self.active_job_id().is_some() || !self.render_queue.lock().unwrap().is_empty() {
            return Err(anyhow!("a render is running"));
        }
        Ok(self.store.clear_all_clips())
    }

    fn pump(self: &Arc<Self>) {
        if self.render_worker_running.swap(true, Ordering::SeqCst) {
            return;
        }
        let engine = self.clone();
        let spawned = std::thread::Builder::new().name("render-worker".into()).spawn(move || {
            loop {
                let id = {
                    let mut queue = engine.render_queue.lock().unwrap();
                    let Some(id) = queue.pop_front() else {
                        // Enqueue cannot observe an empty queue with a worker still marked busy.
                        engine.render_worker_running.store(false, Ordering::SeqCst);
                        return;
                    };
                    id
                };
                if let Some(job) = engine.store.get_job(&id) {
                    if job.status == JobStatus::Queued {
                        engine.run_job(job);
                    }
                }
            }
        });
        if let Err(e) = spawned {
            eprintln!("could not start the render worker: {e}");
            self.render_worker_running.store(false, Ordering::SeqCst);
        }
    }

    /// Write a job record and tell the UI; a failed write is logged, not fatal.
    fn persist(&self, job: &RenderJob) {
        if let Err(e) = self.store.save_job(job) {
            eprintln!("could not save job {}: {e:#}", job.id);
        }
        self.notify.notify(Event::JobChanged { job: job.clone() });
    }

    fn run_job(self: &Arc<Self>, mut job: RenderJob) {
        let cancel = Arc::new(AtomicBool::new(false));
        *self.active_job.lock().unwrap() = Some((job.id.clone(), cancel.clone()));
        job.status = JobStatus::Running;
        job.started_at = Some(now());
        job.stage = Some("starting".into());
        self.persist(&job);

        let progress = Mutex::new(job.clone());
        let mut log = |line: String| {
            let mut j = progress.lock().unwrap();
            j.log.push(line);
            if j.log.len() > 400 {
                let excess = j.log.len() - 400;
                j.log.drain(0..excess);
            }
            self.persist(&j);
        };
        let mut stage = |s: &str| {
            let mut j = progress.lock().unwrap();
            j.stage = Some(s.to_string());
            self.persist(&j);
        };
        let outcome = self.get_demo(&job.demo_id).ok_or_else(|| anyhow!("demo not found")).and_then(|(meta, parsed)| {
            let parsed = parsed.ok_or_else(|| anyhow!("demo not parsed (parse it again after restarting the app)"))?;
            let wanted: HashSet<&str> = job.highlight_ids.iter().map(|s| s.as_str()).collect();
            let highlights: Vec<_> = parsed.highlights.iter().filter(|h| wanted.contains(h.id.as_str())).cloned().collect();
            render_highlights(RenderJobInput {
                demo: &parsed.info,
                demo_path: PathBuf::from(&meta.path),
                highlights,
                output_dir: self.store.job_dir(&job.id),
                options: job.options.clone(),
                tools: self.tool_paths(),
                cancel: cancel.clone(),
                log: &mut log,
                stage: &mut stage,
            })
        });
        job.log = progress.into_inner().unwrap().log;

        *self.active_job.lock().unwrap() = None;
        job.finished_at = Some(now());
        job.stage = None;
        match outcome {
            Ok(result) => {
                job.outputs = job_outputs(&result);
                job.status = if job.outputs.is_empty() { JobStatus::Error } else { JobStatus::Done };
                if job.outputs.is_empty() {
                    job.error = Some("no clip was recorded".into());
                }
            }
            Err(e) => {
                let msg = format!("{e:#}");
                job.status = if msg == "cancelled" { JobStatus::Cancelled } else { JobStatus::Error };
                job.error = Some(msg);
            }
        }
        self.persist(&job);
    }
}

/// The game's `playdemo` only takes ASCII paths; refuse early with a clear message.
fn check_ascii_path(path: &Path) -> Result<()> {
    if path.to_string_lossy().is_ascii() {
        Ok(())
    } else {
        Err(anyhow!("CS2 can only play demos from an ASCII path (playdemo limitation) — move or rename it: {}", path.display()))
    }
}

/// One row per clip file, or the single merged video.
fn job_outputs(result: &crate::render::RenderResult) -> Vec<JobOutput> {
    let mut outputs: Vec<JobOutput> = result
        .clips
        .iter()
        .filter_map(|c| c.file.as_ref().map(|f| JobOutput { file: f.to_string_lossy().to_string(), bytes: c.bytes.unwrap_or(0), highlight_id: Some(c.highlight_id.clone()), title: c.title.clone(), is_final: false }))
        .collect();
    if let (Some(f), Some(b)) = (&result.final_video, result.final_bytes) {
        outputs.push(JobOutput { file: f.to_string_lossy().to_string(), bytes: b, highlight_id: None, title: "highlights (merged)".into(), is_final: true });
    }
    outputs
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    struct Events(mpsc::Sender<Event>);
    impl Notify for Events {
        fn notify(&self, event: Event) { let _ = self.0.send(event); }
    }

    #[test]
    fn setup_switches_only_the_installed_tool_to_managed_path() {
        let temp = tempfile::tempdir().unwrap();
        let (send, receive) = mpsc::channel();
        let engine = Engine::new(temp.path().join("data"), Arc::new(Events(send))).unwrap();
        for file in ["hlae/HLAE.exe", "hlae/x64/AfxHookSource2.dll", "ffmpeg/ffmpeg.exe", "ffmpeg/ffprobe.exe"] {
            let path = engine.tools_dir().join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, []).unwrap();
        }
        let vrf = engine.tools_dir().join("vrf").join(crate::render::setup::vrf_exe_name());
        std::fs::create_dir_all(vrf.parent().unwrap()).unwrap();
        std::fs::write(&vrf, []).unwrap();
        for tool in [SetupTool::Hlae, SetupTool::Ffmpeg, SetupTool::Vrf] {
            let mut expected = Settings {
                language: Some("ja".into()), hlae_exe: Some("missing-hlae.exe".into()),
                ffmpeg_exe: Some("missing-ffmpeg.exe".into()), vrf_exe: Some("missing-vrf.exe".into()),
                ..Settings::default()
            };
            engine.store.save_settings(&expected).unwrap();
            expected = engine.settings();
            assert!(engine.start_setup(tool, false));
            loop {
                if let Event::SetupFinished { ok, error, .. } = receive.recv_timeout(Duration::from_secs(5)).unwrap() {
                    assert!(ok, "{error:?}");
                    break;
                }
            }
            let paths = engine.tool_paths();
            match tool {
                SetupTool::Hlae => { expected.hlae_exe = None; assert!(paths.hlae_exe.unwrap().starts_with(engine.tools_dir())); }
                SetupTool::Ffmpeg => { expected.ffmpeg_exe = None; assert!(paths.ffmpeg_exe.unwrap().starts_with(engine.tools_dir())); }
                SetupTool::Vrf => { expected.vrf_exe = None; assert_eq!(paths.vrf_exe, Some(vrf.clone())); }
            }
            assert_eq!(serde_json::to_value(engine.settings()).unwrap(), serde_json::to_value(expected).unwrap());
        }
    }

    fn terminal_events(receiver: &mpsc::Receiver<Event>, count: usize) -> Vec<DemoMeta> {
        let mut finished = vec![];
        let mut running = HashSet::new();
        while finished.len() < count {
            if let Event::DemoChanged { demo } = receiver.recv_timeout(Duration::from_secs(10)).unwrap() {
                match demo.status {
                    DemoStatus::Parsing => {
                        assert!(running.insert(demo.id.clone()));
                        assert_eq!(running.len(), 1, "automatic parsing must be sequential");
                    }
                    DemoStatus::Error | DemoStatus::Parsed => {
                        assert!(running.remove(&demo.id));
                        finished.push(demo);
                    }
                    _ => {}
                }
            }
        }
        finished
    }

    #[test]
    fn list_jobs_filters_deleted_videos_without_changing_records() {
        let temp = tempfile::tempdir().unwrap();
        let (send, _receive) = mpsc::channel();
        let engine = Engine::new(temp.path().join("data"), Arc::new(Events(send))).unwrap();
        let mut job = engine.store.new_job("demo", vec![], RenderOptions::default()).unwrap();
        job.status = JobStatus::Done;
        for name in ["clip.mp4", "merged.mp4"] {
            let file = engine.store.job_dir(&job.id).join(name);
            std::fs::write(&file, b"video").unwrap();
            job.outputs.push(JobOutput { file: file.to_string_lossy().into_owned(), bytes: 5,
                highlight_id: None, title: name.into(), is_final: name == "merged.mp4" });
        }
        engine.store.save_job(&job).unwrap();
        let record = engine.store.job_dir(&job.id).join("job.json");
        let original = std::fs::read(&record).unwrap();
        assert_eq!(engine.list_jobs().unwrap()[0].outputs.len(), 2);
        std::fs::remove_file(&job.outputs[0].file).unwrap();
        let listed = engine.list_jobs().unwrap();
        assert_eq!(listed[0].outputs.len(), 1);
        assert!(listed[0].outputs[0].is_final);
        std::fs::remove_file(&job.outputs[1].file).unwrap();
        let listed = engine.list_jobs().unwrap();
        assert_eq!(listed.len(), 1);
        assert!(listed[0].outputs.is_empty());
        assert_eq!(listed[0].status, JobStatus::Done);
        std::fs::write(&job.outputs[0].file, b"restored").unwrap();
        assert_eq!(engine.list_jobs().unwrap()[0].outputs.len(), 1);
        assert_eq!(std::fs::read(record).unwrap(), original);
    }

    #[test]
    fn list_jobs_reports_video_check_errors_without_changing_records() {
        let temp = tempfile::tempdir().unwrap();
        let (send, _receive) = mpsc::channel();
        let engine = Engine::new(temp.path().join("data"), Arc::new(Events(send))).unwrap();
        let mut job = engine.store.new_job("demo", vec![], RenderOptions::default()).unwrap();
        let file = engine.store.job_dir(&job.id).join("invalid\0.mp4");
        job.outputs.push(JobOutput { file: file.to_string_lossy().into_owned(), bytes: 5,
            highlight_id: None, title: "invalid".into(), is_final: false });
        engine.store.save_job(&job).unwrap();
        let record = engine.store.job_dir(&job.id).join("job.json");
        let original = std::fs::read(&record).unwrap();
        assert!(engine.list_jobs().unwrap_err().to_string().contains("Cannot check video"));
        assert_eq!(std::fs::read(record).unwrap(), original);
    }

    #[test]
    fn registered_same_named_demos_stay_in_place_and_survive_restart() {
        let temp = tempfile::tempdir().unwrap();
        let data = temp.path().join("data");
        let first = temp.path().join("first/match.dem");
        let second = temp.path().join("second/match.dem");
        let store = Store::open(data.clone()).unwrap();
        store.save_settings(&Settings {
            scan_game_replays: false,
            cs2_dir: Some(temp.path().join("no-game").to_string_lossy().into_owned()),
            ..Settings::default()
        }).unwrap();
        for (path, bytes) in [(&first, "first source"), (&second, "second source")] {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, bytes).unwrap();
            // Keep this registration test independent of the parser worker.
            store.write_parse_error(&Engine::demo_id(path), bytes).unwrap();
        }
        let sibling = first.with_file_name("unselected.dem");
        std::fs::write(&sibling, "unselected").unwrap();
        let (send, _) = mpsc::channel();
        let engine = Engine::new(data.clone(), Arc::new(Events(send))).unwrap();
        let a = engine.add_demo(&first).unwrap();
        let b = engine.add_demo(&second).unwrap();
        assert_ne!(a.id, b.id);
        assert_eq!(Path::new(&a.path), first);
        assert_eq!(Path::new(&b.path), second);
        assert_eq!(engine.add_demo(&first).unwrap().id, a.id);
        assert_eq!(engine.list_demos().len(), 2);
        assert_eq!(store.registered_demos().unwrap(), vec![first.clone(), second.clone()]);
        assert!(engine.settings().replay_folders.is_empty());
        assert_eq!(std::fs::read_to_string(&first).unwrap(), "first source");
        assert_eq!(std::fs::read_to_string(&second).unwrap(), "second source");
        assert_eq!(std::fs::read_dir(first.parent().unwrap()).unwrap().count(), 2);
        assert_eq!(std::fs::read_dir(second.parent().unwrap()).unwrap().count(), 1);
        assert!(!data.join("match.dem").exists());
        drop(engine);
        let (send, _) = mpsc::channel();
        let restarted = Engine::new(data, Arc::new(Events(send))).unwrap();
        let listed = restarted.list_demos();
        assert_eq!(listed.len(), 2);
        assert!(listed.iter().any(|demo| demo.id == a.id && demo.error.as_deref() == Some("first source")));
        assert!(listed.iter().any(|demo| demo.id == b.id && demo.error.as_deref() == Some("second source")));
    }

    fn complete_invalid_demo() -> Vec<u8> {
        let mut bytes = b"PBDEMS2\0".to_vec();
        bytes.extend_from_slice(&[0; 8]);
        bytes.extend_from_slice(&[0, 0, 0]);
        bytes
    }

    #[test]
    fn automatic_parse_waits_for_complete_source() {
        let temp = tempfile::tempdir().unwrap();
        let data = temp.path().join("data");
        let source = temp.path().join("match.dem");
        let bytes = complete_invalid_demo();
        std::fs::write(&source, &bytes[..16]).unwrap();
        let store = Store::open(data.clone()).unwrap();
        store.save_settings(&Settings { scan_game_replays: false, ..Settings::default() }).unwrap();
        let (send, receive) = mpsc::channel();
        let engine = Engine::new(data, Arc::new(Events(send))).unwrap();
        let demo = engine.add_demo(&source).unwrap();
        assert_eq!(demo.status, DemoStatus::New);
        assert!(store.parse_error(&demo.id).is_none());
        engine.list_demos();
        assert!(receive.try_recv().is_err());
        assert_eq!(engine.demos.lock().unwrap()[&demo.id].auto_complete, Some(false));
        std::fs::write(&source, &bytes).unwrap();
        #[cfg(windows)]
        {
            let writer = std::fs::OpenOptions::new().append(true).open(&source).unwrap();
            engine.list_demos();
            assert!(receive.try_recv().is_err());
            assert!(store.parse_error(&demo.id).is_none());
            drop(writer);
        }
        engine.list_demos();
        // Complete framing is ready immediately; invalid game data still fails once.
        let completed = terminal_events(&receive, 1);
        assert_eq!(completed[0].status, DemoStatus::Error);
        assert!(store.parse_error(&demo.id).is_some());
    }

    #[test]
    fn auto_parse_failure_is_not_retried_until_manual_parse() {
        let temp = tempfile::tempdir().unwrap();
        let data = temp.path().join("data");
        let demos = temp.path().join("demos");
        std::fs::create_dir_all(&demos).unwrap();
        for name in ["first.dem", "second.dem"] {
            std::fs::write(demos.join(name), complete_invalid_demo()).unwrap();
        }
        Store::open(data.clone()).unwrap().save_settings(&Settings {
            scan_game_replays: false,
            replay_folders: vec![demos.to_string_lossy().into_owned()],
            cs2_dir: Some(temp.path().join("no-game").to_string_lossy().into_owned()),
            ..Settings::default()
        }).unwrap();
        let (send, receive) = mpsc::channel();
        let engine = Engine::new(data.clone(), Arc::new(Events(send))).unwrap();
        assert_eq!(engine.list_demos().len(), 2);
        let completed = terminal_events(&receive, 2);
        assert!(completed.iter().all(|d| d.status == DemoStatus::Error));
        // Scans, changed file contents and clearing cached analysis do not retry errors.
        std::fs::write(&completed[0].path, b"still not a demo, but changed").unwrap();
        engine.clear_all_analysis().unwrap();
        assert!(engine.list_demos().iter().all(|d| d.status == DemoStatus::Error));
        assert!(receive.try_iter().all(|e| !matches!(e, Event::DemoChanged { demo } if demo.status == DemoStatus::Parsing)));
        let settings = engine.settings();
        let mut disconnected = settings.clone();
        disconnected.replay_folders.clear();
        engine.store.save_settings(&disconnected).unwrap();
        assert!(engine.list_demos().is_empty());
        engine.store.save_settings(&settings).unwrap();
        assert!(engine.list_demos().iter().all(|d| d.status == DemoStatus::Error));
        drop(engine);
        let (send, receive) = mpsc::channel();
        let restarted = Engine::new(data, Arc::new(Events(send))).unwrap();
        assert!(restarted.list_demos().iter().all(|d| d.status == DemoStatus::Error));
        assert!(receive.try_recv().is_err());
        // A failed reparse must not leave old statistics or a reusable 2D cache.
        let id = &completed[0].id;
        let cache_dir = restarted.store.replay_path(id).parent().unwrap().to_path_buf();
        let caches: Vec<_> = ["json", "summary.json", "replay.json"].iter().map(|suffix| cache_dir.join(format!("{id}.{suffix}"))).collect();
        for path in &caches { std::fs::write(path, b"old cached data").unwrap(); }
        let other_replay = restarted.store.replay_path(&completed[1].id);
        std::fs::write(&other_replay, b"other demo").unwrap();
        let source_before = std::fs::read(&completed[0].path).unwrap();
        restarted.parse_demo(&completed[0].id).unwrap();
        let retried = terminal_events(&receive, 1);
        assert_eq!(retried[0].id, completed[0].id);
        assert_eq!(retried[0].status, DemoStatus::Error);
        assert!(caches.iter().all(|path| !path.exists()));
        assert!(restarted.parsed(id).is_none());
        assert_eq!(std::fs::read(&other_replay).unwrap(), b"other demo");
        assert_eq!(std::fs::read(&completed[0].path).unwrap(), source_before);
    }
}
