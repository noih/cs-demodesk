# CS DemoDesk

[English](README.md) | [繁體中文](README.zh-TW.md) | [简体中文](README.zh-CN.md) | 日本語 | [한국어](README.ko.md) | [Русский](README.ru.md)

Counter-Strike 2 のデモ向け Windows デスクトップツール（ポータブルな単一 `.exe`）。1 つの `.dem` ファイルから試合スタッツ、ハイライト動画の書き出し、試合全体の 2D リプレイを行います。

## 機能

### 試合スタッツ

試合結果とプレイヤーのパフォーマンスを、交戦、ユーティリティの使用、ラウンドの状況から振り返れます。比較チャート、ラウンドごとの推移、リコイル軌跡で試合を多角的に確認できます。

### ハイライト動画

- ハイライト（マルチキル、クラッチ、ニンジャディフューズ）を自動で検出してスコア付け
- 選んだクリップをバックグラウンドの CS2 で録画し、FFmpeg でエンコード
- 解像度、FPS、H.264 / H.265、CPU または NVIDIA
- HUD 要素ごとの表示切替、1 本の動画への結合
- チャットアプリで共有しやすいファイルサイズ上限（10 / 20 / 50 MB）

### 2D リプレイ

- プレイヤーの位置、視線方向、体力、アーマー、武器、所持金
- グレネード、スモーク / 炎 / フラッシュの範囲、C4 と解除のカウントダウン、キルフィード、音の聞こえる範囲
- ラウンド移動、再生速度、プレイヤーの追従
- 複数階層のマップは階層ごとにパネル表示。レーダー画像はローカルのゲームファイルから抽出

## ゲームファイルは変更しません

ゲームフォルダには何も書き込みません。プラグイン、スクリプト、cfg の追加も、ゲームファイルの変更もありません。録画は [HLAE](https://github.com/advancedfx/advancedfx) 経由で `-insecure` 付きの別 CS2 プロセスを起動し（HLAE を手動で使うのと同じ）、ゲーム内蔵の netcon コンソールでコマンドを送り、ゲーム設定は別の `USRLOCALCSGO` フォルダに保持するため、プレイヤー自身の設定には触れません。サードパーティのツールはすべてアプリ自身のデータフォルダにダウンロードされます。

| ツール | 用途 | 入手元 |
| --- | --- | --- |
| HLAE | 録画（mirv_streams） | [advancedfx/advancedfx](https://github.com/advancedfx/advancedfx) |
| FFmpeg | エンコード、結合、サイズ上限 | [BtbN/FFmpeg-Builds](https://github.com/BtbN/FFmpeg-Builds)（GPL） |
| Source 2 Viewer CLI | vpk からのレーダー画像抽出 | [ValveResourceFormat](https://github.com/ValveResourceFormat/ValveResourceFormat)（MIT） |
| demoparser | デモ解析（vendored） | [LaihoE/demoparser](https://github.com/LaihoE/demoparser)（MIT） |

## 注意事項

- Windows 専用。CS2 のインストールが必要です。
- 録画中の CS2 は既定で非表示です。ゲームウィンドウを表示するには、書き出し設定の「ゲームを非表示でバックグラウンド実行」をオフにしてください。
- 1 つの Steam アカウントで同時に起動できる CS2 は 1 つだけなので、録画中はゲームをプレイできません。書き出しジョブは 1 つずつ実行され、残りはキューに入ります。
- 録画用の CS2 は `-insecure` で起動するため VAC サーバーには接続できません。録画が終わると終了し、通常の起動には影響しません。
- CS2 のアップデートで HLAE が動かなくなることがあります。HLAE の新版が出たら設定からツールを再ダウンロードしてください。
- チャットアプリやブラウザで再生するなら H.264 を選んでください。
- NVIDIA エンコーダーには NVIDIA GPU が必要です。

## 設計

データベースは使用しません。設定、書き出しジョブ、解析キャッシュはファイルとして保存されます。既定の保存先はユーザーごとのローカル App データフォルダー `%LOCALAPPDATA%\dev.noih.demodesk\demodesk-data` で、設定から変更できます。デモは元の場所に保持され、個別に追加するか、指定フォルダのスキャンで読み込めます。

## ビルド

[Rust](https://rustup.rs)（stable）と Visual Studio Build Tools（C++ によるデスクトップ開発）、Node.js 24.20.0（`.nvmrc`）、WebView2（Windows 同梱）が必要です。

```powershell
npm install
npm run app:dev      # 開発：Vite + Tauri ウィンドウ
npm run app:build    # dist-portable\CS-DemoDesk-<version>.exe
npm run test:core    # Rust ユニットテスト
```

## リリース

リリースは [GitHub Actions](.github/workflows/release.yml) でビルドされ、build provenance の証明が付きます。ダウンロードしたファイルがこのリポジトリからビルドされたことを確認するには：

```powershell
gh attestation verify CS-DemoDesk-<version>.exe --owner noih
```

## ライセンス

Copyright (C) 2026 NOIH - <https://github.com/noih>

[GNU AGPL-3.0](LICENSE)。サードパーティのコンポーネントはそれぞれのライセンスに従います（上の表を参照。vendored の demoparser は `vendor/demoparser/LICENSE` に MIT ライセンスを保持）。
