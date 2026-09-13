# 交接：整場通用分析資料與效能重整

> 2026-09-13：手動異常分析改由記憶體 FIFO 佇列執行，同場重複請求沿用工作，跨場依序處理。全域分析佇列顯示等待順位、目前步驟、完成與失敗原因，可重試與返回該場；切頁保留進度。佇列僅保留於本次執行，重啟後清空；分析結果另行保留。基本 demo 自動分析維持原流程。

> 2026-09-13 最新需求已改為逐規則行為次數，取消所有分數、扣分、頻率倍率及舊版相容。schema 2／10-match-behavior-counts 的現行定義見 [異常行為統計](scoring-rules.md)。以下舊分數與版本說明僅作歷史效能紀錄，不再是現行需求。

> 射擊轉向研究更新：普通轉向的平均速度與高速比例仍與合法回報樣本重疊；指定最後回合三次射擊同步角度切換約4537／4591／4898°/s，應獨立於低速直線轉向分析，前兩次下一tick立即切回。新增shot-synchronized-snap設計及逐規則頻率要求已記錄，原始對照在out/behavior-audit/turn-speed-review.md；本輪沒有調整production速度門檻。

> 最新頻率要求：所有規則（含命中率／TTD分布）皆須按獨立異常次數加重；各規則先計次加重，再合併同來源扣分，不能因成為支持證據就遺失頻率。事件型累加S_i×F(i)，統計型重估S×F(K)，長序列另提高程度。完整逐條頻率單位、上限與補差額例子見 [規則定義](scoring-rules.md#所有規則都必須因頻率加重)。此為設計要求，現行v9仍有未實作部分。

> 最新研究與設計：兩場逐發／逐tick分析已確認高命中長序列、長連跳保速及固定視角空中交替加速是原本七規則漏看的不同證據來源。新增規則設計涵蓋射擊成功率、首發、命中序列、連殺、煙／穿透／受閃命中、bhop、空中轉向、壓槍／落點與TTD，見 [下一版規則設計](scoring-rules.md#下一版獨立規則設計)。本輪未改production，v9仍是七規則；新設計不可宣稱已上線。原始研究在ignored out/behavior-audit/report.md、out/movement-audit/movement-report.md。

> 最新規則重設：`9-match-small-rules` 拆為瞬間對準、快速直線轉向、持續固定點追蹤、視角震盪及三個擊殺情境規則。先共用事件去重，再依勝出的小規則各自計頻率，避免弱轉向放大首次追蹤。快速直線轉向改基本 1／上限 15；透視方向的遮蔽跟隨、動態預瞄定義與資料啟用條件見 [完整規則定義](scoring-rules.md)。它們尚未有可靠遮蔽資料，不宣稱本次已能可靠識別純透視。

> 使用者最新共識：信用分數依異常程度與頻率累積，容許偶發輕微誤判。`8-match-frequency` 在事件去重後，前 2 次維持基本扣分，第 3／6／10 次起分別 ×2／×3／×4；反覆發生不再受偶發輕微扣分上限限制，但規則／類別與總分下限仍有效。以下 `7-match-scored` 數值保留作比較基準。

> 2026-09-13 最新：已修復所有測量被 adapter 轉為不扣分觀察的問題，規則集 `7-match-scored`；新增真實模型固定 hitbox 中心與獨立視角震盪規則，共用全場掃描。回報樣本冷啟動準備 24.473 秒、全部規則／10 人 10.489 秒，generic 11.91 MB、靜態資源 106.07 MB、外部 journal 0 bytes。兩位回報玩家重算為 40／92；後者仍只有輕微證據，不能宣稱已可靠辨識作弊。較長樣本在 CPU 91%／CS2 運行時的完全冷啟動超時（53.8／41.7 秒），仍待可比負載下驗證，不能概括宣稱所有冷啟動均達標。詳見 [最新實測與限制](analysis-performance.md#回報樣本修正2026-09-13)。以下「診斷永不扣分」敘述為歷史紀錄。

> 接續完成：真正 App 原生路徑在全新資料目錄通過驗收：每場 36.18 MB、首次準備 23.77 秒、全部規則／10 位玩家 24.37 秒，外部 journal 0 bytes。手動入口、單行三步進度、全員獨立分數與歷史均已驗證；基本自動分析保持不變。詳見 [最終 App 驗收](analysis-performance.md#app-效能驗收2026-09-12)。獨立規則校準與部分來源缺值仍如實標示，實驗觀察不冒充正式扣分。以下保留歷史交接證據。


> 最新使用者覆寫：本階段接受目前約 35 MB 的整場共用資料；10 MB 延後優化，不再是本階段驗收阻擋。轉換 ≤ 30 秒、全部 rules／全部玩家合計 ≤ 30 秒，以及手動信用分析／保留基本自動分析的要求不變。容量仍如實統計，不因放寬容量而放寬身體量測完整性。

> 最新使用者覆寫：完成分析後沒有符合扣分條件的資料（包含缺樣本）維持 100 分，失敗不給分；診斷狀態收於分析細節。三步驟進度（準備資料、分析玩家、評分）、全員獨立分數表及版本 `4-match-baseline` 見 [驗收更新](analysis-performance.md)。以下舊的「缺資料不得給 100」不再適用於總分定義。

> 2026-09-12 接續結果見 [整場分析驗收](analysis-performance.md)。手動整場入口已驗證；時間數字仍依賴另外準備的 body journal，不代表完整功能通過。通用資料 35.22 MB，容量及完整身體／遮蔽還原仍未達標。以下保留原交接證據。

更新：2026-09-11。這份文件記錄本次 session 最後的使用者共識，優先於舊文件中的效能目標與候選擷取提案。
本次只寫交接，沒有開始下一輪實作，也沒有建立新 session 或提交 Git commit。

## 1. 最新目標（不要改回舊方向）

```text
整場 demo（所有人物、事件、環境）
  → 一次轉成共用、與 rules 無關的通用分析資料
      轉換時間 ≤ 30 秒；整場通用資料先接受約 35 MB（10 MB 延後優化）
  → 所有啟用 rules 分析整場所有人物
      全部 rules 與全部人物合計 ≤ 30 秒
  → 統一去重、扣分上限與計分，按玩家歸屬輸出結果
整體 ≤ 1 分鐘
```

- 「約 5 分鐘、挑戰 30 秒」是較早目標，已被上述明確目標取代。
- **整場 demo 是轉換與分析的工作單位，玩家只是結果歸屬單位。**
- 不要每位玩家各匯出一份、各解壓一次、各掃描整場。每條 rule 處理全部玩家；不同 rules 可共用已解碼狀態與空間查詢。
- UI 切換玩家讀取已完成結果，不重新分析整場。最新要求：demo 基本自動分析保留；信用分數只由 detail 右上角手動觸發，一次全場全員計算，重新評分追加歷史。
- 通用資料預備多條 rules 所需的基礎資料：人物位置／視角／姿態與身體範圍、地圖幾何與邊緣、牆面／動態障礙、煙霧、閃光、事件、tick、身分、生命／隊伍、來源版本及缺值資訊。
- 規則各自使用通用資料執行演算法、產生觀察／證據與扣分候選；統一計分器處理去重與上限。
- **已否決將通用轉換綁定「準星吸附」候選。** 不可先用這條 rule 初篩，再只準備它的候選區間資料，然後宣稱那是完整通用資料。
- 規則內可以有自己的候選初篩，但不應決定共用資料是否存在。不能依開槍、可見、某個身體部位刪除通用來源。

容量核算：本階段接受約 35 MB 的整場通用資料，不是單一 rule 的結果。靜態地圖可按內容／版本共用，但須明列共用資源、每場資料、暫存尖峰，不能透過移到另一個目錄或只算結果來冒充達標。10 MB 保留為未來近似優化目標；當前驗收如實報 bytes，不設 10,000,000 bytes 阻擋。
冷啟動、暖快取及首次地圖準備成本分開報告，不能只以快取命中聲稱 30 秒。

## 2. 架構與產品共識

- 地圖／場景資料獨立於 rules。牆面、煙霧、人物姿態等模組可個別更新，以窄資料契約連接，維持 SOLID；不要建立動態外掛系統或不必要後台。
- 格式版本、模組實作版本、來源 demo／遊戲／地圖內容版本、ruleset 與 rule 版本分開保存；未知版本不可當成相容。
- 以狀態變更取代重複完整快照；靜態內容只存一次，共用位置／世界點不依觀察者重複存。
- 應優先調整資料表示、排除重複、直接解析、串流和緊湊編碼。近似必須交代誤差界限；不要直接把所有數字捨入到 3～4 位。
- 100 分不是作弊機率，也不能證明未使用外掛；有評估且無扣分才可 100。缺資料、無可評估規則或失敗不能視為通過。以減少誤判為優先。
- 去重後採同組最高扣分，套用規則／類別／整體上限，總分不低於 0，明細與總分對帳。歷史結果不覆寫。
- 第一條 rule 叫「準星吸附」：進入吸附的軌跡／速度、持續跟隨誤差、持續時間；不採看到敵人的反應時間，不要求射擊／壓槍，不限頭胸部。固定對準靜止目標不算。
- 牆、煙霧、致盲是加重情境，但未知遮蔽不加重。目前演算法保留 `rapid || follows`，不要未驗證就改成 AND。

## 3. 現況：流程可跑，但沒有達到最新目標

所有程式與文件變更仍在工作樹，很多檔案為 untracked。不要 reset、clean、刪除輸出或覆蓋它們。
沒有本 session 留下的長時間擷取／測試程序。沒有授權 commit；若後續授權，使用 `git -c commit.gpgsign=false commit ...`，不修改持久 signing 設定，交付 amend 簽署指令。

### 主要入口

| 功能 | 檔案 |
| --- | --- |
| 來源與版本契約 | `crates/demodesk-core/src/analysis.rs` |
| 封包場景／狀態差分 | `analysis/scene.rs`、`analysis/journal.rs` |
| 人物與致盲事件、煙霧狀態 | `analysis/measurements.rs`、`analysis/smoke.rs` |
| 共用身體量測 journal 消費 | `analysis/body_journal.rs` |
| 原生解析接入 | `crates/demodesk-core/src/parser.rs`、`vendor/demoparser/parser/src/` |
| 規則註冊、統一計分、歷史 | `scoring/rules.rs`、`scoring/calculate.rs`、`scoring/history.rs` |
| 準星吸附及串流狀態 | `scoring/crosshair_lock.rs`、`scoring/crosshair_lock/stream.rs` |
| App 整合 | `crates/demodesk-core/src/engine.rs`、`src-tauri/src/lib.rs`、`src/components/ScoringTab.tsx`、`src/api.ts` |
| HLAE 擷取與解析 | `scripts/capture-analysis.ps1`、`scripts/analysis-capture-io.ps1`、`scripts/capture-analysis-attachments.mjs`、`scripts/analyze-attachment-capture.mjs` |
| 對齊與共用量測匯出 | `scripts/build-crosshair-input.mjs` |
| 完整性摘要稽核 | `scripts/check-analysis-run.mjs` |
| 真實 App Engine 驗證 | `crates/demodesk-core/examples/scoring_app_check.rs` |

上表 `analysis/` 與 `scoring/` 簡寫均相對於 `crates/demodesk-core/src/`。

現行 body journal 已共用世界點、視角、時間軸，以 gzip NDJSON 差分儲存；但仍是準星規則導向的量測格式，**不是新的完整通用分析資料格式**。
規則的串流狀態已可跨窗口保持、缺 tick 中斷、不保留整條軌跡；然而 Engine 仍逐玩家重新掃描同一檔案，這是要修改的效能缺口。
目前 gzip 使用已存在依賴 flate2；不要為壓縮再隨意引入新依賴。

### 正式扣分尚未啟用的原因

資料目前僅供診斷，正式 score 為 null。HLAE render attachment 不等於伺服器 hitbox；歷史模型內容版本和骨骼時間語意未核實，煙霧種子格不等於視覺濃度，物理碰撞面不等於不透明牆。
24 筆診斷觀察不是作弊真值。新的容量／速度測試不能移除這些資格限制；缺值不補零，不捏造位置／遮蔽。

## 4. 已完成的實測（可重用，不要重跑整場渲染當起手式）

同一本機 demo：23 回合、64 Hz、120,851 ticks，約 31 分 28 秒，30 個擷取段。

| 項目 | 結果 |
| --- | --- |
| 場景差分 journal | 90,398,136 bytes；109.3 秒；峰值工作集 248.7 MB |
| 全場舊版原始擷取 | 939,934,332 bytes；兩次程序合計 2,182.8 秒；CS2 峰值 4.65 GB |
| body journal | 409,939,343 bytes → gzip 123,400,937 bytes |
| body 匯出 | 82.37 秒；Node 峰值 RSS 870.5 MB |
| 有效資料 | 83,237 狀態、11,500 軌跡、19,890,760 樣本 |
| 對齊 | 945 窗口：872 matched、60 mismatch、13 insufficientData；不合格窗口排除並留原因 |
| App debug | 10 人首次／歷史重用／重新評分通過；每人分析約 36 秒，歷史約 0.23 秒 |
| App debug 合計 | 20 次分析＋10 次歷史讀取約 729.7 秒；峰值 331.1 MB |
| 規則輸出 | 19,890,600 有效樣本、24 筆診斷觀察；160 筆差異已核對為不足 3 ticks 的連續片段 |

Debug 數字不能當 release 效能。Cargo.toml 的 dev dependencies 已 opt-level 2，但本地核心仍非 release。正式驗收要報編譯模式與硬體，不以跨模式數字宣稱同等比較。

附加實驗：

- 解 FPS 上限：`capture-analysis.ps1 -FrameCap 0`，保留 64 Hz 步長，4096 ticks 完整，但含啟動仍 76.43 秒。沒有明顯加速；預設未更改，非預設參數寫入 job，避免混用檢查點。
- 從有效區間只取位置／視角：502,191 次更新，JSON gzip 13,839,131 bytes；float64 二進位 gzip 9,502,509；float32 gzip 8,699,653；float32 XOR 差分 gzip 7,544,729。這些數值原本就可精確以 float32 表示，還原後與基準 hash 相同，並非降低測量精度。
- **7.54 MB 不是達標**：缺完整時間涵蓋、生命週期、身體範圍、地圖、煙霧等。只能證明緊湊編碼有價值。
- 捨入實驗：96 個故意放在門檻邊界的合成案例，位置 2／3／4 位小數各改變 27／15／3 個案例；4 位仍新增 2 個候選。真實短片段原本零候選，不能證明捨入安全。目前不啟用有損捨入。

### 本機證據位置（皆 ignored，勿提交私人資料）

- `out/scoring-full-match/`：整場 tracking.ndjson、capture/captures.json 與 30 段檢查點、store/analysis/body-measurements/*.ndjson.gz、coverage.json、size-metrics.json、scene-metrics.json、app-metrics.json、app-stdout.txt。
- `out/scoring-long-validation/context.json`：來源、rounds 與玩家識別；不要把原文輸出到報告。
- `out/scoring-budget-experiment/`：上述容量投影與 metrics.json；檔案只是實驗格式，沒有正式契約。
- `out/scoring-uncapped-check/`：4096-tick 不限 FPS 實機擷取與 body 匯出。
- `out/scoring-capture-gzip-check/`：兩段 128 ticks 的直接 gzip 擷取、檢查點、匯出。
- `out/scoring-stream-compact/`：早期精度／容量對照。
- 實際 demo 路徑可從隔離 `out/scoring-full-match/store` 的資料庫／來源設定找回，或在遊戲 replays 目錄以 context 的 demo fingerprint 核對；不要猜另一份 demo。原始 demo 約 229.7 MB。
- HLAE 位於 `target/debug/demodesk-data/tools/hlae/`，window hook 位於 `out/scoring-packed-capture/window-hook.dll`。遊戲安裝在非系統碟 SteamLibrary；需要時讀取本機既有設定確認。

修過的完整性問題：引擎無換行日誌會替事件加前綴，解析器現在可識別命名空間，不漏兩筆完整事件。已補測試。壓縮日誌在完成 trailer、驗證後原子發布，失敗 pending 保留；檢查點不可覆寫，完整整場重跑可全數重用且不啟動遊戲。

## 5. 下一個 session 的工作順序

1. 先讀本文件，確認 git 工作樹及專案規範；不要把舊的單一 rule 候選方案當成已批准架構。
2. 盤點通用欄位：現成可從 demo 取得、可由共用資料計算、需版本地圖資源、仍無法可靠還原。定義最小實用資料契約與完整性／誤差語意，避免盲目匯出所有原始屬性。
3. 建立整場容量預算：人物／事件／動態環境／索引與版本資訊各自量測，共用地圖資源另列。不得省掉未知資料欄位的狀態而假裝完整。
4. 從現有 parser 做純離線、單次掃描、所有人物的最小原型。沿用已有的來源／版本契約，避免全場 JSON Value／反覆字串化；測試緊湊二進位、位元差分、共用字典與必要索引。
5. 用整場基準量測轉換 ≤ 30 秒及實際共用容量（本階段約 35 MB）。先驗證容量、還原一致性、缺值與時間連續性；不要先為了某條 rule 篩掉其他資料。
6. 改分析入口為整場，解碼／解壓一次供 rules 處理所有人物。保留既有 rule 狀態與統一計分器，最後分玩家保存結果；歷史、UI 與手動重新評分要一併核對，不要只加新 API 卻保留舊的重複掃描。
7. 驗證所有 rules／所有玩家合計 ≤ 30 秒、總流程 ≤ 1 分鐘，並對照舊診斷結果。若來源還原或預算做不到，交代實測瓶頸及未涵蓋內容，不以降低正確性或隱藏成本宣稱完成。

這個順序是實作建議，不是已證實能達標的承諾。使用者希望自主完成，不需要每一步等「繼續」；但本次交接請停在文件完成，下一個 session 才開始工作。

## 6. 檢查與操作注意

最近通過：核心 87 項、2 原有忽略；Clippy；受影響 Node／PowerShell 檢查；隱私與 diff 格式檢查。最後一輪只變更 FrameCap 實驗參數與文件，PowerShell 語法檢查及下面三項 Node／checkpoint 檢查已通過。

```powershell
cargo test -p demodesk-core
cargo clippy -p demodesk-core -p demodesk --all-targets -- -D warnings -A clippy::too_many_arguments -A clippy::useless_vec
node --test scripts/test-analysis-attachments.mjs scripts/test-crosshair-input.mjs
./scripts/test-analysis-capture.ps1
npm run test:privacy
git -c safe.directory=E:/projects/cs2-demodesk diff --check
```

兩項 Clippy allowance 是既有基準，不能擴大略過。不要把新目標當成全部已驗收。
其他通用資料測試見 `scripts/README.md`。前端 build/UI 在較早輪通過；如果改 App 整場入口，應再測 UI。UI 測試用 Vite preview 的 dist，不與 build 同時跑。

環境：Windows PowerShell；優先 rg；Python 讀寫中文指定 UTF-8。本 session 預設 shell sandbox 曾遇到 helper setup error，使用 require_escalated 通過自動審核。正常路徑仍先依新 session 工具狀態處理，不假定必然要提權。
避免動態 ScriptBlock 載入測試：先前被 AMSI 阻擋，已改靜態 dot-source 共用 IO，沒有停用防毒。不要為跑測試關閉防護。
本機真實 demo、玩家識別、個人路徑／日誌留在 ignored 輸出；測試使用合成資料。不要為節省容量刪除本機驗證證據。

延伸文件：`docs/analysis-data.md`（歷次實測與限制）、`docs/crosshair-lock.md`（rule 與資料來源研究）、`docs/match-behavior-scoring-plan.md`（較早產品共識）。舊文件若與本文件第 1 節衝突，依第 1 節的最新使用者要求。
