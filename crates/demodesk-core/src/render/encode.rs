//! FFmpeg helpers that run after HLAE has produced the raw clip: probing,
//! audio muxing, concatenation and — the part messaging apps care about —
//! re-encoding so the file lands under a byte budget (two-pass bitrate targeting).

use super::paths::to_forward_slashes;
use super::record::ClipOutput;
use super::process::ProcessTree;
use anyhow::{anyhow, Result};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn run(exe: &Path, args: &[String]) -> Result<()> {
    run_progress(exe, args, 1.0, &mut |_| {})
}

fn run_progress(exe: &Path, args: &[String], seconds: f64, progress: &mut dyn FnMut(f64)) -> Result<()> {
    let mut cmd = Command::new(exe);
    cmd.args(["-progress", "pipe:1", "-nostats"]);
    cmd.args(args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped());
    let mut pending = String::new();
    let out = ProcessTree::new()?.output_with_progress(&mut cmd, &mut |chunk| {
        pending.push_str(&String::from_utf8_lossy(chunk));
        while let Some(end) = pending.find('\n') {
            if let Some(micros) = pending[..end].trim().strip_prefix("out_time_us=").and_then(|v| v.parse::<f64>().ok()).filter(|v| v.is_finite()) {
                progress((micros / 1_000_000.0 / seconds).clamp(0.0, 1.0));
            }
            pending.drain(..=end);
        }
    })?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let tail: Vec<&str> = err.lines().rev().take(5).collect();
        return Err(anyhow!("{} failed: {}", exe.file_name().unwrap().to_string_lossy(), tail.into_iter().rev().collect::<Vec<_>>().join(" | ")));
    }
    Ok(())
}

fn s(v: &str) -> String {
    v.to_string()
}

pub fn ffprobe_exe(ffmpeg_exe: &Path) -> PathBuf {
    ffmpeg_exe.parent().unwrap().join(if cfg!(windows) { "ffprobe.exe" } else { "ffprobe" })
}

fn probe_media(ffmpeg_exe: &Path, file: &Path) -> Result<(f64, bool)> {
    let mut cmd = Command::new(ffprobe_exe(ffmpeg_exe));
    cmd.args(["-v", "error", "-show_entries", "format=duration:stream=codec_type", "-of", "json"]).arg(file);
    let out = ProcessTree::new()?.output(&mut cmd)?;
    if !out.status.success() { return Err(anyhow!("cannot probe input video")); }
    let info: serde_json::Value = serde_json::from_slice(&out.stdout)?;
    let seconds = info["format"]["duration"].as_str().and_then(|v| v.parse::<f64>().ok()).filter(|v| v.is_finite() && *v > 0.0).ok_or_else(|| anyhow!("input video has no valid duration"))?;
    let audio = info["streams"].as_array().is_some_and(|streams| streams.iter().any(|stream| stream["codec_type"] == "audio"));
    Ok((seconds, audio))
}

pub fn probe_duration_seconds(ffmpeg_exe: &Path, file: &Path) -> Result<f64> {
    Ok(probe_media(ffmpeg_exe, file)?.0)
}

pub fn mux_clip(ffmpeg_exe: &Path, clip: &ClipOutput, dest: &Path, audio_kbps: u32) -> Result<()> {
    let video = clip.video.as_ref().ok_or_else(|| anyhow!("clip {}: no video file", clip.index + 1))?;
    let mut args = vec![s("-y"), s("-i"), video.to_string_lossy().to_string()];
    if let Some(audio) = &clip.audio {
        args.extend([s("-i"), audio.to_string_lossy().to_string(), s("-map"), s("0:v:0"), s("-map"), s("1:a:0"), s("-c:v"), s("copy"), s("-c:a"), s("aac"), s("-b:a"), format!("{audio_kbps}k"), s("-ar"), s("48000"), s("-ac"), s("2"), s("-shortest")]);
    } else {
        args.extend([s("-c:v"), s("copy")]);
    }
    args.extend([s("-movflags"), s("+faststart")]);
    args.push(dest.to_string_lossy().to_string());
    run(ffmpeg_exe, &args)
}

pub fn concat_clips(ffmpeg_exe: &Path, clips: &[PathBuf], dest: &Path) -> Result<()> {
    let list = dest.parent().unwrap().join("concat.txt");
    let body: Vec<String> = clips.iter().map(|c| format!("file '{}'", to_forward_slashes(c).replace('\'', "'\\''"))).collect();
    std::fs::write(&list, body.join("\n"))?;
    let r = run(ffmpeg_exe, &[s("-y"), s("-f"), s("concat"), s("-safe"), s("0"), s("-i"), list.to_string_lossy().to_string(), s("-c"), s("copy"), s("-movflags"), s("+faststart"), dest.to_string_lossy().to_string()]);
    let _ = std::fs::remove_file(&list);
    r
}

/// Bitrate in kbit/s that fits `max_size_mb` for `seconds` of video, leaving room for audio and container overhead.
pub fn video_bitrate_for_size(max_size_mb: f64, seconds: f64, audio_kbps: u32) -> u32 {
    if !max_size_mb.is_finite() || max_size_mb <= 0.0 || !seconds.is_finite() || seconds <= 0.0 { return 0; }
    // MB and FFmpeg kbit/s are decimal. Reserve 2% for muxing and rate-control error.
    ((max_size_mb * 1_000_000.0 * 8.0 * 0.98 / seconds / 1000.0 - audio_kbps as f64).floor().max(0.0)) as u32
}

pub struct SizeResult {
    pub bitrate_kbps: u32,
    pub bytes: u64,
}

/// Video codecs we know how to drive: (ffmpeg encoder name, label).
pub const CODECS: [(&str, &str); 4] = [("libx264", "H.264 CPU"), ("libx265", "H.265 CPU"), ("h264_nvenc", "H.264 NVIDIA"), ("hevc_nvenc", "H.265 NVIDIA")];

fn is_hevc(codec: &str) -> bool {
    codec == "libx265" || codec == "hevc_nvenc"
}

/// `hvc1` tag so HEVC-in-mp4 plays in QuickTime / iOS / Windows Media Player.
fn tag_args(codec: &str) -> Vec<String> {
    if is_hevc(codec) { vec![s("-tag:v"), s("hvc1")] } else { vec![] }
}

const NVENC_RECORDING: &str = "-preset p5 -tune hq -multipass qres -rc-lookahead 0 -spatial-aq 1 -bf 2";

const NVENC_COMPATIBLE: &str = "-preset p4 -tune hq -multipass disabled -rc-lookahead 0 -spatial-aq 0 -temporal-aq 0 -bf 0 -b_ref_mode disabled";

fn with_nvenc_fallback<T>(compatible: &mut bool, mut encode: impl FnMut(bool) -> Result<T>) -> Result<T> {
    match encode(*compatible) {
        Ok(value) => Ok(value),
        Err(first) if !*compatible => {
            *compatible = true;
            eprintln!("NVENC failed; retrying with compatibility settings: {first:#}");
            encode(true).map_err(|last| anyhow!("NVENC failed: {first:#}; compatibility retry failed: {last:#}"))
        }
        Err(error) => Err(error),
    }
}

/// Probe the actual capture format before launching the game, with one compatibility retry.
pub fn checked_record_preset(ffmpeg: &Path, options: &super::RenderOptions, log: &mut dyn FnMut(String)) -> Result<(String, bool)> {
    let preset = record_preset(&options.codec, options.crf, options.fps);
    if !options.codec.contains("nvenc") { return Ok((preset, false)); }
    let mut compatible = false;
    let selected = with_nvenc_fallback(&mut compatible, |fallback| {
        let selected = if fallback { preset.replace(NVENC_RECORDING, NVENC_COMPATIBLE).replace("main -b_ref_mode middle", "main") } else { preset.clone() };
        let mut args = vec![s("-f"), s("lavfi"), s("-i"), format!("color=size={}x{}:rate={}", options.width, options.height, options.fps), s("-frames:v"), s("3")];
        args.extend(selected.split_whitespace().map(s));
        args.extend([s("-f"), s("null"), s(if cfg!(windows) { "NUL" } else { "/dev/null" })]);
        run(ffmpeg, &args)?;
        Ok(selected)
    })?;
    if compatible { log("NVENC: using compatibility settings (B-frames, AQ and multipass disabled)".into()); }
    Ok((selected, compatible))
}

/// Quality-targeted arguments HLAE hands to ffmpeg while recording (`crf` ≈ quality, lower = better).
/// NVENC uses constant QP for capture; size fitting uses VBR separately.
pub fn record_preset(codec: &str, crf: u32, fps: u32) -> String {
    let mut parts = vec![format!("-c:v {codec}"), s("-pix_fmt yuv420p")];
    if codec.contains("nvenc") {
        parts.push(format!("-rc constqp -qp {crf} -b:v 0 {NVENC_RECORDING} -g {} -profile:v {}", u64::from(fps) * 2, if is_hevc(codec) { "main -b_ref_mode middle" } else { "high" }));
    } else {
        parts.push(format!("-crf {crf} -preset {} -profile:v {} -g {}", if is_hevc(codec) { "fast" } else { "veryfast" }, if is_hevc(codec) { "main" } else { "high" }, u64::from(fps) * 2));
    }
    parts.extend(tag_args(codec));
    parts.join(" ")
}

/// Re-encode from the same source on each attempt; publish only a size-checked file.
/// NVENC multipass is per-frame analysis, not the CPU encoders' file-level two passes.
pub fn encode_to_size(ffmpeg_exe: &Path, input: &Path, output: &Path, max_size_mb: f64, codec: &str, audio_kbps: u32) -> Result<SizeResult> {
    encode_to_size_with_progress(ffmpeg_exe, input, output, max_size_mb, codec, audio_kbps, &mut false, &mut |_| {})
}

pub fn encode_to_size_with_progress(ffmpeg_exe: &Path, input: &Path, output: &Path, max_size_mb: f64, codec: &str, audio_kbps: u32, compatible: &mut bool, progress: &mut dyn FnMut(f64)) -> Result<SizeResult> {
    if !CODECS.iter().any(|(name, _)| *name == codec) { return Err(anyhow!("unsupported video encoder: {codec}")); }
    if !max_size_mb.is_finite() || max_size_mb <= 0.0 { return Err(anyhow!("invalid file size limit")); }
    if input == output { return Err(anyhow!("input and output must be different files")); }
    let (seconds, has_audio) = probe_media(ffmpeg_exe, input)?;
    let audio_kbps = if has_audio { audio_kbps } else { 0 };
    let limit = (max_size_mb * 1_000_000.0).floor() as u64;
    let mut bitrate = video_bitrate_for_size(max_size_mb, seconds, audio_kbps);
    if bitrate == 0 { return Err(anyhow!("size limit is too small for this duration and audio bitrate")); }
    let temp = tempfile::Builder::new().prefix("encode-").suffix(".mp4").tempfile_in(output.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new(".")))?.into_temp_path();
    let input_s = input.to_string_lossy().to_string();
    let output_s = temp.to_string_lossy().to_string();
    let mut common = vec![s("-y"), s("-i"), input_s.clone(), s("-map"), s("0:v:0"), s("-map"), s("0:a:0?"), s("-c:a"), s("aac"), s("-b:a"), format!("{audio_kbps}k"), s("-ar"), s("48000"), s("-ac"), s("2"), s("-movflags"), s("+faststart"), s("-pix_fmt"), s("yuv420p"), s("-fps_mode"), s("passthrough")];
    common.extend(tag_args(codec));
    let mut reported = 0.0_f64;
    for attempt in 0..4 {
        // Leave space for size verification and retries without moving backwards.
        let start = 1.0 - 0.1_f64.powi(attempt);
        let span = 0.9 * 0.1_f64.powi(attempt);
        if codec.contains("nvenc") {
            with_nvenc_fallback(compatible, |fallback| {
                let mut args = common.clone();
                args.extend([s("-c:v"), s(codec), s("-rc"), s("vbr"), s("-b:v"), format!("{bitrate}k"), s("-maxrate"), format!("{bitrate}k"), s("-bufsize"), format!("{}k", u64::from(bitrate) * 2)]);
                args.extend(if fallback { NVENC_COMPATIBLE } else { NVENC_RECORDING }.split_whitespace().map(s));
                args.extend([s("-force_key_frames"), s("expr:gte(t,n_forced*2)"), s("-profile:v"), s(if is_hevc(codec) { "main" } else { "high" })]);
                if is_hevc(codec) && !fallback { args.extend([s("-b_ref_mode"), s("middle")]); }
                args.push(output_s.clone());
                run_progress(ffmpeg_exe, &args, seconds, &mut |p| {
                    reported = reported.max(start + span * p);
                    progress(reported);
                })
            })?;
        } else {
            let tmp = tempfile::tempdir()?;
            let passlog = tmp.path().join("ffmpeg2pass").to_string_lossy().replace('\\', "/");
            let null_sink = if cfg!(windows) { "NUL" } else { "/dev/null" };
            let rate = [s("-c:v"), s(codec), s("-b:v"), format!("{bitrate}k"), s("-maxrate"), format!("{}k", (bitrate as f64 * 1.3) as u32), s("-bufsize"), format!("{}k", u64::from(bitrate) * 2), s("-preset"), s("medium"), s("-pix_fmt"), s("yuv420p"), s("-fps_mode"), s("passthrough")];
            let pass_args = |n: u32| -> Vec<String> {
                if codec == "libx265" { vec![s("-x265-params"), format!("pass={n}:stats={}.x265", passlog.replace(':', "\\:"))] }
                else { vec![s("-pass"), n.to_string(), s("-passlogfile"), passlog.clone()] }
            };
            let mut pass1 = vec![s("-y"), s("-i"), input_s.clone(), s("-map"), s("0:v:0")];
            pass1.extend(rate.iter().cloned());
            pass1.extend(pass_args(1));
            pass1.extend([s("-an"), s("-f"), s("null"), s(null_sink)]);
            run_progress(ffmpeg_exe, &pass1, seconds, &mut |p| progress(start + span * p / 2.0))?;
            let mut pass2 = common.clone();
            pass2.extend(rate.iter().cloned());
            pass2.extend(pass_args(2));
            pass2.push(output_s.clone());
            run_progress(ffmpeg_exe, &pass2, seconds, &mut |p| progress(start + span * (0.5 + p / 2.0)))?;
        }
        let bytes = std::fs::metadata(&temp)?.len();
        if bytes > 0 && bytes <= limit {
            temp.persist(output)?;
            progress(1.0);
            return Ok(SizeResult { bitrate_kbps: bitrate, bytes });
        }
        let audio_bytes = audio_kbps as f64 * 1000.0 * seconds / 8.0;
        let ratio = ((limit as f64 - audio_bytes) / (bytes as f64 - audio_bytes).max(1.0) * 0.97).clamp(0.0, 0.95);
        bitrate = (bitrate as f64 * ratio).floor() as u32;
        if bitrate == 0 { break; }
    }
    Err(anyhow!("could not meet {max_size_mb} MB without changing resolution, FPS or encoder; source video was retained"))
}

pub fn bytes_to_mb(bytes: u64) -> f64 {
    (bytes as f64 / 1_000_000.0 * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compatibility_retry_is_bounded_and_reused() {
        let mut compatible = false;
        let mut calls = vec![];
        let result = with_nvenc_fallback(&mut compatible, |fallback| {
            calls.push(fallback);
            if fallback { Ok(42) } else { Err(anyhow!("unsupported B-frames")) }
        }).unwrap();
        assert_eq!(result, 42);
        assert_eq!(calls, [false, true]);
        calls.clear();
        assert!(with_nvenc_fallback::<()>(&mut compatible, |fallback| {
            calls.push(fallback); Err(anyhow!("still fails"))
        }).is_err());
        assert_eq!(calls, [true]);
        calls.clear();
        let error = with_nvenc_fallback::<()>(&mut false, |fallback| {
            calls.push(fallback); Err(anyhow!(if fallback { "fallback error" } else { "primary error" }))
        }).unwrap_err().to_string();
        assert_eq!(calls, [false, true]);
        assert!(error.contains("primary error") && error.contains("fallback error"));
        calls.clear();
        with_nvenc_fallback(&mut false, |fallback| { calls.push(fallback); Ok(()) }).unwrap();
        assert_eq!(calls, [false]);
    }

    #[test]
    fn size_budget_accounts_for_decimal_units_audio_and_invalid_inputs() {
        assert_eq!(video_bitrate_for_size(20.0, 60.0, 192), 2421);
        assert_eq!(video_bitrate_for_size(20.0, 60.0, 0), 2613);
        assert_eq!(video_bitrate_for_size(0.1, 60.0, 192), 0);
        for seconds in [0.0, -1.0, f64::NAN, f64::INFINITY] { assert_eq!(video_bitrate_for_size(20.0, seconds, 192), 0); }
    }

    #[test]
    fn record_presets_preserve_encoder_and_quality_without_resizing() {
        for (codec, _) in CODECS {
            let preset = record_preset(codec, 20, 90);
            assert!(preset.contains(&format!("-c:v {codec}")));
            assert!(preset.contains(if codec.contains("nvenc") { "-rc constqp -qp 20" } else { "-crf 20" }));
            assert!(!preset.contains("scale="));
            assert!(!preset.contains("-r "));
            if codec.contains("nvenc") {
                assert!(preset.contains(NVENC_RECORDING));
                assert!(preset.contains("-g 180"));
                assert!(!preset.contains("-cq "));
                assert!(record_preset(codec, 20, 60).contains("-g 120"));
            } else {
                assert!(preset.contains(if is_hevc(codec) { "-preset fast -profile:v main" } else { "-preset veryfast -profile:v high" }));
                assert!(preset.contains("-g 180"));
                assert!(!preset.contains("-tune"));
            }
        }
    }
    #[test]
    #[ignore = "requires FFMPEG; set TEST_NVENC=1 to include NVIDIA encoders"]
    fn real_encoding_preserves_format_and_never_publishes_oversized_files() {
        let ffmpeg = PathBuf::from(std::env::var("FFMPEG").expect("set FFMPEG to ffmpeg executable"));
        let started = std::time::Instant::now();
        let mut live_update = false;
        let sink = if cfg!(windows) { "NUL" } else { "/dev/null" };
        run_progress(&ffmpeg, &[s("-re"), s("-f"), s("lavfi"), s("-i"), s("testsrc2=size=64x64:rate=30"), s("-t"), s("3"), s("-f"), s("null"), s(sink)], 3.0, &mut |p| {
            if p > 0.0 && p < 0.5 && started.elapsed().as_secs_f64() < 2.5 { live_update = true; }
        }).unwrap();
        assert!(live_update, "FFmpeg progress must arrive before the process exits");
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("source.mkv");
        let mut generate = ["-y", "-f", "lavfi", "-i", "testsrc2=size=960x540:rate=90", "-f", "lavfi", "-i", "sine=frequency=900:sample_rate=48000", "-t", "4", "-c:v", "ffv1", "-c:a", "pcm_s16le"].map(s).to_vec();
        generate.push(input.to_string_lossy().into_owned());
        run(&ffmpeg, &generate).unwrap();
        for (codec, _) in CODECS {
            if codec.contains("nvenc") && std::env::var_os("TEST_NVENC").is_none() { continue; }
            let output = dir.path().join(format!("{codec}.mp4"));
            let mut updates = vec![];
            let capture = dir.path().join(format!("capture-{codec}.mp4"));
            let mut args = vec![s("-y"), s("-i"), input.to_string_lossy().into_owned()];
            let (preset, _) = checked_record_preset(&ffmpeg, &super::super::RenderOptions {
                codec: codec.into(), crf: 20, fps: 90, width: 960, height: 540,
                ..super::super::RenderOptions::default()
            }, &mut |_| {}).unwrap();
            args.extend(preset.split_whitespace().map(s));
            args.extend([s("-c:a"), s("aac"), capture.to_string_lossy().into_owned()]);
            run(&ffmpeg, &args).unwrap();
            let result = encode_to_size_with_progress(&ffmpeg, &capture, &output, 0.5, codec, 192, &mut false, &mut |p| updates.push(p)).unwrap();
            assert_eq!(updates.last(), Some(&1.0));
            // Fast hardware encodes may first report at pass completion (0.9), before verification.
            assert!(updates.iter().any(|p| *p > 0.0 && *p < 1.0));
            assert!(updates.windows(2).all(|w| w[0] <= w[1]), "progress must not reset between passes or retries: {updates:?}");
            assert!(result.bytes > 0 && result.bytes <= 500_000);
            let mut probe = Command::new(ffprobe_exe(&ffmpeg));
            probe.args(["-v", "error", "-show_entries", "stream=codec_type,width,height,avg_frame_rate,sample_rate,channels", "-of", "json"]).arg(&output);
            let out = ProcessTree::new().unwrap().output(&mut probe).unwrap();
            let info: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
            let streams = info["streams"].as_array().unwrap();
            let video = streams.iter().find(|v| v["codec_type"] == "video").unwrap();
            assert_eq!((video["width"].as_u64(), video["height"].as_u64()), (Some(960), Some(540)));
            assert_eq!(video["avg_frame_rate"], "90/1");
            let audio = streams.iter().find(|v| v["codec_type"] == "audio").unwrap();
            assert_eq!(audio["sample_rate"], "48000");
            assert_eq!(audio["channels"], 2);
            let saved = std::fs::read(&output).unwrap();
            assert!(encode_to_size(&ffmpeg, &input, &output, 0.001, codec, 192).is_err());
            assert_eq!(std::fs::read(&output).unwrap(), saved);
            if codec.contains("nvenc") {
                let compatible_capture = dir.path().join(format!("compatible-{codec}.mp4"));
                let compatible_preset = record_preset(codec, 20, 90).replace(NVENC_RECORDING, NVENC_COMPATIBLE).replace("main -b_ref_mode middle", "main");
                let mut args = vec![s("-y"), s("-i"), input.to_string_lossy().into_owned()];
                args.extend(compatible_preset.split_whitespace().map(s));
                args.extend([s("-c:a"), s("aac"), compatible_capture.to_string_lossy().into_owned()]);
                run(&ffmpeg, &args).unwrap();
                let fitted = dir.path().join(format!("compatible-fitted-{codec}.mp4"));
                let result = encode_to_size_with_progress(&ffmpeg, &compatible_capture, &fitted, 0.5, codec, 192, &mut true, &mut |_| {}).unwrap();
                assert!(result.bytes > 0 && result.bytes <= 500_000);
                assert!((probe_duration_seconds(&ffmpeg, &fitted).unwrap() - 4.0).abs() < 0.1);
            }
            println!("{codec}: {} bytes, 960x540 at 90 FPS, stereo 48 kHz; failure preserves output", result.bytes);
        }
        let vfr = dir.path().join("vfr.mkv");
        run(&ffmpeg, &[s("-y"), s("-i"), input.to_string_lossy().into_owned(), s("-vf"), s("select=mod(n\\,5)"), s("-fps_mode"), s("passthrough"), s("-c:v"), s("ffv1"), s("-an"), vfr.to_string_lossy().into_owned()]).unwrap();
        let fitted = dir.path().join("vfr.mp4");
        encode_to_size(&ffmpeg, &vfr, &fitted, 0.5, "libx264", 192).unwrap();
        let timestamps = |path: &Path| -> Vec<f64> {
            let out = Command::new(ffprobe_exe(&ffmpeg)).args(["-v", "error", "-select_streams", "v:0", "-show_entries", "frame=best_effort_timestamp_time", "-of", "json"]).arg(path).output().unwrap();
            assert!(out.status.success());
            let info: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
            info["frames"].as_array().unwrap().iter().map(|f| f["best_effort_timestamp_time"].as_str().unwrap().parse().unwrap()).collect()
        };
        let before = timestamps(&vfr);
        let after = timestamps(&fitted);
        assert_eq!(before.len(), after.len(), "Size fitting must not duplicate or drop VFR frames");
        assert!(before.iter().zip(&after).all(|(a,b)| ((a-before[0])-(b-after[0])).abs() < 0.002), "Size fitting must preserve frame timing");
        let silent = dir.path().join("silent.mkv");
        run(&ffmpeg, &[s("-y"), s("-i"), input.to_string_lossy().into_owned(), s("-an"), s("-c:v"), s("copy"), silent.to_string_lossy().into_owned()]).unwrap();
        let output = dir.path().join("silent.mp4");
        assert!(encode_to_size(&ffmpeg, &silent, &output, 0.5, "libx264", 192).unwrap().bytes <= 500_000);
        assert!(!probe_media(&ffmpeg, &output).unwrap().1);
    }

}
