# Video encoding

Resolution, FPS and encoder follow the export options. Size fitting never resizes or changes frame rate.

**Scope clarified by the user on 2026-10-24:** optimize each of the existing four
choices independently: H.264 CPU, H.265 CPU, H.264 NVIDIA and H.265 NVIDIA.
Users should choose the encoder, not select tuning parameters. In a subsequent
message on the same date, the user asked to retain the acceptable AV1 results
for a future additional encoder option. Preserve its artifacts, settings and
subjective feedback separately; AV1 was not implemented at that stage and
must not substitute for any of the four encoders. The second AV1 grid batch was interrupted after the earlier
clarification; its partial outputs are excluded from comparisons. Future grids
compare parameter sets within one encoder, with a source reference. ROI remains
on hold.

**Scope extended by the user on 2026-10-25:** add AV1 as a default option using
the earlier preferred candidate, and test AV1 NVIDIA separately. The new default
is AV1 CPU (`libsvtav1`); existing explicitly saved codec choices are preserved.
Both AV1 encoders use the app-downloaded FFmpeg, which exposes `libsvtav1` and
`av1_nvenc`; no additional encoding package is required. NVIDIA still needs a
GPU/driver that can actually encode AV1, verified before starting the game.

- AV1 CPU capture: CRF 20, preset 8, 10-bit, `tune=0:enable-tf=1:lp=6`, two-second
  GOP. Size fitting: preset 3, same pixel format and SVT parameters, two file
  passes with FFmpeg `-pass`/`-passlogfile`. This retains the user-preferred tile 2
  configuration. Unlike x264/x265, no peak/buffer flags are added to SVT VBR.
  After an oversize result, only SVT reuses that job's completed first-pass
  analysis and reruns pass two at the adjusted bitrate. This applies both with
  and without combat priority; the size check still gates publication.
- AV1 NVIDIA capture: CQP 20, P5/HQ, 10-bit (`p010le` input), the existing capture
  lookahead/AQ/multipass policy. Size fitting: P7/HQ, VBR, full-resolution
  multipass, lookahead 32, temporal AQ on, spatial AQ off, four B-frames with
  middle references, peak/buffer twice target. No H.264/HEVC profile option is
  passed to AV1. Compatibility mode stays AV1 and disables the advanced settings.
- Both support combat-priority temporal allocation with the same total size cap.
  Unlimited/already-small files retain the quality-targeted capture. The slower
  preset-3 two-pass policy only applies when size fitting is required.
- The UI retains all four H.264/H.265 choices and briefly notes that AV1
  takes longer to encode but offers better quality. No automatic codec
  substitution or duplicate H.264 output is introduced.

The tuning baseline comes from the user's own completed comparisons, not inferred
codecs on Allstar/Medal/Discord/YouTube. Parameter references:
[FFmpeg SVT-AV1 wrapper](https://ffmpeg.org/ffmpeg-codecs.html#libsvtav1),
[SVT-AV1 parameters](https://gitlab.com/AOMediaCodec/SVT-AV1/-/blob/master/Docs/Parameters.md).

### AV1 integration measurements (2026-10-25)

Full 41-second source, scaled once to lossless 1080p 10-bit, same Phase7 event
windows and decimal 20 MB limit; the two encoders ran sequentially after all
other encoding tests finished. Times include probes, all size retries and final
publication checks, but exclude capture, reference preparation and subsequent
decode/VMAF/playback validation.

| Candidate | Bytes | Encoding seconds | Attempts | Full-frame VMAF mean / min | Enemy edge error | Enemy temporal residual |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Prior preferred AV1 CPU, no combat allocation | 19,549,515 | Not rerun | — | 91.345 / 76.500 | 9.3752 | 4.2461 |
| AV1 CPU preset 3, combat priority | 18,653,873 | 200.12 | 1 | 89.462 / 61.741 | 8.2172 | 3.8517 |
| AV1 NVIDIA P7, combat priority | 17,623,273 | 70.02 | 3 | 80.175 / 47.307 | 11.9970 | 5.0158 |

The crop metrics use the same newly prepared reference for all three rows, so
their absolute values differ slightly from earlier direct-scaling experiments.
Lower errors are better: CPU combat allocation reduced enemy edge error 12.35%
and temporal residual 9.29% versus the earlier preferred CPU file. Its whole-frame
VMAF fell, particularly in the weakest frame: foreground allocation trades off
other intervals rather than improving every frame. These diagnostics do not
establish subjective acceptance or prove flicker is eliminated.

NVIDIA was about 2.86x faster including its retries, but retained less detail and
undershot the cap more. Its first two attempts were 21,942,780 and 20,145,869 bytes;
only the third was published. This initial NVIDIA option is not a quality match
for SVT-AV1 and is not the default. No claim is made that AV1 NVIDIA exceeds a
VMAF target of 85, or that either setting is optimal for every demo.

Both new files passed 2,458-frame 1080p60 10-bit AV1, stereo 48 kHz AAC, exact
source audio duration (41.006458 s), full decode and frame timestamp checks
(maximum error < 0.000001 s). Chrome played each and sought to 10, 12 and 39 s.
Full-decode versus seeked samples matched; sampled frame-content comparisons
around 10/12 s found no temporal offset. This does not validate every recipient,
mobile app or the installed Tauri WebView runtime.

Artifacts: `out/quality-diag/av1-production-libsvtav1.mp4`,
`av1-production-av1_nvenc.mp4`, `av1-production-comparison.csv`, and
`av1-production-preview.mp4`. The five-second preview covers source 8.5–13.5 s:
1 reference, 2 prior CPU, 3 CPU combat priority, 4 NVIDIA combat priority. It is a
cropped H.264 comparison export, not a 20 MB delivery file or a codec substitution.
Reproduce using `av1_production_bench.py`, `av1_production_review.py`, and
`av1_playback_test.mjs` in that directory. The lossless test input is temporary.

Validation after adding both codecs: 317 core tests, frontend build and UI suite
passed. The real ordinary-encoding and combat-encoding tests passed with all six
encoders, including capture preflight, compatibility mode, audio/silent inputs,
size limits, frame timing and cancellation/output preservation. No new dependency
was added; no installer was packaged. The earlier all-target Clippy baseline
still contains unrelated warnings and was not expanded into a cleanup task.

### AV1 CPU speed/quality sweep (2026-10-25)

Following the user's request to shorten processing time, vary only the SVT speed
preset (3/4/5/6). Keep 1080p60, 10-bit, two passes, `tune=0:enable-tf=1:lp=6`,
the same 11 combat-allocation segments, audio, starting bitrate and 20 MB cap.
Re-run preset 3 as the control; all four full clips run sequentially before any
metrics/preview work. Input preparation and post-encode validation are excluded
from timing. Each candidate needed one attempt and passed full decode, 2,458
frames, exact frame timestamps and stereo 48 kHz AAC validation.

| Preview tile | Preset | Bytes | Seconds | Time saved | VMAF mean / min | Enemy edge error | Temporal residual |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 2 | 3, current | 18,652,502 | 201.17 | — | 89.496 / 62.022 | 8.2360 | 3.8432 |
| 3 | 4 | 18,754,228 | 117.15 | 41.8% | 87.995 / 61.487 | 8.9429 | 4.0708 |
| 4 | 5 | 18,748,056 | 83.40 | 58.5% | 87.035 / 60.979 | 9.2070 | 4.1690 |
| 5 | 6 | 19,052,177 | 67.47 | 66.5% | 85.961 / 51.789 | 9.5559 | 4.3273 |

Preset 3 spent 25.25 s in pass one and 174.97 s in pass two; preset 4 spent
26.64 / 89.51 s, preset 5 24.88 / 57.62 s, and preset 6 24.07 / 42.49 s.
Removing pass one would not halve the current runtime and would change the
rate-allocation behavior. Parallelism is already level 6 (the maximum), not six
worker threads; see [SVT parallelism levels](https://gitlab.com/AOMediaCodec/SVT-AV1/-/merge_requests/2315).

The user rejected all three faster presets and explicitly retained tile 2
(preset 3). Do not promote presets 4/5/6: full-frame averages above 85 do not
establish that moving-enemy quality is acceptable. These are single-run,
single-demo measurements, not portable runtime guarantees. Further speed tests
must preserve preset 3 and verify quality before changing production settings.

Follow-up execution-only experiments retained preset 3, two passes, 1080p60,
10-bit, temporal filtering and the same segment bitrate allocation. On this
Ryzen 7 9800X3D (8 visible logical processors), automatic `lp=0` selected level 4
instead of the forced level 6. Timings exclude input preparation and validation.

| Execution | Bytes | Seconds | Time saved | Enemy edge error | Temporal residual |
| --- | ---: | ---: | ---: | ---: | ---: |
| Current serial, LP 6 | 18,652,502 | 201.17 | — | 8.2360 | 3.8432 |
| Two segments concurrently, LP 6 each | 18,622,693 | 183.98 | 8.5% | 8.2446 | 3.8450 |
| Serial, automatic LP | 18,695,687 | 191.11 | 5.0% | 8.1410 | 3.8095 |

Both passed full decode, 2,458-frame timestamp checks, 1080p60/10-bit AV1 and
48 kHz stereo AAC checks, and the decimal 20 MB cap. Crop metrics use the same
8.5–13.5 s region as the preset sweep. Similar metrics do not prove identical
perceived quality: the encoded outputs differ. These single runs show modest
potential savings, not a demonstrated large speedup or a portable guarantee.
Production remains serial LP 6 / preset 3; no concurrency code or lower preset
was introduced. No fresh whole-frame VMAF run was made for these two candidates.

Reproduction scripts: `out/quality-diag/av1_parallel_bench.py`,
`av1_auto_bench.py`, and `av1_execution_review.py` in the same directory.
Results: `av1-execution-review.json` / `av1-execution-comparison.csv`.
The 2.8 s `av1-execution-preview.mp4` shows reference/current/two-workers/auto-LP
as tiles 1/2/3/4, using the same visible-enemy intervals as the preset preview.

The user retained the original execution settings and tile 2 quality. A further
decoder-side investigation left all production parameters unchanged:

- Input-side `-threads 2`, same 41 s intermediate and complete 20 MB pipeline:
  199.65 s versus 201.17 s (0.8% difference), 18,666,056 bytes. Pass one took
  31.66 s and pass two 167.07 s. Full decode, timestamps, all 2,458 frames and
  audio checks passed. This difference does not establish a useful speedup.
- Software versus CUDA decoding of the original HEVC source: all 2,458 decoded
  frames matched by frame MD5 after the same CPU Lanczos/10-bit conversion.
  One/two-thread decoding of the intermediate also matched its own baseline.
  The directly scaled original does not match the existing intermediate hashes;
  these are separate controls, not interchangeable benchmark inputs.
- For a controlled 593-frame combat segment from the original, software decode
  plus SVT preset 3 two-pass encoding took 46.30 s; CUDA decode plus the identical
  scale/encoder settings took 44.44 s (4.0%). Pass two alone was 40.48 versus
  40.03 s. This is a segment experiment, not a full-clip 20 MB validation, and
  neither output replaces tile 2. Decoding on the GPU does not remove the main
  SVT search cost. No AV1 NVENC substitution was made.
- SVT already selects `avx512icl` on this machine. The application directly
  processes its recorded input; the lossless intermediate belongs to these
  experiments, so eliminating its preparation is not a production speedup.

No decoder setting was promoted. All timings are single runs, and frame-MD5
generation timings in `av1-decode-probe.json` include hashing, so they must not
be presented as pure decoder throughput. Scripts/results live in
`out/quality-diag/av1_decode_probe.py`, `av1_decode2_bench.py`,
`av1_hwdecode_bench.py`, `av1-decode2-results.json`, and
`av1-hwdecode-results.json`. Reusing first-pass analysis during size retries is
still untested; this reference clip succeeds on the first attempt, so such a
cache would not shorten its current encode. The subsequent controlled retry
experiment below tests this separately.

### AV1 retry analysis reuse experiment (2026-10-25)

The user requested a test while retaining the original settings and tile 2
quality. Production code and defaults were not changed. The same 41 s source,
11 combat segments, preset 3, LP 6, tune 0, temporal filtering, 10-bit, 1080p60,
two-pass algorithm and AAC 192 kbps were retained. Only the experimental initial
rate was raised to 4,300 kbps to force an oversize result: 21,960,883 bytes in
213.766 s. Both retry branches then used the existing proportional adjustment
to 3,781 kbps and the same decimal 20,000,000-byte limit.

| Retry method | Retry seconds | First pass | Second pass | Total incl. shared initial attempt | Final bytes |
| --- | ---: | ---: | ---: | ---: | ---: |
| Regenerate analysis | 202.351 | 24.537 | 176.890 | 416.117 | 19,349,722 |
| Reuse initial analysis | 177.531 | 0 | 176.607 | 391.297 | 19,336,986 |

Measured saving: 24.820 s, 12.3% of the retry and 6.0% of the complete two-attempt
job. The initial attempt was performed once and shared by both branches; totals
are sums of measured stages, not two independently measured end-to-end runs.
Input preparation and final validation are excluded. A first-attempt success
receives no benefit from this optimization.

Whole-file analysis hashes initially differed. Four short control runs showed
differences even at the same bitrate. Parsing the stats established that all
2,458 individual frame records across all 11 segments were byte-identical
between the initial and fresh retry; only the trailing summary records differed.
The exact bundled SVT revision's
[second-pass initialization](https://gitlab.com/AOMediaCodec/SVT-AV1/-/blob/5ed9ffbcb/Source/Lib/Codec/pass2_strategy.c#L997)
zeros and recomputes that summary from the individual records. No stats files
were edited or normalized for the experiment, and pass two did not modify them.
This evidence is specific to SVT-AV1 v4.2.0-151-g5ed9ffbcb and these inputs; it
does not authorize reusing x264/x265 analysis or cross-job caches.

Both final files passed full decode, all 2,458 frame timestamps, 1080p60/10-bit
AV1, stereo 48 kHz AAC and the size limit. They are not pixel-identical (2,214
decoded frame hashes differ); do not describe the resulting files as lossless
copies of each other. The 8.5–13.5 s enemy crop measurements were:

| Metric (lower is better) | Regenerated | Reused |
| --- | ---: | ---: |
| Enemy edge error | 8.0958 | 8.0792 |
| Temporal residual | 3.8033 | 3.7965 |
| Around 00:10 edge error | 7.4132 | 7.4276 |
| Around 00:12 edge error | 9.5088 | 9.3613 |

These measurements show no material regression in the tested crop, but are not
a guarantee of perceptual equivalence on other footage. No fresh whole-frame
VMAF sweep or packaged-app test was performed, since this was an isolated
experiment. The original production settings remain in place.

Artifacts in `out/quality-diag/`: `av1_retry_cache_bench.py`,
`av1_retry_cache_quality.py`, `av1_retry_cache_preview.py`,
`av1-retry-cache-results.json`, `av1-retry-cache-comparison.json`,
`av1-retry-stats-controls.json`, `av1-retry-stats-validation.json`, and
`av1-retry-cache-quality.json`. `av1-retry-cache-preview.mp4` is a verified
168-frame / 2.8 s enemy-only comparison, regenerated analysis left and reused
analysis right. The oversize `av1-retry-initial.mp4` is a stress-test intermediate,
not a valid final export; `av1-retry-fresh.mp4` and `av1-retry-cached.mp4` both fit.

### AV1 retry analysis reuse integration (2026-10-25)

After the user accepted the small visible differences and requested integration,
both ordinary size fitting and combat-priority size fitting now retain SVT's
completed first-pass files across capacity retries. The first attempt still
runs both passes; subsequent oversize retries run pass two at the existing
adjusted bitrate. x264/x265 still regenerate analysis; NVIDIA paths do not use
this optimization. Preset 3, LP 6, tune 0, temporal filtering, 10-bit, resolution,
FPS, audio and the allocation/size-adjustment policies are unchanged.

Analysis belongs only to the current job's temporary directory. Cancellation,
failure and completion release it; a watchdog restart creates a new directory
and reruns analysis. No cross-job cache, UI option, dependency or public API was
added. The existing byte-limit check and temporary-file publication remain in
place: an oversized file is never published as the final output.

The actual `encode_size` example, using the integrated library, was run on the
same 2,458-frame 1080p60 input and 20 MB cap. Because that input normally fits
on the first attempt, a test-only FFmpeg launcher appended one valid MP4 `free`
box to the first completed temporary mux, raising it to 20,200,000 bytes. It
saved the unpadded output separately and did not alter encoded frames or codec
arguments. This deliberately exercises the real retry branch, not a claim of
natural overshoot for this sample.

- Normal unpadded first output: **18,650,396 bytes** at 3,635 kbps.
- The real capacity check rejected the injected 20,200,000-byte temporary file.
- Retry at the unchanged rate-adjustment result of 3,453 kbps published
  **17,750,152 bytes**, below the decimal 20,000,000-byte limit.
- Recorded FFmpeg calls: **11 first passes, 22 second passes, 2 muxes** across
  both attempts. The retry skipped all 11 first passes; their measured initial
  cost was **24.714 s**, consistent with the earlier paired experiment's saving.
- Application result timestamps: 200.5 s after the initial attempt, 372.9 s after
  the retry (172.4 s for the retry stage). FFmpeg child durations sum to 194.988 s
  initially and 169.748 s on retry; these exclude launcher/application overhead.
  This integration run does not contain an independently timed uncached retry.
- Both saved videos passed full decode, all frame timestamps, 2,458 frames,
  1920x1080 at 60 FPS, 10-bit AV1, stereo 48 kHz AAC and the 20 MB cap. Temporary
  combat/output files were absent after completion.

The test harness initially failed to decode the application's final Unicode
console line using Windows CP950 after the encode had already succeeded. The
harness now reads UTF-8 explicitly; validation resumed against the existing
output. Its result JSON intentionally leaves whole-process `seconds` null for
that run rather than fabricating a lost wall-clock measurement.

Reproduction artifacts: `out/quality-diag/av1_retry_probe.rs`,
`av1_reuse_production_check.py`, `av1_reuse_plain_check.py`,
`av1-reuse-production-results.json`, and `retry-production-probe/production.log`
and `calls.tsv`. The launcher is confined to the experiment output directory;
the application continues to use its normal downloaded FFmpeg.

The ordinary (no combat windows) entry point was also tested with a two-second
120-frame clip and the same one-time 20.2 MB padding injection. It published
17,675,508 bytes after one first pass and two second passes; full decode and all
source/output frame timestamps matched. The timestamp comparison excludes
codec-specific side-data fields, which naturally differ between H.264 input
and AV1 output. Results: `av1-reuse-plain-results.json`.

Validation after integration:

- Core library suite: 317 passed; 9 ignored integration tests.
- Existing ordinary/combat real-FFmpeg suites: both passed with all six encoders
  (`TEST_NVENC=1`), including size limits, format/audio, monotonic progress and
  preservation of existing output on failure. They now also check AV1 reuse
  logging/pass counts and exclude reuse for other encoders.
- New `real_av1_analysis_reuse_cancellation_preserves_files` test: passed for
  both ordinary and combat paths. A deliberately small cap triggers a real
  oversize retry; cancellation at analysis reuse preserves the input and an
  existing destination and leaves no output-directory temporary files.
- The actual 20 MB entry-point checks described above assert real FFmpeg pass
  invocation counts, independently of application log messages.

No UI behavior changed, so frontend tests were not repeated for this integration.
No desktop installer was rebuilt or installed.

The NVIDIA log was also inspected: its prior 70 s included three roughly 23 s
attempts. A static lower initial rate would be specific to this sample and could
waste the bitrate budget elsewhere, so no speculative global rate cut was added.
This CPU sweep addresses the default's main bottleneck with measured tradeoffs.

`out/quality-diag/av1-speed-preview.mp4` has 168 frames (2.8 seconds), using only
source intervals 8.8–9.5, 10.3–11.5 and 12.3–13.2 s to show the visible opponents.
Tile 1 is the reference; tiles 2–5 map to the table. It is a cropped H.264 review
artifact, not the full delivery file. The full AV1 files are `av1-speed-p3.mp4`
through `av1-speed-p6.mp4`. Frame-content samples at 10/12 s matched the source
with no temporal offset; seeking and full decoding agreed. The preview's labels
and alignment were inspected. All full-frame metrics use the original 1440p
source downscaled exactly as in prior experiments.

Reproduce with `av1_speed_bench.py`, `av1_speed_review.py` and
`global_vmaf.ps1 -Names @('av1-speed-p3','av1-speed-p4','av1-speed-p5','av1-speed-p6')`.
Results and per-pass times: `av1-speed-results.json`; merged diagnostics:
`av1-speed-comparison.csv`. Bench scripts and media stay under `out/quality-diag`.

- CPU capture defaults: x264 CRF 19 / veryfast / High, x265 CRF 20 / fast / Main; both use a two-second GOP and no tune override. Selecting NVIDIA in the export dialog uses CQP 20, P5/HQ, quarter-resolution multipass, lookahead disabled, spatial AQ enabled, two B-frames and a two-second GOP. HEVC uses Main profile and B-frame references; H.264 uses High profile. Resolution and FPS stay user-selected.
- NVIDIA capture parameters are tested with three synthetic frames at the requested resolution/FPS before launching HLAE. On error, retry once with P4, B-frames/references, AQ and multipass disabled. Reuse the selected compatibility mode across all clips and size fitting; HLAE records only once. Size fitting also permits one compatibility retry if its initial NVENC encode fails. If compatibility mode fails, report failure without switching codec, resolution or FPS.
- CQP has no size guarantee. Limited NVIDIA exports use VBR with P7/HQ, full-resolution multipass, 32-frame lookahead, temporal AQ instead of spatial AQ, four B-frames and B-frame references. HEVC allows peak bitrate twice the target average; H.264 keeps peak equal to average. The buffer is twice the target bitrate. Compatibility mode retains its P4 preset and peak equal to average.
- CPU fitting uses two file passes with peak and buffer twice the target bitrate: HEVC uses slow; H.264 uses veryslow with at most four reference frames. The reference limit keeps this 1080p60 H.264 sample at High Level 4.2 rather than raising it to Level 5.1; higher resolutions/frame rates still select their required level automatically. No tune or AQ override is applied. Actual bytes are verified before publishing.
- Audio defaults to AAC stereo, 192 kbit/s, 48 kHz, including size fitting.
- Limits use decimal MB (20 MB = 20,000,000 bytes). Reserve 2% initially for muxing and rate-control error, then subtract audio. Silent inputs reserve no audio.
- Check actual output bytes. Retry from the same source at a lower bitrate at most three times. Publish only an output within the limit; on failure preserve the source and any existing destination.

The NVIDIA capture settings follow the [OBS recording baseline](https://obsproject.com/kb/advanced-recording-settings-guide), mapped to FFmpeg. Offline compression uses a separate quality preset: spatial AQ trades detailed textures for smooth regions, whereas temporal AQ helps preserve detailed reference frames. The export dialog offers 30 and 60 FPS; existing jobs with other frame rates remain readable.

### CS2 quality comparison (2026-10-23)

RTX 4080, the user's 41.006-second round-six ace in `match730_003849578294051602860_0169636553_403.dem`. All comparisons use the same 2560x1440 HEVC CQP source from clip job `20261022T180751-8860`, Lanczos downsampling to 1920x1080, 60 FPS, 2,458 video frames and AAC 192 kbit/s. The original native-1080p export (`20261021T175144-a28a`) has a different HUD scale and is not a valid pixel-aligned metric reference. VMAF uses every fifth frame against the downsampled source; it is supporting evidence, not a universal perceptual ranking.

| Encoding parameters | Bytes | Mean VMAF |
| --- | ---: | ---: |
| Existing P5/spatial AQ, peak = average | 19,739,059 | 78.94 |
| Same, spatial AQ disabled | 19,734,263 | 81.00 |
| Same, temporal AQ instead | 19,739,089 | 81.65 |
| P7/lookahead/full multipass/4 B-frames/2x peak, spatial AQ | 19,472,025 | 80.15 |
| Same P7 settings, no AQ | 19,534,620 | 81.98 |
| Same P7 settings, temporal AQ (selected) | 19,488,678 | 82.37 |
| Same P7 temporal AQ, Main10 | 19,511,925 | 82.85 |
| AV1 NVENC P7/full multipass/lookahead/temporal AQ | 19,504,826 | 83.39 |
| Existing x265 medium, file-level two-pass, 1.3x peak | 19,602,384 | 81.42 |
| x265 slow, file-level two-pass, 2x peak | 19,565,449 | 86.12 |
| Selected HEVC settings through production size retry | 19,382,712 | 82.27 |
| Selected x265 slow settings through production path | 19,562,484 | 86.11 |

The P7 variants initially exceeded 20 MB and were re-encoded from the source with a lower average bitrate before comparison. Do not compare their initial oversized outputs against the baseline. Spatial AQ was the largest isolated contributor to lost texture in this sample; offline analysis settings added a smaller improvement. The 20 MB budget still leaves only about 3.6 Mbit/s for 1080p60 video, so rapid movement remains visibly lossy. Capture and size-fitting presets must stay separate. Broader maps and hardware were not calibrated by this experiment.

For this sample, the highest-scoring tested encode used `libx265 -preset slow -b:v 3631k -maxrate 7262k -bufsize 7262k -pix_fmt yuv420p`, with `-x265-params pass=1:stats=...:pools=8` then `pass=2` using the same stats. No tune override, default GOP, AAC 192 kbit/s on pass two. It took about 556 seconds across both passes; the production NVENC encode plus size retry took about 33 seconds. Timings include concurrent diagnostic work and are not isolated speed benchmarks. CPU slow retained more detail overall, but individual frames can still favor a different encoder. Main10 and AV1 remain comparison files, not automatic format substitutions. See [x265 presets](https://x265.readthedocs.io/en/master/presets.html) and [NVIDIA AQ guidance](https://docs.nvidia.com/video-technologies/video-codec-sdk/13.1/ffmpeg-with-nvidia-gpu/index.html#adaptive-quantization-aq).

The updated production x265 path leaves thread pools at the encoder default. A final run took 221 seconds, produced 19,562,484 bytes and scored 86.11. Both production outputs retain 2,458 frames at 1080p60, 41.006458 seconds including audio, and stereo AAC at 48 kHz. All ten encoding tests passed with `FFMPEG` and `TEST_NVENC=1`, including real codec checks and compatibility fallback. The changed Rust file passes rustfmt; the workspace-wide format check has pre-existing differences in unrelated files.

### H.264 follow-up, same source and 20 MB limit

Same sampling, scaling, frame rate and audio settings as above. Every candidate is a full-length encode below 20,000,000 bytes. NVIDIA candidates use the existing production bitrate-retry formula. P10 is the tenth percentile of sampled frame VMAF, not a playback smoothness measurement.

| H.264 parameters | Bytes | Mean VMAF | P10 |
| --- | ---: | ---: | ---: |
| NVIDIA original P5/spatial AQ | 19,724,745 | 69.36 | 59.34 |
| P7/lookahead/full multipass/4 B-frames/references, spatial AQ, 2x peak | 18,037,181 | 68.50 | 58.12 |
| Same P7, temporal AQ, 2x peak | 17,967,183 | 75.22 | 65.48 |
| Same P7, no AQ, 2x peak | 18,017,652 | 74.84 | 65.09 |
| Same P7, temporal AQ, 1.3x peak | 17,910,441 | 75.37 | 65.59 |
| Same P7, temporal AQ, peak = average (selected) | 19,694,968 | 77.35 | 65.18 |
| Same P7, no AQ, peak = average | 19,701,974 | 77.05 | 64.86 |
| CPU original medium, 1.3x peak | 19,795,765 | 75.46 | 70.46 |
| CPU slow, 2x peak, default references | 19,745,409 | 76.03 | 70.63 |
| CPU veryslow, 2x peak, default references | 19,668,496 | 77.53 | 72.02 |
| CPU slow, 2x peak, four references | 19,758,845 | 75.88 | 70.41 |
| CPU veryslow, 2x peak, four references (selected) | 19,703,202 | 77.05 | 71.87 |
| CPU slow, 2x peak, four references, AQ 2 | 19,771,481 | 75.54 | 70.27 |
| CPU slow, 2x peak, four references, AQ 3 | 19,777,106 | 74.69 | 69.62 |

The looser NVIDIA H.264 peak settings overshot the initial budget; the existing bounded retry then produced roughly 18 MB. Keeping peak equal to average used the budget more effectively and gave the best tested NVIDIA mean, although the looser peak had a slightly better P10. CPU veryslow with unrestricted references scored another 0.48 points but raised the 1080p60 bitstream to High Level 5.1. Four references preserved High Level 4.2, as did the selected NVIDIA output. Both selections were also inspected at 30 seconds (close enemy/gunfire) and 37.5 seconds (smoke, wall and barrel details). These choices balance measured quality and compatibility on this sample; they are not universal FPS optima.

CPU veryslow takes longer than medium. The candidate runs overlapped with other encodes and metrics, so their wall times should not be treated as isolated speed benchmarks. H.265 settings from the first experiment remain unchanged. Capture parameters, codec choice, resolution, frame rate, audio and compatibility fallback remain independently controlled.

### Final production outputs, all four encoders

| Encoder | Size-fitting settings | Bytes | Mean VMAF | P10 |
| --- | --- | ---: | ---: | ---: |
| H.264 CPU | veryslow, two file passes, four references, 2x peak | 19,697,924 | 77.04 | 71.93 |
| H.264 NVIDIA | P7/HQ, full multipass, lookahead 32, temporal AQ, four B-frames with references, 1x peak | 19,694,968 | 77.35 | 65.18 |
| H.265 CPU | slow, two file passes, 2x peak | 19,562,484 | 86.11 | 81.54 |
| H.265 NVIDIA | P7/HQ, full multipass, lookahead 32, temporal AQ, four B-frames with references, 2x peak | 19,382,712 | 82.27 | 74.31 |

H.264 production runs took approximately 90 seconds (CPU) and 12 seconds (NVIDIA), with build/test work overlapping. They were measured again against the same losslessly downsampled reference. H.265 scores reuse the already-verified production outputs, whose encoder settings did not change. All four files were probed again: 1920x1080, 60 FPS, 2,458 frames, 41.006458 seconds including stereo AAC at 48 kHz, and below 20,000,000 bytes. Both H.264 files remain High Level 4.2. Ten encoding tests passed with real FFmpeg and NVIDIA enabled; the regression now also asserts the requested codec and all 360 frames in its four-second 90 FPS fixture. This validates export logic, not a new recording session or an installed application build.

### Follow-up: an average VMAF target of 85

The threshold check evaluates **all 2,458 frames**, rather than every fifth frame as in the tables above. These scores supersede sampled scores when judging the 85 target; differences between the two methods are not encoding improvements. The source, Lanczos 1080p reference, 60 FPS and AAC 192 kbps audio remain the same.

| Encoder | Full-frame mean at the existing 20 MB settings | Tested budget reaching 85 | Actual bytes | Full-frame mean | P10 |
| --- | ---: | ---: | ---: | ---: | ---: |
| H.264 CPU | 78.12 | 30 MB | 29,436,481 | 87.66 | 83.66 |
| H.264 NVIDIA | 75.98 | 30 MB | 29,561,575 | 85.27 | 77.77 |
| H.265 CPU | 87.09 | 20 MB | 19,562,484 | 87.09 | 82.78 |
| H.265 NVIDIA | 81.66 | 25 MB | 24,293,747 | 85.62 | 79.76 |

The larger-budget files use the selected codec settings above. All four outputs were checked for 1920x1080, 60 FPS, 2,458 frames, stereo 48 kHz audio and their respective byte limits. Artifacts are in ignored `out/quality-diag`: `libx264-30mb.mp4`, `h264_nvenc-30mb.mp4`, `01-r6-noii-ace.quality.20mb.mp4`, and `hevc_nvenc-25mb.mp4`. Full-frame measurements are recorded in `target85-full-results.json` and the corresponding VMAF JSON logs.

Further 20 MB experiments did not establish settings that make all four codecs reach 85. CPU H.264 AQ disabled slightly raised the sampled mean but lowered P10; qcomp 0.8 reduced quality. NVIDIA constant-quality searches with a longer GOP did not improve the sampled average for either codec. HEVC UHQ with temporal filtering disabled and lookahead level 3 initially scored 85.23 when sampled, but only **82.57** over every frame (19,283,576 bytes). Applying UHQ to the existing bounded VBR workflow scored **81.73** (19,261,340 bytes), versus HQ's 81.66. These experiments were not adopted as defaults: they do not establish a worthwhile improvement, and UHQ adds compatibility and encoding-cost considerations.

For this clip, allowing up to 30 MB demonstrably makes all four **average** scores exceed 85. This is neither a minimum-budget search nor a guarantee for every frame or every FPS clip. The NVIDIA H.264 30 MB file still has a lowest frame score of 54.06. The application's 20 MB choice remains unchanged; this follow-up changed the experiment record, not encoding code or the installed application.

### Motion edges and distant enemies

The user reported blurred, flickering enemy outlines around 10 and 12 seconds despite sharper overall output. Synchronized crops of 8.5–13.5 seconds distinguish the 1440p source, its Lanczos 1080p downsample and size-fitted output. The user observed only slight softness from downsampling, with much stronger degradation after compression. UHQ was rejected by the user on a brief enemy-fire sequence near the beginning of this preview, despite some favorable numerical comparisons. Do not adopt UHQ on the strength of those numbers.

Diagnostics decode every frame of this five-second interval to grayscale. The central screen crop is 320x220 at (800,430) in 1080p coordinates. For three short windows (9.0–9.4, 10.6–11.2 and 12.3–12.8 seconds), the scripts measure absolute pixel error, horizontal-gradient error on reference edges, and `mean(abs(diff(distorted-reference, time)))`. The last quantity subtracts the reference's temporal change but still includes camera movement, background and displaced edges: it is **not a validated flicker score or an isolated tracked-enemy measurement**. Numerical reductions must not be presented as percentages of perceived flicker removed. The preview files use nearest-neighbor enlargement without sharpening, followed by x264 CRF 10 for convenient playback; they are viewing aids with an additional encode, not metric inputs.

Full-length, 1080p60, 20 MB experiments retained the same source and audio. HEVC NVENC with no B-frames, one-second keyframes, or AQ disabled did not consistently improve all three regions. CPU x265 `cutree=0` and `psy-rd=4` increased both edge and temporal-residual errors. `psy-rd=1` reduced temporal residual by approximately 3–4%, a small numerical change rather than proof of a perceptual fix. A 1440p NVENC 20 MB candidate did not improve the common-1080p diagnostic; this comparison does not establish its perceptual ranking on a native-1440p display.

A separate 50 MB NVENC control produced 48,140,980 bytes. Relative to the current NVENC 20 MB output, its local edge errors fell about 36% and temporal residuals fell 25–29% across the three windows. This supports bitrate pressure as a contributor, but does not prove that further tuning at 20 MB is impossible. The application's size limit was not changed.

#### Central region allocation experiment

An x264 experiment adds `addroi=x=iw/3:y=ih/3:w=iw/3:h=ih/3:qoffset=-0.12` after downsampling, keeping veryslow, four references, two passes, 3,631 kbps target, 2x peak/buffer and AAC 192 kbps. Both passes use the ROI. The fixed rectangle is the central third of each dimension, or one ninth of the image; it does not detect players. Six encoder threads are used in both the ROI encode and its dedicated no-ROI control to isolate the effect.

The ROI file is 19,685,317 bytes; its matching control is 19,694,570 bytes. Across all 300 frames of the central crop, edge error decreased from 14.17 to 10.26 (27.6%), and temporal residual from 5.92 to 4.70 (20.6%). Left and right crops of the same dimensions, at (160,480) and (1440,430), had approximately 2–4% higher errors. These are sample-specific numerical tradeoffs, not perceptual percentages. The user judged the ROI preview clearly improved but still insufficient.

Increasing only the ROI offset strength to `-0.20` produced 19,677,435 bytes. Central edge error fell to 8.23 (41.9% below the matching no-ROI control) and temporal residual to 4.03 (31.9% below control); side-region errors rose approximately 5–10%, depending on region and metric. A fixed central rectangle can sacrifice an enemy near the screen edge, and the sharp allocation boundary may be noticeable in other scenes. These are experimental candidates, not production defaults or a claim that all four encoders support this filter's metadata.

Applying the `-0.12` ROI to x265 slow with eight pool threads and both file passes produced 19,584,568 bytes. Compared with the previous `x265-slow.mp4` control using the same pool/preset/rate settings, central edge error fell from 12.41 to 8.85 (28.7%) and temporal residual from 5.22 to 4.11 (21.2%). Side-region errors increased approximately 2–4%. The stronger x264 candidate has slightly lower central diagnostic errors, while this x265 candidate retains more peripheral detail in the measured crops. Both retain all 2,458 frames, 1080p60 and stereo 48 kHz AAC below 20 MB.

A one-second HEVC NVENC probe with and without the `addroi` metadata produced identical decoded-frame MD5 (`7125ecae55b0cec3891f00cb94258691`) on this installed build. Thus adding this filter alone did not activate ROI in the tested NVENC path; do not silently apply the CPU recipe to NVIDIA and claim equivalent behavior. The [FFmpeg NVENC implementation inspected](https://github.com/FFmpeg/FFmpeg/blob/master/libavcodec/nvenc.c) also did not contain a regions-of-interest side-data handler. This does not claim that the underlying NVIDIA SDK or every future FFmpeg build lacks ROI support.

Reproduction scripts and artifacts are under ignored `out/quality-diag`: `motion_encode.ps1`, `motion_roi.ps1`, `motion_resolution.ps1`, `motion_metrics.py`, `roi_review.py`, and `motion_verify.py`; measurements are in `motion-metrics.json`, `roi-review.json`, and `motion-verified.json`. Verification checks complete frame count, resolution, 60 FPS, stereo 48 kHz AAC and actual size. No application code was changed for this investigation.

References: [x265 psycho-visual tradeoffs](https://x265.readthedocs.io/en/latest/cli.html#psycho-visual-options), [FFmpeg x265 ROI handling](https://www.ffmpeg.org/doxygen/7.0/libx265_8c_source.html), and [NVIDIA AQ behavior](https://docs.nvidia.com/video-technologies/video-codec-sdk/13.1/nvenc-video-encoder-api-prog-guide/index.html).

### Whole-frame alternatives to ROI

The user accepted that ROI helped but requested broader alternatives because of
peripheral quality and boundary concerns. Keep ROI as an experiment; it is not
enabled in the application. The following investigation preserves the full
41-second source, 1080p60, stereo AAC 192 kbps and a strict 20,000,000-byte limit,
without ROI or sharpening. This budget is about 3.9 Mbps including audio, so the
short duration alone does not imply an easy compression target.

Reproduction is in ignored `out/quality-diag/global_quality.ps1`, with completed
settings/sizes in `global-encode-results.jsonl`. `global_review.py` verifies full
frame count, format and byte limit, then measures 300-frame crops at 8.5 seconds
(enemy, left and right) and 35 seconds (late action including smoke/background).
Its edge and temporal residual errors are diagnostic differences against the
downsampled source, not perceptual flicker scores. `global_vmaf.ps1` compares all
2,458 frames after explicitly converting both inputs to 8-bit 1080p.

The controlled H.265 baseline is `x265-slow.mp4` (slow, eight pool threads, two
passes, 3,631 kbps, 2x peak and buffer). Removing the peak/buffer limit reduced
enemy-crop edge error only 0.4%; changing to 10-bit reduced it 1.5%. Neither is a
substantial solution for this sample. SVT-AV1 preset 4, tune 0, 10-bit VBR at the
same target, tested with temporal filtering both off and on, reduced that crop's
edge error about 21–22%, with improvements in both peripheral crops as well.
Those files are 19,978,368 and 19,985,906 bytes. Full-frame mean VMAF is 90.01 and
90.22, respectively, but their worst frames near 39.4 seconds are worse than the
H.265 baseline. A higher mean is not sufficient to accept the encoder.

Standard FFmpeg two-pass SVT-AV1 encoding, with temporal filtering off and the
other settings unchanged, produced 19,566,148 bytes. Compared with controlled
H.265 slow, enemy edge error decreased 17.4% and late-action edge error decreased
15.9%, while left/right edge errors decreased 16.3%/18.9%. Mean/minimum VMAF was
89.588940/75.305107, versus 87.098700/76.055554 for the controlled H.265 file.
Compared with single-pass AV1 TF-off, two passes improved the tail at a small cost
to the early enemy region; its mean fell from 90.01 but its minimum rose from
65.14. This is a candidate for subjective review, not proof that visible flicker
is solved. `global-balanced-preview.mp4` shows all four sources synchronized in
two five-second crops (8.5–13.5 and 35–40 seconds); its CRF-10 comparison export is
not itself a 20 MB deliverable.

H.265 `aq-mode=4:qg-size=8` produced 19,581,284 bytes, but increased enemy edge
error 3.5% and late-action edge error 6.8%; it is not recommended for this sample.
The slower-preset 10-bit candidate was interrupted after roughly eleven minutes
with only about a third of its first pass complete. It has no complete quality
result and is excluded from comparisons, rather than classified as a quality
failure. The completed slow 10-bit result above is a separate experiment.

H.265 `multi-pass-opt-distortion=1` completed at 19,581,259 bytes. Enemy edge
error improved only 0.5%, with temporal residual slightly worse and no consistent
gain in the other crops. Thus neither this allocation option nor edge AQ merits
a production change on the current evidence. All seven completed whole-frame
candidates passed frame-count, 1080p60, stereo 48 kHz AAC and 20 MB validation.
The interrupted slower candidate is excluded. In a later reply to the original
`global-av1-preview.mp4` question, the user judged both lower AV1 tiles better
than the H.265 baseline and preferred the bottom-right TF-on tile to the
bottom-left TF-off tile. That preview compares single-pass temporal-filter
settings; this feedback does not establish acceptance of the separate two-pass
candidate in `global-balanced-preview.mp4`.

On 2026-10-24, the corresponding two-pass TF-on candidate completed with all
other settings unchanged: `global-av1-twopass-tf1.mp4`, 19,554,637 bytes, 2,458
frames, 1080p60, 10-bit AV1 and stereo 48 kHz AAC. Full-frame VMAF mean/minimum
was 89.823340/76.879385. Enemy/late-action edge errors were 10.0546/10.0720,
about 1.9%/2.0% below two-pass TF-off and 19.0%/17.6% below the H.265 control.
However, enemy temporal residual increased from 4.4270 to 4.4634 versus TF-off;
this mixed diagnostic result is not proof that perceived flicker improved.
Compared with single-pass TF-on, the early enemy edge error is about 4.1%
higher while late-action edge error is about 17.6% lower, so the tradeoff remains
better whole-clip balance rather than dominance in every region.

`global-tf-balanced-preview.mp4` is a verified 600-frame, 60 FPS comparison:
top-left downsampled source, top-right single-pass TF-on (previously preferred),
bottom-left two-pass TF-off, bottom-right two-pass TF-on. Its first five seconds
show source 8.5–13.5 seconds, then source 35–40 seconds. Reproduce with
`global_quality.ps1 -Mode av1passfilter` and
`global_preview.ps1 -TemporalFilterComparison`. The user subsequently preferred
the bottom-right two-pass TF-on candidate, but described its advantage over
two-pass TF-off as small and requested further alternatives. This establishes a
preference, not that the original distant-enemy quality concern is fully solved.
It is not integrated into the application.

#### AV1 search effort and variance allocation follow-up (2026-10-24)

Keeping two passes, TF-on, tune 0, 10-bit, 3,631 kbps and all media constraints
unchanged, preset 3 produced `global-av1-preset3.mp4` at 19,549,515 bytes.
Mean/minimum VMAF was 91.345330/76.499934. Against the user-preferred preset-4
two-pass TF-on file, enemy edge error fell from 10.0546 to 9.5533 (5.0%), late
action from 10.0720 to 9.3169 (7.5%), and left/right from 11.6459/11.0498 to
11.1539/10.1293. Enemy temporal residual fell from 4.4634 to 4.3010; these are
diagnostics, not a perceptual flicker rating. The minimum VMAF is slightly lower
than preset 4 despite improved averages. Elapsed time was 348.84 seconds while
other experiments ran, so it is not an isolated performance benchmark.

A separate preset-4 trial enabled variance boost with strength 2 and octile 5.
The log confirms active AQ/variance configuration (2/2/5/0), but its 19,475,215-byte
output had mean/minimum VMAF 88.335082/70.469676. Enemy edge error increased to
10.9806 (+9.2%), late action to 11.9257 (+18.4%), and only the left crop improved.
Do not adopt this configuration for the current sample. The rationale was to
protect low-contrast detail across the frame, per the
[SVT variance boost documentation](https://github.com/AOMediaCodec/SVT-AV1/blob/main/Docs/Appendix-Variance-Boost.md),
not to prioritize a fixed central region.

Both files passed complete frame-count, 1080p60, 10-bit AV1, stereo AAC 48 kHz
and 20 MB checks. `global-detail-preview.mp4` has 600 frames at 60 FPS and the
same two five-second windows: top-left source, top-right previous preferred
candidate, bottom-left preset 3, bottom-right variance boost. Reproduce with
`global_quality.ps1 -Mode av1preset3`, `-Mode av1variance`, and
`global_preview.ps1 -DetailComparison`. The user subsequently judged bottom-left
preset 3 slightly better than the previous preferred top-right candidate,
apparently referring to reduced blockiness (the wording contained a typo), and
judged bottom-right variance boost worse. Retain preset 3 as the best candidate
so far and reject this variance-boost configuration. This does not establish that
distant-enemy motion softness or flicker is solved. Neither experiment changes
application defaults.

#### Nine-tile screening and speed comparison

The user requested broader simultaneous visual choices, a comparison table that
includes encoding speed, and previews restricted to the enemy sequence. The
fixed `grid-nine-preview.mp4` layout is:

| Tile | Candidate | Change from the preset-4 two-pass TF-on control |
| --- | --- | --- |
| 1 | Downsampled source | Reference only |
| 2 | `global-av1-preset3` | Previous user-preferred preset 3 |
| 3 | `global-av1-twopass-tf1` | Control |
| 4 | `grid-av1-qpc1` | `qp-scale-compress-strength=1` |
| 5 | `grid-av1-acbias` | `ac-bias=1.2` |
| 6 | `grid-av1-fast6` | Faster preset 6 |
| 7 | `grid-av1-restoff` | `enable-restoration=0` |
| 8 | `grid-av1-tx32` | `max-tx-size=32` |
| 9 | `grid-av1-gop120` | `keyint=120` |

An additional `grid-av1-tfweak` candidate (`tf-strength=1`) is retained in the
table/artifacts, but the grid uses the faster preset 6 in its place to address
the user's speed preference. New candidates retain the entire source and the
20 MB/1080p60/audio constraints; the preview only selects source 8.5–13.5 seconds.
Each tile crops the same 320x220 region and doubles it with nearest-neighbor
scaling. The grid is 1920x1320, 300 frames at 60 FPS, with numbered labels; view
large enough to distinguish the enemy. Its CRF-10 comparison export is not a
20 MB delivery encode. `grid-nine-manifest.json` fixes tile-to-file mappings.

`grid_speed.ps1` runs each candidate sequentially on the same five-second source
window, retaining scaling, both passes, AAC and muxing, and validates 300 output
frames. Run only after all other experiment encodes/metrics/previews finish.
These single-run measurements include startup overhead and reflect this clip
and host; do not extrapolate them linearly into promised full-clip durations.
`grid_report.py` joins completed full-clip sizes, short-clip timings and local
diagnostics into `grid-comparison.csv`. Concurrent full-clip experiment elapsed
times remain recorded separately and must not be used for speed rankings.
The user will choose preferred tile numbers before combining parameter changes.

All seven new full-clip candidates passed 2,458-frame, 1080p60, 10-bit AV1,
stereo 48 kHz AAC and 20,000,000-byte validation. The preview passed its
300-frame/1920x1320/60 FPS checks and a visual label/alignment inspection.
After all other experiment jobs finished, sequential timing results were:

| Tile | Full-clip bytes | Seconds to encode the 5-second window, both passes |
| --- | ---: | ---: |
| 2 | 19,549,515 | 28.80 |
| 3 | 19,554,637 | 17.34 |
| 4 | 19,579,473 | 16.22 |
| 5 | 19,547,790 | 16.87 |
| 6 | 19,599,577 | 8.84 |
| 7 | 19,560,280 | 17.26 |
| 8 | 19,546,775 | 15.59 |
| 9 | 19,519,639 | 15.50 |
| Supplemental weak TF | 19,563,304 | 16.38 |

These are single-run short-window timings, not confidence intervals; small
differences should not be interpreted as reliable performance wins. Local enemy
edge/temporal diagnostic errors remain lowest for tile 2 in this sample. The
new controls produce mixed small differences versus tile 3, while tile 6 trades
measured detail for substantially shorter encoding time. In response to the
nine-tile comparison, the user chose tile 2 ("2 吧"), retaining preset 3 as the
preferred quality candidate. No ordering of the other candidates or acceptance
of the speed tradeoff was stated. This preference does not establish that the
original distant-enemy quality concern is fully solved. No production encoder
code or installed application was changed during this screening.

References for the experimental controls:
[SVT-AV1 parameter documentation](https://github.com/AOMediaCodec/SVT-AV1/blob/main/Docs/Parameters.md).
The installed encoder accepts these options; logs confirm the QP compression
strength and AC bias. The 32x32 transform trial emits a coding-efficiency warning,
so it is a test of a tradeoff, not an assumed improvement.

Experiments run concurrently; their elapsed times are not isolated speed
benchmarks. SVT `lp=6` selects a level of parallelism, not six worker threads.
AV1 playback/recipient compatibility and subjective moving-edge quality must be
checked before adding an application preset. No application settings were changed
for these whole-frame experiments.

### Four existing encoders: separate parameter grids

Following the user's scope correction, `four-manifest.json` defines four groups
of eight same-encoder candidates. Each grid has source reference tile 1 and a
same-encoder control at tile 2. Twenty previously completed valid files are
reused; twelve new full-source encodes explore additional parameters. No ROI,
AV1, resolution reduction, frame-rate reduction or automatic codec substitution
is included in these groups.

| Tile | H.264 CPU | H.265 CPU | H.264 NVIDIA | H.265 NVIDIA |
| --- | --- | --- | --- | --- |
| 2 | Veryslow, refs 4 control | Slow control | P7/temporal AQ control | P7/temporal AQ control |
| 3 | Medium, 1.3x peak | Medium | AQ off | AQ off |
| 4 | Slow, refs 4 | psy-rd 1 | Original P5/spatial AQ | Main10 |
| 5 | Slow, AQ 2 | psy-rd 4 | P6 | B frames off |
| 6 | Slow, AQ 3 | CU tree off | 4-second forced keyframes | 1-second forced keyframes |
| 7 | Veryslow, AQ off | 10-bit | B frames off | Spatial AQ |
| 8 | Veryslow, psy-rd 0.5:0 | SAO off | Spatial AQ strength 2 | P6 |
| 9 | Veryslow, deblock -1:-1 | Deblock -1,-1 | P5/current analysis | 4-second forced keyframes |

These are complete configurations rather than claims that every row isolates
one variable: older medium/slow trials include their recorded peak/reference/AQ
settings, and NVENC retries can yield different final average bitrates. CPU
H.265 trials explicitly use eight pool threads for consistency with the earlier
controlled samples; production currently uses automatic pools. Retest any
selected parameters through the actual production path before adoption. HEVC
Main10 candidates also require playback compatibility consideration.

`four_encode.ps1` supports full encodes and sequential five-second speed tests
using the same manifest. CPU paths use two file passes; NVIDIA uses its actual
single file invocation with internal multipass settings. Speed timings include
scaling, encoding and audio/muxing but exclude retries needed to discover the
full-file target rate. Full-run logs retain retry counts separately.
`four_review.py` verifies codec identity, 2,458 frames, 1080p60, AAC stereo at
48 kHz and the 20 MB cap, then measures the same five-second enemy crop. Its
edge/temporal errors are diagnostic differences, not perceptual flicker scores.
`grid_preview.ps1 -CodecGroup <group>` validates eight candidates and rejects a
different output codec before producing a five-second, 300-frame numbered grid.
Groups are `x264`, `x265`, `h264gpu`, and `hevcgpu`; no number should be interpreted
without its group name.

H.264 NVIDIA tile 5 (P6) is byte-identical to tile 2 (P7) on this sample:
SHA-256 `C79FC79BDA9CC9DAAE8B45401D5CAA9F149C7750645D6B31BDD3B9E3DE54C9E7`.
The identical diagnostic values therefore are not an independent quality gain.
The user's per-encoder grid feedback is recorded below. The
application still exposes only the original four codec choices; experiment
parameters are not new user-facing settings.

All 32 full-file candidates passed the validation above: 19,359,970 to
19,795,765 bytes including audio. All four comparison grids contain 300 frames
at 60 FPS. Sequential speed tests also completed for all 32 configurations;
`out/quality-diag/four-comparison.csv` joins file sizes, diagnostic crop errors,
parameters labels and timings. The table below gives seconds to encode the same
five-second source excerpt, including both CPU passes:

| Tile | H.264 CPU | H.265 CPU | H.264 NVIDIA | H.265 NVIDIA |
| --- | ---: | ---: | ---: | ---: |
| 2 | 9.61 | 23.23 | 2.23 | 2.72 |
| 3 | 5.14 | 10.61 | 2.26 | 2.72 |
| 4 | 5.80 | 23.99 | 1.83 | 2.71 |
| 5 | 5.80 | 22.25 | 2.21 | 3.74 |
| 6 | 5.92 | 24.53 | 2.24 | 2.75 |
| 7 | 9.13 | 31.11 | 2.26 | 2.76 |
| 8 | 9.34 | 24.26 | 2.23 | 2.52 |
| 9 | 9.36 | 23.53 | 2.10 | 2.72 |

These are single-run short-excerpt measurements without other experiment jobs
running, not full-export estimates. Small timing differences may be noise.
The full-file H.265 NVIDIA P6 and four-second-keyframe candidates each required
two size-fitting attempts; the speed test uses their final fitted bitrate and
does not include that retry cost. This round has not changed production defaults
or rebuilt the installed app.

User feedback on 2026-10-24:

| Encoder | Feedback | Configuration to carry forward |
| --- | --- | --- |
| H.264 CPU | All candidates poor and unacceptable | No acceptable winner |
| H.265 CPU | Tile 6 slightly better | `motion-x265-cutree0`, slow with CU-tree off |
| H.264 NVIDIA | Tiles 7 and 8 slightly better | `four-h264gpu-bf0` and `four-h264gpu-spatial2` |
| H.265 NVIDIA | Tile 5 slightly better | `motion-hevc-bf0`, B frames off |

These are relative preferences, not acceptance of final defaults. Both NVIDIA
groups favor a B-frame-disabled candidate, and H.264 NVIDIA also favors mild
spatial AQ without temporal AQ. Follow-up trials should test those independently
and in combination; CPU H.265 should use CU-tree off as the comparison anchor.
CPU H.264 needs a different parameter search rather than promoting a rejected
candidate. This feedback does not establish the cause of motion blur or imply
that disabling B frames or CU-tree helps other footage. Keep resolution, frame
rate, codec identity and the full-file 20 MB cap fixed for this comparison.

#### Controlled convergence plan (2026-10-24)

The user clarified that all current results remain poor; the selected tiles are
only the least objectionable, not acceptable quality. Avoid unrelated parameter
lotteries. Keep AV1 candidates separately for a future encoder option.

1. Isolate temporal allocation interactions. `phase1-manifest.json` contains a
   2x2 matrix for each CPU encoder (tree allocation on/off, B frames on/off)
   and a 2x3 matrix for each NVIDIA encoder (B frames on/off, AQ off/temporal/
   spatial strength 2). All other encoder settings stay fixed within each group.
   x264 uses 8 B frames when enabled, matching its veryslow control; x265 and
   NVENC use 4. There are 20 candidates: 10 reused and 10 new full encodes.
2. After visual evaluation, retain the better combination as the next control.
   Adjust AQ strength and detail-preservation settings around that combination,
   one axis at a time; only test combinations after their individual effects
   are understood. If no gain is visible, retain the control and reject the
   hypothesis rather than declare a winner.
3. Compare speed presets with the selected quality settings fixed. Retain a
   faster configuration only if both distant-person passages remain comparable.
   Verify the final choice through production encoding and size fitting.

All trials retain 1080p60, AAC 192 kbit/s, the same full source and decimal 20 MB
ceiling. Size fitting can change the final target bitrate, which must be logged;
the comparison tests a complete size-constrained configuration rather than
pretending those effective rates are identical. CPU tests retain pools=8 for
x265. Parallel full encodes are for screening, not speed rankings; speed tests
run sequentially after all other encoding jobs finish. Grids contain only the
necessary candidates plus reference, leaving unused cells black instead of
filling nine positions with unrelated experiments. Inspect both source 00:10
and 00:12 within the synchronized 8.5-13.5 second excerpt. Whole-frame/crop
metrics may help diagnose regressions but cannot overrule the user's motion
assessment or certify a universal optimum from a single clip.

Phase 1 tile keys (tile 1 is always the source reference):

| Tile | CPU, each codec separately | NVIDIA, each codec separately |
| --- | --- | --- |
| 2 | Tree on, B frames on | B4, temporal AQ |
| 3 | Tree off, B frames on | B4, AQ off |
| 4 | Tree on, B frames off | B4, spatial AQ strength 2 |
| 5 | Tree off, B frames off | B0, temporal AQ |
| 6 | Unused | B0, AQ off |
| 7 | Unused | B0, spatial AQ strength 2 |

CPU grids have five populated cells; NVIDIA grids have seven. Numbers from the
earlier nine-tile search must not be carried across without the configuration
mapping. `phase1_review.py` separately records reference edge differences and
temporal residuals for source 9.5-10.5 and 11.5-12.5 seconds, alongside the whole
five-second crop. These are diagnostic measurements, not subjective scores.

Phase 1 completed: all 20 full files passed codec, 2,458-frame, 1080p60,
stereo 48 kHz AAC and size checks (19,369,578-19,722,792 bytes). All four previews
passed 300-frame/60 FPS checks and their still layouts were visually inspected.
`out/quality-diag/phase1-comparison.csv` contains the detailed joined results.
Sequential isolated five-second timings, including both CPU passes:

| Tile | H.264 CPU seconds | H.265 CPU seconds | H.264 NVIDIA seconds | H.265 NVIDIA seconds |
| --- | ---: | ---: | ---: | ---: |
| 2 | 9.41 | 23.35 | 2.25 | 2.72 |
| 3 | 9.40 | 24.31 | 2.21 | 2.72 |
| 4 | 6.93 | 21.62 | 2.25 | 2.71 |
| 5 | 6.83 | 22.34 | 2.31 | 3.78 |
| 6 | - | - | 2.29 | 3.75 |
| 7 | - | - | 2.25 | 3.75 |

The particularly slow concurrent x265 tree-on/B0 full first pass is not reflected
in this isolated excerpt benchmark; do not reject the configuration on that
parallel timing. HEVC NVENC B0 paths were slower than B4 paths in the excerpt.
These remain single runs, not whole-export latency estimates. The three new
HEVC NVENC candidates needed two full-file size-fitting attempts; that cost is
excluded from the excerpt timing. No production defaults were changed. Phase 1
visual feedback rejected both H.264 groups as unacceptable. H.265 CPU was almost
indistinguishable across the tested combinations. H.265 NVIDIA tile 6 (B0/AQ
off) was barely better than tile 5; tile 7 (B0/spatial AQ 2) was worse. No
configuration was accepted. Do not refine the B-frame/tree/AQ combinations as
though they established a useful improvement.

#### Phase 2: test the rate envelope before further tuning

`allocation_audit.py` measures video packet bytes over fixed source-time windows.
Anchor rates at 9.5-10.5 / 11.5-12.5 seconds were 4.688/3.901 Mbps for x264,
4.086/3.411 for x265 CU-tree off, 3.754/3.731 for H.264 NVENC B0, and
4.230/3.414 for HEVC NVENC B0/AQ off. This suggests a relatively flat H.264
NVENC allocation, not proof of a binding VBV limit or the cause of blur.

Test one rate-envelope alternative per codec using those anchors. CPU variants
disable VBV maxrate/buffer while retaining two-pass average bitrate fitting;
NVENC variants loosen both peak and buffer to eight times the fitted average.
This tests the envelope as a paired constraint, not the separate effect of peak
and buffer. All other encoder settings remain fixed. All full outputs still
must fit 20 MB and retain 1080p60/audio; rate retries and final target rates are
logged. Each `phase2` preview uses tile 1 reference, tile 2 anchor and tile 3
relaxed envelope. If there is no meaningful visual gain, reject this branch
rather than search adjacent peak/buffer values. These are experimental outputs,
not application defaults or an increase to the user's size limit.

Phase 2 findings: x264 and x265 outputs were 19,736,518 and 19,454,073 bytes;
HEVC NVENC was 19,329,572. Their source 9.5-10.5 / 11.5-12.5 rates became
4.762/3.984, 4.070/3.398 and 4.137/3.439 Mbps respectively. These small changes
do not support a large rate-envelope bottleneck at the reported moments.
Crop diagnostics did not establish a substantial gain; visual feedback remains
necessary before calling motion equivalent.

H.264 NVENC with an 8x envelope showed a sharp size step during fitting:
3490 and 3456 kbit/s targets both produced 20,549,247 bytes; retries at 3290
and 3258 produced 17,896,517. Earlier 3631/3555 initial-target searches also
fell back to approximately 17.9 MB. Backing off to a 4x peak and 4x buffer
produced 20,537,651 bytes at 3631, then 17,927,649 at 3425. Preserve
`phase2-attempts.jsonl` and the underfilled diagnostic; do not treat this as a
matched-byte comparison or infer that no intermediate target could ever fit.
The retained tile 3 is the 4x/3425 variant; its two window rates are
3.839/3.210 Mbps, with worse overall crop diagnostics. Capacity utilization
remains unresolved for this relaxed H.264 NVENC configuration. Do not promote
it into production or keep searching neighboring rates without evidence of a
meaningful visual benefit. The setup script records initial experiment seeds;
the saved manifest and completion records describe the final fitted variants.

All eight retained full files passed codec identity, 2,458 frames, 1080p60,
stereo 48 kHz AAC and <=20,000,000-byte validation. All four three-tile previews
passed 300-frame/60 FPS checks and their layouts were visually inspected.
`phase2-comparison.csv` joins diagnostic windows, actual sizes and sequential
five-second timings. Anchor/relaxed seconds were x264 9.77/9.51, x265
26.63/27.02, H.264 NVENC 2.32/2.30 and HEVC NVENC 3.75/3.78. These single-run
excerpt timings exclude full-file retries and cannot establish small speed
advantages. No application defaults were changed in either controlled phase.

User verdict for Phase 2: no obvious improvement, and some situations were
worse. Reject the relaxed-envelope branch; do not promote any of these settings
or keep searching nearby peak/buffer values. Phase 1 and Phase 2 have produced
no acceptable default. The unresolved H.264 NVENC capacity step is a diagnostic
finding, not a demonstrated explanation for all four codecs' visible blur.

Search inventory after this verdict: psycho-visual strength (x264 0.5, x265
1/4), AQ modes/off, B frames/tree allocation, CPU and GPU speed presets,
deblocking/SAO, GOP length, HEVC 10-bit, bounded versus relaxed rate control,
and an HEVC UHQ candidate have already been tested. Several were explicitly
rejected or produced only negligible differences. Do not recycle those into
another grid without a distinct hypothesis. Earlier x265 no-VBV testing also
showed little improvement, consistent with Phase 2 rather than a new discovery.
This does not exhaust the encoder parameter space or prove an absolute codec
quality ceiling. AV1 remains a separately retained, user-acceptable candidate;
ROI remains a separately held alternative. Neither substitutes for an existing
encoder without an explicit product decision.

#### Phase 3: detail retention and lookahead effort

The next controlled trial adds one candidate per encoder, retaining Phase 1
anchors and the original bounded rate control. x264/x265 add `-tune grain`:
this tests a coordinated detail-retention/quantization strategy, not a single
isolated psycho-visual slider or a claim that gameplay is film grain. NVENC keeps
HQ and sets `-lookahead_level 3`, distinct from the previously rejected HEVC UHQ
experiment. Both local NVENC encoders passed 60-frame initialization probes
without unsupported/ignored-option warnings. File identity and decoded output
must still be checked; a successful option parse alone does not prove a quality
benefit. `phase3` manifests/runners retain full 1080p60, audio and 20 MB checks.
If these fail, the next independent hypothesis is light whole-frame edge-aware
preprocessing to reduce texture cost. Do not combine that filter with encoding
changes before measuring its own loss of distant-enemy detail.
If grain helps visibly, separate its quantizer-stability and detail-retention
components to find the necessary subset; if explicit NVENC level 3 helps, test
levels 1/2 for a comparable-quality faster setting. If neither helps, move to
the independent preprocessing hypothesis rather than sweep nearby values.

The user also asked whether the smaller FFmpeg package affected quality.
Commit `b533e70` changed BtbN win64 GPL static asset selection to GPL shared,
with DLL retention/startup checks, not encode settings or a different encoder
library. The current installation still executes FFmpeg and the four original
encoder implementations. Its small executable loads substantial avcodec and
other DLLs. Shared linkage itself is not a reduced-quality encoding mode.
There is no same-version static/shared pixel equivalence test in this work;
updates to codec versions/build options are a separate possible source of
output changes. Only the shared binary was found under the user's tools folder.

Phase 3 user feedback identified only a very small improvement in HEVC NVENC
with explicit lookahead level 3; this remains a backup, not an accepted default.
The later x265 grain feedback was explicitly worse, with an oversharpened
appearance; reject that strategy despite the absence of an actual sharpening
filter. All
four new full encodes passed the media/20 MB checks: x264 19,384,114, x265
19,678,937, H.264 NVENC 19,626,925 and HEVC NVENC 19,310,572 bytes. The last
required two size attempts. The measured diagnostics do not show a general
improvement; do not override subjective feedback with them.

#### Phase 4: independent spatial preprocessing

Hold each Phase 1 encoder anchor fixed and add only a whole-frame luma bilateral
filter after Lanczos downsampling: `bilateral=sigmaS=2:sigmaR=0.05:planes=1`.
This is a spatial-only filter, without temporal accumulation, sharpening, ROI,
resolution or FPS changes. It tests whether reducing small texture variations
can improve the compressed moving outline enough to justify the filter's own
detail loss. It is not combined with grain or the Phase 3 lookahead override.

`phase4_prefilter.py` checks the source before compression over the full
five-second enemy crop. The initial sigmaR=0.025 check changed grayscale pixels
by mean 0.737, 99th percentile 2 and maximum 3 (8-bit levels); the retained
0.05 check gave 1.232, 5 and 7, with mean reference-edge gradient change 3.164.
Both before/after stills were inspected; no obvious outline deformation was
seen at that frame. This does not prove zero loss or motion equivalence.
Retain the filter-only PNG/metrics separately so softer source detail cannot
be misrepresented as better compression. Full outputs still require 1080p60,
AAC stereo 48 kHz and <=20 MB validation. Subjective success requires a more
readable moving person, not simply fewer background blocks.

All four full Phase 4 candidates passed codec, 2458-frame, 1080p60, AAC stereo
48 kHz and decimal 20 MB checks. Sizes: x264 19,664,311; x265 19,464,305;
H.264 NVENC 19,630,442; HEVC NVENC 19,458,457 bytes. All four five-second
previews passed 300-frame/60 FPS checks and their comparison stills were inspected.
User motion-quality feedback (2026-10-25), comparing tile 3 with tile 2:

| Encoder | User feedback | Decision |
| --- | --- | --- |
| H.264 CPU | Worse | Reject this prefilter candidate |
| H.265 CPU | Very slightly better | Retain as an experimental candidate only |
| H.264 NVIDIA | About the same, no obvious difference | No demonstrated benefit |
| H.265 NVIDIA | Very slightly better | Retain as an experimental candidate only |

None meets the user's quality target. Do not generalize the small HEVC gains to
H.264 or increase filtering strength without a separate comparison.

#### Phase 3/4 isolated speed comparison

Sequential measurements on the same five-second source excerpt, without other
encoding jobs running. CPU times include both passes; Phase 4 candidate times
include filtering. These are single-run excerpt timings, not full-export times
or estimates including size retries. Small differences can be measurement noise.

| Encoder | Phase 3 control / candidate (s) | Phase 4 control / candidate (s) |
| --- | ---: | ---: |
| H.264 CPU | 10.12 / 10.24 | 10.04 / 10.88 |
| H.265 CPU | 27.72 / 49.64 | 28.08 / 29.10 |
| H.264 NVIDIA | 2.32 / 3.20 | 2.31 / 2.66 |
| H.265 NVIDIA | 3.75 / 3.87 | 3.83 / 3.81 |

Phase 3 H.265 CPU grain is rejected by the user for an oversharpened appearance
and also costs substantially more time. Phase 3 H.265 NVIDIA has only a very
small reported improvement, not acceptance. Keep these conclusions separate
from the Phase 4 feedback above. No production defaults change from these runs.
The combined 16-row artifact `out/quality-diag/phase34-comparison.csv` records
full-file sizes, crop diagnostics at 10/12 seconds and isolated excerpt timings;
the diagnostics are not perceptual acceptance scores.

Parameter references: [x265 options](https://x265.readthedocs.io/en/stable/cli.html),
[FFmpeg x264 options](https://www.ffmpeg.org/doxygen/8.1/libx264_8c_source.html).

#### Phase 5: chroma versus luma allocation

Keep the same four Phase 1 controls, without Phase 3/4 changes. Test chroma QP
offset +6: x264 `-chromaoffset 6`, x265 `cbqpoffs=6:crqpoffs=6`, NVENC
`-qp_cb_offset 6 -qp_cr_offset 6`. Local FFmpeg encoder help exposes the x264
and NVENC options; x265 documents its offsets in its CLI reference.
This is a coarse directional test, not a claim that equal offsets imply equal
effective quantizers across encoders. Inspect encoder logs/bitstream headers for
application. The hypothesis is that reduced chroma precision may leave more
of the fixed budget for luma structure; rate control may not use it that way.
Color bleeding is a failure even if grayscale diagnostics improve.

Encode the full source under 20,000,000 bytes, with unchanged dimensions, FPS
and audio. Reuse the independent five-second action previews and sequential
speed tests including both CPU passes. If the direction visibly helps, bracket
with +3/+9; if it does not, do not sweep nearby values. Do not combine it with
prefiltering until its isolated effect is established. Production defaults stay
unchanged pending visual acceptance and full validation.

Completed 2026-10-25: all four full candidates passed the media and size checks;
all previews passed 300 frames at 60 FPS and their stills were inspected.
`phase5_headers.py` checks the first packet's parameter sets with `trace_headers`:
x264 offsets changed from -2 to +4, x265 and both NVENC codecs from 0 to +6.
Thus the signaled change is +6 in every group, not merely an accepted CLI option.

| Encoder | Full bytes | Control / candidate seconds per 5s |
| --- | ---: | ---: |
| H.264 CPU | 19,715,879 | 9.00 / 9.14 |
| H.265 CPU | 19,475,460 | 21.51 / 21.47 |
| H.264 NVIDIA | 19,627,436 | 2.26 / 2.27 |
| H.265 NVIDIA | 19,378,913 | 3.75 / 3.81 |

Speed runs were sequential without competing encoding/preview jobs and include
both CPU passes. Single-run timing differences are small and may be noise; use
within-phase controls, not earlier runs, for comparison. The HEVC NVENC full
encode needed two size attempts. Full-run times included preview contention
in parts of this batch and are not the isolated speed comparison.
Gray crop diagnostics improved slightly overall, not enough to claim a visible
gain; they do not measure chroma bleeding. User feedback: H.265 improves only
very slightly; the others show almost no difference or are slightly worse.
The user clarified that both H.265 CPU and NVIDIA improve very slightly, but
neither is sufficient. No encoder is accepted; do not expand to +3/+9 on this
evidence alone.
Detailed metrics and timing: `out/quality-diag/phase5-comparison.csv`.
No production changes were made for this experiment.

References: [x265 chroma offsets](https://x265.readthedocs.io/en/3.1/cli.html),
[FFmpeg encoder options](https://www.ffmpeg.org/ffmpeg-all.html).

#### Phase 6: combat-window temporal allocation

Test a larger budget redistribution rather than another adjacent parameter:
8-14 seconds gets twice the nominal video rate, and the other 2098 frames get
1738/2098 of it. Duration-weighted requested video bits remain unchanged across
2458 frames. Each encoder has a fresh equal-rate segmented control and a priority
candidate, plus the retained continuous control. Every segment uses the same
codec and anchor configuration; no Phase 3/4/5 additions. CPU segments each use
two passes. Three video segments are concatenated without re-encoding; source
audio is encoded once to avoid separate AAC segment priming gaps.

This is an explicitly hand-selected diagnostic window, not an automatic general
combat detector. It can show whether substantial local gains are possible by
sacrificing other times, but cannot establish a production optimum. Other fights
outside the window may suffer. Inspect segment joins and the rest of the full
video before any adoption. Do not present improved preview-only quality as a
whole-video improvement.

Run all eight full encodes sequentially without competing encoding jobs. Record
both final-attempt and total encode/mux times, including CPU passes and size
retries; metadata and decode validation are outside the timer. Require <=20 MB,
2458 frames at 1080p60, AAC stereo 48 kHz, every decoded video timestamp matching
its frame index, and an error-free full decode. The equal-rate segmented control
isolates the extra boundaries from the allocation change.

Completed: all eight full encodes passed the declared media, timestamp and
full-decode checks. The two HEVC NVENC variants each needed two size attempts;
all others needed one. Sequential full encode/mux timings below exclude
validation and include both CPU passes. No preview jobs ran during encoding.

| Encoder | Priority bytes | Action kbps, segmented control / priority | Other kbps, control / priority | Final attempt seconds, control / priority | Total seconds, control / priority |
| --- | ---: | ---: | ---: | ---: | ---: |
| H.264 CPU | 19,675,445 | 3675 / 7236 | 3633 / 3020 | 71.82 / 72.31 | 71.82 / 72.31 |
| H.265 CPU | 19,340,669 | 3569 / 7134 | 3575 / 2959 | 204.27 / 228.68 | 204.27 / 228.68 |
| H.264 NVIDIA | 19,511,793 | 3545 / 7129 | 3623 / 3005 | 15.62 / 15.64 | 15.62 / 15.64 |
| H.265 NVIDIA | 19,246,877 | 3221 / 6635 | 3604 / 3029 | 27.27 / 27.28 | 54.52 / 54.47 |

Actual video packet bytes by PTS confirm the redistribution. Versus segmented
controls, the five-second enemy-crop edge error fell approximately 25-28% and
temporal residual 18-21%. These are diagnostic error reductions, not perceptual
quality percentages or proof of whole-video improvement. The user confirmed
that tile 4 is clearly better than tile 2 for all four encoders. This is the first
clear subjective improvement across all four in these controlled phases. The
user subsequently confirmed tile 3 is not better than tile 2. Together, these
observations support temporal budget redistribution as the main visible benefit,
rather than segmentation alone. Full-video quality is not yet accepted.
`phase6-comparison.csv` includes the original
continuous controls too.
No production defaults change; the hand-picked window and quality transitions
need broader subjective validation before any automatic allocation design.

Next gate: identify all combat windows independently of the favored preview,
include approach/reaction margins, merge overlaps, then calculate a global
duration-weighted budget with a minimum allocation for the remaining footage.
Do not assume every fight can receive 2x: the affordable multiplier falls as
combat coverage grows. Compare full-video quality, boundary transitions, size
and sequential speed for each encoder before integrating any automatic policy.

#### Phase 7: event-derived coverage and bounded ratio sweep

Use this job's parsed highlight (start tick 35309, 64 ticks/s) and cached replay
shot/death events for POV steamid 76561198001286772. The contiguous key-moment
union matches this 2458-frame capture; kills map to 5.000, 12.781, 30.016,
37.078 and 37.984 seconds. Inspect a one-frame-per-second full-source contact
sheet for coarse alignment. This is not proof that every visible opponent is
covered: shot/death events do not encode visibility or incoming nonlethal hits.

Pad each own shot or involved death by 1.5 seconds before and 0.75 seconds after,
merge gaps <=0.5 seconds, quantize outward to 60 FPS and clamp to the capture.
Result: [3.167,5.900), [6.667,16.550), [17.983,23.767), [28.433,31.117),
[35.567,39.150), covering 60.21% of the video and all five kill times.
`phase7-plan.json` records source events, frame boundaries and budget weights.

Correct the earlier proposed 1.5x/2x/2.5x terminology: at this coverage, 2x of
the original average for all combat is impossible within the same total budget.
Test combat:other ratios 1.5:1, 2:1 and 2.5:1, normalized by
`1 + coverage * (ratio - 1)`. Combat weights become 1.153, 1.248 and 1.314;
other weights become 0.769, 0.624 and 0.525. Assert the duration-weighted sum
is unchanged and no requested other weight falls below 0.5. These are requested
bit allocations, not promises of achieved packet rates.

Each codec also gets an equal-rate control with identical eleven boundaries;
retain the original continuous encode for comparison. Six tiles per codec:
reference, original, equal-rate segments, 1.5:1, 2:1, 2.5:1. Full encodes run
sequentially using the Phase 6 media/size/timestamp/full-decode validation and
timing protocol. No production setting changes until whole-video visual review.

User redirected this phase before the sweep completed: first deliver one full
HEVC NVENC 2:1 example to judge whether sharp/soft quality changes are acceptable.
Defer the remaining ratios/codecs until that experience is assessed. The first
x264 equal-rate attempt was rejected by timestamp validation (sub-millisecond
duration rounding accumulated at segment joins), so it is not a validated result.
Write explicit concat durations from each segment's frame count rather than
using rounded MP4 durations, and retain the strict timestamp check.

The requested full example `out/quality-diag/phase7-hevcgpu-r2.mp4` completed at
19,770,571 bytes after two attempts (36.00 + 35.80 seconds of encoding/muxing).
It passed 2458-frame 1080p60, AAC stereo 48 kHz, strict per-frame timestamp and
error-free full-decode validation with explicit concat durations. Deliver this
unmodified full file for subjective quality-transition review, not a re-encoded
preview that could mask the effect. The user found this full example acceptable:
quality changes are not noticeable without close inspection. This accepts this
sample's HEVC NVENC viewing experience, not all encoders or a universal policy.
The same 2:1 event-allocation candidate has also completed for x264, x265 and
H.264 NVENC (results below). The 1.5:1 and 2.5:1 runs stay deferred.

### Integrated option: combat quality priority

The accepted label is **交戰畫質優先** (Prioritize combat quality), enabled by
default and shown only with a file-size limit. Disabling it retains ordinary VBR /
two-pass size fitting. Unlimited exports do not use this policy.

Normal demo parsing stores per-player ticks for gunfire, enemy damage dealt or
received, and kills/deaths. Windows cover 1.5 seconds before and 0.75 seconds after
each event; gaps up to 0.5 seconds merge. Windows follow the recorded POV, cuts,
round-result camera switches and final merge order. No visibility model or AI is
used. Older parsed data can use an existing current replay cache (shots/deaths
only); without either source, the job logs a fallback to normal size fitting.
Reparsing adds damage events to old data. These may widen the windows compared
with the experiment below. Whole-clip combat and empty windows also use ordinary
size fitting because there is no useful temporal allocation to make.

All four encoders use the tested normalized 2:1 combat/other allocation. This is
not twice the original average bitrate: the total budget stays fixed. Short CPU
segments use two passes; NVENC uses its tested per-frame multipass settings.
Audio is encoded once after video segments join. Final output is checked for
size, frame count, frame rate, dimensions and audio presence before publication;
cancellation/failure retains the source and an existing destination. Small video
timestamp gaps from concatenated audio padding are converted to the nominal
constant frame rate. Unsupported frame-rate metadata falls back to normal fitting.

Full-source Phase7 results (1080p60, 2,458 frames, decimal 20 MB cap):

| Encoder | Size MB | Combat / other kbit/s | Total encoding seconds | Attempts |
| --- | ---: | ---: | ---: | ---: |
| H.264 CPU | 19.790 | 4482.6 / 2415.4 | 88.44 | 1 |
| H.265 CPU | 19.107 | 4386.1 / 2221.7 | 219.90 | 1 |
| H.264 NVIDIA | 19.675 | 4584.8 / 2211.6 | 23.59 | 1 |
| H.265 NVIDIA | 19.771 | 4556.0 / 2301.9 | 71.80 | 2 |

These are experimental-run timings on this machine, including failed size
attempts, not guaranteed app conversion times. All four passed full decoding,
frame timestamp and stereo 48 kHz AAC checks. Only the full H.265 NVIDIA result
has the user's subjective acceptance; that does not establish acceptance for
the other three encoders. CSV: `out/quality-diag/phase7-r2-comparison.csv`.

## Verification

Unit checks: `cargo test -p demodesk-core --lib`.

The ignored `real_combat_encoding` test checks all four encoders (with
`TEST_NVENC=1`), irregular segment boundaries, size, decoded frame timestamps,
audio/silent inputs, concatenated input, progress, and cancellation preservation.

Integration verification (2026-10-25): 317 core tests passed (8 ignored by the
ordinary run); the real combat-encoding test passed separately with all four
encoders, and the real key-parts join test passed with event-window offsets.
Frontend build and the existing UI suite passed, including default-on behavior,
explicit opt-out and hiding the option for unlimited exports. Strict all-target
Clippy remains blocked by 23 existing warnings in unrelated/unchanged code;
these were not folded into this feature.

The integrated Rust path was also run on the complete source, scaled once to a
lossless 1080p intermediate, using the same Phase7 windows. HEVC NVENC produced
`out/quality-diag/phase7-production-hevcgpu-20mb.mp4`: 19,707,272 bytes after two
attempts (~73.5 s before final verification), 2,458 frames at 60 FPS, AAC stereo
48 kHz, audio duration 41.006458 s. Full decode passed; maximum per-frame timestamp
error was 0.000000334 s. The temporary lossless input was removed after validation.

For real FFmpeg checks, set `FFMPEG` to the executable path (with ffprobe beside it), and run:

```powershell
cargo test -p demodesk-core --lib render::encode::tests::real_encoding -- --ignored --nocapture
```

Set `TEST_NVENC=1` to include NVIDIA H.264 and HEVC on a supported GPU. Synthetic temporary clips check the byte limit, dimensions, 90 FPS, stereo 48 kHz audio, silent inputs, and preservation on failure.

References: [NVIDIA guidance](https://docs.nvidia.com/video-technologies/video-codec-sdk/13.1/ffmpeg-with-nvidia-gpu/index.html), [FFmpeg options](https://www.ffmpeg.org/ffmpeg.html), [codec options](https://www.ffmpeg.org/ffmpeg-codecs.html).

## Export progress

Highlight and analysis exports merge adjacent windows in the same round and player
view when their gap is strictly less than one second, including overlaps. The
merged window records continuously from the earliest start to the latest end;
a gap of exactly one second stays separate. Analysis previews use the same rule.

Both exports share the recording schedule. Within each new CS2 session, the first
clip keeps its normal setup. Each later clip pauses at its recording start after
seeking, camera setup and preroll. The app waits at least three real-time seconds
for lingering sounds, then starts recording and resumes playback over netcon.
Later recording starts are not prequeued in the tick schedule, so a frame crossing
both setup and start cannot bypass the wait. If a frame also crosses the clip's end
before the wait completes, the job fails rather than starting a recording with no
remaining end command.
Each pause uses a per-clip alias that the app clears before resuming, because HLAE
can execute the pause tick again on resume. Duplicate wait markers do not restart
the timer or recording. Resume precedes recording startup in the same command batch.
Cancellation remains available during the wait. Actual sound decay must be
checked in CS2 with grenade, fire, round-win and C4 sounds across clip boundaries.

Recording reports scheduled demo-time markers, weighted by clip duration. Encoding reads FFmpeg's `out_time_us` while the process runs and combines both CPU passes. Size retries use the remaining progress range; completed output is still verified before reporting success.

The UI labels overall progress as estimated: recording gets 70% of the work budget when size fitting is enabled, 90% otherwise. These phase weights are not time estimates. Elapsed time updates each second; there is no countdown. Progress stays below 100% until the job succeeds, and older job records can omit the optional progress field.

Post-processing checks cancellation every 250 ms and terminates the owned process
tree before returning. Every five seconds, the watchdog checks for increasing
FFmpeg frame counts or reported output bytes, or changes to the current output
file's size or modification time. Six consecutive quiet intervals (30 seconds)
stop the process; any progress resets the quiet interval count. This tolerance
also applies at startup and to each new pass. It allows bursty writes and slower
machines, but is a heuristic rather than proof that the encoder is deadlocked.
CPU usage, elapsed video time and repeated progress messages alone do not count
as progress.

A stalled size-fitting operation restarts that video's encoding from the source
immediately, at most three retries after the initial attempt. Other completed
videos are retained. Cancellation and ordinary encoder errors do not trigger
stall retries. Interrupted attempts clean up their temporary output and pass
logs; the source and existing destination are preserved. Job logs record the
encoding passes, size checks, stall counters and retry number.
