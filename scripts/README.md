# Developer scripts

See [Compatibility diagnostics](../docs/compatibility-diagnostics.md) for the CS2
update scanner, baseline workflow, regression checks, and log retention rules.
The same guide covers `check-analysis-compatibility.ps1` for offline anomaly-analysis
coverage and `compare-native-attachments.mjs` for measured game attachment positions.
See [Background recording](../docs/background-recording.md) for runtime ownership,
hidden-window hooks, and crash behavior.

Run `npm run test:replay` for pure replay-state regressions. It covers the
reported round-9 fake defuses, normal kit/no-kit defuses, explicit aborts,
death, and seeking. Current CS2 demos can omit `bomb_abortdefuse`; sampled
`is_defusing` state is therefore checked even when no abort event arrives.
Replay schema 3 rebuilds earlier cached streams to include that state.
Replay schema 7 also preserves carried inventory and sampled ownerless equipment.
Replay tests cover inventory counts, pickup/seek state, local equipment icons and team-colored death markers.

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
Set `TEST_UI_DEV=1` to run the same checks against Vite development mode, including
dependency preprocessing and source loading. This uses an isolated cache under `out/`.

## Documentation screenshots

After `npm run build`, run `node scripts/store-screenshots.mjs <data-directory>/parsed [more-parsed-directories...]`
with `PLAYWRIGHT_MODULE` set when necessary. The source needs a completed anomaly
analysis in the sibling `behavior-analysis` directory and enough matches to fill
the sidebar. Additional source directories are deduplicated by demo path. The script reads caches,
anonymizes players, and writes eight screenshots per language plus a theme comparison
to ignored `out/store-screenshots/`. Copy only the selected README images into
`docs/images/<language>/`; keep source caches and diagnostics local.
Replay screenshots require cached smoke and radar data. Videos are read from the
sibling `clips` directory; without completed videos, the video page shows its empty state.

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
To release, prepare a UTF-8 Markdown file containing concise English release notes without a heading
and run:

```powershell
.\scripts\release.ps1 1.3.0 -NotesFile out/release-notes/1.3.0.md
```

Keep notes in the ignored `out/` directory or outside the repository. They are
read as local input and are never staged by the script. Missing or empty notes
stop the release before any version changes. Relative paths are resolved from
the directory where the command is run.

The script requires [GitHub CLI](https://cli.github.com/) authenticated with
repository write access (`gh auth login`), plus the existing Git signing setup.
It keeps the version/checks/signed-commit/signed-tag flow, pushes the release
commit, and creates a draft with the supplied notes, download links and a full
changelog link. Only after draft creation succeeds does it push the signed tag.
Actions verifies the draft's notes and target commit before building, then
uploads the EXE and SHA256SUMS and publishes the draft without rewriting its
description. A failed build or upload leaves the release unpublished.

If draft creation fails after the commit was pushed, fix the reported issue and
create the draft manually (include the download/changelog footer in the notes):

```powershell
$commit = git rev-parse HEAD
gh release create v1.3.0 --repo noih/cs-demodesk --draft --target $commit --title v1.3.0 --notes-file out/release-notes/1.3.0.md
git push origin v1.3.0
```

If the draft already exists but its tag was never pushed, verify its notes and
target commit before pushing the local signed tag; do not recreate the tag.
For a transient workflow failure with no code changes, rerun the failed jobs.

If a failed draft build needs code fixes, commit and sign the fixes, push main,
then retry the existing version:

```powershell
git push origin main
.\scripts\release.ps1 1.0.31 -RetryDraft
```

Retry requires a clean main branch synced with origin, matching project versions,
a signed HEAD, an existing remote tag, and an unpublished draft with notes.
It runs the checks again, replaces the local tag with a signed tag at HEAD,
updates the draft target while preserving its notes, and pushes the tag with
an explicit force-with-lease against the previously observed remote tag ID.
Published releases are rejected and the draft is checked again after the tests.
Do not retry while an earlier release workflow is still running.

If signing or draft update fails, the remote tag is unchanged. A rejected lease
means the remote tag changed: inspect that change before retrying; never replace
the lease with a plain force push. The draft target may already point at the new
commit, and the local tag may already have been updated. Retry fetches without
overwriting tags so it can recover after these partial failures.

Run `powershell -NoProfile -File scripts/test-release.ps1` for offline flow tests
(all Git, GitHub, build and signing commands are mocked).

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

## Cached 2D load benchmark

See [Replay loading performance](../docs/replay-loading-performance.md) for the
large-cache stage benchmark, validation and measurement limits.

Native 2D first-frame benchmark: benchmark-native-replay.mjs takes CDP endpoint, map filter and optional run count; see [measurement details](../docs/replay-loading-performance.md#native-webview2-binary-cache-comparison).

Utility camera checks: `node --test scripts/test-utility-camera.mjs` verifies both
flash and HE tracking, the shoulder-to-flight bridge, detonation hold and callback
release at the next shot. Event-selection and timing checks live in
`cargo test -p demodesk-core render::utility::`.

Opening camera experiment: `node scripts/test-editing-opening.mjs` checks the
1.8-second route following, acceleration and handoff. The ignored `record_editing_experiment`
harness accepts variant `opening`; set teammate equal to player and switchTick
to startTick + 1.8 * tickRate. Production exports bundle `render/opening-camera.mjs`; map collision and
route selection are not automatic. See `scripts/editing-opening.mjs`.

Combat replay selection and tail preservation: `cargo test -p demodesk-core render::perspectives::`. UI checks exercise all four independent editing switches and verify their true/false values in actual export submissions (native render mocked).

Shared camera transitions: `node --test scripts/test-camera-transition.mjs scripts/test-editing-team-push.mjs`
checks near/far decisions, shortest rotation, seek/obstruction cuts and stable decisions.
`render/camera-transition.mjs` exports `createCameraTransition`: create once per shot boundary,
then sample live source/destination views with elapsed seconds. Views require x/y/z/rX/rY/rZ/fov.
Defaults: at most 200 world units, 60 degrees per rotation axis and 25 degrees FOV difference;
nearby shots blend over 0.75 seconds. Pass `continuous: false` for timeline jumps,
or `pathBlocked: true` for known obstructions. Proximity alone does not prove collision clearance.
Used by team-push exports and experiments; combat/utility replay cuts and the opening's
approved magnet approach keep their existing behavior.

Shared fixed encounter camera search: `cargo test -p demodesk-core render::camera_search::`
checks full-interval framing, blocked/unknown geometry rejection, and lead-in trimming
without dropping required combat. The solver is shared by team-push exports and the local fixture runner.
It generates 24 directions × 3 distances × 3 heights × 3 FOVs relative to subject bounds,
checks body margins/size/separation and static chest/head rays over every supplied sample,
then ranks accepted cameras. An empty result means keep the existing POV.

For a local fixture set `DEMODESK_CAMERA_SEARCH` to a JSON file containing `dataRoot`,
`game`, `vrf`, `map`, `samples: [{tick, subjects: [[x,y,z], ...]}]`, `requiredTick`, and
`output`, then run `cargo test -p demodesk-core render::camera_search::plan_camera_experiment -- --ignored --nocapture`.
Subjects are stable-order feet positions. Supply dense samples covering the whole shot;
`requiredTick` is the earliest indispensable action, including pre-hit context. The search
may shorten only the lead-in, returning `startTick` and ranked candidates plus rejection
diagnostics. Copy the first candidate's position/target/fov and startTick into the
team-push experiment's `encounter` manifest to record the result without manual placement.
Limits: standing body envelopes and static physics approximate the image; dynamic props,
smoke, corpse animation, map-version matching and continuous between-sample visibility
are not yet qualified. Production uses conservative nearby T-team advancement and entry/trade rules.

Subtle encounter pan: `planEncounterAim(samples, encounter, tickRate)` in
`render/team-push-camera.mjs` (re-exported by `editing-team-push.mjs`) follows changes in the two-subject centroid, with 0.45-second
damping and 35% angular response. Yaw offset is capped at 2 degrees and pitch offset at
0.35 degrees; there is no random motion, roll, zoom or camera translation. Stable subject
tracks retain the deceased player's position. Store its result in `encounter.aimPath`;
playback interpolates by demo tick. Recheck projected body margins before recording a
new pan; the static search's framing qualification alone does not qualify the changed aim.
`node scripts/test-editing-team-push.mjs` checks stationary subjects, direction, bounds,
settling, deterministic output and playback integration. Production also verifies the new pan margins and falls back to the static camera if needed.

Team-push export checks: `cargo test -p demodesk-core render::team_push::` and
`node --test scripts/test-team-push-camera.mjs scripts/test-editing-team-push.mjs scripts/test-camera-transition.mjs`.
`render::experiments::plan_team_push_experiment` reads the existing experiment manifest from
`DEMODESK_EDITING_EXPERIMENT`, plans from real demo positions/events, and writes a sibling
`.plan.json` without launching CS2. The `utility` recording variant with `teamPush: true`
exercises actual exports; `openingCamera` and `combatPerspectives` may also be enabled.
