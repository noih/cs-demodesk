# Video encoding

Resolution, FPS and encoder follow the export options. Size fitting never resizes or changes frame rate.

- CPU capture defaults: x264 CRF 19 / veryfast / High, x265 CRF 20 / fast / Main; both use a two-second GOP and no tune override. Selecting NVIDIA in the export dialog uses CQP 20, P5/HQ, quarter-resolution multipass, lookahead disabled, spatial AQ enabled, two B-frames and a two-second GOP. HEVC uses Main profile and B-frame references; H.264 uses High profile. Resolution and FPS stay user-selected.
- NVIDIA capture parameters are tested with three synthetic frames at the requested resolution/FPS before launching HLAE. On error, retry once with P4, B-frames/references, AQ and multipass disabled. Reuse the selected compatibility mode across all clips and size fitting; HLAE records only once. Size fitting also permits one compatibility retry if its initial NVENC encode fails. If compatibility mode fails, report failure without switching codec, resolution or FPS.
- CQP has no size guarantee. Limited NVIDIA exports use VBR with the same preset/AQ settings, while CPU fitting uses two file passes with preset medium. Actual bytes are verified before publishing.
- Audio defaults to AAC stereo, 192 kbit/s, 48 kHz, including size fitting.
- Limits use decimal MB (20 MB = 20,000,000 bytes). Reserve 2% initially for muxing and rate-control error, then subtract audio. Silent inputs reserve no audio.
- Check actual output bytes. Retry from the same source at a lower bitrate at most three times. Publish only an output within the limit; on failure preserve the source and any existing destination.

The NVIDIA settings follow the [OBS recording baseline](https://obsproject.com/kb/advanced-recording-settings-guide), mapped to FFmpeg. The export dialog offers 30 and 60 FPS; existing jobs with other frame rates remain readable.

## Verification

Unit checks: `cargo test -p demodesk-core --lib render::encode`.

For real FFmpeg checks, set `FFMPEG` to the executable path (with ffprobe beside it), and run:

```powershell
cargo test -p demodesk-core --lib render::encode::tests::real_encoding -- --ignored --nocapture
```

Set `TEST_NVENC=1` to include NVIDIA H.264 and HEVC on a supported GPU. Synthetic temporary clips check the byte limit, dimensions, 90 FPS, stereo 48 kHz audio, silent inputs, and preservation on failure.

References: [NVIDIA guidance](https://docs.nvidia.com/video-technologies/video-codec-sdk/13.1/ffmpeg-with-nvidia-gpu/index.html), [FFmpeg options](https://www.ffmpeg.org/ffmpeg.html), [codec options](https://www.ffmpeg.org/ffmpeg-codecs.html).

## Export progress

Recording reports scheduled demo-time markers, weighted by clip duration. Encoding reads FFmpeg's `out_time_us` while the process runs and combines both CPU passes. Size retries use the remaining progress range; completed output is still verified before reporting success.

The UI labels overall progress as estimated: recording gets 70% of the work budget when size fitting is enabled, 90% otherwise. These phase weights are not time estimates. Elapsed time updates each second; there is no countdown. Progress stays below 100% until the job succeeds, and older job records can omit the optional progress field.
