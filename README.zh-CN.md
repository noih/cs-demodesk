# CS DemoDesk

[English](README.md) | [繁體中文](README.zh-TW.md) | 简体中文 | [日本語](README.ja.md) | [한국어](README.ko.md) | [Русский](README.ru.md)

Counter-Strike 2 demo 的 Windows 桌面工具（单个便携 `.exe`）：从一个 `.dem` 文件获取比赛统计、导出集锦视频、以 2D 回放整场比赛。

## 功能

### 比赛统计

汇总比赛结果与玩家表现，涵盖交战、道具使用与各类回合情境，并通过对比图表、回合趋势和压枪轨迹，从不同角度回顾比赛。

### 集锦视频

- 自动检测并评分集锦片段（多杀、残局、ninja defuse）
- 勾选的片段由后台运行的 CS2 录制，FFmpeg 编码
- 分辨率、FPS、H.264 / H.265、CPU 或 NVIDIA
- 逐项画面元素开关，可合并为单个视频
- 文件大小上限（10 / 20 / 50 MB），便于在聊天软件分享

### 2D 回放

- 玩家位置、视角、血量、护甲、武器、金钱
- 投掷物、烟雾 / 燃烧 / 闪光范围、C4 与拆弹倒计时、击杀信息、可听见范围
- 回合切换、播放速度、跟随玩家
- 多层地图每层各一面板；雷达图从本机游戏文件提取

## 不修改游戏文件

不在游戏目录写入任何 plugin、script 或 cfg，也不修改任何游戏文件。录制通过 [HLAE](https://github.com/advancedfx/advancedfx) 以 `-insecure` 启动独立的 CS2 进程（与手动使用 HLAE 相同），经游戏内置的 netcon 控制台发送指令，并以独立的 `USRLOCALCSGO` 目录保存游戏设置，不影响玩家自己的设置。所有第三方工具都下载到程序自己的数据目录：

| 工具 | 用途 | 来源 |
| --- | --- | --- |
| HLAE | 录制（mirv_streams） | [advancedfx/advancedfx](https://github.com/advancedfx/advancedfx) |
| FFmpeg | 编码、合并、大小上限 | [BtbN/FFmpeg-Builds](https://github.com/BtbN/FFmpeg-Builds)（GPL） |
| Source 2 Viewer CLI | 从 vpk 提取雷达图 | [ValveResourceFormat](https://github.com/ValveResourceFormat/ValveResourceFormat)（MIT） |
| demoparser | 解析 demo（vendored） | [LaihoE/demoparser](https://github.com/LaihoE/demoparser)（MIT） |

## 注意事项

- 仅支持 Windows，且须安装 CS2。
- 录制时默认隐藏 CS2；可在导出设置中开启“显示游戏画面”。同一 Steam 账号同时只能运行一个 CS2，录制期间无法游玩。导出任务一次执行一个，其余排队。
- 录制用的 CS2 以 `-insecure` 启动，无法连接 VAC 服务器；录制完成后自动关闭，不影响正常启动。
- CS2 更新可能导致 HLAE 失效，需等待 HLAE 发布新版后，再在设置页重新下载工具。
- 需在聊天软件或浏览器直接播放时请选 H.264。NVIDIA 编码器需要 NVIDIA 显卡。

## 设计

没有数据库，并尽可能不保留内部状态。所有内容都是可执行文件旁 `demodesk-data\` 下的纯文件：设置与导出任务是 JSON 记录；解析结果、回放流、雷达图是带版本号的缓存，随时可删，demo、格式或游戏版本变更时自动重建。demo 列表每次刷新都直接扫描 replays 目录，在程序外新增、移动、删除文件都不会有副作用。

## 构建

需要 [Rust](https://rustup.rs)（stable）与 Visual Studio Build Tools（使用 C++ 的桌面开发）、Node.js 24.20.0（`.nvmrc`）、WebView2（Windows 内置）。

```powershell
npm install
npm run app:dev      # 开发：Vite + Tauri 窗口
npm run app:build    # dist-portable\CS-DemoDesk-<version>.exe
npm run test:core    # Rust 单元测试
```

## 发布

Release 由 [GitHub Actions](.github/workflows/release.yml) 构建并附 build provenance 证明。验证下载的文件确实由本仓库构建：

```powershell
gh attestation verify CS-DemoDesk-<version>.exe --owner noih
```

## 许可

Copyright (C) 2026 NOIH - <https://github.com/noih>

[GNU AGPL-3.0](LICENSE)。第三方组件依各自许可（见上表；vendored 的 demoparser 保留其 MIT 许可于 `vendor/demoparser/LICENSE`）。
