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
    let mut cmd = Command::new(exe);
    cmd.args(args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped());
    let out = ProcessTree::new()?.output(&mut cmd)?;
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

pub fn probe_duration_seconds(ffmpeg_exe: &Path, file: &Path) -> Result<f64> {
    let mut cmd = Command::new(ffprobe_exe(ffmpeg_exe));
    cmd.args(["-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0"]).arg(file);
    let out = ProcessTree::new()?.output(&mut cmd)?;
    Ok(String::from_utf8_lossy(&out.stdout).trim().parse().unwrap_or(0.0))
}

pub fn mux_clip(ffmpeg_exe: &Path, clip: &ClipOutput, dest: &Path, audio_kbps: u32) -> Result<()> {
    let video = clip.video.as_ref().ok_or_else(|| anyhow!("clip {}: no video file", clip.index + 1))?;
    let mut args = vec![s("-y"), s("-i"), video.to_string_lossy().to_string()];
    if let Some(audio) = &clip.audio {
        args.extend([s("-i"), audio.to_string_lossy().to_string(), s("-map"), s("0:v:0"), s("-map"), s("1:a:0"), s("-c:v"), s("copy"), s("-c:a"), s("aac"), s("-b:a"), format!("{audio_kbps}k"), s("-shortest")]);
    } else {
        args.extend([s("-c:v"), s("copy")]);
    }
    args.push(dest.to_string_lossy().to_string());
    run(ffmpeg_exe, &args)
}

pub fn concat_clips(ffmpeg_exe: &Path, clips: &[PathBuf], dest: &Path) -> Result<()> {
    let list = dest.parent().unwrap().join("concat.txt");
    let body: Vec<String> = clips.iter().map(|c| format!("file '{}'", to_forward_slashes(c).replace('\'', "'\\''"))).collect();
    std::fs::write(&list, body.join("\n"))?;
    let r = run(ffmpeg_exe, &[s("-y"), s("-f"), s("concat"), s("-safe"), s("0"), s("-i"), list.to_string_lossy().to_string(), s("-c"), s("copy"), dest.to_string_lossy().to_string()]);
    let _ = std::fs::remove_file(&list);
    r
}

/// Bitrate in kbit/s that fits `max_size_mb` for `seconds` of video, leaving room for audio and container overhead.
pub fn video_bitrate_for_size(max_size_mb: f64, seconds: f64, audio_kbps: u32) -> u32 {
    let total_kbit = max_size_mb * 8.0 * 1024.0 * 0.95;
    let video_kbit = total_kbit - audio_kbps as f64 * seconds;
    ((video_kbit / seconds.max(0.1)).floor() as i64).max(200) as u32
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

/// Quality-targeted arguments HLAE hands to ffmpeg while recording (`crf` ≈ quality, lower = better).
/// NVENC ignores `-crf`; its equivalent is `-cq` with `-b:v 0`.
pub fn record_preset(codec: &str, crf: u32) -> String {
    let mut parts = vec![format!("-c:v {codec}"), s("-pix_fmt yuv420p")];
    if codec.contains("nvenc") {
        parts.push(format!("-rc vbr -cq {crf} -b:v 0 -preset p5"));
    } else {
        parts.push(format!("-crf {crf}"));
    }
    parts.extend(tag_args(codec));
    parts.join(" ")
}

/// Re-encode `input` so the output is at most `max_size_mb`. Two-pass x264/x265 hits
/// the budget reliably; NVENC uses VBR with a hard maxrate instead.
pub fn encode_to_size(ffmpeg_exe: &Path, input: &Path, output: &Path, max_size_mb: f64, codec: &str, audio_kbps: u32) -> Result<SizeResult> {
    let seconds = probe_duration_seconds(ffmpeg_exe, input)?;
    let bitrate = video_bitrate_for_size(max_size_mb, seconds, audio_kbps);
    let input_s = input.to_string_lossy().to_string();
    let output_s = output.to_string_lossy().to_string();
    let mut common = vec![s("-y"), s("-i"), input_s.clone(), s("-c:a"), s("aac"), s("-b:a"), format!("{audio_kbps}k"), s("-movflags"), s("+faststart"), s("-pix_fmt"), s("yuv420p")];
    common.extend(tag_args(codec));
    if codec.contains("nvenc") {
        let mut args = common.clone();
        args.extend([s("-c:v"), s(codec), s("-rc"), s("vbr"), s("-b:v"), format!("{bitrate}k"), s("-maxrate"), format!("{bitrate}k"), s("-bufsize"), format!("{}k", bitrate * 2), s("-preset"), s("p5"), output_s]);
        run(ffmpeg_exe, &args)?;
    } else {
        let tmp = tempfile::tempdir()?;
        let passlog = tmp.path().join("ffmpeg2pass").to_string_lossy().replace('\\', "/");
        let null_sink = if cfg!(windows) { "NUL" } else { "/dev/null" };
        let rate = [s("-c:v"), s(codec), s("-b:v"), format!("{bitrate}k"), s("-maxrate"), format!("{}k", (bitrate as f64 * 1.3) as u32), s("-bufsize"), format!("{}k", bitrate * 2), s("-preset"), s("medium")];
        // x264 reads -pass/-passlogfile; x265 wants them inside -x265-params.
        let pass_args = |n: u32| -> Vec<String> {
            if codec == "libx265" {
                vec![s("-x265-params"), format!("pass={n}:stats={passlog}.x265")]
            } else {
                vec![s("-pass"), n.to_string(), s("-passlogfile"), passlog.clone()]
            }
        };
        let mut pass1 = vec![s("-y"), s("-i"), input_s.clone()];
        pass1.extend(rate.iter().cloned());
        pass1.extend(pass_args(1));
        pass1.extend([s("-an"), s("-f"), s("mp4"), s(null_sink)]);
        run(ffmpeg_exe, &pass1)?;
        let mut pass2 = common.clone();
        pass2.extend(rate.iter().cloned());
        pass2.extend(pass_args(2));
        pass2.push(output_s);
        run(ffmpeg_exe, &pass2)?;
    }
    Ok(SizeResult { bitrate_kbps: bitrate, bytes: std::fs::metadata(output)?.len() })
}

pub fn bytes_to_mb(bytes: u64) -> f64 {
    (bytes as f64 / 1024.0 / 1024.0 * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_presets() {
        assert_eq!(record_preset("libx264", 23), "-c:v libx264 -pix_fmt yuv420p -crf 23");
        assert_eq!(record_preset("libx265", 23), "-c:v libx265 -pix_fmt yuv420p -crf 23 -tag:v hvc1");
        assert_eq!(record_preset("h264_nvenc", 23), "-c:v h264_nvenc -pix_fmt yuv420p -rc vbr -cq 23 -b:v 0 -preset p5");
        assert_eq!(record_preset("hevc_nvenc", 23), "-c:v hevc_nvenc -pix_fmt yuv420p -rc vbr -cq 23 -b:v 0 -preset p5 -tag:v hvc1");
    }
}
