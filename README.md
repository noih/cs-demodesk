# CS DemoDesk

English | [繁體中文](README.zh-TW.md) | [简体中文](README.zh-CN.md) | [日本語](README.ja.md) | [한국어](README.ko.md) | [Русский](README.ru.md)

A Windows desktop tool for Counter-Strike 2 demos (single portable `.exe`): match statistics, highlight video export, and a 2D replay of the whole match from one `.dem` file.

## Features

### Match statistics

Review match results and player performance across combat, utility and round situations. Comparison charts, round trends and recoil plots help explore the match from different angles.

### Highlight videos

- Highlights (multi-kills, clutches, ninja defuses) are detected and scored automatically
- Selected clips are recorded by CS2 in the background and encoded with FFmpeg
- Resolution, FPS, H.264 / H.265, CPU or NVIDIA
- Per-element HUD toggles, optional merge into one video
- File size limit (10 / 20 / 50 MB) for sharing in chat apps

### 2D replay

- Player positions, view angles, health, armor, weapon, money
- Grenades, smoke / fire / flash areas, C4 and defuse countdowns, kill feed, hearing range
- Round navigation, playback speed, follow a player
- Multi-level maps shown one level per panel; radar images extracted from the local game files

## Game files are never modified

Nothing is written into the game folder: no plugin, script, or cfg, and no game file is changed. Recording launches a separate CS2 process through [HLAE](https://github.com/advancedfx/advancedfx) with `-insecure` (the same as using HLAE manually), sends commands over the game's own netcon console, and keeps game settings in a separate `USRLOCALCSGO` folder so the player's settings are untouched. All third-party tools are downloaded into the app's own data folder:

| Tool | Purpose | Source |
| --- | --- | --- |
| HLAE | Recording (mirv_streams) | [advancedfx/advancedfx](https://github.com/advancedfx/advancedfx) |
| FFmpeg | Encoding, merging, size limit | [BtbN/FFmpeg-Builds](https://github.com/BtbN/FFmpeg-Builds) (GPL) |
| Source 2 Viewer CLI | Radar image extraction from vpk | [ValveResourceFormat](https://github.com/ValveResourceFormat/ValveResourceFormat) (MIT) |
| demoparser | Demo parsing (vendored) | [LaihoE/demoparser](https://github.com/LaihoE/demoparser) (MIT) |

## Notes

- Windows only; CS2 must be installed.
- Recording hides CS2 by default. Turn off "Hide game in background" in the export dialog to show the game window.
- One Steam account can run only one CS2 at a time, so the game cannot be played while recording. Export jobs run one at a time and queue.
- The recording instance is launched with `-insecure` and cannot join VAC-secured servers; it closes when recording finishes and does not affect normal launches.
- A CS2 update can break HLAE until HLAE releases a fix; re-download the tools from Settings once a new HLAE version is available.
- Choose H.264 for playback in chat apps and browsers.
- NVIDIA encoders require an NVIDIA GPU.

## Design

No database is required. Settings, export jobs and analysis caches are stored as files in the per-user local app data directory (`%LOCALAPPDATA%\dev.noih.demodesk\demodesk-data`) by default; the data folder can be changed in Settings. Clearing that setting restores the default on restart. Existing files are not moved; to reuse an older portable data folder, select it in Settings. Demos stay at their original paths and can be added individually or discovered through scan folders.

## Build

Requires [Rust](https://rustup.rs) (stable) with Visual Studio Build Tools (Desktop development with C++), Node.js 24.20.0 (`.nvmrc`), and WebView2 (included in Windows).

```powershell
npm install
npm run app:dev      # development: Vite + Tauri window
npm run app:build    # dist-portable\CS-DemoDesk-<version>.exe
npm run app:release  # portable EXE + unsigned Store MSIX (Windows SDK required)
npm run test:core    # Rust unit tests
```

## Releases

Releases are built by [GitHub Actions](.github/workflows/release.yml) and include the portable EXE and its SHA256 checksum. The unsigned MSIX and its checksum are available in the workflow run as the `store-msix-<tag>` artifact, retained for 90 days for manual Microsoft Store submission. Both binaries receive build-provenance attestations. To verify that a download was built from this repository:

```powershell
gh attestation verify CS-DemoDesk-<version>.exe --owner noih
```

## License

Copyright (C) 2026 NOIH - <https://github.com/noih>

[GNU AGPL-3.0](LICENSE). Third-party components keep their own licenses (see the table above; the vendored demoparser retains its MIT license in `vendor/demoparser/LICENSE`).
