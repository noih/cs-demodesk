//! Explicit, advisory smoke tests; results never control encoder availability.
use super::super::process::ProcessTree;
use serde::Serialize;
use std::{fs, path::Path, process::Command, time::Instant};

const CODECS: [&str; 6] = [
    "libx264",
    "libx265",
    "libsvtav1",
    "h264_nvenc",
    "hevc_nvenc",
    "av1_nvenc",
];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub checked_at: String,
    pub duration_ms: u128,
    pub results: Vec<EncodingCheck>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EncodingCheck {
    codec: &'static str,
    status: Status,
    duration_ms: u128,
    error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
enum Status {
    Passed,
    Failed,
    Skipped,
}

fn inputs(root: &Path) -> std::io::Result<()> {
    let mut frame = vec![80; 256 * 256];
    frame.extend(vec![128; 256 * 256 / 2]);
    fs::write(root.join("video.yuv"), frame.repeat(4))?;
    let length = 8000u32 * 4;
    let mut wav = Vec::from(*b"RIFF");
    wav.extend((36 + length).to_le_bytes());
    wav.extend(b"WAVEfmt ");
    wav.extend(16u32.to_le_bytes());
    wav.extend(1u16.to_le_bytes());
    wav.extend(2u16.to_le_bytes());
    wav.extend(48000u32.to_le_bytes());
    wav.extend(192000u32.to_le_bytes());
    wav.extend(4u16.to_le_bytes());
    wav.extend(16u16.to_le_bytes());
    wav.extend(b"data");
    wav.extend(length.to_le_bytes());
    wav.resize(44 + length as usize, 0);
    fs::write(root.join("audio.wav"), wav)
}

fn run(command: &mut Command) -> Result<String, (Status, String)> {
    match ProcessTree::new().and_then(|tree| tree.output(command)) {
        Ok(output) if output.status.success() => {
            Ok(String::from_utf8_lossy(&output.stdout).into_owned())
        }
        Ok(output) => Err((
            Status::Failed,
            format!(
                "Exit {:?}: {}{}",
                output.status.code(),
                String::from_utf8_lossy(&output.stderr),
                String::from_utf8_lossy(&output.stdout)
            ),
        )),
        Err(error) => Err((Status::Failed, error.to_string())),
    }
}

fn valid_streams(text: &str, codec: &str) -> bool {
    let expected = match codec {
        "libx264" | "h264_nvenc" => "h264",
        "libx265" | "hevc_nvenc" => "hevc",
        _ => "av1",
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
        return false;
    };
    let Some(streams) = value["streams"].as_array() else {
        return false;
    };
    streams.len() == 2
        && streams
            .iter()
            .any(|s| s["codec_name"] == expected && s["nb_frames"] == "4")
        && streams.iter().any(|s| {
            s["codec_name"] == "aac"
                && s["nb_frames"]
                    .as_str()
                    .and_then(|n| n.parse::<u32>().ok())
                    .is_some_and(|n| n > 0)
        })
}

fn encode(exe: &Path, root: &Path, codec: &str) -> Result<(), (Status, String)> {
    let output = root.join("output.mp4");
    let mut command = Command::new(exe);
    command
        .args([
            "-v",
            "error",
            "-nostdin",
            "-y",
            "-f",
            "rawvideo",
            "-pixel_format",
            "yuv420p",
            "-video_size",
            "256x256",
            "-framerate",
            "24",
            "-i",
        ])
        .arg(root.join("video.yuv"))
        .arg("-i")
        .arg(root.join("audio.wav"))
        .args([
            "-c:v",
            codec,
            "-threads",
            "2",
            "-pix_fmt",
            if codec.contains("av1") {
                "yuv420p10le"
            } else {
                "yuv420p"
            },
            "-c:a",
            "aac",
            "-b:a",
            "64k",
            "-t",
            "0.166667",
            "-movflags",
            "+faststart",
        ]);
    match codec {
        "libx264" => {
            command.args(["-preset", "ultrafast"]);
        }
        "libx265" => {
            command.args([
                "-preset",
                "ultrafast",
                "-x265-params",
                "pools=2:frame-threads=1:log-level=error",
            ]);
        }
        "libsvtav1" => {
            command.args(["-preset", "12", "-svtav1-params", "lp=2"]);
        }
        _ => {
            command.args(["-preset", "p1"]);
        }
    }
    run(command.arg(&output))?;
    let probe = exe.with_file_name(if cfg!(windows) {
        "ffprobe.exe"
    } else {
        "ffprobe"
    });
    let text = run(Command::new(probe)
        .args([
            "-v",
            "error",
            "-show_entries",
            "stream=codec_name,nb_frames",
            "-of",
            "json",
        ])
        .arg(output))?;
    if !valid_streams(&text, codec) {
        return Err((
            Status::Failed,
            "Output does not contain four expected video frames and AAC audio".into(),
        ));
    }
    Ok(())
}

pub fn check(exe: Option<&Path>) -> Report {
    let start = Instant::now();
    let work = tempfile::tempdir().and_then(|work| {
        inputs(work.path())?;
        Ok(work)
    });
    let mut results = Vec::new();
    for codec in &CODECS {
        let begin = Instant::now();
        let result = match (exe, &work) {
            (None, _) => Err((Status::Skipped, "FFmpeg executable not found".into())),
            (_, Err(error)) => Err((
                Status::Skipped,
                format!("Cannot prepare encoding diagnostic: {error}"),
            )),
            (Some(exe), Ok(work)) => encode(exe, work.path(), codec),
        };
        let (status, error) = match result {
            Ok(()) => (Status::Passed, None),
            Err((status, error)) => (status, Some(error)),
        };
        results.push(EncodingCheck {
            codec,
            status,
            duration_ms: begin.elapsed().as_millis(),
            error,
        });
    }
    drop(work);
    Report {
        checked_at: crate::store::now(),
        duration_ms: start.elapsed().as_millis(),
        results,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_requires_expected_video_frames_and_audio() {
        let good = r#"{"streams":[{"codec_name":"h264","nb_frames":"4"},{"codec_name":"aac","nb_frames":"9"}]}"#;
        assert!(valid_streams(good, "libx264"));
        assert!(!valid_streams(good, "hevc_nvenc"));
        assert!(!valid_streams(&good.replace("\"4\"", "\"0\""), "libx264"));
        assert!(!valid_streams(&good.replace("\"9\"", "\"0\""), "libx264"));
        assert!(!valid_streams("{}", "libx264"));
        let work = tempfile::tempdir().unwrap();
        inputs(work.path()).unwrap();
        assert_eq!(
            fs::metadata(work.path().join("video.yuv")).unwrap().len(),
            393216
        );
        assert_eq!(
            fs::metadata(work.path().join("audio.wav")).unwrap().len(),
            32044
        );
    }

    #[test]
    fn missing_tools_are_not_successes() {
        let report = check(None);
        assert_eq!(report.results.len(), 6);
        assert!(report.results.iter().all(|r| r.status == Status::Skipped));
        let report = check(Some(Path::new("missing-encoding-test-executable")));
        assert!(report.results.iter().all(|r| r.status == Status::Failed));
    }

    #[test]
    #[ignore = "requires DEMODESK_TEST_FFMPEG; exercises installed CPU and NVIDIA encoders"]
    fn installed_encoders_report_results() {
        let exe = std::env::var_os("DEMODESK_TEST_FFMPEG").expect("FFmpeg path");
        let report = check(Some(Path::new(&exe)));
        println!("{}", serde_json::to_string(&report).unwrap());
        assert_eq!(report.results.len(), 6);
        assert!(report.results[..3]
            .iter()
            .all(|r| r.status == Status::Passed));
    }
}
