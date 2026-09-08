//! Flat-file persistence under the app data folder. No database; every file can
//! be deleted and is rebuilt on demand. What is on disk:
//!   settings.json                 config (paths, folders)
//!   parsed/<id>.summary.json      tiny parse summary for the sidebar (+ validity stamp)
//!   parsed/<id>.json              full parse result, loaded when a demo is opened
//!   parsed/<id>.replay.json       position stream for the 2D replay, built on first use
//!   clips/<jobId>/job.json        render job record (options, status, log, outputs)
//!   clips/<jobId>/*.mp4           output videos
//!   radar/<map>/                  radar images extracted from the game (see radar.rs)

use crate::render::RenderOptions;
use crate::replay::{ReplayData, REPLAY_SCHEMA_VERSION};
use crate::stats::ParsedDemo;
use std::collections::HashSet;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// UI language (en / zh-TW / zh-CN / ja); None = follow the system language
    #[serde(default)]
    pub language: Option<String>,
    /// CS2 install folder ("…\steamapps\common\Counter-Strike Global Offensive")
    pub cs2_dir: Option<String>,
    /// Extra folders to scan for .dem files
    pub replay_folders: Vec<String>,
    /// Scan <cs2Dir>/game/csgo/replays automatically
    pub scan_game_replays: bool,
    pub hlae_exe: Option<String>,
    pub ffmpeg_exe: Option<String>,
    pub tools_dir: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self { language: None, cs2_dir: None, replay_folders: vec![], scan_game_replays: true, hlae_exe: None, ffmpeg_exe: None, tools_dir: None }
    }
}

impl Settings {
    /// Trimmed paths, blanks turned into "not set". Applied once when saving so
    /// every reader can use the fields as they are.
    pub fn normalized(mut self) -> Self {
        let trim = |v: &mut Option<String>| {
            if let Some(s) = v {
                let t = s.trim().to_string();
                *v = if t.is_empty() { None } else { Some(t) };
            }
        };
        trim(&mut self.language);
        trim(&mut self.cs2_dir);
        trim(&mut self.hlae_exe);
        trim(&mut self.ffmpeg_exe);
        trim(&mut self.tools_dir);
        self.replay_folders = self.replay_folders.iter().map(|f| f.trim().to_string()).filter(|f| !f.is_empty()).collect();
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DemoStatus {
    New,
    Parsing,
    Parsed,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DemoSummary {
    pub rounds: usize,
    pub kills: usize,
    pub highlights: usize,
    pub score_a: u32,
    pub score_b: u32,
    pub players: Vec<String>,
}

impl DemoSummary {
    pub fn of(parsed: &ParsedDemo) -> Self {
        Self {
            rounds: parsed.rounds.len(),
            kills: parsed.kills.len(),
            highlights: parsed.highlights.len(),
            score_a: *parsed.score.get("A").unwrap_or(&0),
            score_b: *parsed.score.get("B").unwrap_or(&0),
            players: parsed.info.players.iter().map(|p| p.name.clone()).collect(),
        }
    }
}

/// Bump when the parse output or the highlight rules change: every stored
/// result then silently counts as "not parsed" and is re-computed on demand.
pub const PARSED_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParsedSummaryFile {
    pub schema_version: u32,
    pub demo_bytes: u64,
    pub demo_mtime_ms: f64,
    pub map_name: String,
    pub parsed_at: String,
    pub summary: DemoSummary,
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

/// One .dem file as the UI sees it. Built from the file system on each scan;
/// status / summary come from the in-memory parse state.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DemoMeta {
    pub id: String,
    pub name: String,
    pub path: String,
    pub bytes: u64,
    pub mtime_ms: f64,
    pub status: DemoStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub map_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parsed_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<DemoSummary>,
}

impl DemoMeta {
    /// Same size and (within a millisecond) same mtime: the file has not changed.
    pub fn same_file(&self, bytes: u64, mtime_ms: f64) -> bool {
        self.bytes == bytes && (self.mtime_ms - mtime_ms).abs() < 1.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    Queued,
    Running,
    Done,
    Error,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobOutput {
    pub file: String,
    pub bytes: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub highlight_id: Option<String>,
    pub title: String,
    #[serde(default)]
    pub is_final: bool,
}

/// Layout version written into every job.json. Jobs are records, never
/// rebuilt: a future layout change reads the old version and converts in code.
pub const JOB_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderJob {
    pub schema_version: u32,
    pub id: String,
    pub demo_id: String,
    pub highlight_ids: Vec<String>,
    pub options: RenderOptions,
    pub status: JobStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage: Option<String>,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default)]
    pub outputs: Vec<JobOutput>,
    #[serde(default)]
    pub log: Vec<String>,
}

pub struct Store {
    root: PathBuf,
}

/// RFC 3339 UTC with milliseconds — the one timestamp format in every file.
pub fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

impl Store {
    pub fn open(root: PathBuf) -> Result<Self> {
        // Layout before 2026-09-08 was renders/<job>/video; keep those jobs reachable.
        let old = root.join("renders");
        let new = root.join("clips");
        if old.is_dir() && !new.exists() && fs::rename(&old, &new).is_ok() {
            if let Ok(rd) = fs::read_dir(&new) {
                for e in rd.flatten() {
                    let jf = e.path().join("job.json");
                    if let Ok(t) = fs::read_to_string(&jf) {
                        let _ = fs::write(&jf, t.replace("\\\\renders\\\\", "\\\\clips\\\\").replace("/renders/", "/clips/"));
                    }
                }
            }
        }
        for d in ["parsed", "clips"] {
            fs::create_dir_all(root.join(d))?;
        }
        Ok(Self { root })
    }

    // ---- settings ----
    pub fn settings(&self) -> Settings {
        fs::read_to_string(self.root.join("settings.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    }
    pub fn save_settings(&self, s: &Settings) -> Result<()> {
        write_atomic(&self.root.join("settings.json"), serde_json::to_string_pretty(s)?.as_bytes())
    }

    // ---- parse results (disposable cache) ----
    fn summary_path(&self, id: &str) -> PathBuf {
        self.root.join("parsed").join(format!("{id}.summary.json"))
    }
    fn parsed_path(&self, id: &str) -> PathBuf {
        self.root.join("parsed").join(format!("{id}.json"))
    }
    /// Replay stream next to the parse result; it shares the parse result's
    /// lifetime (deleted with it, so a schema change = bump PARSED_SCHEMA_VERSION).
    pub fn replay_path(&self, id: &str) -> PathBuf {
        self.root.join("parsed").join(format!("{id}.replay.json"))
    }
    /// True when the stored stream was written by the current replay layout
    /// (`schemaVersion` is the first field, so only the head of the file is read).
    pub fn replay_is_current(&self, id: &str) -> bool {
        use std::io::Read;
        let mut head = [0u8; 64];
        let n = fs::File::open(self.replay_path(id)).and_then(|mut f| f.read(&mut head)).unwrap_or(0);
        String::from_utf8_lossy(&head[..n]).contains(&format!("\"schemaVersion\":{},", REPLAY_SCHEMA_VERSION))
    }
    pub fn write_replay(&self, id: &str, replay: &ReplayData) -> Result<PathBuf> {
        let path = self.replay_path(id);
        write_atomic(&path, &serde_json::to_vec(replay)?)?;
        Ok(path)
    }
    /// Write both files atomically (tmp + rename) so a crash never leaves a half file.
    pub fn write_parsed(&self, id: &str, meta: &DemoMeta, parsed: &ParsedDemo) -> Result<()> {
        let summary = ParsedSummaryFile {
            schema_version: PARSED_SCHEMA_VERSION,
            demo_bytes: meta.bytes,
            demo_mtime_ms: meta.mtime_ms,
            map_name: parsed.info.map_name.clone(),
            parsed_at: parsed.parsed_at.clone(),
            summary: DemoSummary::of(parsed),
        };
        write_atomic(&self.parsed_path(id), &serde_json::to_vec(parsed)?)?;
        write_atomic(&self.summary_path(id), &serde_json::to_vec_pretty(&summary)?)?;
        Ok(())
    }
    /// The stored summary, only if it still describes this exact demo file and
    /// was produced by the current schema; stale files are removed.
    pub fn read_summary(&self, id: &str, demo_bytes: u64, demo_mtime_ms: f64) -> Option<ParsedSummaryFile> {
        let text = fs::read_to_string(self.summary_path(id)).ok()?;
        let s: ParsedSummaryFile = serde_json::from_str(&text).ok()?;
        let fresh = s.schema_version == PARSED_SCHEMA_VERSION && s.demo_bytes == demo_bytes && (s.demo_mtime_ms - demo_mtime_ms).abs() < 1.0 && self.parsed_path(id).is_file();
        if !fresh {
            self.delete_parsed(id);
            return None;
        }
        Some(s)
    }
    pub fn read_parsed(&self, id: &str) -> Option<ParsedDemo> {
        fs::read_to_string(self.parsed_path(id)).ok().and_then(|t| serde_json::from_str(&t).ok())
    }
    pub fn delete_parsed(&self, id: &str) {
        let _ = fs::remove_file(self.summary_path(id));
        let _ = fs::remove_file(self.parsed_path(id));
        let _ = fs::remove_file(self.replay_path(id));
    }
    /// Delete every stored parse result. Returns the number of bytes freed.
    pub fn clear_all_parsed(&self) -> u64 {
        clear_dir(&self.root.join("parsed"))
    }
    /// Total size of the stored parse results.
    pub fn parsed_bytes(&self) -> u64 {
        dir_size(&self.root.join("parsed"))
    }
    /// Drop results whose demo is no longer in any scanned folder.
    pub fn prune_parsed(&self, live: &HashSet<String>) {
        let Ok(rd) = fs::read_dir(self.root.join("parsed")) else { return };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            let id = name.split('.').next().unwrap_or("").to_string();
            if !id.is_empty() && !live.contains(&id) {
                let _ = fs::remove_file(e.path());
            }
        }
    }

    // ---- jobs ----
    pub fn clips_dir(&self) -> PathBuf {
        self.root.join("clips")
    }
    pub fn job_dir(&self, id: &str) -> PathBuf {
        self.clips_dir().join(id)
    }
    /// Total size of everything under clips/ (videos + job.json).
    pub fn clips_bytes(&self) -> u64 {
        dir_size(&self.clips_dir())
    }
    /// Delete every job folder (videos + records). Returns bytes freed.
    pub fn clear_all_clips(&self) -> u64 {
        clear_dir(&self.clips_dir())
    }

    // ---- radar images (see radar.rs) ----
    pub fn radar_dir(&self) -> PathBuf {
        self.root.join("radar")
    }
    pub fn radar_bytes(&self) -> u64 {
        dir_size(&self.radar_dir())
    }
    /// Delete every extracted map folder. Returns bytes freed.
    pub fn clear_radar(&self) -> u64 {
        clear_dir(&self.radar_dir())
    }
    pub fn new_job(&self, demo_id: &str, highlight_ids: Vec<String>, options: RenderOptions) -> Result<RenderJob> {
        let id = format!("{}-{}", chrono::Utc::now().format("%Y%m%dT%H%M%S"), &sha1_smol::Sha1::from(format!("{demo_id}{:?}{}", highlight_ids, now()).as_bytes()).digest().to_string()[..4]);
        let job = RenderJob { schema_version: JOB_SCHEMA_VERSION, id, demo_id: demo_id.to_string(), highlight_ids, options, status: JobStatus::Queued, stage: None, created_at: now(), started_at: None, finished_at: None, error: None, outputs: vec![], log: vec![] };
        self.save_job(&job)?;
        Ok(job)
    }
    pub fn save_job(&self, job: &RenderJob) -> Result<()> {
        let dir = self.job_dir(&job.id);
        fs::create_dir_all(&dir)?;
        write_atomic(&dir.join("job.json"), serde_json::to_string_pretty(job)?.as_bytes())
    }
    pub fn get_job(&self, id: &str) -> Option<RenderJob> {
        fs::read_to_string(self.job_dir(id).join("job.json")).ok().and_then(|t| serde_json::from_str(&t).ok())
    }
    pub fn list_jobs(&self) -> Vec<RenderJob> {
        let mut jobs: Vec<RenderJob> = fs::read_dir(self.root.join("clips"))
            .map(|rd| rd.flatten().filter_map(|e| self.get_job(&e.file_name().to_string_lossy())).collect())
            .unwrap_or_default();
        jobs.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        jobs
    }
    pub fn delete_job(&self, id: &str) -> Result<()> {
        let dir = self.job_dir(id);
        if dir.exists() {
            fs::remove_dir_all(dir)?;
        }
        Ok(())
    }
}

/// Size of a file or of everything under a directory.
pub(crate) fn dir_size(path: &Path) -> u64 {
    match fs::metadata(path) {
        Ok(m) if m.is_file() => m.len(),
        Ok(m) if m.is_dir() => fs::read_dir(path).map(|rd| rd.flatten().map(|e| dir_size(&e.path())).sum()).unwrap_or(0),
        _ => 0,
    }
}

/// Delete everything inside `dir` (the folder itself stays). Returns bytes freed.
fn clear_dir(dir: &Path) -> u64 {
    let mut freed = 0;
    if let Ok(rd) = fs::read_dir(dir) {
        for e in rd.flatten() {
            freed += dir_size(&e.path());
            let _ = if e.path().is_dir() { fs::remove_dir_all(e.path()) } else { fs::remove_file(e.path()) };
        }
    }
    freed
}
