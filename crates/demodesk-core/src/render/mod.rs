//! HLAE render backend: plays the demo in CS2 through HLAE and records each
//! [`Highlight`] with mirv_streams → FFmpeg, then muxes each clip (or joins
//! them into one video) and optionally shrinks the result to a size budget.

pub mod actions;
#[cfg(windows)]
mod audio;
pub(crate) mod combat;
pub mod diagnostics;
pub mod encode;
mod leftovers;
pub mod paths;
pub(crate) mod process;

/// Check a data root through the same process launcher used by rendering tools.
pub fn verify_data_directory(
    executable: &std::path::Path,
    root: &std::path::Path,
) -> anyhow::Result<()> {
    let tree = process::ProcessTree::new()?;
    let mut command = std::process::Command::new(executable);
    command.arg("--demodesk-verify-data-directory").arg(root);
    let mut child = tree.spawn(&mut command)?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        if let Some(status) = child.try_wait()? {
            tree.finish()?;
            anyhow::ensure!(
                status.success(),
                "External process cannot use data directory {} ({status})",
                root.display()
            );
            return Ok(());
        }
        if std::time::Instant::now() >= deadline {
            tree.finish()?;
            anyhow::bail!("Data directory verification timed out: {}", root.display());
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
}
mod record;
pub mod setup;
mod startup;
mod window;

use crate::model::{DemoInfo, Highlight};
use crate::{error::ExportError, ErrorCode};
use actions::{build_schedule, steamid_to_account_id, ActionsOptions, Camera, RenderClip};
use anyhow::{anyhow, Result};
use encode::{bytes_to_mb, concat_clips, encode_to_size_with_progress, mux_clip, Control};
use leftovers::{has_leftovers, remove_leftovers};
use paths::{resolve_tool_paths, to_forward_slashes, PathOverrides, ToolPaths, IS_WINDOWS};
use record::{collect_clip_outputs, run_recording_session, RecordSession};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// Run a helper process without flashing a console window (Windows only).
#[cfg(windows)]
pub(crate) fn hide(cmd: &mut std::process::Command) -> &mut std::process::Command {
    use std::os::windows::process::CommandExt;
    cmd.creation_flags(0x0800_0000) // CREATE_NO_WINDOW
}
#[cfg(not(windows))]
pub(crate) fn hide(cmd: &mut std::process::Command) -> &mut std::process::Command {
    cmd
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DoctorReport {
    pub ok: bool,
    pub problems: Vec<String>,
    pub paths: ToolPaths,
}

/// Environment check. Pure: it never touches the game folder, so it is safe to
/// call while a recording is running (the UI polls it).
pub fn doctor(tools_dir: &Path, o: &PathOverrides) -> DoctorReport {
    let paths = resolve_tool_paths(tools_dir, o);
    let mut problems = vec![];
    if !IS_WINDOWS {
        problems.push("rendering only runs on Windows (HLAE is Windows-only)".to_string());
    }
    if paths.steam_dir.is_none() {
        problems.push("Steam not found (HKCU\\Software\\Valve\\Steam\\SteamPath) — set the CS2 folder manually".into());
    }
    if paths.cs2_exe.is_none() {
        problems.push("cs2.exe not found — set the CS2 install folder in settings".into());
    }
    if paths.hlae_exe.is_none() {
        problems.push("HLAE not installed — use \"Download tools\"".into());
    } else if paths.hlae_dll.is_none() {
        problems.push("x64/AfxHookSource2.dll missing next to HLAE.exe".into());
    }
    if paths.ffmpeg_exe.is_none() {
        problems.push("ffmpeg.exe not found — use \"Download tools\" or set the path".into());
    }
    DoctorReport {
        ok: problems.is_empty(),
        problems,
        paths,
    }
}

/// Remove what an old version left in the game folder (see leftovers.rs).
/// Must NOT run while a recording is in progress.
pub fn clean_leftovers(tools_dir: &Path, o: &PathOverrides) -> bool {
    let paths = resolve_tool_paths(tools_dir, o);
    match &paths.cs2_dir {
        Some(cs2) if has_leftovers(cs2) => remove_leftovers(cs2).is_ok(),
        _ => false,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SetupTool {
    Hlae,
    Ffmpeg,
    Vrf,
}

pub fn run_setup(
    tools_dir: &Path,
    o: &PathOverrides,
    tool: SetupTool,
    force: bool,
    log: setup::Log,
) -> Result<DoctorReport> {
    std::fs::create_dir_all(tools_dir)?;
    match tool {
        SetupTool::Hlae => {
            setup::install_hlae(
                &o.hlae_exe
                    .as_deref()
                    .map(paths::installation_directory)
                    .unwrap_or_else(|| tools_dir.to_path_buf())
                    .join("hlae"),
                force,
                log,
            )?;
        }
        SetupTool::Ffmpeg => {
            setup::install_ffmpeg(
                &o.ffmpeg_exe
                    .as_deref()
                    .map(paths::installation_directory)
                    .unwrap_or_else(|| tools_dir.to_path_buf())
                    .join("ffmpeg"),
                force,
                log,
            )?;
        }
        SetupTool::Vrf => {
            setup::install_vrf(
                &o.vrf_exe
                    .as_deref()
                    .map(paths::installation_directory)
                    .unwrap_or_else(|| tools_dir.to_path_buf())
                    .join("vrf"),
                force,
                log,
            )?;
        }
    }
    Ok(doctor(tools_dir, o))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RenderOptions {
    pub fps: u32,
    pub width: u32,
    pub height: u32,
    /// One of encode::CODECS.
    pub codec: String,
    pub crf: u32,
    pub container: String,
    pub camera: Camera,
    pub death_notice_seconds: u32,
    /// Base interface (health, ammo, score…)
    pub hud: bool,
    /// Crosshair (only possible while `hud` is on — CS2 hides it with the base interface)
    pub crosshair: bool,
    /// Individual elements, each independent of `hud`
    pub radar: bool,
    pub kill_feed: bool,
    pub player_kill_feed_only: bool,
    pub player_avatars: bool,
    pub chat: bool,
    /// First-person weapon model
    pub viewmodel: bool,
    /// Bullet tracers
    pub tracers: bool,
    /// HUD size multiplier (hud_scaling).
    pub hud_scale: f64,
    pub true_view: bool,
    pub xray: bool,
    pub voice: bool,
    pub show_game: bool,
    pub quit_when_done: bool,
    pub keep_raw_files: bool,
    /// Join all clips into one video (`highlights.<container>`) instead of one file per clip
    pub merge: bool,
    pub key_moments_only: bool,
    pub short_combat_clips: bool,
    pub round_result_label: String,
    /// Re-encode so each output file — the merged video, or every clip — is at most this many MB
    pub max_size_mb: Option<f64>,
    pub combat_quality_priority: bool,
    pub audio_kbps: u32,
    pub extra_launch_options: Vec<String>,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            fps: 60,
            width: 1920,
            height: 1080,
            codec: "libsvtav1".into(),
            crf: 20,
            container: "mp4".into(),
            camera: Camera::Slot,
            death_notice_seconds: 5,
            hud: true,
            crosshair: true,
            radar: true,
            kill_feed: true,
            player_kill_feed_only: false,
            player_avatars: true,
            chat: false,
            viewmodel: true,
            tracers: true,
            hud_scale: 0.85,
            true_view: false,
            xray: false,
            voice: false,
            show_game: false,
            quit_when_done: true,
            keep_raw_files: false,
            merge: false,
            key_moments_only: true,
            short_combat_clips: false,
            round_result_label: "Round result".into(),
            max_size_mb: Some(20.0),
            combat_quality_priority: true,
            audio_kbps: 192,
            extra_launch_options: vec![],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderedClip {
    #[serde(skip)]
    pub combat_windows: Vec<[f64; 2]>,
    pub highlight_id: String,
    pub title: String,
    pub file: Option<PathBuf>,
    pub bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderResult {
    /// the merged video (`RenderOptions::merge`); clips then carry no files
    pub final_video: Option<PathBuf>,
    pub final_bytes: Option<u64>,
    pub clips: Vec<RenderedClip>,
    #[serde(default)]
    pub failed_highlights: Vec<String>,
    #[serde(default)]
    pub failure: Option<ExportError>,
}

impl RenderResult {
    fn require_complete_merge(&self, merge: bool) -> Result<()> {
        if merge && !self.failed_highlights.is_empty() {
            if let Some(failure) = &self.failure {
                return Err(anyhow!(failure.message.clone()).context(failure.error_code));
            }
            return Err(anyhow!(
                "merge aborted: incomplete highlights: {}",
                self.failed_highlights.join(", ")
            )
            .context(ErrorCode::MergeIncomplete));
        }
        Ok(())
    }
}

pub fn to_render_clips(demo: &DemoInfo, highlights: &[Highlight]) -> Vec<RenderClip> {
    highlights
        .iter()
        .map(|h| RenderClip {
            highlight: h.clone(),
            slot: demo
                .players
                .iter()
                .find(|p| p.steamid == h.player.steamid)
                .and_then(|p| p.user_id)
                .map(|u| u + 1),
            round_result_slot: h.round_result.as_ref().and_then(|view| {
                demo.players
                    .iter()
                    .find(|p| p.steamid == view.player)
                    .and_then(|p| p.user_id)
                    .map(|id| id + 1)
            }),
            account_id: steamid_to_account_id(&h.player.steamid),
        })
        .collect()
}

/// Union adjacent recording windows; retain the first clip's identity for output mapping.
pub(crate) fn merge_nearby_clips(mut clips: Vec<Highlight>, tick_rate: f64) -> Vec<Highlight> {
    clips.sort_by_key(|h| (h.round, h.player.steamid.clone(), h.start_tick, h.end_tick));
    let mut merged: Vec<Highlight> = Vec::new();
    for clip in clips {
        if let Some(last) = merged.last_mut() {
            let gap = i64::from(clip.start_tick) - i64::from(last.end_tick);
            if last.round == clip.round
                && last.player.steamid == clip.player.steamid
                && (gap as f64) < tick_rate
            {
                if last.key_moments.is_empty() {
                    last.key_moments.push([last.start_tick, last.end_tick]);
                }
                if clip.key_moments.is_empty() {
                    last.key_moments.push([clip.start_tick, clip.end_tick]);
                } else {
                    last.key_moments.extend(clip.key_moments);
                }
                if clip.round_result.is_some() {
                    last.round_result = clip.round_result;
                }
                last.end_tick = last.end_tick.max(clip.end_tick);
                last.anchor_tick = last.anchor_tick.min(clip.anchor_tick);
                for tag in clip.tags {
                    if !last.tags.contains(&tag) {
                        last.tags.push(tag);
                    }
                }
                continue;
            }
        }
        merged.push(clip);
    }
    merged.sort_by_key(|h| (h.round, h.start_tick, h.end_tick));
    merged
}

/// Expand event windows before recording; repeated IDs are joined into one highlight afterwards.
fn recording_windows(
    highlights: &[Highlight],
    tick_rate: f64,
    key_only: bool,
    short: bool,
) -> Vec<Highlight> {
    let mut clips = Vec::new();
    for h in highlights {
        if !key_only || h.key_moments.is_empty() {
            clips.push(h.clone());
            continue;
        }
        let mut windows = h.key_moments.clone();
        if short {
            // Keep the three-second aftermath and preserve shorter objective/result shots.
            let max_ticks = (6.0 * tick_rate).round() as i32;
            for window in &mut windows {
                window[0] = window[0].max(window[1].saturating_sub(max_ticks));
            }
        }
        windows.sort_unstable();
        let mut merged: Vec<[i32; 2]> = Vec::new();
        for [start, end] in windows {
            if end <= start {
                continue;
            }
            if let Some(last) = merged.last_mut() {
                if f64::from(start) - f64::from(last[1]) < tick_rate {
                    last[1] = last[1].max(end);
                    continue;
                }
            }
            merged.push([start, end]);
        }
        for [start, end] in merged {
            let mut clip = h.clone();
            clip.start_tick = start;
            clip.end_tick = end;
            clip.anchor_tick = h.anchor_tick.clamp(start, end);
            clips.push(clip);
        }
    }
    clips.sort_by_key(|h| h.start_tick);
    clips
}

// Leave setup time between clips; overlapping views need separate forward-only sessions.
fn recording_passes(clips: &[RenderClip], tick_rate: f64) -> Vec<Vec<usize>> {
    let mut order: Vec<_> = (0..clips.len()).collect();
    order.sort_by_key(|&i| clips[i].highlight.start_tick);
    let mut passes: Vec<Vec<usize>> = Vec::new();
    for i in order {
        let start = i64::from(clips[i].highlight.start_tick);
        if let Some(pass) = passes.iter_mut().find(|pass| {
            let end = i64::from(clips[*pass.last().unwrap()].highlight.end_tick);
            let rate = tick_rate.round() as i64;
            start - end >= rate * i64::from(actions::AUDIO_PREROLL_SECONDS) + rate / 2 + 2
        }) {
            pass.push(i);
        } else {
            passes.push(vec![i]);
        }
    }
    passes
}

fn joined_combat_windows(
    ffmpeg: &Path,
    parts: &[&RenderedClip],
    control: &mut Control<'_>,
) -> Result<Vec<[f64; 2]>> {
    if parts.iter().all(|p| p.combat_windows.is_empty()) {
        return Ok(vec![]);
    }
    let mut result = Vec::new();
    let mut offset = 0.0;
    for part in parts {
        let Some(file) = &part.file else {
            return Ok(vec![]);
        };
        result.extend(
            part.combat_windows
                .iter()
                .map(|w| [w[0] + offset, w[1] + offset]),
        );
        offset += encode::probe_media(ffmpeg, file, control)?.0;
    }
    Ok(result)
}

fn join_highlight_parts(
    ffmpeg: &Path,
    highlights: &[Highlight],
    parts: Vec<RenderedClip>,
    output_dir: &Path,
    options: &RenderOptions,
    log: &mut dyn FnMut(String),
    cancel: &AtomicBool,
) -> Result<Vec<RenderedClip>> {
    let mut joined = Vec::new();
    for (index, h) in highlights.iter().enumerate() {
        Control::new(cancel, log).check_cancelled()?;
        let group: Vec<_> = parts.iter().filter(|c| c.highlight_id == h.id).collect();
        let files: Vec<_> = group.iter().filter_map(|c| c.file.clone()).collect();
        let combat_windows = if files.len() == group.len() {
            joined_combat_windows(ffmpeg, &group, &mut Control::new(cancel, log))?
        } else {
            vec![]
        };
        let file = if files.len() != group.len() || files.is_empty() {
            None
        } else if files.len() == 1 {
            Some(files[0].clone())
        } else {
            let dest = output_dir.join(format!("highlight-{:02}.{}", index + 1, options.container));
            if let Err(error) = concat_clips(ffmpeg, &files, &dest, &mut Control::new(cancel, log))
            {
                Control::new(cancel, log).check_cancelled()?;
                log(format!("{}: {error:#}", h.title));
                joined.push(RenderedClip {
                    combat_windows: vec![],
                    highlight_id: h.id.clone(),
                    title: h.title.clone(),
                    file: None,
                    bytes: None,
                });
                continue;
            }
            if !options.keep_raw_files {
                for file in files {
                    let _ = std::fs::remove_file(file);
                }
            }
            Some(dest)
        };
        joined.push(RenderedClip {
            combat_windows,
            highlight_id: h.id.clone(),
            title: h.title.clone(),
            file,
            bytes: None,
        });
    }
    Ok(joined)
}

pub struct RenderJobInput<'a> {
    pub combat_ticks: Option<&'a combat::CombatTicks>,
    pub demo: &'a DemoInfo,
    pub demo_path: PathBuf,
    pub highlights: Vec<Highlight>,
    pub preserve_merge_order: bool,
    pub output_dir: PathBuf,
    pub options: RenderOptions,
    pub tools: ToolPaths,
    pub cancel: Arc<AtomicBool>,
    pub log: &'a mut dyn FnMut(String),
    pub stage: &'a mut dyn FnMut(&str),
    pub progress: &'a mut dyn FnMut(f64),
}

fn safe_name(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

pub fn render_highlights(input: RenderJobInput) -> Result<RenderResult> {
    let RenderJobInput {
        combat_ticks,
        demo,
        demo_path,
        mut highlights,
        preserve_merge_order,
        output_dir,
        options: o,
        tools,
        cancel,
        log,
        stage,
        progress,
    } = input;
    let (Some(cs2_dir), Some(cs2_exe), Some(hlae_exe), Some(hlae_dll), Some(ffmpeg_exe)) = (
        tools.cs2_dir,
        tools.cs2_exe,
        tools.hlae_exe,
        tools.hlae_dll,
        tools.ffmpeg_exe,
    ) else {
        return Err(ErrorCode::ToolsMissing.into());
    };
    let ffmpeg = &ffmpeg_exe;
    if !demo_path.is_file() {
        return Err(
            anyhow!("demo not found: {}", demo_path.display()).context(ErrorCode::DemoMissing)
        );
    }
    if !demo_path.to_string_lossy().is_ascii() {
        return Err(ErrorCode::DemoPathUnsupported.into());
    }
    if highlights.is_empty() {
        return Err(ErrorCode::NoHighlights.into());
    }
    let merge_order: Vec<_> = if preserve_merge_order {
        highlights.iter().map(|h| h.id.clone()).collect()
    } else {
        vec![]
    };
    // Render in demo order so the game only seeks forward.
    highlights.sort_by_key(|h| h.start_tick);
    std::fs::create_dir_all(&output_dir)?;
    let recording = recording_windows(
        &highlights,
        demo.tick_rate,
        o.key_moments_only,
        o.short_combat_clips,
    );
    let recording = merge_nearby_clips(recording, demo.tick_rate);
    highlights.retain(|h| recording.iter().any(|clip| clip.id == h.id));
    let clips = to_render_clips(demo, &recording);
    if o.kill_feed
        && o.player_kill_feed_only
        && clips
            .iter()
            .any(|clip| actions::death_notice_player(clip).is_none())
    {
        return Err(ErrorCode::CameraUnavailable.into());
    }
    if clips.iter().any(|c| {
        c.highlight
            .round_result
            .as_ref()
            .is_some_and(|v| v.from_tick < c.highlight.end_tick)
            && c.round_result_slot.is_none()
    }) {
        return Err(ErrorCode::CameraUnavailable.into());
    }
    let missing_slots: Vec<&str> = clips
        .iter()
        .filter(|c| c.slot.is_none() && o.camera == Camera::Slot)
        .map(|c| c.highlight.player.name.as_str())
        .collect();
    if !missing_slots.is_empty() {
        log(format!(
            "warning: no player slot for {} — camera will not follow them",
            missing_slots.join(", ")
        ));
    }

    if cancel.load(std::sync::atomic::Ordering::Relaxed) {
        return Err(ErrorCode::ExportCancelled.into());
    }
    let (preset, mut compatible) = encode::checked_record_preset(ffmpeg, &o, log, &cancel)?;
    let total_seconds: f64 = recording
        .iter()
        .map(|h| (h.end_tick - h.start_tick) as f64 / demo.tick_rate)
        .sum();

    // ponytail: phase weights estimate work, not elapsed time; measure costs if adding time estimates.
    let recording_end = if o.max_size_mb.is_some() { 0.70 } else { 0.90 };
    let muxing_end = if o.max_size_mb.is_some() { 0.75 } else { 0.98 };
    let passes = recording_passes(&clips, demo.tick_rate);
    let mut outputs = Vec::new();
    let mut failure = None;
    let mut pass_dirs = Vec::new();
    let mut recorded_seconds = 0.0;
    for (pass_index, indices) in passes.iter().enumerate() {
        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(ErrorCode::ExportCancelled.into());
        }
        let pass_dir = output_dir.join(format!("pass-{}", pass_index + 1));
        let pass_clips: Vec<_> = indices.iter().map(|&i| clips[i].clone()).collect();
        let seconds: f64 = pass_clips
            .iter()
            .map(|c| (c.highlight.end_tick - c.highlight.start_tick) as f64 / demo.tick_rate)
            .sum();
        let mut recording_progress = |p: f64| {
            progress((recorded_seconds + p * seconds) / total_seconds.max(0.001) * recording_end)
        };
        stage("recording");
        log(format!(
            "recording pass {}/{}",
            pass_index + 1,
            passes.len()
        ));
        let mut pass_options = o.clone();
        if pass_index + 1 < passes.len() {
            pass_options.quit_when_done = true;
        }
        let mut session = RecordSession {
            demo_path: demo_path.clone(),
            cs2_dir: cs2_dir.clone(),
            cs2_exe: cs2_exe.clone(),
            hlae_exe: hlae_exe.clone(),
            hlae_dll: hlae_dll.clone(),
            ffmpeg_exe: ffmpeg_exe.clone(),
            output_dir: pass_dir.clone(),
            cfg_dir: tools
                .tools_dir
                .parent()
                .map(|p| p.join("cfg"))
                .unwrap_or_else(|| output_dir.join("cfg")),
            show_game: o.show_game,
            width: o.width,
            height: o.height,
            schedule: build_schedule(
                &pass_clips,
                &ActionsOptions {
                    render: &pass_options,
                    tick_rate: demo.tick_rate,
                    output_dir: to_forward_slashes(&pass_dir),
                    ffmpeg_preset: preset.clone(),
                },
            ),
            timeout_seconds: (180.0 + indices.len() as f64 * 30.0 + seconds * 6.0) as u64,
            extra_launch_options: o.extra_launch_options.clone(),
            cancel: cancel.clone(),
            completed_clips: Vec::new(),
            log: &mut *log,
            stage: &mut *stage,
            progress: &mut recording_progress,
        };
        let recorded =
            run_recording_session(&mut session).map_err(|e| ErrorCode::RecordingFailed.wrap(e));
        let completed = session.completed_clips.clone();
        drop(session);
        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(ErrorCode::ExportCancelled.into());
        }
        if let Err(error) = recorded {
            failure.get_or_insert_with(|| ExportError::new(&error, ErrorCode::RecordingFailed));
            log(format!(
                "recording pass {} failed: {error:#}",
                pass_index + 1
            ));
            // Preserve startup failures instead of reporting an empty recording result.
            if completed.is_empty() && outputs.is_empty() {
                return Err(error);
            }
        }
        for mut out in collect_clip_outputs(&pass_dir, indices.len(), &o.container) {
            if !completed.contains(&out.index) {
                out.video = None;
            }
            out.index = indices[out.index];
            if o.merge && out.video.is_none() {
                if let Some(failure) = &failure {
                    return Err(anyhow!(failure.message.clone()).context(failure.error_code));
                }
                return Err(anyhow!(
                    "merge aborted: incomplete highlight: {}",
                    recording[out.index].title
                )
                .context(ErrorCode::MergeIncomplete));
            }
            outputs.push(out);
        }
        pass_dirs.push(pass_dir);
        recorded_seconds += seconds;
    }
    outputs.sort_by_key(|out| out.index);
    stage("encoding");
    progress(recording_end);
    let mut fit_to_size = |file: PathBuf,
                           combat_windows: &[[f64; 2]],
                           log: &mut dyn FnMut(String),
                           report: &mut dyn FnMut(f64)|
     -> Result<PathBuf> {
        Control::new(&cancel, log).check_cancelled()?;
        let Some(mb) = o.max_size_mb else {
            report(1.0);
            return Ok(file);
        };
        if std::fs::metadata(&file)?.len() <= (mb * 1_000_000.0) as u64 {
            report(1.0);
            return Ok(file);
        }
        let small = file.with_extension(format!("{}mb.mp4", mb as u32));
        let r = encode_to_size_with_progress(
            ffmpeg,
            &file,
            &small,
            encode::SizeOptions {
                combat_windows: if o.combat_quality_priority {
                    combat_windows
                } else {
                    &[]
                },
                max_size_mb: mb,
                codec: &o.codec,
                audio_kbps: o.audio_kbps,
            },
            &mut compatible,
            report,
            &mut Control::new(&cancel, log),
        )?;
        log(format!(
            "{} → {} ({} kbps, {} MB)",
            file.file_name().unwrap().to_string_lossy(),
            small.file_name().unwrap().to_string_lossy(),
            r.bitrate_kbps,
            bytes_to_mb(r.bytes)
        ));
        let _ = std::fs::remove_file(&file);
        Ok(small)
    };

    let mut result = RenderResult {
        final_video: None,
        final_bytes: None,
        clips: vec![],
        failed_highlights: vec![],
        failure,
    };
    for out in &outputs {
        stage(&format!(
            "encoding {}/{}: muxing",
            out.index + 1,
            outputs.len()
        ));
        let h = &recording[out.index];
        let tags = h
            .tags
            .iter()
            .filter(|t| t.ends_with('k') || *t == "ace" || *t == "clutch")
            .cloned()
            .collect::<Vec<_>>()
            .join("_");
        let name = format!(
            "{:02}-r{}-{}-{}.{}",
            out.index + 1,
            h.round,
            safe_name(&h.player.name),
            if tags.is_empty() { "clip".into() } else { tags },
            o.container
        );
        let dest = output_dir.join(name);
        if out.video.is_none() {
            log(format!(
                "clip {}: missing video.{} — skipped",
                out.index + 1,
                o.container
            ));
            result.clips.push(RenderedClip {
                combat_windows: vec![],
                highlight_id: h.id.clone(),
                title: h.title.clone(),
                file: None,
                bytes: None,
            });
            continue;
        }
        let file = match mux_clip(
            ffmpeg,
            out,
            &dest,
            o.audio_kbps,
            &mut Control::new(&cancel, log),
        ) {
            Ok(_) => Some(dest),
            Err(error) => {
                Control::new(&cancel, log).check_cancelled()?;
                log(format!("clip {}: {error:#}", out.index + 1));
                result
                    .failure
                    .get_or_insert_with(|| ExportError::new(&error, ErrorCode::EncodingFailed));
                None
            }
        };
        progress(
            recording_end
                + (muxing_end - recording_end) * (out.index + 1) as f64 / outputs.len() as f64,
        );
        let combat_windows = if o.combat_quality_priority && o.max_size_mb.is_some() {
            if let (Some(ticks), Some(path)) = (combat_ticks, file.as_ref()) {
                let seconds = encode::probe_media(ffmpeg, path, &mut Control::new(&cancel, log))?.0;
                combat::windows(h, ticks, demo.tick_rate, seconds)
            } else {
                vec![]
            }
        } else {
            vec![]
        };
        result.clips.push(RenderedClip {
            combat_windows,
            highlight_id: h.id.clone(),
            title: h.title.clone(),
            file,
            bytes: None,
        });
    }
    result.clips = join_highlight_parts(
        ffmpeg,
        &highlights,
        result.clips,
        &output_dir,
        &o,
        log,
        &cancel,
    )?;
    result.failed_highlights = result
        .clips
        .iter()
        .filter(|c| c.file.is_none())
        .map(|c| c.title.clone())
        .collect();
    result.require_complete_merge(o.merge)?;
    let mut muxed: Vec<PathBuf> = result.clips.iter().filter_map(|c| c.file.clone()).collect();
    if o.merge && muxed.len() > 1 {
        // One video: the size limit applies to the joined file; the per-clip files are intermediates.
        let merged = output_dir.join(format!("highlights.{}", o.container));
        stage("encoding: merging");
        // Restore the saved analysis clip order for the final video.
        if preserve_merge_order {
            muxed = merge_order
                .iter()
                .filter_map(|id| {
                    result
                        .clips
                        .iter()
                        .find(|c| c.highlight_id == *id)
                        .and_then(|c| c.file.clone())
                })
                .collect();
        }
        let ordered: Vec<_> = muxed
            .iter()
            .filter_map(|path| result.clips.iter().find(|c| c.file.as_ref() == Some(path)))
            .collect();
        let combat_windows =
            joined_combat_windows(ffmpeg, &ordered, &mut Control::new(&cancel, log))?;
        concat_clips(ffmpeg, &muxed, &merged, &mut Control::new(&cancel, log))?;
        stage("encoding: fitting");
        let merged = fit_to_size(merged, &combat_windows, log, &mut |p| {
            progress(muxing_end + (0.99 - muxing_end) * p)
        })?;
        result.final_bytes = Some(std::fs::metadata(&merged)?.len());
        result.final_video = Some(merged);
        for c in &mut result.clips {
            if let Some(file) = c.file.take() {
                if !o.keep_raw_files {
                    let _ = std::fs::remove_file(file);
                }
            }
        }
    } else {
        let mut completed_seconds = 0.0;
        for (i, c) in result.clips.iter_mut().enumerate() {
            let seconds: f64 = recording
                .iter()
                .filter(|h| h.id == c.highlight_id)
                .map(|h| (h.end_tick - h.start_tick) as f64 / demo.tick_rate)
                .sum();
            if let Some(file) = c.file.take() {
                stage(&format!("encoding {}/{}: fitting", i + 1, highlights.len()));
                let fitted = fit_to_size(file, &c.combat_windows, log, &mut |p| {
                    progress(
                        muxing_end
                            + (0.99 - muxing_end) * (completed_seconds + seconds * p)
                                / total_seconds.max(0.001),
                    )
                });
                Control::new(&cancel, log).check_cancelled()?;
                match fitted {
                    Ok(file) => {
                        c.bytes = Some(std::fs::metadata(&file)?.len());
                        c.file = Some(file);
                    }
                    Err(error) => {
                        if o.merge {
                            return Err(error);
                        }
                        log(format!("{}: {error:#}", c.title));
                        result.failure.get_or_insert_with(|| {
                            ExportError::new(&error, ErrorCode::EncodingFailed)
                        });
                        result.failed_highlights.push(c.title.clone());
                    }
                }
            }
            completed_seconds += seconds;
        }
    }
    Control::new(&cancel, log).check_cancelled()?;
    if !o.keep_raw_files {
        for (pass_dir, indices) in pass_dirs.iter().zip(&passes) {
            for i in 0..indices.len() {
                let _ = std::fs::remove_dir_all(pass_dir.join(actions::sequence_folder_name(i)));
            }
            let _ = std::fs::remove_file(pass_dir.join("commands.xml"));
        }
        let _ = std::fs::remove_dir_all(output_dir.join("cfg"));
        let _ = std::fs::remove_file(output_dir.join("commands.xml"));
    }
    log(format!(
        "done: {}/{} highlights",
        highlights.len() - result.failed_highlights.len(),
        highlights.len()
    ));
    Ok(result)
}

#[cfg(test)]
mod tests {
    #[test]
    fn recording_start_failure_reaches_the_caller() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("output");
        std::fs::create_dir(&output).unwrap();
        // Prevent recording from starting without launching any external tools.
        std::fs::write(output.join("pass-1"), b"blocked").unwrap();
        let demo_path = dir.path().join("test.dem");
        std::fs::write(&demo_path, []).unwrap();
        let demo = super::DemoInfo {
            path: demo_path.to_string_lossy().into_owned(),
            map_name: "test".into(),
            server_name: "test".into(),
            tick_rate: 64.0,
            players: vec![],
        };
        let mut log = Vec::new();
        let result = super::render_highlights(super::RenderJobInput {
            combat_ticks: None,
            demo: &demo,
            demo_path,
            highlights: vec![sample_highlight("clip", "player", vec![])],
            preserve_merge_order: false,
            output_dir: output,
            options: super::RenderOptions {
                codec: "libx264".into(),
                ..Default::default()
            },
            tools: super::ToolPaths {
                tools_dir: dir.path().join("tools"),
                cs2_dir: Some(dir.path().join("cs2")),
                cs2_exe: Some(dir.path().join("cs2.exe")),
                hlae_exe: Some(dir.path().join("HLAE.exe")),
                hlae_dll: Some(dir.path().join("hook.dll")),
                ffmpeg_exe: Some(dir.path().join("ffmpeg.exe")),
                ..Default::default()
            },
            cancel: Default::default(),
            log: &mut |line| log.push(line),
            stage: &mut |_| {},
            progress: &mut |_| {},
        });
        let error = format!(
            "{:#}",
            result.expect_err("recording failure must not become an empty success")
        );
        assert!(log
            .iter()
            .any(|line| line == &format!("recording pass 1 failed: {error}")));
        assert!(!log.iter().any(|line| line.contains("missing video.mp4")));
    }

    #[test]
    fn setup_only_selected_tool() {
        let dir = tempfile::tempdir().unwrap();
        for file in [
            "hlae/HLAE.exe",
            "hlae/x64/AfxHookSource2.dll",
            "ffmpeg/ffmpeg.exe",
            "ffmpeg/ffprobe.exe",
        ] {
            let path = dir.path().join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, []).unwrap();
        }
        let vrf = dir.path().join("vrf").join(super::setup::vrf_exe_name());
        std::fs::create_dir_all(vrf.parent().unwrap()).unwrap();
        std::fs::write(vrf, []).unwrap();
        for (tool, name) in [
            (super::SetupTool::Hlae, "HLAE"),
            (super::SetupTool::Ffmpeg, "FFmpeg"),
            (super::SetupTool::Vrf, "Source 2 Viewer CLI"),
        ] {
            let mut log = Vec::new();
            super::run_setup(
                dir.path(),
                &Default::default(),
                tool,
                false,
                &mut super::setup::Progress {
                    workspace: None,
                    before_replace: None,
                    cancel: &std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
                    report: &mut |line| log.push(line),
                },
            )
            .unwrap();
            assert!(
                log.is_empty(),
                "{name} must not report progress when already installed"
            );
        }
        let overrides = super::paths::PathOverrides {
            hlae_exe: Some(dir.path().to_path_buf()),
            ffmpeg_exe: Some(dir.path().to_path_buf()),
            vrf_exe: Some(dir.path().to_path_buf()),
            ..Default::default()
        };
        let unused = dir.path().join("unused-default");
        for tool in [
            super::SetupTool::Hlae,
            super::SetupTool::Ffmpeg,
            super::SetupTool::Vrf,
        ] {
            super::run_setup(
                &unused,
                &overrides,
                tool,
                false,
                &mut super::setup::Progress {
                    workspace: None,
                    before_replace: None,
                    cancel: &std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
                    report: &mut |_| panic!("custom installed tool must not download"),
                },
            )
            .unwrap();
        }
        assert_eq!(std::fs::read_dir(unused).unwrap().count(), 0);
        assert!(serde_json::from_str::<super::SetupTool>("\"unknown\"").is_err());
    }

    use super::RenderOptions;

    #[test]
    fn old_game_mute_setting_cannot_silence_new_recordings() {
        let options: RenderOptions = serde_json::from_str(r#"{"muteMode":"game"}"#).unwrap();
        assert!(!options.show_game);
        assert!(serde_json::to_value(options)
            .unwrap()
            .get("muteMode")
            .is_none());
    }

    #[test]
    fn retired_startup_options_are_not_saved() {
        for mode in ["event", "minimized", "hidden", "synchronous"] {
            let options: RenderOptions = serde_json::from_value(serde_json::json!({
                "hiddenStartup": mode, "showGame": false
            }))
            .unwrap();
            assert!(!options.show_game);
            assert!(serde_json::to_value(options)
                .unwrap()
                .get("hiddenStartup")
                .is_none());
        }
    }

    #[test]
    fn legacy_render_options_keep_game_hidden() {
        let mut saved = serde_json::to_value(RenderOptions::default()).unwrap();
        saved.as_object_mut().unwrap().remove("showGame");
        let options: RenderOptions = serde_json::from_value(saved).unwrap();
        assert!(!options.show_game);
    }

    #[test]
    fn game_visibility_survives_job_serialization() {
        for show_game in [false, true] {
            let options = RenderOptions {
                show_game,
                ..RenderOptions::default()
            };
            let saved = serde_json::to_value(&options).unwrap();
            assert_eq!(saved["showGame"], show_game);
            let restored: RenderOptions = serde_json::from_value(saved).unwrap();
            assert_eq!(restored.show_game, show_game);
        }
    }
    fn sample_highlight(id: &str, player: &str, windows: Vec<[i32; 2]>) -> super::Highlight {
        serde_json::from_value(serde_json::json!({
            "id": id, "player": { "steamid": player, "name": player }, "round": 1,
            "startTick": 0, "endTick": 3200, "anchorTick": 640,
            "keyMoments": windows, "score": 5.0, "tags": [], "title": id, "kills": [], "breakdown": {}
        })).unwrap()
    }

    #[test]
    fn nearby_clips_use_separate_passes_to_preserve_audio_preroll() {
        for rate in [64.0_f64, 128.0] {
            let minimum_gap = 3 * rate as i32 + rate as i32 / 2 + 2;
            for gap in [minimum_gap - 1, minimum_gap] {
                let clips: Vec<_> = [[1000, 2000], [2000 + gap, 3000]]
                    .into_iter()
                    .map(|[start, end]| {
                        let mut highlight = sample_highlight("clip", "player", vec![]);
                        highlight.start_tick = start;
                        highlight.end_tick = end;
                        super::RenderClip {
                            highlight,
                            slot: Some(1),
                            round_result_slot: None,
                            account_id: None,
                        }
                    })
                    .collect();
                assert_eq!(
                    super::recording_passes(&clips, rate).len(),
                    if gap < minimum_gap { 2 } else { 1 }
                );
            }
        }
    }

    #[test]
    fn overlapping_views_preserve_recording_boundaries_in_separate_passes() {
        let windows = [[2310, 3162], [2535, 3605], [3227, 4000], [4066, 4500]];
        let clips: Vec<_> = windows
            .iter()
            .enumerate()
            .map(|(i, &[start, end])| {
                let mut h = sample_highlight(&i.to_string(), &i.to_string(), vec![]);
                h.start_tick = start;
                h.end_tick = end;
                super::RenderClip {
                    highlight: h,
                    slot: Some(1),
                    round_result_slot: None,
                    account_id: None,
                }
            })
            .collect();
        let passes = super::recording_passes(&clips, 64.0);
        assert_eq!(passes, vec![vec![0, 3], vec![1], vec![2]]);
        for pass in passes {
            let group: Vec<_> = pass.iter().map(|&i| clips[i].clone()).collect();
            let options = RenderOptions::default();
            let schedule = super::build_schedule(
                &group,
                &super::ActionsOptions {
                    render: &options,
                    tick_rate: 64.0,
                    output_dir: "test".into(),
                    ffmpeg_preset: "test".into(),
                },
            );
            for (index, clip) in group.iter().enumerate() {
                for (marker, tick) in [
                    ("start", clip.highlight.start_tick),
                    ("end", clip.highlight.end_tick),
                ] {
                    let command = if marker == "start" {
                        format!("demodesk_wait_{}", index + 1)
                    } else {
                        format!(
                            "echo [demodesk] seq {} of {} {marker}",
                            index + 1,
                            group.len()
                        )
                    };
                    assert_eq!(
                        schedule
                            .iter()
                            .find(|s| s.cmd == command)
                            .unwrap()
                            .tick
                            .floor() as i32,
                        tick
                    );
                }
            }
        }
    }

    #[test]
    fn incomplete_exports_can_only_be_published_without_merge() {
        let mut result = super::RenderResult {
            final_video: None,
            final_bytes: None,
            clips: vec![],
            failed_highlights: vec!["missing".into()],
            failure: None,
        };
        assert!(result.require_complete_merge(true).is_err());
        assert!(result.require_complete_merge(false).is_ok());
        result.failure = Some(crate::error::ExportError::new(
            &anyhow::anyhow!("specific diagnostic").context(crate::ErrorCode::DiskFull),
            crate::ErrorCode::ExportFailed,
        ));
        assert_eq!(
            result
                .require_complete_merge(true)
                .unwrap_err()
                .downcast_ref::<crate::ErrorCode>(),
            Some(&crate::ErrorCode::DiskFull)
        );
        result.failed_highlights.clear();
        assert!(result.require_complete_merge(true).is_ok());
    }

    #[test]
    fn key_windows_merge_strictly_under_one_second_and_keep_owner() {
        let h = sample_highlight("a", "p", vec![[320, 832], [1280, 1792], [2496, 3008]]);
        let clips = super::recording_windows(&[h.clone()], 64.0, true, false);
        let short = super::recording_windows(&[h.clone()], 64.0, true, true);
        assert!(short.iter().all(|c| c.end_tick - c.start_tick <= 384));
        assert_eq!(
            short.last().unwrap().end_tick,
            clips.last().unwrap().end_tick
        );

        assert_eq!(clips.len(), 3);
        assert_eq!(
            clips.iter().map(|h| h.end_tick - h.start_tick).sum::<i32>(),
            24 * 64
        );
        assert!(clips.iter().all(|c| c.id == "a"));
        assert_eq!(
            super::recording_windows(&[h], 64.0, false, false)[0].end_tick,
            3200
        );
        for (gap, count) in [(0, 1), (63, 1), (64, 2)] {
            let h = sample_highlight("b", "p", vec![[1000, 1256], [1256 + gap, 1512 + gap]]);
            assert_eq!(
                super::recording_windows(&[h], 64.0, true, false).len(),
                count
            );
        }
    }

    #[test]
    fn overlap_uses_selected_recording_windows_before_merging() {
        let a = sample_highlight("a", "p", vec![[320, 832]]);
        let b = sample_highlight("b", "p", vec![[800, 1312]]);
        for (key_only, short, expected) in [
            (true, false, vec![(320, 1312)]),
            (true, true, vec![(448, 832), (928, 1312)]),
            (false, false, vec![(0, 3200)]),
        ] {
            let recording =
                super::recording_windows(&[a.clone(), b.clone()], 64.0, key_only, short);
            let merged = super::merge_nearby_clips(recording, 64.0);
            assert_eq!(
                merged
                    .iter()
                    .map(|h| (h.start_tick, h.end_tick))
                    .collect::<Vec<_>>(),
                expected
            );
            assert_eq!(merged[0].id, "a");
            if short {
                assert_eq!(merged[1].id, "b");
            }
        }
    }

    #[test]
    fn interleaved_players_do_not_prevent_same_player_window_merging() {
        let a = sample_highlight("a", "p", vec![[320, 832]]);
        let b = sample_highlight("b", "q", vec![[400, 900]]);
        let c = sample_highlight("c", "p", vec![[850, 1100]]);
        let merged = super::merge_nearby_clips(vec![a, b, c], 64.0);
        assert_eq!(merged.len(), 2);
        let windows = super::recording_windows(&merged, 64.0, true, false);
        assert_eq!(windows.len(), 2);
        assert_eq!((windows[0].start_tick, windows[0].end_tick), (320, 1100));
        let legacy = sample_highlight("old", "p", vec![]);
        assert_eq!(
            super::recording_windows(&[legacy], 64.0, true, false)[0].end_tick,
            3200
        );
    }

    #[test]
    fn key_moment_setting_defaults_on_and_preserves_explicit_off() {
        let defaults: RenderOptions = serde_json::from_str("{}").unwrap();
        assert!(defaults.key_moments_only);
        assert!(defaults.combat_quality_priority);
        assert_eq!(defaults.codec, "libsvtav1");
        let existing: RenderOptions =
            serde_json::from_str(r#"{"codec":"hevc_nvenc","crf":22}"#).unwrap();
        assert_eq!(existing.codec, "hevc_nvenc");
        assert_eq!(existing.crf, 22);
        let disabled: RenderOptions =
            serde_json::from_str(r#"{"combatQualityPriority":false}"#).unwrap();
        assert!(!disabled.combat_quality_priority);
        let off: RenderOptions = serde_json::from_str(r#"{"keyMomentsOnly":false}"#).unwrap();
        assert!(!off.key_moments_only);
    }

    #[test]
    fn missing_part_does_not_publish_an_incomplete_highlight() {
        let h = sample_highlight("a", "p", vec![]);
        let parts = vec![
            super::RenderedClip {
                combat_windows: vec![],
                highlight_id: "a".into(),
                title: "a".into(),
                file: Some("part1.mp4".into()),
                bytes: None,
            },
            super::RenderedClip {
                combat_windows: vec![],
                highlight_id: "a".into(),
                title: "a".into(),
                file: None,
                bytes: None,
            },
        ];
        let joined = super::join_highlight_parts(
            std::path::Path::new("unused"),
            &[h],
            parts,
            std::path::Path::new("unused"),
            &RenderOptions::default(),
            &mut |_| {},
            &std::sync::atomic::AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(joined.len(), 1);
        assert!(joined[0].file.is_none());
    }
    #[test]
    #[ignore = "requires FFMPEG"]
    fn real_key_parts_join_into_one_video_per_highlight() {
        let ffmpeg = std::path::PathBuf::from(std::env::var("FFMPEG").expect("set FFMPEG"));
        let dir = tempfile::tempdir().unwrap();
        let mut parts = Vec::new();
        for index in 0..4 {
            let file = dir.path().join(format!("part{index}.mp4"));
            let mut command = std::process::Command::new(&ffmpeg);
            command
                .args([
                    "-y",
                    "-f",
                    "lavfi",
                    "-i",
                    "color=c=blue:s=64x64:r=30",
                    "-t",
                    "1",
                    "-c:v",
                    "libx264",
                    "-pix_fmt",
                    "yuv420p",
                ])
                .arg(&file);
            let output = super::process::ProcessTree::new()
                .unwrap()
                .output(&mut command)
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            parts.push(super::RenderedClip {
                combat_windows: vec![[0.25, 0.75]],
                highlight_id: if index < 3 { "a" } else { "b" }.into(),
                title: "test".into(),
                file: Some(file),
                bytes: None,
            });
        }
        let joined = super::join_highlight_parts(
            &ffmpeg,
            &[
                sample_highlight("a", "p", vec![]),
                sample_highlight("b", "p", vec![]),
            ],
            parts,
            dir.path(),
            &RenderOptions::default(),
            &mut |_| {},
            &std::sync::atomic::AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(joined.len(), 2);
        assert_eq!(
            joined[0].combat_windows,
            vec![[0.25, 0.75], [1.25, 1.75], [2.25, 2.75]]
        );
        assert_eq!(joined[1].combat_windows, vec![[0.25, 0.75]]);
        assert!(!dir.path().join("part0.mp4").exists());
        assert!(dir.path().join("part3.mp4").exists());
        let mut probe = std::process::Command::new(&ffmpeg);
        probe.arg("-i").arg(joined[0].file.as_ref().unwrap()).args([
            "-progress",
            "pipe:1",
            "-f",
            "null",
            "-",
        ]);
        let output = super::process::ProcessTree::new()
            .unwrap()
            .output(&mut probe)
            .unwrap();
        assert!(output.status.success());
        let micros = String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter_map(|line| line.strip_prefix("out_time_us=")?.parse::<u64>().ok())
            .last()
            .unwrap();
        assert!(
            (2_900_000..=3_100_000).contains(&micros),
            "duration {micros}"
        );
    }
}
