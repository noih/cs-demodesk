# Developer scripts

See [Compatibility diagnostics](../docs/compatibility-diagnostics.md) for the CS2
update scanner, baseline workflow, regression checks, and log retention rules.
See [Background recording](../docs/background-recording.md) for runtime ownership,
hidden-window hooks, and crash behavior.

Run `npm run test:replay` for pure replay-state regressions. It covers the
reported round-9 fake defuses, normal kit/no-kit defuses, explicit aborts,
death, and seeking. Current CS2 demos can omit `bomb_abortdefuse`; sampled
`is_defusing` state is therefore checked even when no abort event arrives.
Replay schema 3 rebuilds earlier cached streams to include that state.

See [Recoil calibration SOP](../docs/recoil-calibration.md) for the one-command, hidden CS2 capture and fixed reference update:

```powershell
python scripts/update-recoil-reference.py --game "D:/SteamLibrary/steamapps/common/Counter-Strike Global Offensive/game/csgo" --weapons m4a1_silencer,ak47,m4a1
```

## UI checks

Run `npm run build` before `npm run test:ui`. The browser check requires Chrome
and Playwright; set `PLAYWRIGHT_MODULE` if the package is outside Node's normal
module path. Set `UI_SCREENSHOT_DIR` to an existing folder to save screenshots.
It tests the production build with mocked Tauri IPC; native dialogs, filesystem
operations and CS2/FFmpeg recording require a separate installed-app check.

## Documentation screenshots

After `npm run build`, run `node scripts/store-screenshots.mjs <data-directory>/parsed [more-parsed-directories...]`
with `PLAYWRIGHT_MODULE` set when necessary. The source needs a completed anomaly
analysis in the sibling `behavior-analysis` directory and enough matches to fill
the sidebar. Additional source directories are deduplicated by demo path. The script reads caches,
anonymizes players, and writes five screenshots per language plus a theme comparison
to ignored `out/store-screenshots/`. Copy only the selected README images into
`docs/images/<language>/`; keep source caches and diagnostics local.

## Private test data

Keep real demos, parser exports, screenshots and comparison reports in ignored
`out/`, `target/` or `research/` directories. Use synthetic player names and IDs
in committed tests. Do not copy real Steam IDs or private file paths into comments,
documentation or fixtures. Ignoring a file does not untrack an earlier commit.

Run `npm run test:privacy` before committing, or
`node scripts/check-repo-privacy.mjs --staged` to check the index. The release
workflow runs this check too. It flags common Steam identifiers, profile URLs,
personal paths and tracked capture output without echoing matched identities.
It is a pattern check, not proof of anonymity: review player names and images manually.

## Microsoft Store (MSIX)

Run npm run app:msix on Windows with the Windows SDK installed. It builds the
x64 Tauri app, generates Store icons from icon.png,
and runs MakeAppx validation. Output: out/msix/CS-DemoDesk-<version>-x64.msix.
The package is unsigned for Partner Center upload; it is not directly installable
by double-clicking until signed. The portable build remains npm run app:build.

Store identity is in packaging/msix/AppxManifest.xml. The script reads the version
from package.json and appends .0 (required for Store submissions).
All builds default to Tauri's per-user local app data directory, because the
package installation directory is read-only. Existing custom data selection is
respected. Updates are delivered by Store; there is no app updater integration.

Before submitting, test an installed package on a clean Windows machine:
WebView2 availability, first launch, data directory selection/reset, demo import,
radar extraction, HLAE/CS2 launch, FFmpeg export, and persistence across an update.
MakeAppx validates packaging, not these runtime behaviors. WebView2 is a runtime
prerequisite. Before creating a webview, the app checks the installed Runtime; if unavailable, a native dialog offers the official Evergreen download page. Install it and restart the app. The package does not bundle a Runtime installer.

Certification notes must explain runFullTrust: local demo access, launching
CS2/HLAE for offline recording and FFmpeg for export. Explain that recording
uses -insecure and that third-party tools are downloaded on demand.

Run npm run app:release to build both artifacts locally from one x64 compilation.
scripts/release.ps1 keeps the existing version/tag/push flow; the release workflow
runs app:release and publishes the EXE and its SHA256SUMS to GitHub Releases.
The unsigned MSIX and its own SHA256SUMS are stored in the workflow run as
store-msix-<tag> for 90 days. Download it from Actions, then extract the ZIP.
Both binaries receive GitHub build-provenance attestations. This does not submit
to Partner Center: upload the MSIX there separately after installed-app testing.

Store listing: https://apps.microsoft.com/detail/9N5G4VXSDGS5
Future Store update checks must use Store availability, not GitHub's latest tag:
GitHub releases can precede Store certification. Package identity alone does not
prove Store installation (sideloaded MSIX also has identity).

Portable EXE builds check GitHub's latest published stable release once per
process, with a 10-second timeout. About shows an update indicator and release
link when newer; errors are non-blocking and retried next launch. MSIX identity
short-circuits this request (including sideloads). Store updates remain managed
by Microsoft Store. No automatic binary replacement is performed.

Both release.ps1 and msix.ps1 validate Store versions before build or release
side effects: three numeric segments, major >= 1, each <= 65535, no leading zeros.
Run powershell -NoProfile -File scripts/test-store-version.ps1 for boundary checks.

To manually test the missing-WebView2 native prompt without uninstalling Runtime:
cargo test -p demodesk --lib webview_runtime::tests::missing_runtime_install_guidance -- --ignored
The prompt uses the Windows user interface language (Traditional/Simplified Chinese, Japanese, Korean, Russian, or English; other languages fall back to English). Choose No to exit, or Yes to open Microsoft's official download page.
See [Video encoding](../docs/video-encoding.md) for quality defaults, size limits, and FFmpeg regression checks.


Shared scoring analysis tools and source/version requirements are documented in
[analysis-data.md](../docs/analysis-data.md). Run their synthetic checks with:

```powershell
node --test scripts/test-analysis-contract.mjs scripts/test-analysis-geometry.mjs scripts/test-analysis-attachments.mjs scripts/test-attachment-geometry.mjs scripts/test-analysis-hitboxes.mjs scripts/test-crosshair-input.mjs
```

Use isolated ignored output directories for real captures and scoring integration
checks; `scoring_app_check` parses only the explicitly supplied demo and disables
Steam replay scanning in its isolated store. The app's rule remains diagnostic
until source qualification and independent calibration pass.


State-change journals are the preferred analysis export (`analysis_journal`), not
per-tick JSON snapshots. See [journal format and measurements](../docs/analysis-data.md#正式資料方向起始狀態變更紀錄).
Additional checks: `node --test scripts/test-analysis-journal.mjs scripts/test-analysis-manifest.mjs`.

`build-crosshair-input.mjs LOG TRACKING.ndjson CONTEXT DATA_DIRECTORY RESOLUTION`
now publishes one shared body-measurement delta journal, consumed directly by the
native scoring path. It refuses to replace an existing journal. The JSON scene input
is retained only for legacy measurement comparisons. See `docs/analysis-data.md`.

New body journals are streamed as lossless `.ndjson.gz` files. `--captures MANIFEST`
accepts ordered hash-pinned capture segments and scans the scene once. Use
`capture-analysis.ps1` for controlled, checkpointed offline captures; it requires
explicit tool paths and attachment names and will not attach to an existing game.
Precision experiment commands and limits are in `docs/analysis-data.md`.

Capture logs now stream directly to gzip; legacy plain checkpoints remain readable.
Run `./scripts/test-analysis-capture.ps1` for atomic checkpoint publication and
`node scripts/check-analysis-run.mjs BODY.ndjson.gz CONTEXT.json NEW_SUMMARY.json`
for private-data-free coverage counts. Whole-match measurements and limitations
are recorded in `docs/analysis-data.md`.

Whole-match credit analysis is manual; normal demo auto-analysis is unchanged.
See [analysis performance](../docs/analysis-performance.md) for the compact
contract, cold/warm measurements, input-size accounting and remaining limits.
`analysis_compact` validates lossless generic conversion; `scoring_app_check`
exercises one match call, latest-result reuse and a full-match retry that atomically replaces every player’s previous result.
Do not loop `score_player(..., true)` as a performance benchmark.

The native `scoring_app_check` acceptance gate now fails when no measured samples
reach the rules, when a separately prepared body journal is used, or when the
30 s preparation / 30 s all-player analysis budgets are exceeded. Generic bytes
are reported; the current roughly 35 MB baseline is accepted and the 10 MB
optimization target is deferred. App scoring always uses the native match source;
legacy journals remain offline diagnostic inputs and cannot override App scoring.
`analysis_pose_probe DEMO NEW_OUTPUT_JSON [LAST_TICK]` audits the recorded animation
dictionaries, and `analysis_compact --inspect FILE` reports their round-trip digest.
These dictionary audits do not validate reconstructed body coordinates.
