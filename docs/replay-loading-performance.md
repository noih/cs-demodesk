# Cached 2D replay loading

Measured 2026-11-04 with one existing 76.61 MiB replay cache: 24 rounds,
36,367 frames and one radar layer. No original Demo was reprocessed.
The benchmark selects the largest replay under a supplied data directory,
serves only its files on loopback, and prints timings without player names or paths.

```powershell
node --experimental-transform-types scripts/benchmark-replay-load.mjs <data-directory>
```

Set `PLAYWRIGHT_MODULE` to an existing Playwright installation if needed.
Each mode uses four fresh browser pages; discard its first run and compare
medians of the remaining three. Filesystem cache is warm, not a cold-disk test.

| Stage | Object transfer baseline | Packed transfer |
|---|---:|---:|
| Fetch/read replay text | 194 ms | 186 ms |
| Parse JSON in worker | 237 ms | 237 ms |
| Pack numeric rows | 0 ms | 95 ms |
| Worker message delivery, including serialization | 660 ms | 12 ms |
| Reconstruct original arrays | 0 ms | 90 ms |
| Replay initialization and initial state | 19 ms | 1 ms |
| Radar decode/CAS processing | 33 ms | 35 ms |
| Total, instrumented path | 1,141 ms | 665 ms |

The real `api.replay` path, using the actual load worker and decoder, measured
**661 ms median including Replay initialization and radar**, about **42% less**
than the baseline. This harness uses Chrome and loopback file responses; it
excludes native Tauri command/asset-protocol overhead and React's first paint.
It is evidence for this bottleneck and dataset, not a native-app timing guarantee.

## Change and verification

Replay JSON remains unchanged on disk. Inside the worker, numeric frame/smoke
rows are packed into a transferable Float64Array. The main thread reconstructs
the original array shape, preserving double precision and optional/empty fields.
The worker is still terminated after loading. Current-match memory retention
and release when switching matches are unchanged.

The largest real replay passed full deep equality after pack, transferable
structured clone, and unpack. `npm run test:replay` covers precision, inventory,
grenades, dropped equipment, fire cells, smoke availability, absent fields and
buffer ownership transfer. `npm run test:ui` exercises the real worker through
mocked IPC, repeated tab entry, resource cleanup and drawing.

Do not replace the explicit numeric reconstruction loop with `Array.from` on
subarrays without measuring: an experimental version spent about 810 ms there,
which erased the transfer savings. No disk schema migration, preloading of other
matches, or release is needed for this change.


## Native WebView2 binary-cache comparison

Measured on the same Windows machine and dataset in the native Tauri App,
with production frontend assets and a debug Rust build. The test instance used
a separate data-directory copy and WebView profile; background Demo scanning
was disabled. Neither the original Demo nor existing JSON files was changed.

The timer starts in the capture phase of the 2D tab click. It ends two
requestAnimationFrame callbacks after the first replay-canvas drawImage, allowing
the completed draw a paint opportunity. This measures the native UI path, not
physical display latency. Each run reloads the page so React's retained replay
cannot hide disk loading. Discard run 0 and compare the next four runs;
filesystem caches are warm. The final binary and JSON batches ran without other
test jobs.

| Native click to first frame | Median | Range |
|---|---:|---:|
| Existing JSON + transferable rows | 950 ms | 916–1,011 ms |
| Gzip-compressed binary sidecar | 708 ms | 707–732 ms |

The measured reduction is **242 ms (25.5%)**. Replay worker duration fell from
696 ms to 451 ms (medians); array reconstruction, radar initialization and paint
still contribute to the remaining time. An uncompressed binary experiment was
rejected: its 180.6 MiB file loaded more slowly than JSON.

The retained sidecar is **24.53 MiB**, in addition to the unchanged **76.61 MiB**
JSON. First conversion plus first frame took **2.55 s** in this debug build;
subsequent loads use the sidecar. This is not a cold-disk or release-build result,
and one large Demo does not establish performance for every machine or map.

### Cache compatibility

The .replay.bin sidecar is disposable gzip-compressed data. Its versioned header
holds source JSON size/mtime, metadata length, numeric row count and payload SHA-256.
Numeric rows remain Float64; no lossy quantization is used. Existing JSON stays
the source of truth, including for video-export combat ticks. A source change
invalidates the sidecar; a completed replacement is published atomically.
Clearing parsed data removes both formats.

Failure to create a sidecar returns JSON. A failed binary load, decompression,
checksum or decode retries JSON once and removes the bad sidecar. Older WebViews
without DecompressionStream can therefore still load existing data via JSON.
The full real replay passed deep equality between JSON and Rust-written binary
decoded by TypeScript. Original parsed/summary/replay JSON hashes were unchanged.

Native corruption verification altered only the test sidecar's gzip CRC: the
request sequence was binary then JSON, the first frame rendered successfully,
and the bad sidecar was removed. The next load rebuilt it successfully.
Unit tests cover source changes, truncation, unsupported formats, invalid row
lengths, write failure and preservation of source JSON. UI checks cover the JSON
worker route, playback, repeated tab entry and resource release.

### Native measurement script

Start a native test App with WebView2 remote debugging bound to loopback and
a dedicated test profile, then connect:

~~~powershell
node scripts/benchmark-native-replay.mjs http://127.0.0.1:9494 de_cache 5
~~~

Arguments are CDP endpoint, visible map-name filter and run count.
Set PLAYWRIGHT_MODULE if Playwright is installed outside this repository.
Use an isolated data copy and stop other test workloads before comparison.
The script clicks real UI and reads real native assets; it does not mock IPC.
Temporary benchmark-only data-root/JSON-mode overrides used for this comparison
were removed from the application source after measurement. No release was made.
