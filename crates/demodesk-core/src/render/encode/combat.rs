use super::*;
use anyhow::ensure;

struct Video {
    frames: u64,
    fps: u32,
    width: u64,
    height: u64,
    constant_rate: bool,
}

fn video(ffmpeg: &Path, input: &Path, control: &mut Control<'_>) -> Result<Option<Video>> {
    let mut cmd = Command::new(ffprobe_exe(ffmpeg));
    cmd.args([
        "-v",
        "error",
        "-select_streams",
        "v:0",
        "-show_entries",
        "stream=nb_frames,r_frame_rate,avg_frame_rate,duration,width,height",
        "-of",
        "json",
    ])
    .arg(input);
    let out = monitored_output(&mut cmd, 1.0, &mut |_| {}, control, None)?;
    ensure!(out.status.success(), "cannot probe combat video");
    let info: serde_json::Value = serde_json::from_slice(&out.stdout)?;
    let v = &info["streams"][0];
    let Some(rate) = v["r_frame_rate"].as_str() else {
        return Ok(None);
    };
    let Some((n, d)) = rate.split_once('/') else {
        return Ok(None);
    };
    let fps = n.parse::<f64>().unwrap_or(0.0) / d.parse::<f64>().unwrap_or(0.0);
    let frames = v["nb_frames"]
        .as_str()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);
    let duration = v["duration"]
        .as_str()
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.0);
    if !fps.is_finite()
        || !(1.0..=240.0).contains(&fps)
        || fps.fract() != 0.0
        || frames == 0
        || !duration.is_finite()
        || duration <= 0.0
    {
        return Ok(None);
    }
    let constant_rate = v["avg_frame_rate"].as_str() == Some(rate)
        && (duration - frames as f64 / fps).abs() <= 0.001;
    Ok(Some(Video {
        frames: if constant_rate {
            frames
        } else {
            (duration * fps).round() as u64
        },
        fps: fps as u32,
        width: v["width"].as_u64().unwrap_or(0),
        height: v["height"].as_u64().unwrap_or(0),
        constant_rate,
    }))
}

fn segments(windows: &[[f64; 2]], frames: u64, fps: u32) -> Vec<(u64, u64, f64)> {
    let mut spans: Vec<_> = windows
        .iter()
        .filter(|w| w[0].is_finite() && w[1].is_finite() && w[1] > w[0])
        .map(|w| {
            (
                (w[0].max(0.0) * fps as f64).floor().min(frames as f64) as u64,
                (w[1].max(0.0) * fps as f64).ceil().min(frames as f64) as u64,
            )
        })
        .filter(|(a, b)| b > a)
        .collect();
    spans.sort_unstable();
    let mut merged: Vec<(u64, u64)> = Vec::new();
    for (a, b) in spans {
        if let Some(last) = merged.last_mut() {
            if a <= last.1 {
                last.1 = last.1.max(b);
                continue;
            }
        }
        merged.push((a, b));
    }
    let combat: u64 = merged.iter().map(|(a, b)| b - a).sum();
    if combat == 0 || combat == frames {
        return vec![];
    }
    let normal = 1.0 / (1.0 + combat as f64 / frames as f64);
    let mut result = Vec::new();
    let mut pos = 0;
    for (a, b) in merged {
        if a > pos {
            result.push((pos, a - pos, normal));
        }
        result.push((a, b - a, normal * 2.0));
        pos = b;
    }
    if pos < frames {
        result.push((pos, frames - pos, normal));
    }
    result
}

pub(super) fn encode(
    ffmpeg: &Path,
    input: &Path,
    output: &Path,
    options: &SizeOptions<'_>,
    compatible: &mut bool,
    progress: &mut dyn FnMut(f64),
    control: &mut Control<'_>,
) -> Result<Option<SizeResult>> {
    ensure!(input != output, "input and output must be different files");
    ensure!(
        options.max_size_mb.is_finite() && options.max_size_mb > 0.0,
        "invalid file size limit"
    );
    ensure!(
        CODECS.iter().any(|(name, _)| *name == options.codec),
        "unsupported video encoder"
    );
    let Some(info) = video(ffmpeg, input, control)? else {
        return Ok(None);
    };
    let parts = segments(options.combat_windows, info.frames, info.fps);
    if parts.is_empty() {
        return Ok(None);
    }
    let seconds = info.frames as f64 / info.fps as f64;
    let (_, has_audio) = probe_media(ffmpeg, input, control)?;
    let audio = if has_audio { options.audio_kbps } else { 0 };
    let limit = (options.max_size_mb * 1_000_000.0).floor() as u64;
    let codec = options.codec;
    let mut reported = 0.0_f64;
    retry_stalled(control, |control| {
        let mut bitrate = video_bitrate_for_size(options.max_size_mb, seconds, audio);
        ensure!(
            bitrate > 0,
            "size limit is too small for this duration and audio bitrate"
        );
        let parent = output
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let dir = tempfile::Builder::new()
            .prefix("combat-")
            .tempdir_in(parent)?;
        let temp = tempfile::Builder::new()
            .prefix("encode-")
            .suffix(".mp4")
            .tempfile_in(parent)?
            .into_temp_path();
        let mut previous = None;
        for attempt in 0..4 {
            control.check_cancelled()?;
            (control.log)(format!("combat quality priority: {} segments, 2:1 allocation, attempt {}/4 at {bitrate} kbps",parts.len(),attempt+1));
            let reuse_analysis = codec == "libsvtav1" && attempt > 0;
            if reuse_analysis {
                (control.log)("combat quality priority: reusing AV1 first-pass analysis".into());
            }
            let start = 1.0 - 0.1_f64.powi(attempt);
            let span = 0.9 * 0.1_f64.powi(attempt);
            let mut concat = String::new();
            for (index, &(first, count, weight)) in parts.iter().enumerate() {
                let rate = (bitrate as f64 * weight).floor().max(1.0) as u64;
                let path = dir.path().join(format!("part-{index}.mp4"));
                let part_seconds = count as f64 / info.fps as f64;
                let mut common = vec![
                    s("-y"),
                    s("-ss"),
                    format!("{:.9}", first as f64 / info.fps as f64),
                    s("-i"),
                    input.to_string_lossy().into_owned(),
                    s("-map"),
                    s("0:v:0"),
                    s("-an"),
                    s("-frames:v"),
                    count.to_string(),
                    s("-c:v"),
                    s(codec),
                    s("-pix_fmt"),
                    s(pixel_format(codec)),
                    s("-fps_mode"),
                    s("passthrough"),
                    s("-b:v"),
                    format!("{rate}k"),
                ];
                if codec != "libsvtav1" {
                    common.extend([s("-bufsize"), format!("{}k", rate * 2)]);
                }
                let mut report = |p: f64| {
                    reported = reported.max(
                        start
                            + span * 0.95 * (first as f64 + count as f64 * p) / info.frames as f64,
                    );
                    progress(reported);
                };
                if !info.constant_rate {
                    // Audio padding at joined clip boundaries can leave small gaps in video timestamps.
                    common.extend([s("-vf"), format!("fps={}", info.fps)]);
                }
                if codec.contains("nvenc") {
                    with_nvenc_fallback(compatible, |fallback| {
                        let mut args = common.clone();
                        args.extend([s("-rc"), s("vbr")]);
                        args.extend(
                            if fallback {
                                NVENC_COMPATIBLE
                            } else {
                                NVENC_SIZE
                            }
                            .split_whitespace()
                            .map(s),
                        );
                        args.extend([
                            s("-maxrate"),
                            format!(
                                "{}k",
                                rate * if (is_hevc(codec) || is_av1(codec)) && !fallback {
                                    2
                                } else {
                                    1
                                }
                            ),
                            s("-bf"),
                            s(if is_av1(codec) && !fallback { "4" } else { "0" }),
                            s("-spatial-aq"),
                            s("0"),
                            s("-temporal-aq"),
                            s(if is_hevc(codec) || fallback { "0" } else { "1" }),
                            s("-force_key_frames"),
                            s("expr:gte(t,n_forced*2)"),
                        ]);
                        if !is_av1(codec) {
                            args.extend([
                                s("-profile:v"),
                                s(if is_hevc(codec) { "main" } else { "high" }),
                            ]);
                        }
                        if !fallback {
                            args.extend([
                                s("-b_ref_mode"),
                                s(if is_av1(codec) { "middle" } else { "disabled" }),
                            ]);
                        }
                        args.extend(tag_args(codec));
                        args.push(path.to_string_lossy().into_owned());
                        run_progress(ffmpeg, &args, part_seconds, &mut report, control)
                    })?;
                } else {
                    common.extend([s("-preset"), s(cpu_size_preset(codec))]);
                    if codec == "libsvtav1" {
                        common.extend([s("-svtav1-params"), s(AV1_PARAMS)]);
                    } else {
                        common.extend([s("-maxrate"), format!("{}k", rate * 2)]);
                    }
                    if codec == "libx264" {
                        common.extend([s("-refs"), s("4")]);
                    }
                    let stats = dir
                        .path()
                        .join(format!("pass-{index}"))
                        .to_string_lossy()
                        .replace('\\', "/");
                    for pass in 1..=2 {
                        if pass == 1 && reuse_analysis {
                            continue;
                        }
                        let mut args = common.clone();
                        if is_hevc(codec) {
                            args.extend([
                                s("-x265-params"),
                                format!(
                                    "pass={pass}:stats={}:pools=8:cutree=0",
                                    stats.replace(':', "\\:")
                                ),
                            ]);
                        } else {
                            args.extend([
                                s("-pass"),
                                pass.to_string(),
                                s("-passlogfile"),
                                stats.clone(),
                            ]);
                        }
                        if pass == 1 {
                            args.extend([
                                s("-f"),
                                s("null"),
                                s(if cfg!(windows) { "NUL" } else { "/dev/null" }),
                            ]);
                        } else {
                            args.extend(tag_args(codec));
                            args.push(path.to_string_lossy().into_owned());
                        }
                        run_progress(
                            ffmpeg,
                            &args,
                            part_seconds,
                            &mut |p| report((pass as f64 - 1.0 + p) / 2.0),
                            control,
                        )?;
                    }
                }
                concat.push_str(&format!(
                    "file 'part-{index}.mp4'\nduration {part_seconds:.9}\n"
                ));
            }
            let list = dir.path().join("concat.txt");
            std::fs::write(&list, concat)?;
            let mut args = vec![
                s("-y"),
                s("-f"),
                s("concat"),
                s("-safe"),
                s("0"),
                s("-i"),
                list.to_string_lossy().into_owned(),
                s("-i"),
                input.to_string_lossy().into_owned(),
                s("-map"),
                s("0:v:0"),
                s("-c:v"),
                s("copy"),
            ];
            if has_audio {
                args.extend([
                    s("-map"),
                    s("1:a:0"),
                    s("-c:a"),
                    s("aac"),
                    s("-b:a"),
                    format!("{audio}k"),
                    s("-ar"),
                    s("48000"),
                    s("-ac"),
                    s("2"),
                ]);
            }
            args.extend(tag_args(codec));
            args.extend([
                s("-movflags"),
                s("+faststart"),
                temp.to_string_lossy().into_owned(),
            ]);
            run_progress(
                ffmpeg,
                &args,
                seconds,
                &mut |p| {
                    reported = reported.max(start + span * (0.95 + 0.05 * p));
                    progress(reported);
                },
                control,
            )?;
            let bytes = std::fs::metadata(&temp)?.len();
            (control.log)(format!(
                "combat quality priority result: {bytes} bytes (limit {limit})"
            ));
            if bytes > 0 && bytes <= limit {
                let actual = video(ffmpeg, &temp, control)?
                    .ok_or_else(|| anyhow!("combat output timing invalid"))?;
                ensure!(
                    actual.constant_rate
                        && (actual.frames, actual.fps, actual.width, actual.height)
                            == (info.frames, info.fps, info.width, info.height),
                    "combat output frame count or format mismatch"
                );
                ensure!(
                    probe_media(ffmpeg, &temp, control)?.1 == has_audio,
                    "combat output audio mismatch"
                );
                control.check_cancelled()?;
                temp.persist(output)?;
                progress(1.0);
                return Ok(Some(SizeResult {
                    bitrate_kbps: bitrate,
                    bytes,
                }));
            }
            let audio_bytes = audio as f64 * 1000.0 * seconds / 8.0;
            let ratio = (((limit as f64 - audio_bytes) / (bytes as f64 - audio_bytes).max(1.0))
                * 0.97)
                .clamp(0.0, 0.95);
            let mut next = (bitrate as f64 * ratio).floor() as u32;
            // Short segments have fixed overhead; use measured slope when proportional retries converge slowly.
            if let Some((old_rate, old_bytes)) = previous {
                if old_rate > bitrate && old_bytes > bytes {
                    let slope = (old_bytes - bytes) as f64 / f64::from(old_rate - bitrate);
                    let fitted = (f64::from(bitrate)
                        - (bytes as f64 - limit as f64 * 0.97) / slope)
                        .max(f64::from(bitrate) * 0.5);
                    next = next.min(fitted.floor() as u32);
                }
            }
            previous = Some((bitrate, bytes));
            bitrate = next;
            if bitrate == 0 {
                break;
            }
        }
        Err(anyhow!(
            "combat quality priority could not meet the size limit; source retained"
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires FFMPEG; set TEST_NVENC=1 for GPU checks"]
    fn real_combat_encoding_preserves_timing_audio_and_cancelled_destination() {
        let ffmpeg = PathBuf::from(std::env::var("FFMPEG").expect("FFMPEG"));
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("source.mp4");
        run(
            &ffmpeg,
            &[
                s("-y"),
                s("-f"),
                s("lavfi"),
                s("-i"),
                s("testsrc2=size=320x180:rate=60:duration=3"),
                s("-f"),
                s("lavfi"),
                s("-i"),
                s("sine=sample_rate=48000:duration=3"),
                s("-c:v"),
                s("libx264"),
                s("-crf"),
                s("0"),
                s("-c:a"),
                s("aac"),
                s("-ac"),
                s("2"),
                input.to_string_lossy().into_owned(),
            ],
        )
        .unwrap();
        let windows = [[0.317, 0.91], [1.117, 2.15]];
        for (codec, _) in CODECS {
            if codec.contains("nvenc") && std::env::var_os("TEST_NVENC").is_none() {
                continue;
            }
            for audio in [true, false] {
                let source = if audio {
                    input.clone()
                } else {
                    let silent = dir.path().join("silent.mp4");
                    run(
                        &ffmpeg,
                        &[
                            s("-y"),
                            s("-i"),
                            input.to_string_lossy().into_owned(),
                            s("-c:v"),
                            s("copy"),
                            s("-an"),
                            silent.to_string_lossy().into_owned(),
                        ],
                    )
                    .unwrap();
                    silent
                };
                let output = dir.path().join(format!("{codec}-{audio}.mp4"));
                let mut updates = Vec::new();
                let mut fit_logs = Vec::new();
                let result = encode_to_size_with_progress(
                    &ffmpeg,
                    &source,
                    &output,
                    SizeOptions {
                        combat_windows: &windows,
                        max_size_mb: 0.3,
                        codec,
                        audio_kbps: 96,
                    },
                    &mut false,
                    &mut |p| updates.push(p),
                    &mut Control::new(&AtomicBool::new(false), &mut |line| {
                        println!("{codec}: {line}");
                        fit_logs.push(line);
                    }),
                )
                .unwrap();
                let attempts = fit_logs
                    .iter()
                    .filter(|line| line.contains("2:1 allocation, attempt "))
                    .count();
                let reuses = fit_logs
                    .iter()
                    .filter(|line| line.contains("reusing AV1 first-pass analysis"))
                    .count();
                assert_eq!(
                    reuses,
                    if codec == "libsvtav1" {
                        attempts - 1
                    } else {
                        0
                    }
                );
                assert!(result.bytes <= 300_000);
                assert_eq!(updates.last(), Some(&1.0));
                assert!(updates.windows(2).all(|w| w[1] >= w[0]));
                let out = Command::new(ffprobe_exe(&ffmpeg))
                    .args([
                        "-v",
                        "error",
                        "-select_streams",
                        "v:0",
                        "-show_entries",
                        "frame=best_effort_timestamp_time",
                        "-of",
                        "json",
                    ])
                    .arg(&output)
                    .output()
                    .unwrap();
                assert!(out.status.success());
                let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
                let frames = json["frames"].as_array().unwrap();
                assert_eq!(frames.len(), 180);
                for (i, f) in frames.iter().enumerate() {
                    assert!(
                        (f["best_effort_timestamp_time"]
                            .as_str()
                            .unwrap()
                            .parse::<f64>()
                            .unwrap()
                            - i as f64 / 60.0)
                            .abs()
                            < 0.0001,
                        "{codec} frame {i}"
                    );
                }
                assert_eq!(
                    probe_media(
                        &ffmpeg,
                        &output,
                        &mut Control::new(&AtomicBool::new(false), &mut |_| {})
                    )
                    .unwrap()
                    .1,
                    audio
                );
            }
            println!("combat: {codec} timing, size, audio and silent input passed");
        }
        let joined = dir.path().join("joined.mp4");
        let fitted = dir.path().join("joined-fit.mp4");
        let padded = dir.path().join("padded.mp4");
        run(
            &ffmpeg,
            &[
                s("-y"),
                s("-i"),
                input.to_string_lossy().into_owned(),
                s("-c:v"),
                s("copy"),
                s("-af"),
                s("apad=pad_dur=0.04"),
                s("-c:a"),
                s("aac"),
                padded.to_string_lossy().into_owned(),
            ],
        )
        .unwrap();
        concat_clips(
            &ffmpeg,
            &[padded.clone(), padded],
            &joined,
            &mut Control::new(&AtomicBool::new(false), &mut |_| {}),
        )
        .unwrap();
        let source_info = video(
            &ffmpeg,
            &joined,
            &mut Control::new(&AtomicBool::new(false), &mut |_| {}),
        )
        .unwrap()
        .unwrap();
        assert!(
            !source_info.constant_rate,
            "fixture must exercise audio padding at a cut"
        );
        let mut log = Vec::new();
        encode_to_size_with_progress(
            &ffmpeg,
            &joined,
            &fitted,
            SizeOptions {
                combat_windows: &[[0.317, 0.91], [3.117, 5.15]],
                max_size_mb: 0.6,
                codec: "libx264",
                audio_kbps: 96,
            },
            &mut false,
            &mut |_| {},
            &mut Control::new(&AtomicBool::new(false), &mut |s| log.push(s)),
        )
        .unwrap();
        assert!(!log.iter().any(|s| s.contains("using normal size fitting")));
        let result = video(
            &ffmpeg,
            &fitted,
            &mut Control::new(&AtomicBool::new(false), &mut |_| {}),
        )
        .unwrap()
        .unwrap();
        assert!(result.constant_rate);
        assert_eq!(result.frames, source_info.frames);
        let destination = dir.path().join("keep.mp4");
        std::fs::write(&destination, b"existing destination").unwrap();
        let cancel = AtomicBool::new(false);
        let error = encode_to_size_with_progress(
            &ffmpeg,
            &input,
            &destination,
            SizeOptions {
                combat_windows: &windows,
                max_size_mb: 0.3,
                codec: "libx264",
                audio_kbps: 96,
            },
            &mut false,
            &mut |p| {
                if p > 0.0 {
                    cancel.store(true, Ordering::Relaxed);
                }
            },
            &mut Control::new(&cancel, &mut |_| {}),
        )
        .unwrap_err();
        assert_eq!(error.to_string(), "cancelled");
        assert_eq!(
            std::fs::read(&destination).unwrap(),
            b"existing destination"
        );
        assert!(input.exists());
        assert!(!std::fs::read_dir(dir.path()).unwrap().any(|e| e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("combat-")));
    }
    #[test]
    fn overlap_and_invalid_windows_preserve_frame_and_bit_budget() {
        let parts = segments(
            &[[1.0, 3.0], [2.0, 4.0], [f64::NAN, 5.0], [-2.0, -1.0]],
            600,
            60,
        );
        assert_eq!(parts.iter().map(|p| p.1).sum::<u64>(), 600);
        assert!((parts.iter().map(|p| p.1 as f64 * p.2).sum::<f64>() - 600.0).abs() < 1e-8);
        assert!(parts.iter().all(|p| p.2 >= 0.5));
        assert_eq!(
            parts.iter().map(|p| (p.0, p.1)).collect::<Vec<_>>(),
            vec![(0, 60), (60, 180), (240, 360)]
        );
        assert!(segments(&[[0.0, 10.0]], 600, 60).is_empty());
        assert!(segments(&[], 600, 60).is_empty());
    }
}
