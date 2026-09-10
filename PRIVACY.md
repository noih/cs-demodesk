# CS DemoDesk Privacy Policy / 隱私權政策

Last updated / 最後更新：2026-09-10

## English

### Scope and developer

This policy describes CS DemoDesk, developed by NOIH. CS DemoDesk is a desktop application for analyzing Counter-Strike 2 demo files and exporting highlight videos. It does not require a CS DemoDesk account.

### Local processing

CS DemoDesk reads demo files you select or that are found in configured scan directories. These files may contain player names, Steam IDs, match events, statistics, and gameplay information. The app processes this information locally to provide analysis, replay views, and video export. Exported videos may include player names and other content visible or audible during game replay.

The app stores settings, file paths, parsed results, map caches, downloaded tools, export job information, diagnostic logs, and generated videos on your device. It detects local Steam, CS2, and tool installations, or uses paths you specify. It may launch CS2 and external tools such as HLAE, FFmpeg, and Source 2 Viewer to perform the requested operations.

CS DemoDesk does not upload your demo files, player records, analysis results, logs, or videos to the developer. The developer does not operate a server to receive this data. The app does not include Google Analytics, advertising trackers, usage analytics, or automatic crash-report uploads, and does not sell your data.

### Network connections and third parties

CS DemoDesk is not entirely offline:

- The standalone EXE checks GitHub for application updates at startup and when About is opened. The packaged version does not perform this GitHub application-update check; Microsoft Store distribution and updates are handled by Microsoft.
- When you choose to download a tool, the app contacts GitHub and its download infrastructure to retrieve release information and tool files.
- Website links open the selected third-party website in your browser. A missing WebView2 Runtime prompt can open Microsoft's download page with your confirmation.

These requests disclose ordinary connection information, such as your IP address and request headers, to the service receiving the request. They do not include demo contents or player records as part of CS DemoDesk's update or tool-download requests. Microsoft, GitHub, Steam, CS2, and other external software or websites operate under their own privacy policies. Their independent network activity is outside this policy's scope.

### Storage and deletion

Local data remains until you delete it, clear it using the app's available controls, or remove the relevant files yourself. Settings provides controls to clear analysis data, videos, and map caches. Removing a demo through the app deletes its source demo file; existing exported videos are retained unless separately deleted.

You can inspect the active data directory in Settings. Changing that directory requires a restart and does not move or delete existing data. To remove remaining local data, close the app and delete the relevant data directories and app configuration files. Files in custom directories and exported copies may remain after uninstalling the app. Directories managed by your own backup or synchronization software are subject to that software's behavior.

### Contact and policy changes

For privacy questions, contact NOIH through [GitHub Issues](https://github.com/noih/cs-demodesk/issues). Issues are public: do not post private demo files, logs, or personal information. Information you voluntarily submit there is received by GitHub and may be visible to the developer and others.

If the app's data handling changes, this document will be updated with a new revision date.

## 繁體中文

### 適用範圍與開發者

本政策適用於 NOIH 開發的 CS DemoDesk。這是用於分析 Counter-Strike 2 Demo 與輸出精華影片的桌面程式，不需要建立 CS DemoDesk 帳號。

### 本機處理

程式會讀取您選取，或在設定的掃描目錄中找到的 Demo。檔案可能包含玩家名稱、Steam ID、比賽事件、統計與遊戲資訊。這些資料在您的裝置上處理，用於分析、回放與影片輸出。輸出影片可能包含回放時可見或可聽見的玩家名稱及其他內容。

程式會在裝置上儲存設定、檔案路徑、解析結果、地圖快取、下載的工具、輸出工作資訊、診斷紀錄與影片，並偵測本機 Steam、CS2 與工具安裝位置，或使用您指定的路徑。執行功能時可能啟動 CS2、HLAE、FFmpeg、Source 2 Viewer 等外部程式。

CS DemoDesk 不會將 Demo、玩家資料、分析結果、紀錄或影片上傳給開發者。開發者沒有用來接收這些資料的伺服器。程式不包含 Google Analytics、廣告追蹤、使用行為分析或自動當機回報上傳，也不販售您的資料。

### 網路連線與第三方

CS DemoDesk 並非完全離線：

- 獨立 EXE 版會在啟動及開啟關於視窗時向 GitHub 檢查程式更新。封裝版本不執行這項 GitHub 程式更新檢查；Microsoft Store 的發佈與更新由 Microsoft 處理。
- 您按下工具下載時，程式會連線至 GitHub 及其下載服務，取得版本資訊與工具檔案。
- 網站連結會在瀏覽器開啟第三方網站。缺少 WebView2 Runtime 時，提示視窗可經您確認後開啟 Microsoft 下載頁。

接收請求的服務會取得 IP 位址與請求標頭等一般連線資訊。CS DemoDesk 的更新與工具下載請求不會包含 Demo 內容或玩家資料。Microsoft、GitHub、Steam、CS2 及其他外部程式或網站有各自的隱私政策；其獨立網路活動不在本政策範圍內。

### 儲存與刪除

本機資料會保留至您使用程式提供的清除功能，或自行刪除相關檔案。設定頁可清除分析資料、影片與地圖快取。在程式內移除 Demo 會刪除來源 Demo 檔案；已輸出的影片需另外刪除。

您可在設定查看目前資料目錄。修改資料目錄需重新啟動，既有資料不會搬移或刪除。若要移除剩餘本機資料，請先關閉程式，再刪除相關資料目錄與程式設定檔。自訂目錄中的檔案與匯出的副本可能在解除安裝後保留。若您使用備份或同步軟體管理目錄，資料亦受該軟體的行為影響。

### 聯絡與政策更新

隱私問題可透過 [GitHub Issues](https://github.com/noih/cs-demodesk/issues) 聯絡 NOIH。Issues 是公開的，請勿張貼私人 Demo、紀錄或個人資訊。您主動提交的內容會由 GitHub 接收，並可能供開發者及其他人查看。

若程式的資料處理方式有所變更，我們會更新本文件與最後更新日期。
