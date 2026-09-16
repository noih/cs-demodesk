//! Background work: demo parsing and the sequential render queue. The engine
//! owns the [`Store`] (config, parse results and render jobs), scans demos from
//! disk, and loads saved analysis into memory on demand. Changes are reported
//! through a [`Notify`] sink so the Tauri layer can forward them as window events.

use crate::parser::DemoParser;
use crate::radar::{ensure_map_assets, MapAssets};
use crate::render::paths::{
    find_cs2_dir, find_steam_dir, replays_dir, resolve_tool_paths, PathOverrides, ToolPaths,
};
use crate::render::{
    clean_leftovers, doctor, render_highlights, run_setup, DoctorReport, RenderJobInput,
    RenderOptions, SetupTool,
};
use crate::replay::build_replay;
use crate::stats::build_parsed_demo;
use crate::stats::ParsedDemo;
use crate::store::{
    now, DemoMeta, DemoStatus, DemoSummary, JobOutput, JobStatus, RenderJob, Settings, Store,
};
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
    DemoChanged {
        demo: DemoMeta,
    },
    AnalysisJobChanged {
        job: crate::scoring::queue::Job,
    },
    ScoringProgress {
        id: String,
        step: u8,
    },
    JobChanged {
        job: RenderJob,
    },
    SetupProgress {
        tool: SetupTool,
        progress: String,
    },
    SetupFinished {
        tool: SetupTool,
        ok: bool,
        installed: bool,
        cancelled: bool,
        error: Option<String>,
    },
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

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupState {
    pub running: bool,
    pub stopping: bool,
    pub log: Vec<String>,
    pub progress: Option<String>,
    #[serde(skip)]
    cancel: Arc<AtomicBool>,
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
    // ponytail: one scoring worker; use per-demo locks if concurrent scoring becomes necessary.
    scoring_lock: Mutex<()>,
    analysis_queue: Mutex<crate::scoring::queue::Queue>,
    active_job: Mutex<Option<(String, Arc<AtomicBool>)>>,
    render_worker_running: AtomicBool,
    setup: Mutex<HashMap<SetupTool, SetupState>>,
    tool_checks: Mutex<HashMap<SetupTool, crate::render::diagnostics::ToolCheck>>,
    tool_check_lock: Mutex<()>,
    settings_lock: Mutex<()>,
}

impl Engine {
    pub fn new(data_dir: PathBuf, notify: Arc<dyn Notify>) -> Result<Arc<Self>> {
        let store = Store::open(data_dir.clone())?;
        // Analysis jobs belong only to this process; remove the former on-disk queue.
        if let Err(error) = std::fs::remove_file(data_dir.join("analysis-jobs.json")) {
            if error.kind() != std::io::ErrorKind::NotFound {
                eprintln!("cannot remove old analysis queue: {error}");
            }
        }

        // Jobs that were running when the app died are not running any more.
        for mut job in store.list_jobs() {
            if matches!(job.status, JobStatus::Running | JobStatus::Queued) {
                job.status = JobStatus::Error;
                job.error = Some("app was closed while the job was running".into());
                job.error_code = Some(crate::ErrorCode::AppClosed);
                job.finished_at = Some(now());
                let _ = store.save_job(&job);
            }
        }
        let registered_demos = store.registered_demos()?;
        let engine = Arc::new(Self {
            store,
            data_dir: data_dir.clone(),
            notify,
            demos: Mutex::new(HashMap::new()),
            registered_demos: Mutex::new(registered_demos),
            parsing: Mutex::new(HashSet::new()),
            parser: Arc::new(DemoParser::new()),
            render_queue: Mutex::new(VecDeque::new()),
            replay_lock: Mutex::new(()),
            radar_lock: Mutex::new(()),
            scoring_lock: Mutex::new(()),
            analysis_queue: Mutex::new(crate::scoring::queue::Queue::default()),
            active_job: Mutex::new(None),
            render_worker_running: AtomicBool::new(false),
            setup: Mutex::new(HashMap::new()),
            tool_checks: Mutex::new(HashMap::new()),
            tool_check_lock: Mutex::new(()),
            settings_lock: Mutex::new(()),
        });
        // Nothing can be recording yet, so an old plugin install is a leftover.
        engine.clean_leftovers();
        Ok(engine)
    }

    pub fn analysis_jobs(&self) -> Vec<crate::scoring::queue::Job> {
        self.analysis_queue
            .lock()
            .unwrap()
            .jobs
            .iter()
            .rev()
            .cloned()
            .collect()
    }

    pub fn enqueue_analysis(
        self: &Arc<Self>,
        id: &str,
        force: bool,
    ) -> Result<crate::scoring::queue::Job> {
        let _guard = self.replay_lock.lock().unwrap();
        let (meta, parsed) = self.get_demo(id).ok_or_else(|| anyhow!("demo not found"))?;
        anyhow::ensure!(parsed.is_some(), "demo not parsed");
        let mut queue = self.analysis_queue.lock().unwrap();
        let job = queue.enqueue(&meta.id, force);
        self.notify
            .notify(Event::AnalysisJobChanged { job: job.clone() });
        if !queue.worker_running {
            queue.worker_running = true;
            let engine = self.clone();
            if let Err(error) = std::thread::Builder::new()
                .name("analysis-worker".into())
                .spawn(move || engine.run_analysis_queue())
            {
                queue.worker_running = false;
                for entry in queue
                    .jobs
                    .iter_mut()
                    .filter(|j| j.status == crate::scoring::queue::Status::Queued)
                {
                    entry.status = crate::scoring::queue::Status::Error;
                    entry.error = Some(format!("Cannot start analysis worker: {error}"));
                    entry.finished_at = Some(now());
                    entry.revision += 1;
                    self.notify
                        .notify(Event::AnalysisJobChanged { job: entry.clone() });
                }
                return Err(error.into());
            }
        }
        Ok(job)
    }

    fn analysis_progress(&self, demo_id: &str, step: u8) {
        let mut queue = self.analysis_queue.lock().unwrap();
        if let Some(job) = queue
            .jobs
            .iter_mut()
            .find(|j| j.demo_id == demo_id && j.status == crate::scoring::queue::Status::Running)
        {
            job.step = Some(step);
            job.revision += 1;
            let job = job.clone();
            self.notify.notify(Event::AnalysisJobChanged { job });
        }
    }

    fn run_analysis_queue(self: Arc<Self>) {
        loop {
            let job = {
                let mut queue = self.analysis_queue.lock().unwrap();
                let Some(job) = queue.claim() else {
                    queue.worker_running = false;
                    return;
                };
                self.notify
                    .notify(Event::AnalysisJobChanged { job: job.clone() });
                job
            };
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                self.score_match(&job.demo_id, job.force)
            }))
            .unwrap_or_else(|_| {
                Err(anyhow!(
                    "Analysis worker stopped unexpectedly. Retry the analysis."
                ))
            });
            // A panic must not poison later independent jobs.
            self.scoring_lock.clear_poison();
            let mut queue = self.analysis_queue.lock().unwrap();
            let stored = queue.jobs.iter_mut().find(|j| j.id == job.id).unwrap();
            stored.finished_at = Some(now());
            stored.revision += 1;
            match result {
                Ok(_) => {
                    stored.status = crate::scoring::queue::Status::Done;
                    stored.error = None;
                }
                Err(error) => {
                    stored.status = crate::scoring::queue::Status::Error;
                    stored.error = Some(format!("{error:#}"));
                }
            }
            let job = queue.jobs.iter().find(|j| j.id == job.id).unwrap().clone();
            self.notify.notify(Event::AnalysisJobChanged { job });
        }
    }

    /// History remains readable even if its source demo is missing or has changed.
    pub fn scoring_history(
        &self,
        id: &str,
        player_id: &str,
    ) -> Result<Vec<crate::scoring::Assessment>> {
        crate::scoring::history::list(&self.data_dir, id, player_id)
    }

    pub fn scoring_match_history(
        &self,
        id: &str,
    ) -> Result<std::collections::BTreeMap<String, Vec<crate::scoring::Assessment>>> {
        crate::scoring::history::list_match(&self.data_dir, id)
    }

    /// Explicit scoring only. Basic demo auto-analysis never enters this path.
    pub fn score_match(
        &self,
        id: &str,
        force: bool,
    ) -> Result<crate::scoring::history::MatchResponse> {
        use crate::scoring::{self, Assessment};
        use std::io::Read;
        use std::time::Instant;
        let _guard = self
            .scoring_lock
            .lock()
            .map_err(|_| anyhow!("analysis worker lock failed"))?;
        let progress = |step| {
            self.analysis_progress(id, step);
            self.notify.notify(Event::ScoringProgress {
                id: id.into(),
                step,
            });
        };
        progress(1);
        let started = Instant::now();
        let (meta, parsed) = self.get_demo(id).ok_or_else(|| anyhow!("demo not found"))?;
        let parsed = parsed.ok_or_else(|| anyhow!("demo not parsed"))?;
        let players: Vec<_> = parsed
            .info
            .players
            .iter()
            .map(|p| p.steamid.clone())
            .collect();
        anyhow::ensure!(!players.is_empty(), "demo has no players");
        let mut options = std::fs::OpenOptions::new();
        options.read(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.share_mode(1);
        }
        let mut source_file = options.open(&meta.path)?;
        let metadata = source_file.metadata()?;
        let modified = metadata
            .modified()?
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis() as f64;
        anyhow::ensure!(
            meta.same_file(metadata.len(), modified),
            "demo changed; parse it again before scoring"
        );
        anyhow::ensure!(
            crate::demo_readiness::is_complete(&mut source_file)?,
            "incomplete scoring source"
        );
        use std::io::Seek;
        source_file.rewind()?;
        let mut bytes = Vec::new();
        source_file.read_to_end(&mut bytes)?;
        let fingerprint = format!("sha1:{}", sha1_smol::Sha1::from(bytes.as_slice()).digest());
        crate::scoring::history::track_source(&self.data_dir, id, &fingerprint)?;
        if let Err(error) = scoring::history::clear_intermediates(&self.data_dir, &fingerprint) {
            eprintln!("could not clean old analysis intermediates for {id}: {error:#}");
        }
        let mut histories = self.scoring_match_history(id)?;
        histories.retain(|player, _| players.contains(player));
        for player in &players {
            histories.entry(player.clone()).or_default();
        }
        if !force
            && histories.values().all(|records| {
                records.iter().any(|r| {
                    r.demo_fingerprint == fingerprint
                        && r.ruleset_version == scoring::RULESET_VERSION
                })
            })
        {
            let provenance = &histories
                .values()
                .flat_map(|records| records.iter())
                .find(|r| {
                    r.demo_fingerprint == fingerprint
                        && r.ruleset_version == scoring::RULESET_VERSION
                })
                .expect("all players have matching history")
                .input_provenance;
            let generic_bytes = provenance["shared"]["bytes"].as_u64().unwrap_or(0);
            let diagnostic_bytes = provenance["diagnosticBytes"].as_u64().unwrap_or(0);
            return Ok(scoring::history::MatchResponse {
                source_fingerprint: fingerprint,
                players: histories,
                preparation_seconds: started.elapsed().as_secs_f64(),
                analysis_seconds: 0.0,
                shared_bytes: generic_bytes + diagnostic_bytes,
                generic_bytes,
                diagnostic_bytes,
            });
        }
        let mut shared_file = crate::analysis::compact::prepare(
            &self.parser,
            &bytes,
            &self.data_dir,
            crate::analysis::compact::memory_limit(),
        )?;
        drop(bytes);
        let shared_storage = if shared_file.is_rolled() {
            "temporaryFile"
        } else {
            "memory"
        };
        let mut shared_hash = sha1_smol::Sha1::new();
        let mut shared_bytes = 0;
        let mut buffer = [0; 65536];
        loop {
            let n = shared_file.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            shared_hash.update(&buffer[..n]);
            shared_bytes += n as u64;
        }
        shared_file.rewind()?;
        let tools = self.tool_paths();
        let game = tools.cs2_dir.ok_or_else(|| anyhow!("CS2 folder not set"))?;
        let vrf = tools
            .vrf_exe
            .ok_or_else(|| anyhow!("Source 2 Viewer CLI not installed"))?;
        let native = crate::analysis::native_body::prepare_reader(
            &mut shared_file,
            &self.data_dir,
            &game,
            &vrf,
        )?;
        shared_file.rewind()?;
        let visibility_result = crate::analysis::visibility_assets::prepare(
            &self.data_dir,
            &game,
            &vrf,
            &parsed.info.map_name,
        );
        let visibility_error = visibility_result.as_ref().err().map(|e| format!("{e:#}"));
        let visibility = visibility_result.ok();
        let header = &native.header;
        anyhow::ensure!(
            header.source.demo_fingerprint.as_deref() == Some(&fingerprint),
            "shared source mismatch"
        );
        let preparation_seconds = started.elapsed().as_secs_f64();
        progress(2);
        let started = Instant::now();
        let (mut checks, body_coverage) = scoring::native::evaluate(
            &mut shared_file,
            &native,
            &fingerprint,
            &players,
            &parsed.rounds,
            &parsed.kills,
            visibility.as_ref(),
        )?;
        drop(shared_file);
        let provenance = serde_json::json!({
            "shared":{"contract":header.contract,"source":header.source,"availability":header.data,
                "coverage":native.coverage,"bytes":shared_bytes,"contentFingerprint":format!("sha1:{}", shared_hash.digest()),
                "storage":shared_storage,"retained":false},
            "visibilityError":visibility_error,
            "visibilityAssets":visibility.as_ref().map(|p|serde_json::json!({"bytes":p.bytes,"fingerprint":p.fingerprint})),
            "diagnosticBytes":0,
            "statistics":{"mode":"per-rule-occurrences","deduplication":"same-rule-round-target-overlap","crossRuleCounts":"independent"},
            "native":{
                "producer":"native-animgraph2-3", "coverage":body_coverage,
                "resourceContentId":native.assets.resource_content_id,
                "sharedAssetBytes":native.assets.total_bytes,"clientSha256":native.client_sha256,
                "assetPrecision":"VRF DATA/MDAT text; tested maximum raw clip component error 8.35e-7; network state lossless",
                "qualification":"partial task reconstruction; historical asset compatibility and independent rule calibration unqualified",
                "unknownObstruction":true
            }
        });
        progress(3);
        let mut records = Vec::new();
        let created_at = crate::store::now();
        for player_id in &players {
            let mut checks = checks
                .remove(player_id)
                .ok_or_else(|| anyhow!("missing player checks"))?;
            let state = scoring::statistics::summarize(&mut checks);
            records.push(Assessment {
                schema_version: 2,
                id: String::new(),
                created_at: created_at.clone(),
                demo_id: id.into(),
                demo_fingerprint: fingerprint.clone(),
                player_id: player_id.clone(),
                tick_rate: parsed.info.tick_rate,
                ruleset_version: scoring::RULESET_VERSION.into(),
                checks,
                input_provenance: provenance.clone(),
                state,
            });
        }
        scoring::history::save_match(&self.data_dir, &mut records)?;
        for record in records {
            histories.insert(record.player_id.clone(), vec![record]);
        }
        Ok(scoring::history::MatchResponse {
            source_fingerprint: fingerprint,
            players: histories,
            preparation_seconds,
            analysis_seconds: started.elapsed().as_secs_f64(),
            shared_bytes,
            generic_bytes: shared_bytes,
            diagnostic_bytes: 0,
        })
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
        self.store
            .save_settings(&clean)
            .map_err(|e| vec![format!("{e:#}")])?;
        self.clean_leftovers();
        Ok(())
    }
    /// Sizes of the disposable folders: (parsed, clips, radar).
    pub fn storage_bytes(&self) -> (u64, u64, u64, u64) {
        (
            self.store.parsed_bytes(),
            self.store.clips_bytes(),
            self.store.radar_bytes(),
            self.store.anomaly_bytes(),
        )
    }
    pub fn clear_anomaly_data(&self) -> Result<u64> {
        let _guard = self
            .scoring_lock
            .try_lock()
            .map_err(|_| anyhow!("analysis is running"))?;
        let queue = self.analysis_queue.lock().unwrap();
        anyhow::ensure!(
            !queue.jobs.iter().any(|job| matches!(
                job.status,
                crate::scoring::queue::Status::Queued | crate::scoring::queue::Status::Running
            )),
            "analysis is queued or running"
        );
        self.store.clear_anomaly_data()
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
                    Ok(false) => {}
                    Err(error) => {
                        return Err(anyhow!("Cannot check video {}: {error}", output.file))
                    }
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
        PathOverrides {
            steam_dir: p(&s.steam_dir),
            cs2_dir: p(&s.cs2_dir),
            hlae_exe: p(&s.hlae_exe),
            ffmpeg_exe: p(&s.ffmpeg_exe),
            vrf_exe: p(&s.vrf_exe),
        }
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
        Detected {
            replays_dir: cs2_dir.as_deref().map(replays_dir),
            steam_dir,
            cs2_dir,
        }
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
                problems.push(format!(
                    "game\\bin\\win64\\cs2.exe not found under {}",
                    cs2.display()
                ));
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
        path.extension()
            .map(|e| e.to_string_lossy().eq_ignore_ascii_case("dem"))
            .unwrap_or(false)
    }

    /// Scan configured folders plus individually registered files in their original locations.
    fn demo_paths(&self) -> Vec<PathBuf> {
        let mut out: Vec<PathBuf> = self
            .registered_demos
            .lock()
            .unwrap()
            .iter()
            .filter(|path| Self::is_dem(path) && path.is_file())
            .cloned()
            .collect();
        for folder in self.replay_folders() {
            let Ok(rd) = std::fs::read_dir(&folder) else {
                continue;
            };
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
                let mtime_ms = st
                    .modified()
                    .ok()
                    .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_millis() as f64)
                    .unwrap_or(0.0);
                let created_ms = st
                    .created()
                    .ok()
                    .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_millis() as f64)
                    .unwrap_or(mtime_ms);
                let match_time_ms = crate::store::match_time_ms(&path);
                Some((path, st.len(), mtime_ms, created_ms, match_time_ms))
            })
            .collect();
        let keep: HashSet<String> = scanned
            .iter()
            .map(|(p, _, _, _, _)| Self::demo_id(p))
            .collect();
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
                            name: path
                                .file_name()
                                .map(|f| f.to_string_lossy().to_string())
                                .unwrap_or_default(),
                            path: path.to_string_lossy().to_string(),
                            bytes,
                            mtime_ms,
                            created_ms,
                            match_time_ms,
                            status: if failure.is_some() {
                                DemoStatus::Error
                            } else if stored.is_some() {
                                DemoStatus::Parsed
                            } else {
                                DemoStatus::New
                            },
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
                if entry.meta.status == DemoStatus::Parsed && !entry.meta.same_file(bytes, mtime_ms)
                {
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
                let _ = self.store.delete_parsed(id);
            }
            let mut list: Vec<DemoMeta> = demos.values().map(|e| e.meta.clone()).collect();
            list.sort_by(|a, b| {
                b.date_ms()
                    .total_cmp(&a.date_ms())
                    .then_with(|| a.id.cmp(&b.id))
            });
            list
        };
        self.store.prune_parsed(&keep);
        self.auto_parse_next();
        let demos = self.demos.lock().unwrap();
        list.into_iter()
            .map(|meta| demos.get(&meta.id).map(|e| e.meta.clone()).unwrap_or(meta))
            .collect()
    }

    /// Meta + parse result; a result stored on disk is loaded on first use.
    pub fn get_demo(&self, id: &str) -> Option<(DemoMeta, Option<Arc<ParsedDemo>>)> {
        let (meta, parsed) = self
            .demos
            .lock()
            .unwrap()
            .get(id)
            .map(|e| (e.meta.clone(), e.parsed.clone()))?;
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
                let _ = self.store.delete_parsed(id);
                self.update_meta(id, |e| {
                    if e.meta.status != DemoStatus::Error {
                        e.reset();
                    }
                })
                .map(|m| (m, None))
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
        self.list_demos()
            .into_iter()
            .find(|m| m.id == id)
            .ok_or_else(|| anyhow!("demo no longer available after registering"))
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
        let mut candidates: Vec<_> = self
            .demos
            .lock()
            .unwrap()
            .values()
            .filter(|e| e.meta.status == DemoStatus::New && e.auto_complete != Some(false))
            .map(|e| (e.meta.id.clone(), e.meta.mtime_ms))
            .collect();
        candidates.sort_by(|a, b| b.1.total_cmp(&a.1));
        for (id, _) in candidates {
            if let Err(error) = self.start_parse(&id, true) {
                eprintln!("could not auto-parse {id}: {error:#}");
            }
            if !self.parsing.lock().unwrap().is_empty() {
                break;
            }
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
                if entry.meta.status != DemoStatus::New || entry.auto_complete == Some(false) {
                    return Ok(());
                }
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
            let Ok(mut file) = options.open(&meta.path) else {
                return Ok(());
            };
            let metadata = file.metadata()?;
            let modified = metadata
                .modified()?
                .duration_since(std::time::UNIX_EPOCH)?
                .as_millis() as f64;
            if !meta.same_file(metadata.len(), modified) {
                return Ok(());
            }
            let complete = match complete {
                Some(ready) => ready,
                None => crate::demo_readiness::is_complete(&mut file)?,
            };
            if let Some(entry) = self.demos.lock().unwrap().get_mut(id) {
                if entry.meta.same_file(metadata.len(), modified) {
                    entry.auto_complete = Some(complete);
                }
            }
            if !complete {
                return Ok(());
            }
            Some(file)
        } else {
            None
        };
        // Finish any in-flight replay write before invalidating its cache.
        let _replay_guard = self.replay_lock.lock().unwrap();
        let meta = {
            // Same lock order as scanning; claim the demo and the automatic slot together.
            let mut demos = self.demos.lock().unwrap();
            let entry = demos.get_mut(id).ok_or_else(|| anyhow!("demo not found"))?;
            let mut parsing = self.parsing.lock().unwrap();
            if parsing.contains(id)
                || (automatic && (!parsing.is_empty() || entry.meta.status != DemoStatus::New))
            {
                return Ok(());
            }
            parsing.insert(id.to_string());
            entry.reset();
            entry.meta.status = DemoStatus::Parsing;
            entry.meta.error = None;
            entry.meta.clone()
        };
        let _ = self.store.delete_parsed(id);
        // Also covers app termination during parsing: retry then requires an explicit click.
        if let Err(error) = self
            .store
            .write_parse_error(id, "Parsing was interrupted; parse manually to retry.")
        {
            self.finish_parse_error(id, format!("Cannot save parse state: {error:#}"));
            return Err(error);
        }
        self.notify
            .notify(Event::DemoChanged { demo: meta.clone() });
        let engine = self.clone();
        let id = id.to_string();
        let thread_id = id.clone();
        let spawned = std::thread::Builder::new()
            .name(format!("parse-{id}"))
            .spawn(move || {
                let _source_guard = source_guard;
                let id = thread_id;
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    engine
                        .parser
                        .load_demo(Path::new(&meta.path))
                        .map(build_parsed_demo)
                }));
                let outcome = match result {
                    Ok(Ok(parsed)) => engine
                        .store
                        .write_parsed(&id, &meta, &parsed)
                        .and_then(|_| engine.store.clear_parse_error(&id))
                        .map(|_| parsed)
                        .map_err(|e| format!("Cannot save parse result: {e:#}")),
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
                            e.parsed = if automatic {
                                None
                            } else {
                                Some(Arc::new(parsed))
                            };
                        });
                        engine.parsing.lock().unwrap().remove(&id);
                        if let Some(demo) = fresh {
                            engine.notify.notify(Event::DemoChanged { demo });
                        }
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
        if let Some(demo) = fresh {
            self.notify.notify(Event::DemoChanged { demo });
        }
    }

    /// Path of the replay stream for a parsed demo, building it on first use
    /// (a few seconds: the demo is read again for positions and projectiles).
    pub fn replay_file(&self, id: &str) -> Result<PathBuf> {
        let _guard = self.replay_lock.lock().unwrap();
        let (meta, parsed) = self.get_demo(id).ok_or_else(|| anyhow!("demo not found"))?;
        let parsed = parsed
            .filter(|_| meta.status == DemoStatus::Parsed)
            .ok_or_else(|| anyhow!("demo not parsed"))?;
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
        let vrf = tools.vrf_exe.ok_or_else(|| {
            anyhow!("Source 2 Viewer CLI not installed — use \"Download tools\" in settings")
        })?;
        ensure_map_assets(&radar_dir, &cs2, &vrf, map, tools.cs2_patch_version)
    }

    pub fn active_job_id(&self) -> Option<String> {
        self.active_job
            .lock()
            .unwrap()
            .as_ref()
            .map(|(id, _)| id.clone())
    }

    pub fn clear_match_anomaly(&self, id: &str) -> Result<()> {
        let _guard = self.replay_lock.lock().unwrap();
        let _scoring = self
            .scoring_lock
            .try_lock()
            .map_err(|_| anyhow!("analysis is running"))?;
        self.ensure_idle(id)?;
        let mut queue = self.analysis_queue.lock().unwrap();
        anyhow::ensure!(
            !queue.jobs.iter().any(|j| j.demo_id == id
                && matches!(
                    j.status,
                    crate::scoring::queue::Status::Queued | crate::scoring::queue::Status::Running
                )),
            "analysis is queued or running"
        );
        crate::scoring::history::delete_match(&self.data_dir, id)?;
        queue.jobs.retain(|j| j.demo_id != id);
        Ok(())
    }

    fn delete_demo_data(&self, id: &str) -> Result<()> {
        let mut queue = self.analysis_queue.lock().unwrap();
        anyhow::ensure!(
            !queue.jobs.iter().any(|j| j.demo_id == id
                && matches!(
                    j.status,
                    crate::scoring::queue::Status::Queued | crate::scoring::queue::Status::Running
                )),
            "analysis is queued or running"
        );
        let jobs: HashSet<_> = self
            .store
            .list_jobs()
            .into_iter()
            .filter(|job| job.demo_id == id)
            .map(|job| job.id)
            .collect();
        for job in &jobs {
            self.store.delete_job(job)?;
        }
        self.render_queue
            .lock()
            .unwrap()
            .retain(|job| !jobs.contains(job));
        crate::scoring::history::delete_match(&self.data_dir, id)?;
        self.store.delete_parsed(id)?;
        queue.jobs.retain(|j| j.demo_id != id);
        Ok(())
    }

    /// Clear all demo-owned derived data; preserve the source file.
    pub fn clear_analysis(&self, id: &str) -> Result<()> {
        let _replay_guard = self.replay_lock.lock().unwrap();
        let _scoring = self
            .scoring_lock
            .try_lock()
            .map_err(|_| anyhow!("analysis is running"))?;
        self.ensure_idle(id)?;
        self.delete_demo_data(id)?;
        let meta = self
            .update_meta(id, DemoEntry::reset)
            .ok_or_else(|| anyhow!("demo not found"))?;
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
        let _scoring = self
            .scoring_lock
            .try_lock()
            .map_err(|_| anyhow!("analysis is running"))?;
        anyhow::ensure!(
            !self
                .analysis_queue
                .lock()
                .unwrap()
                .jobs
                .iter()
                .any(|j| matches!(
                    j.status,
                    crate::scoring::queue::Status::Queued | crate::scoring::queue::Status::Running
                )),
            "analysis is queued or running"
        );
        let before =
            self.store.parsed_bytes() + self.store.anomaly_bytes() + self.store.clips_bytes();
        let ids: Vec<_> = self.demos.lock().unwrap().keys().cloned().collect();
        for id in ids {
            self.delete_demo_data(&id)?;
        }
        self.store.clear_all_parsed();
        self.store.clear_anomaly_data()?;
        for job in self.store.list_jobs() {
            self.store.delete_job(&job.id)?;
        }
        self.render_queue.lock().unwrap().clear();
        self.analysis_queue.lock().unwrap().jobs.clear();
        let freed = before.saturating_sub(
            self.store.parsed_bytes() + self.store.anomaly_bytes() + self.store.clips_bytes(),
        );
        let metas: Vec<DemoMeta> = {
            let mut demos = self.demos.lock().unwrap();
            for e in demos.values_mut() {
                if e.meta.status != DemoStatus::Error {
                    e.reset();
                }
            }
            demos.values().map(|e| e.meta.clone()).collect()
        };
        for meta in metas {
            self.notify.notify(Event::DemoChanged { demo: meta });
        }
        Ok(freed)
    }

    /// Delete the demo, its analysis and all of its render jobs; shared radar maps remain.
    pub fn remove_demo(&self, id: &str) -> Result<()> {
        let _guard = self.replay_lock.lock().unwrap();
        let _scoring = self
            .scoring_lock
            .try_lock()
            .map_err(|_| anyhow!("analysis is running"))?;
        self.ensure_idle(id)?;
        let (meta, _) = self.get_demo(id).ok_or_else(|| anyhow!("demo not found"))?;
        self.delete_demo_data(id)?;
        self.store.clear_parse_error(id)?;
        match std::fs::remove_file(&meta.path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(anyhow!("cannot delete {}: {e}", meta.path)),
        }
        {
            let mut registered = self.registered_demos.lock().unwrap();
            let updated: Vec<_> = registered
                .iter()
                .filter(|p| Self::demo_id(p) != id)
                .cloned()
                .collect();
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
            if self
                .store
                .get_job(&active)
                .map(|j| j.demo_id == demo_id)
                .unwrap_or(false)
            {
                return Err(anyhow!("a render for this demo is running"));
            }
        }
        Ok(())
    }

    // ---- setup ----
    pub fn setup_state(&self) -> HashMap<SetupTool, SetupState> {
        self.setup.lock().unwrap().clone()
    }
    pub fn tool_checks(&self) -> HashMap<SetupTool, crate::render::diagnostics::ToolCheck> {
        let paths = self.tool_paths();
        self.tool_checks
            .lock()
            .unwrap()
            .iter()
            .filter(|(tool, check)| {
                check.fingerprint == crate::render::diagnostics::fingerprint(&paths, **tool)
            })
            .map(|(tool, check)| (*tool, check.clone()))
            .collect()
    }
    pub fn check_tools(&self) {
        let _guard = self.tool_check_lock.lock().unwrap();
        for tool in [SetupTool::Hlae, SetupTool::Ffmpeg, SetupTool::Vrf] {
            if self
                .setup
                .lock()
                .unwrap()
                .get(&tool)
                .is_some_and(|s| s.running)
            {
                continue;
            }
            let check = crate::render::diagnostics::check(&self.tool_paths(), tool);
            self.tool_checks.lock().unwrap().insert(tool, check);
        }
    }
    pub fn tool_diagnostics(&self) -> serde_json::Value {
        serde_json::json!({ "environment": crate::render::diagnostics::environment(),
            "dataDirectory": self.data_dir(), "paths": self.tool_paths(),
            "configuredPaths": self.overrides(&self.settings()),
            "checks": self.tool_checks(), "downloads": self.setup_state() })
    }
    pub fn start_setup(self: &Arc<Self>, tool: SetupTool, force: bool) -> bool {
        let cancel = Arc::new(AtomicBool::new(false));
        {
            let mut downloads = self.setup.lock().unwrap();
            let state = downloads.entry(tool).or_default();
            if state.running {
                return false;
            }
            *state = SetupState {
                running: true,
                stopping: false,
                progress: None,
                log: vec!["Download started".into()],
                cancel: cancel.clone(),
            };
        }
        let engine = self.clone();
        std::thread::spawn(move || {
            let settings = engine.store.settings();
            let overrides = engine.overrides(&settings);
            let mut log = |line: String| {
                engine
                    .setup
                    .lock()
                    .unwrap()
                    .get_mut(&tool)
                    .unwrap()
                    .progress = Some(line.clone());
                engine.notify.notify(Event::SetupProgress {
                    tool,
                    progress: line,
                });
            };
            let mut installed = false;
            let result = run_setup(
                &engine.tools_dir(),
                &overrides,
                tool,
                force,
                &mut crate::render::setup::Progress {
                    cancel: &cancel,
                    report: &mut log,
                },
            )
            .and_then(|_| {
                let _guard = engine.settings_lock.lock().unwrap();
                let mut settings = engine.store.settings();
                match tool {
                    SetupTool::Hlae => settings.hlae_exe = None,
                    SetupTool::Ffmpeg => settings.ffmpeg_exe = None,
                    SetupTool::Vrf => settings.vrf_exe = None,
                }
                engine.store.save_settings(&settings)?;
                installed = true;
                drop(_guard);
                let check = crate::render::diagnostics::check(&engine.tool_paths(), tool);
                let error = check.error.clone();
                engine.tool_checks.lock().unwrap().insert(tool, check);
                if let Some(error) = error {
                    anyhow::bail!("Tool installed but startup verification failed: {error}");
                }
                Ok(())
            });
            let cancelled = result.is_err() && cancel.load(Ordering::Relaxed);
            {
                let mut downloads = engine.setup.lock().unwrap();
                let state = downloads.get_mut(&tool).unwrap();
                state.running = false;
                state.stopping = false;
                state.progress = None;
                state.log.push(match &result {
                    Ok(_) => "Download completed".into(),
                    Err(_) if cancelled => "Download cancelled".into(),
                    Err(e) => format!("error: {e:#}"),
                });
            }
            match result {
                Ok(_) => engine.notify.notify(Event::SetupFinished {
                    tool,
                    ok: true,
                    installed,
                    cancelled: false,
                    error: None,
                }),
                Err(e) => engine.notify.notify(Event::SetupFinished {
                    tool,
                    ok: false,
                    installed,
                    cancelled,
                    error: Some(format!("{e:#}")),
                }),
            }
        });
        true
    }

    pub fn cancel_setup(&self, tool: SetupTool) {
        if let Some(state) = self.setup.lock().unwrap().get_mut(&tool) {
            if state.running {
                state.stopping = true;
                state.cancel.store(true, Ordering::Relaxed);
            }
        }
    }

    // ---- render queue ----
    pub fn enqueue_render(
        self: &Arc<Self>,
        demo_id: &str,
        highlight_ids: Vec<String>,
        options: RenderOptions,
    ) -> Result<RenderJob> {
        let _guard = self.replay_lock.lock().unwrap();
        let (meta, parsed) = self
            .get_demo(demo_id)
            .ok_or_else(|| anyhow!("demo not found"))?;
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

    pub fn analysis_clips(
        &self,
        demo_id: &str,
        selection: &crate::scoring::clips::Selection,
    ) -> Result<Vec<crate::scoring::clips::RuleClips>> {
        let (_, parsed) = self
            .get_demo(demo_id)
            .ok_or_else(|| anyhow!("demo not found"))?;
        let parsed = parsed.ok_or_else(|| anyhow!("demo not parsed"))?;
        let record = self
            .scoring_history(demo_id, &selection.player_id)?
            .into_iter()
            .find(|r| r.id == selection.assessment_id)
            .ok_or_else(|| anyhow!("analysis record not found"))?;
        let end = parsed
            .rounds
            .iter()
            .map(|r| r.end_tick.max(r.officially_ended_tick))
            .max()
            .ok_or_else(|| anyhow!("demo timeline unavailable"))?;
        crate::scoring::clips::build(&record, &parsed.info, end, &selection.rule_ids)
    }

    pub fn enqueue_analysis_render(
        self: &Arc<Self>,
        demo_id: &str,
        selection: crate::scoring::clips::Selection,
        mut options: RenderOptions,
    ) -> Result<Vec<RenderJob>> {
        let _guard = self.replay_lock.lock().unwrap();
        let mut groups = self.analysis_clips(demo_id, &selection)?;
        if options.merge && groups.len() > 1 {
            let mut combined = groups.remove(0);
            for group in groups.drain(..) {
                combined.rule_id.push('+');
                combined.rule_id.push_str(&group.rule_id);
                combined.title.push_str(" / ");
                combined.title.push_str(&group.title);
                combined.highlights.extend(group.highlights);
            }
            groups.push(combined);
        }
        let (meta, _) = self
            .get_demo(demo_id)
            .ok_or_else(|| anyhow!("demo not found"))?;
        check_ascii_path(Path::new(&meta.path))?;
        options.merge = true;
        let mut jobs: Vec<RenderJob> = Vec::new();
        let result = (|| -> Result<()> {
            for group in groups {
                let mut job = self.store.new_job(
                    demo_id,
                    group.highlights.iter().map(|h| h.id.clone()).collect(),
                    options.clone(),
                )?;
                job.analysis_clips = Some(Box::new(group));
                jobs.push(job);
                self.store.save_job(jobs.last().unwrap())?;
            }
            Ok(())
        })();
        if let Err(error) = result {
            for mut job in jobs {
                job.status = JobStatus::Error;
                job.error = Some(format!("Export batch was not queued: {error:#}"));
                job.finished_at = Some(now());
                self.persist(&job);
            }
            return Err(error);
        }
        self.render_queue
            .lock()
            .unwrap()
            .extend(jobs.iter().map(|j| j.id.clone()));
        for job in &jobs {
            self.notify.notify(Event::JobChanged { job: job.clone() });
        }
        self.pump();
        Ok(jobs)
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
        let _guard = self.replay_lock.lock().unwrap();
        if self.active_job_id().as_deref() == Some(id) {
            return Err(anyhow!("job is running"));
        }
        self.cancel_job(id);
        self.store.delete_job(id)
    }

    /// Delete every render job and its videos. Refused while a render is running or queued.
    pub fn clear_all_clips(&self) -> Result<u64> {
        let _guard = self.replay_lock.lock().unwrap();
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
        let spawned = std::thread::Builder::new()
            .name("render-worker".into())
            .spawn(move || {
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
        // Serialize claiming a queued job with demo removal, including already-dequeued jobs.
        let guard = self.replay_lock.lock().unwrap();
        if !self
            .store
            .get_job(&job.id)
            .is_some_and(|j| j.status == JobStatus::Queued)
        {
            return;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        *self.active_job.lock().unwrap() = Some((job.id.clone(), cancel.clone()));
        drop(guard);
        job.status = JobStatus::Running;
        job.started_at = Some(now());
        job.stage = Some("starting".into());
        job.progress = Some(0.0);
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
        let mut report_progress = |value: f64| {
            let mut j = progress.lock().unwrap();
            let value = value.clamp(0.0, 0.99).max(j.progress.unwrap_or(0.0));
            if value - j.progress.unwrap_or(0.0) < 0.005 {
                return;
            }
            j.progress = Some(value);
            self.persist(&j);
        };
        let outcome = self
            .get_demo(&job.demo_id)
            .ok_or_else(|| anyhow!("demo not found"))
            .and_then(|(meta, parsed)| {
                let parsed = parsed.ok_or_else(|| {
                    anyhow!("demo not parsed (parse it again after restarting the app)")
                })?;
                let wanted: HashSet<&str> = job.highlight_ids.iter().map(|s| s.as_str()).collect();
                let highlights = if let Some(clips) = &job.analysis_clips {
                    use std::io::Read;
                    let mut source = std::fs::File::open(&meta.path)?;
                    let mut hash = sha1_smol::Sha1::new();
                    let mut buffer = [0u8; 65536];
                    loop {
                        let n = source.read(&mut buffer)?;
                        if n == 0 {
                            break;
                        }
                        hash.update(&buffer[..n]);
                    }
                    anyhow::ensure!(
                        format!("sha1:{}", hash.digest()) == clips.demo_fingerprint,
                        "demo content changed since analysis; analyze again before exporting"
                    );
                    clips.highlights.clone()
                } else {
                    parsed
                        .highlights
                        .iter()
                        .filter(|h| wanted.contains(h.id.as_str()))
                        .cloned()
                        .collect()
                };
                render_highlights(RenderJobInput {
                    demo: &parsed.info,
                    demo_path: PathBuf::from(&meta.path),
                    highlights,
                    preserve_merge_order: job.analysis_clips.is_some(),
                    output_dir: self.store.job_dir(&job.id),
                    options: job.options.clone(),
                    tools: self.tool_paths(),
                    cancel: cancel.clone(),
                    log: &mut log,
                    stage: &mut stage,
                    progress: &mut report_progress,
                })
            });
        let latest = progress.into_inner().unwrap();
        job.log = latest.log;
        job.progress = latest.progress;

        let _guard = self.replay_lock.lock().unwrap();
        job.finished_at = Some(now());
        job.stage = None;
        match outcome {
            Ok(result) => {
                job.outputs = job_outputs(&result);
                if let Some(clips) = &job.analysis_clips {
                    for output in &mut job.outputs {
                        if output.is_final {
                            output.title = clips.title.clone();
                        }
                    }
                }
                if !job.outputs.is_empty() {
                    job.progress = Some(1.0);
                }
                job.status = if job.outputs.is_empty() {
                    JobStatus::Error
                } else {
                    JobStatus::Done
                };
                if job.outputs.is_empty() {
                    job.error = Some("no clip was recorded".into());
                }
            }
            Err(e) => {
                let msg = format!("{e:#}");
                job.status = if msg == "cancelled" {
                    JobStatus::Cancelled
                } else {
                    JobStatus::Error
                };
                job.error = Some(msg);
            }
        }
        self.persist(&job);
        *self.active_job.lock().unwrap() = None;
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
        .filter_map(|c| {
            c.file.as_ref().map(|f| JobOutput {
                file: f.to_string_lossy().to_string(),
                bytes: c.bytes.unwrap_or(0),
                highlight_id: Some(c.highlight_id.clone()),
                title: c.title.clone(),
                is_final: false,
            })
        })
        .collect();
    if let (Some(f), Some(b)) = (&result.final_video, result.final_bytes) {
        outputs.push(JobOutput {
            file: f.to_string_lossy().to_string(),
            bytes: b,
            highlight_id: None,
            title: "highlights (merged)".into(),
            is_final: true,
        });
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
        fn notify(&self, event: Event) {
            let _ = self.0.send(event);
        }
    }

    #[test]
    fn setup_switches_only_the_installed_tool_to_managed_path() {
        let temp = tempfile::tempdir().unwrap();
        let (send, receive) = mpsc::channel();
        let engine = Engine::new(temp.path().join("data"), Arc::new(Events(send))).unwrap();
        for file in [
            "hlae/HLAE.exe",
            "hlae/x64/AfxHookSource2.dll",
            "ffmpeg/ffmpeg.exe",
            "ffmpeg/ffprobe.exe",
        ] {
            let path = engine.tools_dir().join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, []).unwrap();
        }
        let vrf = engine
            .tools_dir()
            .join("vrf")
            .join(crate::render::setup::vrf_exe_name());
        std::fs::create_dir_all(vrf.parent().unwrap()).unwrap();
        std::fs::write(&vrf, []).unwrap();
        // Keep both workers active at the settings write boundary without network access.
        let settings_guard = engine.settings_lock.lock().unwrap();
        assert!(engine.start_setup(SetupTool::Hlae, false));
        assert!(engine.start_setup(SetupTool::Ffmpeg, false));
        assert!(!engine.start_setup(SetupTool::Hlae, false));
        let active = engine.setup_state();
        assert!(active[&SetupTool::Hlae].running && active[&SetupTool::Ffmpeg].running);
        drop(settings_guard);
        for _ in 0..2 {
            assert!(matches!(
                receive.recv_timeout(Duration::from_secs(5)).unwrap(),
                Event::SetupFinished { ok: false, .. }
            ));
        }
        for tool in [SetupTool::Hlae, SetupTool::Ffmpeg, SetupTool::Vrf] {
            let mut expected = Settings {
                language: Some("ja".into()),
                hlae_exe: Some("missing-hlae.exe".into()),
                ffmpeg_exe: Some("missing-ffmpeg.exe".into()),
                vrf_exe: Some("missing-vrf.exe".into()),
                ..Settings::default()
            };
            engine.store.save_settings(&expected).unwrap();
            expected = engine.settings();
            assert!(engine.start_setup(tool, false));
            loop {
                if let Event::SetupFinished { ok, error, .. } =
                    receive.recv_timeout(Duration::from_secs(5)).unwrap()
                {
                    assert!(!ok && error.unwrap().contains("startup verification failed"));
                    break;
                }
            }
            let paths = engine.tool_paths();
            let setup = engine.setup_state().remove(&tool).unwrap();
            assert!(!setup.running);
            assert!(setup.progress.is_none());
            assert_eq!(setup.log[0], "Download started");
            assert!(setup.log[1].contains("startup verification failed"));
            assert!(!engine.tool_checks()[&tool].ok);
            match tool {
                SetupTool::Hlae => {
                    expected.hlae_exe = None;
                    assert!(paths.hlae_exe.unwrap().starts_with(engine.tools_dir()));
                }
                SetupTool::Ffmpeg => {
                    expected.ffmpeg_exe = None;
                    assert!(paths.ffmpeg_exe.unwrap().starts_with(engine.tools_dir()));
                }
                SetupTool::Vrf => {
                    expected.vrf_exe = None;
                    assert_eq!(paths.vrf_exe, Some(vrf.clone()));
                }
            }
            assert_eq!(
                serde_json::to_value(engine.settings()).unwrap(),
                serde_json::to_value(expected).unwrap()
            );
        }
    }

    fn terminal_events(receiver: &mpsc::Receiver<Event>, count: usize) -> Vec<DemoMeta> {
        let mut finished = vec![];
        let mut running = HashSet::new();
        while finished.len() < count {
            if let Event::DemoChanged { demo } =
                receiver.recv_timeout(Duration::from_secs(10)).unwrap()
            {
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
        let mut job = engine
            .store
            .new_job("demo", vec![], RenderOptions::default())
            .unwrap();
        job.status = JobStatus::Done;
        for name in ["clip.mp4", "merged.mp4"] {
            let file = engine.store.job_dir(&job.id).join(name);
            std::fs::write(&file, b"video").unwrap();
            job.outputs.push(JobOutput {
                file: file.to_string_lossy().into_owned(),
                bytes: 5,
                highlight_id: None,
                title: name.into(),
                is_final: name == "merged.mp4",
            });
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
        let mut job = engine
            .store
            .new_job("demo", vec![], RenderOptions::default())
            .unwrap();
        let file = engine.store.job_dir(&job.id).join("invalid\0.mp4");
        job.outputs.push(JobOutput {
            file: file.to_string_lossy().into_owned(),
            bytes: 5,
            highlight_id: None,
            title: "invalid".into(),
            is_final: false,
        });
        engine.store.save_job(&job).unwrap();
        let record = engine.store.job_dir(&job.id).join("job.json");
        let original = std::fs::read(&record).unwrap();
        assert!(engine
            .list_jobs()
            .unwrap_err()
            .to_string()
            .contains("Cannot check video"));
        assert_eq!(std::fs::read(record).unwrap(), original);
    }

    #[test]
    fn completing_job_keeps_deletion_locked_until_result_is_persisted() {
        struct ObserveCompletion(Mutex<Option<std::sync::Weak<Engine>>>);
        impl Notify for ObserveCompletion {
            fn notify(&self, event: Event) {
                if let Event::JobChanged { job } = event {
                    if job.status == JobStatus::Error {
                        let engine = self.0.lock().unwrap().as_ref().unwrap().upgrade().unwrap();
                        assert_eq!(engine.active_job_id(), Some(job.id.clone()));
                        assert!(engine.replay_lock.try_lock().is_err());
                        assert_eq!(
                            engine.store.get_job(&job.id).unwrap().status,
                            JobStatus::Error
                        );
                    }
                }
            }
        }
        let temp = tempfile::tempdir().unwrap();
        let notify = Arc::new(ObserveCompletion(Mutex::new(None)));
        let engine = Engine::new(temp.path().join("data"), notify.clone()).unwrap();
        *notify.0.lock().unwrap() = Some(Arc::downgrade(&engine));
        let job = engine
            .store
            .new_job("missing-demo", vec![], RenderOptions::default())
            .unwrap();
        engine.run_job(job.clone());
        assert!(engine.active_job_id().is_none());
        engine.delete_job(&job.id).unwrap();
        assert!(engine.store.get_job(&job.id).is_none());
    }

    #[test]
    fn cleanup_clears_anomalies_and_owned_data_with_correct_scope() {
        for mode in 0..4 {
            let temp = tempfile::tempdir().unwrap();
            let data = temp.path().join("data");
            let store = Store::open(data.clone()).unwrap();
            store
                .save_settings(&Settings {
                    scan_game_replays: false,
                    ..Settings::default()
                })
                .unwrap();
            let source = temp.path().join("source.dem");
            std::fs::write(&source, b"demo").unwrap();
            let id = Engine::demo_id(&source);
            store.write_parse_error(&id, "manual retry").unwrap();
            let (send, _receive) = mpsc::channel();
            let engine = Engine::new(data.clone(), Arc::new(Events(send))).unwrap();
            engine.add_demo(&source).unwrap();
            std::fs::write(data.join("parsed").join(format!("{id}.json")), b"parsed").unwrap();
            std::fs::write(store.replay_path(&id), b"replay").unwrap();
            let job = store
                .new_job(&id, vec![], RenderOptions::default())
                .unwrap();
            let other = store
                .new_job("other", vec![], RenderOptions::default())
                .unwrap();
            crate::scoring::history::track_source(&data, &id, "sha1:owned").unwrap();
            crate::scoring::history::track_source(&data, "other", "sha1:other").unwrap();
            let states = data.join("analysis/match-state");
            std::fs::create_dir_all(&states).unwrap();
            let owned = states.join(format!(
                "{}-v7-test.gz",
                sha1_smol::Sha1::from("sha1:owned").digest()
            ));
            let untouched = states.join(format!(
                "{}-v7-test.gz",
                sha1_smol::Sha1::from("sha1:other").digest()
            ));
            std::fs::write(&owned, b"state").unwrap();
            std::fs::write(&untouched, b"state").unwrap();
            let clean = || match mode {
                0 => engine.clear_analysis(&id),
                1 => engine.remove_demo(&id),
                2 => engine.clear_match_anomaly(&id),
                _ => engine.clear_all_analysis().map(|_| ()),
            };
            engine.analysis_queue.lock().unwrap().enqueue(&id, false);
            assert!(clean().is_err());
            assert!(source.exists() && owned.exists() && store.job_dir(&job.id).exists());
            engine.analysis_queue.lock().unwrap().jobs.clear();
            {
                let _guard = engine.scoring_lock.lock().unwrap();
                assert!(clean().is_err());
            }
            clean().unwrap();
            assert_eq!(source.exists(), mode != 1);
            assert!(!owned.exists());
            assert_eq!(untouched.exists(), mode != 3);
            assert_eq!(store.job_dir(&job.id).exists(), mode == 2);
            assert_eq!(store.job_dir(&other.id).exists(), mode != 3);
            assert_eq!(
                data.join("parsed").join(format!("{id}.json")).exists(),
                mode == 2
            );
            assert_eq!(store.replay_path(&id).exists(), mode == 2);
            let match_dir = data
                .join("behavior-analysis/matches")
                .join(sha1_smol::Sha1::from(id.as_str()).digest().to_string());
            assert!(!match_dir.exists());
        }
    }

    #[test]
    fn removing_demo_cleans_owned_data_and_queued_jobs_but_preserves_shared_files() {
        let temp = tempfile::tempdir().unwrap();
        let data = temp.path().join("data");
        let store = Store::open(data.clone()).unwrap();
        store
            .save_settings(&Settings {
                scan_game_replays: false,
                ..Settings::default()
            })
            .unwrap();
        let source = temp.path().join("remove.dem");
        std::fs::write(&source, b"demo").unwrap();
        let id = Engine::demo_id(&source);
        store.write_parse_error(&id, "manual retry").unwrap();
        let (send, _receive) = mpsc::channel();
        let engine = Engine::new(data.clone(), Arc::new(Events(send))).unwrap();
        engine.add_demo(&source).unwrap();
        let queued = store
            .new_job(&id, vec!["queued".into()], RenderOptions::default())
            .unwrap();
        let mut done = store
            .new_job(&id, vec!["done".into()], RenderOptions::default())
            .unwrap();
        done.status = JobStatus::Done;
        store.save_job(&done).unwrap();
        let other = store
            .new_job("another-demo", vec![], RenderOptions::default())
            .unwrap();
        for job in [&queued, &done, &other] {
            std::fs::write(store.job_dir(&job.id).join("video.mp4"), b"video").unwrap();
        }
        for suffix in ["json", "summary.json", "replay.json"] {
            std::fs::write(
                data.join("parsed").join(format!("{id}.{suffix}")),
                b"analysis",
            )
            .unwrap();
        }
        let radar = store.radar_dir().join("shared.png");
        std::fs::create_dir_all(store.radar_dir()).unwrap();
        std::fs::write(&radar, b"radar").unwrap();
        engine
            .render_queue
            .lock()
            .unwrap()
            .extend([queued.id.clone(), other.id.clone()]);
        engine.parsing.lock().unwrap().insert(id.clone());
        assert!(engine.remove_demo(&id).is_err());
        engine.parsing.lock().unwrap().clear();
        *engine.active_job.lock().unwrap() =
            Some((queued.id.clone(), Arc::new(AtomicBool::new(false))));
        assert!(engine.remove_demo(&id).is_err());
        assert!(source.exists() && store.job_dir(&done.id).exists());
        *engine.active_job.lock().unwrap() = None;
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            let handles: Vec<_> = [&queued, &done]
                .iter()
                .map(|job| {
                    std::fs::OpenOptions::new()
                        .read(true)
                        .share_mode(1)
                        .open(store.job_dir(&job.id).join("job.json"))
                        .unwrap()
                })
                .collect();
            let failed = engine.remove_demo(&id);
            drop(handles);
            assert!(failed.is_err());
            assert!(source.exists());
            assert!(engine.render_queue.lock().unwrap().contains(&queued.id));
            assert_eq!(store.get_job(&queued.id).unwrap().status, JobStatus::Queued);
        }
        engine.remove_demo(&id).unwrap();
        assert!(!source.exists());
        assert!(engine.get_demo(&id).is_none());
        assert!(store.registered_demos().unwrap().is_empty());
        assert!(!store.job_dir(&done.id).exists());
        assert!(!store.job_dir(&queued.id).exists());
        assert!(store.job_dir(&other.id).join("video.mp4").exists());
        assert_eq!(
            engine
                .render_queue
                .lock()
                .unwrap()
                .iter()
                .collect::<Vec<_>>(),
            vec![&other.id]
        );
        assert!(!std::fs::read_dir(data.join("parsed")).unwrap().any(|e| e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(&id)));
        assert_eq!(std::fs::read(radar).unwrap(), b"radar");
        engine.run_job(queued.clone()); // A worker may have dequeued it before removal.
        assert!(!store.job_dir(&queued.id).exists());
        assert!(store.delete_job("..").is_err());
        assert!(source.parent().unwrap().exists());
    }

    #[test]
    fn registered_same_named_demos_stay_in_place_and_survive_restart() {
        let temp = tempfile::tempdir().unwrap();
        let data = temp.path().join("data");
        let first = temp.path().join("first/match.dem");
        let second = temp.path().join("second/match.dem");
        let store = Store::open(data.clone()).unwrap();
        store
            .save_settings(&Settings {
                scan_game_replays: false,
                cs2_dir: Some(temp.path().join("no-game").to_string_lossy().into_owned()),
                ..Settings::default()
            })
            .unwrap();
        for (path, bytes) in [(&first, "first source"), (&second, "second source")] {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, bytes).unwrap();
            // Keep this registration test independent of the parser worker.
            store
                .write_parse_error(&Engine::demo_id(path), bytes)
                .unwrap();
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
        assert_eq!(
            store.registered_demos().unwrap(),
            vec![first.clone(), second.clone()]
        );
        assert!(engine.settings().replay_folders.is_empty());
        assert_eq!(std::fs::read_to_string(&first).unwrap(), "first source");
        assert_eq!(std::fs::read_to_string(&second).unwrap(), "second source");
        assert_eq!(
            std::fs::read_dir(first.parent().unwrap()).unwrap().count(),
            2
        );
        assert_eq!(
            std::fs::read_dir(second.parent().unwrap()).unwrap().count(),
            1
        );
        assert!(!data.join("match.dem").exists());
        drop(engine);
        let (send, _) = mpsc::channel();
        let restarted = Engine::new(data, Arc::new(Events(send))).unwrap();
        let listed = restarted.list_demos();
        assert_eq!(listed.len(), 2);
        assert!(listed
            .iter()
            .any(|demo| demo.id == a.id && demo.error.as_deref() == Some("first source")));
        assert!(listed
            .iter()
            .any(|demo| demo.id == b.id && demo.error.as_deref() == Some("second source")));
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
        store
            .save_settings(&Settings {
                scan_game_replays: false,
                ..Settings::default()
            })
            .unwrap();
        let (send, receive) = mpsc::channel();
        let engine = Engine::new(data, Arc::new(Events(send))).unwrap();
        let demo = engine.add_demo(&source).unwrap();
        assert_eq!(demo.status, DemoStatus::New);
        assert!(store.parse_error(&demo.id).is_none());
        engine.list_demos();
        assert!(receive.try_recv().is_err());
        assert_eq!(
            engine.demos.lock().unwrap()[&demo.id].auto_complete,
            Some(false)
        );
        std::fs::write(&source, &bytes).unwrap();
        #[cfg(windows)]
        {
            let writer = std::fs::OpenOptions::new()
                .append(true)
                .open(&source)
                .unwrap();
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
        Store::open(data.clone())
            .unwrap()
            .save_settings(&Settings {
                scan_game_replays: false,
                replay_folders: vec![demos.to_string_lossy().into_owned()],
                cs2_dir: Some(temp.path().join("no-game").to_string_lossy().into_owned()),
                ..Settings::default()
            })
            .unwrap();
        let (send, receive) = mpsc::channel();
        let engine = Engine::new(data.clone(), Arc::new(Events(send))).unwrap();
        assert_eq!(engine.list_demos().len(), 2);
        let completed = terminal_events(&receive, 2);
        assert!(completed.iter().all(|d| d.status == DemoStatus::Error));
        // Scans, changed file contents and clearing cached analysis do not retry errors.
        std::fs::write(&completed[0].path, b"still not a demo, but changed").unwrap();
        engine.clear_all_analysis().unwrap();
        assert!(engine
            .list_demos()
            .iter()
            .all(|d| d.status == DemoStatus::Error));
        assert!(receive.try_iter().all(
            |e| !matches!(e, Event::DemoChanged { demo } if demo.status == DemoStatus::Parsing)
        ));
        let settings = engine.settings();
        let mut disconnected = settings.clone();
        disconnected.replay_folders.clear();
        engine.store.save_settings(&disconnected).unwrap();
        assert!(engine.list_demos().is_empty());
        engine.store.save_settings(&settings).unwrap();
        assert!(engine
            .list_demos()
            .iter()
            .all(|d| d.status == DemoStatus::Error));
        drop(engine);
        let (send, receive) = mpsc::channel();
        let restarted = Engine::new(data, Arc::new(Events(send))).unwrap();
        assert!(restarted
            .list_demos()
            .iter()
            .all(|d| d.status == DemoStatus::Error));
        assert!(receive.try_recv().is_err());
        // A failed reparse must not leave old statistics or a reusable 2D cache.
        let id = &completed[0].id;
        let cache_dir = restarted
            .store
            .replay_path(id)
            .parent()
            .unwrap()
            .to_path_buf();
        let caches: Vec<_> = ["json", "summary.json", "replay.json"]
            .iter()
            .map(|suffix| cache_dir.join(format!("{id}.{suffix}")))
            .collect();
        for path in &caches {
            std::fs::write(path, b"old cached data").unwrap();
        }
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

#[cfg(test)]
mod analysis_queue_tests {
    use super::*;
    #[derive(Default)]
    struct Events(Mutex<Vec<(String, crate::scoring::queue::Status)>>);
    impl Notify for Events {
        fn notify(&self, event: Event) {
            if let Event::AnalysisJobChanged { job } = event {
                self.0.lock().unwrap().push((job.demo_id, job.status));
            }
        }
    }
    #[test]
    fn interrupted_render_persists_language_independent_error_code() {
        let root = tempfile::tempdir().unwrap();
        let engine = Engine::new(root.path().into(), Arc::new(Events::default())).unwrap();
        let job = engine
            .store
            .new_job("demo", vec!["clip".into()], RenderOptions::default())
            .unwrap();
        let reopened = Engine::new(root.path().into(), Arc::new(Events::default())).unwrap();
        let job = reopened.store.get_job(&job.id).unwrap();
        assert_eq!(job.status, JobStatus::Error);
        assert_eq!(job.error_code, Some(crate::ErrorCode::AppClosed));
        assert_eq!(
            serde_json::to_value(&job).unwrap()["errorCode"],
            "app-closed"
        );
        assert!(job.error.is_some());
    }

    #[test]
    fn anomaly_storage_counts_both_folders_and_clears_only_when_idle() {
        let root = tempfile::tempdir().unwrap();
        let engine = Engine::new(root.path().into(), Arc::new(Events::default())).unwrap();
        for name in ["analysis", "behavior-analysis", "parsed", "clips"] {
            std::fs::create_dir_all(root.path().join(name)).unwrap();
            std::fs::write(root.path().join(name).join("keep-or-clear"), b"123").unwrap();
        }
        assert_eq!(engine.storage_bytes().3, 6);
        {
            let _guard = engine.scoring_lock.lock().unwrap();
            assert!(engine.clear_anomaly_data().is_err());
        }
        engine.analysis_queue.lock().unwrap().enqueue("demo", true);
        assert!(engine.clear_anomaly_data().is_err());
        assert_eq!(engine.storage_bytes().3, 6);
        engine.analysis_queue.lock().unwrap().jobs.clear();
        assert_eq!(engine.clear_anomaly_data().unwrap(), 6);
        assert_eq!(engine.storage_bytes().3, 0);
        assert_eq!(engine.clear_anomaly_data().unwrap(), 0);
        for name in ["parsed", "clips"] {
            assert!(root.path().join(name).join("keep-or-clear").exists());
        }
    }

    #[test]
    fn anomaly_exports_share_highlight_fifo_and_force_one_video_per_rule() {
        use serde_json::json;
        let root = tempfile::tempdir().unwrap();
        let engine = Engine::new(root.path().into(), Arc::new(Events::default())).unwrap();
        // Hold the worker without launching CS2; inspect the shared queue and saved jobs.
        engine.render_worker_running.store(true, Ordering::SeqCst);
        let (record, info) = crate::scoring::clips::tests::fixture();
        let mut records = vec![record];
        crate::scoring::history::save_match(root.path(), &mut records).unwrap();
        let parsed:ParsedDemo=serde_json::from_value(json!({"info":info,"rounds":[{"round":1,"startTick":0,"freezeEndTick":0,"endTick":6400,"officiallyEndedTick":6400,"reason":"","roster":{}}],"kills":[],"highlights":[],"stats":[],"score":{},"roundSummaries":[],"parsedAt":""})).unwrap();
        let meta=serde_json::from_value(json!({"id":"demo","name":"test.dem","path":"test.dem","bytes":0,"mtimeMs":0,"createdMs":0,"status":"parsed"})).unwrap();
        engine.demos.lock().unwrap().insert(
            "demo".into(),
            DemoEntry {
                meta,
                parsed: Some(Arc::new(parsed)),
                auto_complete: None,
            },
        );
        let first = engine
            .enqueue_render("demo", vec!["highlight".into()], RenderOptions::default())
            .unwrap();
        let selection = crate::scoring::clips::Selection {
            player_id: "1".into(),
            assessment_id: records[0].id.clone(),
            rule_ids: vec!["jump".into(), "view".into()],
        };
        let jobs = engine
            .enqueue_analysis_render(
                "demo",
                selection.clone(),
                RenderOptions {
                    merge: false,
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(jobs.len(), 2);
        assert_eq!(
            *engine.render_queue.lock().unwrap(),
            VecDeque::from([first.id, jobs[0].id.clone(), jobs[1].id.clone()])
        );
        assert!(jobs
            .iter()
            .all(|j| j.options.merge && j.status == JobStatus::Queued));
        assert_eq!(
            engine
                .store
                .get_job(&jobs[0].id)
                .unwrap()
                .analysis_clips
                .unwrap()
                .highlights
                .len(),
            6
        );
        assert!(engine.cancel_job(&jobs[0].id));
        assert_eq!(
            engine.store.get_job(&jobs[0].id).unwrap().status,
            JobStatus::Cancelled
        );
        assert_eq!(engine.render_queue.lock().unwrap().len(), 2);
        let merged = engine
            .enqueue_analysis_render(
                "demo",
                selection,
                RenderOptions {
                    merge: true,
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(merged.len(), 1);
        let snapshot = engine
            .store
            .get_job(&merged[0].id)
            .unwrap()
            .analysis_clips
            .unwrap();
        let expected: Vec<_> = jobs
            .iter()
            .flat_map(|j| {
                j.analysis_clips
                    .as_ref()
                    .unwrap()
                    .highlights
                    .iter()
                    .map(|h| (&h.id, h.start_tick, h.end_tick))
            })
            .collect();
        assert_eq!(
            snapshot
                .highlights
                .iter()
                .map(|h| (&h.id, h.start_tick, h.end_tick))
                .collect::<Vec<_>>(),
            expected
        );
        assert_eq!(
            engine.render_queue.lock().unwrap().back(),
            Some(&merged[0].id)
        );
        assert!(merged[0].options.merge);
    }

    #[test]
    fn failed_job_does_not_block_next_and_queue_is_session_only() {
        use crate::scoring::queue::Status;
        let root = tempfile::tempdir().unwrap();
        let events = Arc::new(Events::default());
        let engine = Engine::new(root.path().into(), events.clone()).unwrap();
        {
            let mut q = engine.analysis_queue.lock().unwrap();
            q.enqueue("missing-a", true);
            q.enqueue("missing-b", true);
            q.worker_running = true;
        }
        engine.clone().run_analysis_queue();
        let jobs = engine.analysis_jobs();
        assert_eq!(jobs.len(), 2);
        assert!(jobs.iter().all(|j| j.status == Status::Error
            && j.error.as_deref().unwrap().contains("demo not found")
            && j.finished_at.is_some()));
        let events = events.0.lock().unwrap();
        let first_failed = events
            .iter()
            .position(|(id, status)| id == "missing-a" && *status == Status::Error)
            .unwrap();
        let next_started = events
            .iter()
            .position(|(id, status)| id == "missing-b" && *status == Status::Running)
            .unwrap();
        assert!(first_failed < next_started);
        assert!(!engine.analysis_queue.lock().unwrap().worker_running);
        let queue_file = root.path().join("analysis-jobs.json");
        assert!(!queue_file.exists());
        let (record, _) = crate::scoring::clips::tests::fixture();
        crate::scoring::history::save_match(root.path(), &mut [record]).unwrap();
        std::fs::write(&queue_file, b"old queue is no longer read").unwrap();
        let reopened = Engine::new(root.path().into(), Arc::new(Events::default())).unwrap();
        assert!(reopened.analysis_jobs().is_empty());
        assert!(!queue_file.exists());
        assert_eq!(reopened.scoring_history("demo", "1").unwrap().len(), 1);
    }
}
