# Video encoding

Resolution, FPS and encoder follow the export options. Size fitting never resizes or changes frame rate.

- No limit: CRF/CQ 20 by default, CPU preset slow; NVIDIA p6, HQ, 20-frame lookahead, spatial AQ, full-resolution multipass.
- Limited: capture at CRF/CQ 16 or better to reduce loss before fitting. Already-small files are kept. CPU fitting uses two file passes; NVIDIA multipass is per-frame rate control with peaks allowed at twice the average bitrate.
- Audio defaults to AAC stereo, 192 kbit/s, 48 kHz, including size fitting.
- Limits use decimal MB (20 MB = 20,000,000 bytes). Reserve 2% initially for muxing and rate-control error, then subtract audio. Silent inputs reserve no audio.
- Check actual output bytes. Retry from the same source at a lower bitrate at most three times. Publish only an output within the limit; on failure preserve the source and any existing destination.

These quality defaults may take longer and need more temporary disk space than the previous CRF/CQ 23 defaults. Synthetic SSIM comparisons are regression evidence, not a guarantee for every CS2 scene.

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
