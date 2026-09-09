# CS DemoDesk

[English](README.md) | 繁體中文 | [简体中文](README.zh-CN.md) | [日本語](README.ja.md) | [한국어](README.ko.md) | [Русский](README.ru.md)

Counter-Strike 2 demo 的 Windows 桌面工具（單一可攜 `.exe`）：從一個 `.dem` 檔取得比賽統計、輸出高光影片、以 2D 回放整場比賽。

## 功能

### 比賽統計

整理比賽結果與玩家表現，涵蓋交戰、道具使用與各種回合情境，並透過比較圖表、回合趨勢及壓槍軌跡，從不同角度回顧比賽。

### 高光影片

- 自動偵測並評分高光片段（多殺、clutch、ninja defuse）
- 勾選的片段由背景執行的 CS2 錄製，FFmpeg 編碼
- 解析度、FPS、H.264 / H.265、CPU 或 NVIDIA
- 逐項畫面元素開關，可合併為單一影片
- 檔案大小上限（10 / 20 / 50 MB），便於在通訊軟體分享

### 2D 回放

- 玩家位置、視角、血量、護甲、武器、金錢
- 手榴彈、煙霧 / 火 / 閃光範圍、C4 與拆彈倒數、擊殺訊息、可聽見範圍
- 回合切換、播放速度、跟隨玩家
- 多樓層地圖每層各一面板；雷達圖自本機遊戲檔抽取

## 不修改遊戲檔案

不在遊戲目錄寫入任何 plugin、script 或 cfg，也不修改任何遊戲檔案。錄影透過 [HLAE](https://github.com/advancedfx/advancedfx) 以 `-insecure` 啟動獨立的 CS2 程序（與手動使用 HLAE 相同），經遊戲內建的 netcon 主控台送出指令，並以獨立的 `USRLOCALCSGO` 目錄保存遊戲設定，不影響玩家本身的設定。所有第三方工具皆下載至程式自己的資料目錄：

| 工具 | 用途 | 來源 |
| --- | --- | --- |
| HLAE | 錄影（mirv_streams） | [advancedfx/advancedfx](https://github.com/advancedfx/advancedfx) |
| FFmpeg | 編碼、合併、大小上限 | [BtbN/FFmpeg-Builds](https://github.com/BtbN/FFmpeg-Builds)（GPL） |
| Source 2 Viewer CLI | 自 vpk 抽取雷達圖 | [ValveResourceFormat](https://github.com/ValveResourceFormat/ValveResourceFormat)（MIT） |
| demoparser | 解析 demo（vendored） | [LaihoE/demoparser](https://github.com/LaihoE/demoparser)（MIT） |

## 注意事項

- 僅支援 Windows，且須安裝 CS2。
- 錄影時預設隱藏 CS2；可在輸出設定開啟「顯示遊戲畫面」。同一 Steam 帳號同時只能執行一個 CS2，錄影期間無法遊玩。輸出工作一次執行一個，其餘排隊。
- 錄影用的 CS2 以 `-insecure` 啟動，無法連線 VAC 伺服器；錄影完成後自動關閉，不影響正常啟動。
- CS2 更新可能導致 HLAE 失效，需等待 HLAE 發布新版後，再於設定頁重新下載工具。
- 需在通訊軟體或瀏覽器直接播放時請選 H.264。NVIDIA 編碼器需要 NVIDIA 顯示卡。

## 設計

沒有資料庫，並盡可能不保留內部狀態。所有內容都是執行檔旁 `demodesk-data\` 下的純檔案：設定與輸出工作是 JSON 紀錄；解析結果、回放串流、雷達圖是帶版號的快取，隨時可刪，demo、格式或遊戲版本變更時自動重建。demo 清單每次重新整理都直接掃描 replays 目錄，在程式外新增、搬移、刪除檔案都不會有副作用。

## 建置

需要 [Rust](https://rustup.rs)（stable）與 Visual Studio Build Tools（使用 C++ 的桌面開發）、Node.js 24.20.0（`.nvmrc`）、WebView2（Windows 內建）。

```powershell
npm install
npm run app:dev      # 開發：Vite + Tauri 視窗
npm run app:build    # dist-portable\CS-DemoDesk-<version>.exe
npm run test:core    # Rust 單元測試
```

## 發版

Release 由 [GitHub Actions](.github/workflows/release.yml) 建置並附 build provenance 證明。驗證下載的檔案確實由本 repo 建置：

```powershell
gh attestation verify CS-DemoDesk-<version>.exe --owner noih
```

## 授權

Copyright (C) 2026 NOIH - <https://github.com/noih>

[GNU AGPL-3.0](LICENSE)。第三方元件依各自授權（見上表；vendored 的 demoparser 保留其 MIT 授權於 `vendor/demoparser/LICENSE`）。
