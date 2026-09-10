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
