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
