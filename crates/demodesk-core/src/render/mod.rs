//! HLAE render backend: plays the demo in CS2 through HLAE and records each
//! [`Highlight`] with mirv_streams → FFmpeg, then muxes each clip (or joins
//! them into one video) and optionally shrinks the result to a size budget.

pub mod actions;
pub mod encode;
mod leftovers;
pub mod paths;
mod record;
pub mod setup;
mod window;
mod startup;
pub(crate) mod process;
#[cfg(windows)]
mod audio;

use crate::model::{DemoInfo, Highlight};
use actions::{build_schedule, steamid_to_account_id, ActionsOptions, Camera, RenderClip};
use anyhow::{anyhow, Result};
use encode::{bytes_to_mb, concat_clips, encode_to_size, mux_clip};
use paths::{resolve_tool_paths, to_forward_slashes, PathOverrides, ToolPaths, IS_WINDOWS};
use leftovers::{has_leftovers, remove_leftovers};
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
pub fn doctor(default_tools_dir: &Path, o: &PathOverrides) -> DoctorReport {
    let paths = resolve_tool_paths(default_tools_dir, o);
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
    DoctorReport { ok: problems.is_empty(), problems, paths }
}

/// Remove what an old version left in the game folder (see leftovers.rs).
/// Must NOT run while a recording is in progress.
pub fn clean_leftovers(default_tools_dir: &Path, o: &PathOverrides) -> bool {
    let paths = resolve_tool_paths(default_tools_dir, o);
    match &paths.cs2_dir {
        Some(cs2) if has_leftovers(cs2) => remove_leftovers(cs2).is_ok(),
        _ => false,
    }
}

pub fn run_setup(default_tools_dir: &Path, o: &PathOverrides, force: bool, log: &mut dyn FnMut(String)) -> Result<DoctorReport> {
    let paths = resolve_tool_paths(default_tools_dir, o);
    std::fs::create_dir_all(&paths.tools_dir)?;
    setup::install_hlae(&paths.tools_dir, force, log)?;
    if o.ffmpeg_exe.is_none() && (force || paths.ffmpeg_exe.is_none()) {
        setup::install_ffmpeg(&paths.tools_dir, force, log)?;
    }
    setup::install_vrf(&paths.tools_dir, force, log)?;
    Ok(doctor(default_tools_dir, o))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RenderOptions {
    pub fps: u32,
    pub width: u32,
    pub height: u32,
    /// one of encode::CODECS: libx264 | libx265 | h264_nvenc | hevc_nvenc
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
    pub chat: bool,
    /// First-person weapon model
    pub viewmodel: bool,
    /// Bullet tracers
    pub tracers: bool,
    /// hud_scaling (CS2 allows 0.5–0.95)
    pub hud_scale: f64,
    pub true_view: bool,
    pub xray: bool,
    pub voice: bool,
    pub show_game: bool,
    pub quit_when_done: bool,
    pub keep_raw_files: bool,
    /// Join all clips into one video (`highlights.<container>`) instead of one file per clip
    pub merge: bool,
    /// Re-encode so each output file — the merged video, or every clip — is at most this many MB
    pub max_size_mb: Option<f64>,
    pub audio_kbps: u32,
    pub extra_launch_options: Vec<String>,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            fps: 60,
            width: 1920,
            height: 1080,
            codec: "libx264".into(),
            crf: 23,
            container: "mp4".into(),
            camera: Camera::Slot,
            death_notice_seconds: 5,
            hud: true,
            crosshair: true,
            radar: true,
            kill_feed: true,
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
            max_size_mb: Some(20.0),
            audio_kbps: 192,
            extra_launch_options: vec![],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderedClip {
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
}

pub fn to_render_clips(demo: &DemoInfo, highlights: &[Highlight]) -> Vec<RenderClip> {
    highlights
        .iter()
        .map(|h| RenderClip {
            highlight: h.clone(),
            slot: demo.players.iter().find(|p| p.steamid == h.player.steamid).and_then(|p| p.user_id).map(|u| u + 1),
            account_id: steamid_to_account_id(&h.player.steamid),
        })
        .collect()
}

pub struct RenderJobInput<'a> {
    pub demo: &'a DemoInfo,
    pub demo_path: PathBuf,
    pub highlights: Vec<Highlight>,
    pub output_dir: PathBuf,
    pub options: RenderOptions,
    pub tools: ToolPaths,
    pub cancel: Arc<AtomicBool>,
    pub log: &'a mut dyn FnMut(String),
    pub stage: &'a mut dyn FnMut(&str),
}

fn safe_name(s: &str) -> String {
    s.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect()
}

pub fn render_highlights(input: RenderJobInput) -> Result<RenderResult> {
    let RenderJobInput { demo, demo_path, mut highlights, output_dir, options: o, tools, cancel, log, stage } = input;
    let (Some(cs2_dir), Some(cs2_exe), Some(hlae_exe), Some(hlae_dll), Some(ffmpeg_exe)) = (tools.cs2_dir, tools.cs2_exe, tools.hlae_exe, tools.hlae_dll, tools.ffmpeg_exe) else {
        return Err(anyhow!("environment not ready — check the settings page"));
    };
    let ffmpeg = &ffmpeg_exe;
    if !demo_path.is_file() {
        return Err(anyhow!("demo not found: {}", demo_path.display()));
    }
    if !demo_path.to_string_lossy().is_ascii() {
        return Err(anyhow!("demo path must be ASCII only (CS2 +playdemo limitation) — copy the demo somewhere else"));
    }
    if highlights.is_empty() {
        return Err(anyhow!("nothing to render"));
    }
    // Render in demo order so the game only seeks forward.
    highlights.sort_by_key(|h| h.start_tick);
    std::fs::create_dir_all(&output_dir)?;
    let clips = to_render_clips(demo, &highlights);
    let missing_slots: Vec<&str> = clips.iter().filter(|c| c.slot.is_none() && o.camera == Camera::Slot).map(|c| c.highlight.player.name.as_str()).collect();
    if !missing_slots.is_empty() {
        log(format!("warning: no player slot for {} — camera will not follow them", missing_slots.join(", ")));
    }

    let schedule = build_schedule(
        &clips,
        &ActionsOptions { render: &o, tick_rate: demo.tick_rate, output_dir: to_forward_slashes(&output_dir), ffmpeg_preset: encode::record_preset(&o.codec, o.crf) },
    );
    let total_seconds: f64 = highlights.iter().map(|h| (h.end_tick - h.start_tick) as f64 / demo.tick_rate).sum();
    let timeout_seconds = (180.0 + highlights.len() as f64 * 30.0 + total_seconds * 6.0) as u64;

    stage("recording");
    let mut session = RecordSession {
        demo_path,
        cs2_dir,
        cs2_exe,
        hlae_exe,
        hlae_dll,
        ffmpeg_exe: ffmpeg_exe.clone(),
        output_dir: output_dir.clone(),
        cfg_dir: tools.tools_dir.parent().map(|p| p.join("cfg")).unwrap_or_else(|| output_dir.join("cfg")),
        show_game: o.show_game,
        width: o.width,
        height: o.height,
        schedule,
        timeout_seconds,
        extra_launch_options: o.extra_launch_options.clone(),
        cancel: cancel.clone(),
        log,
        stage,
    };
    run_recording_session(&mut session)?;
    let log = session.log;
    let stage = session.stage;

    stage("encoding");
    let outputs = collect_clip_outputs(&output_dir, clips.len(), &o.container);
    let fit_to_size = |file: PathBuf, log: &mut dyn FnMut(String)| -> Result<PathBuf> {
        let Some(mb) = o.max_size_mb else { return Ok(file) };
        if std::fs::metadata(&file)?.len() <= (mb * 1024.0 * 1024.0) as u64 {
            return Ok(file);
        }
        let small = file.with_extension(format!("{}mb.mp4", mb as u32));
        let r = encode_to_size(ffmpeg, &file, &small, mb, &o.codec, 128)?;
        log(format!("{} → {} ({} kbps, {} MB)", file.file_name().unwrap().to_string_lossy(), small.file_name().unwrap().to_string_lossy(), r.bitrate_kbps, bytes_to_mb(r.bytes)));
        let _ = std::fs::remove_file(&file);
        Ok(small)
    };

    let mut result = RenderResult { final_video: None, final_bytes: None, clips: vec![] };
    let mut muxed: Vec<PathBuf> = vec![];
    for out in &outputs {
        let h = &highlights[out.index];
        let tags = h.tags.iter().filter(|t| t.ends_with('k') || *t == "ace" || *t == "clutch").cloned().collect::<Vec<_>>().join("_");
        let name = format!("{:02}-r{}-{}-{}.{}", out.index + 1, h.round, safe_name(&h.player.name), if tags.is_empty() { "clip".into() } else { tags }, o.container);
        let dest = output_dir.join(name);
        if out.video.is_none() {
            log(format!("clip {}: missing video.{} — skipped", out.index + 1, o.container));
            result.clips.push(RenderedClip { highlight_id: h.id.clone(), title: h.title.clone(), file: None, bytes: None });
            continue;
        }
        mux_clip(ffmpeg, out, &dest, o.audio_kbps)?;
        muxed.push(dest.clone());
        result.clips.push(RenderedClip { highlight_id: h.id.clone(), title: h.title.clone(), file: Some(dest), bytes: None });
    }
    if o.merge && muxed.len() > 1 {
        // One video: the size limit applies to the joined file; the per-clip files are intermediates.
        let merged = output_dir.join(format!("highlights.{}", o.container));
        concat_clips(ffmpeg, &muxed, &merged)?;
        let merged = fit_to_size(merged, log)?;
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
        for c in &mut result.clips {
            if let Some(file) = c.file.take() {
                let fitted = fit_to_size(file, log)?;
                c.bytes = Some(std::fs::metadata(&fitted)?.len());
                c.file = Some(fitted);
            }
        }
    }
    if !o.keep_raw_files {
        for i in 0..clips.len() {
            let _ = std::fs::remove_dir_all(output_dir.join(actions::sequence_folder_name(i)));
        }
        let _ = std::fs::remove_dir_all(output_dir.join("cfg"));
        let _ = std::fs::remove_file(output_dir.join("commands.xml"));
    }
    log(format!("done: {}/{} clips", muxed.len(), clips.len()));
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::RenderOptions;

    #[test]
    fn old_game_mute_setting_cannot_silence_new_recordings() {
        let options: RenderOptions = serde_json::from_str(r#"{"muteMode":"game"}"#).unwrap();
        assert!(!options.show_game);
        assert!(serde_json::to_value(options).unwrap().get("muteMode").is_none());
    }

    #[test]
    fn retired_startup_options_are_not_saved() {
        for mode in ["event", "minimized", "hidden", "synchronous"] {
            let options: RenderOptions = serde_json::from_value(serde_json::json!({
                "hiddenStartup": mode, "showGame": false
            })).unwrap();
            assert!(!options.show_game);
            assert!(serde_json::to_value(options).unwrap().get("hiddenStartup").is_none());
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
            let options = RenderOptions { show_game, ..RenderOptions::default() };
            let saved = serde_json::to_value(&options).unwrap();
            assert_eq!(saved["showGame"], show_game);
            let restored: RenderOptions = serde_json::from_value(saved).unwrap();
            assert_eq!(restored.show_game, show_game);
        }
    }
}
