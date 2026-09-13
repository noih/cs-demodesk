# 共用分析資料

最新整場手動入口、緊湊格式與實測見 [整場分析驗收](analysis-performance.md)。demo 基本自動分析維持原樣；信用分數不再於開啟或切換玩家時自動觸發。

此層重建可供多條規則使用的資料，不判定作弊、不扣分。`scoring` 不擁有地圖或場景重建實作。

| 模組 | 輸入 → 輸出 | 實作位置 |
| --- | --- | --- |
| packet-scene | demo 封包 → 原始 entity、pose fields、煙霧 bytes | `crates/demodesk-core/src/analysis/scene.rs` |
| player-measurements | demo → 玩家位置、角度、事件、原始閃光資料 | `analysis/measurements.rs` |
| smoke-journal | 接收到的 bytes → journal records 或解碼錯誤 | `analysis/smoke.rs` |
| 物理幾何適配器 | Source 2 Viewer glTF／buffer → 射線碰撞 | `scripts/analysis-physics.mjs` |
| dynamic-collision | 單一 packet frame、模型參照、幾何 provider → 世界座標碰撞 | `scripts/analysis-scene-geometry.mjs` |
| model-resolution | 模型 ID、明確 VPK 清單 → 候選資源 | `scripts/resolve-analysis-models.mjs` |
| rendered-hitboxes | 模型命中盒、實測骨骼 provider → 世界膠囊／未量測部位 | `scripts/analysis-hitboxes.mjs` |
| attachment-geometry | 同骨骼附件、模型局部定義 → 世界位置／旋轉殘差 | `scripts/analysis-attachment-geometry.mjs` |
| attachment-alignment | 引擎擷取記錄、packet-scene → 姿態樣本、時間對齊診斷 | `scripts/analyze-attachment-capture.mjs` |

## 依賴與替換

原始場景保留煙霧 bytes，不依賴煙霧解码。`analysis_smoke` 範例是獨立組裝入口，消費 scene v1，逐實體輸出 records/error。煙霧解碼更新不需重跑原始擷取。

動態幾何只接受 `physicsProvider(resourceReference) → Promise<{trace(start, end)}>`，`trace` 回傳 `{triangles, hits}`；每筆 hit 包含 `fraction`、`surface`、可選 `interactAs`。座標為模型局部 game units，fraction 為線段比例。`trace-analysis-geometry.mjs` 負責將磁碟／glTF 適配器接上此介面。測試以另一個純記憶體提供者替換，無需修改動態場景。

規則只應消費符合需要的量測資料，不能回頭呼叫特定遊戲解码器。準星吸附目前仍接收離線 Input；正式 scene→rule 轉接須等身體位置、時間及遮蔽能力驗證完成。沒有建立外掛載入器或全域服務容器。

## 版本化 artifact

Rust `analysis::Artifact<T>` 與 Node `analysis-contract.mjs` 使用相同外層格式：

```json
{
  "contract": {"module": "packet-scene", "schemaVersion": 1, "implementationVersion": "0.1.0"},
  "source": {"demoFingerprint": null, "gameBuild": null, "gamePatch": null, "mapContentFingerprint": null},
  "dependencies": [],
  "data": []
}
```

- `schemaVersion`：輸入／輸出格式及語意的契約；不相容修改升版，消費端明確檢查模組及支援版號。
- `implementationVersion`：各模組獨立的演算法版號；修正煙霧不增加物理幾何或規則版號。同一介面可以有不同實作版本，但結果不可視為相同分析。
- `source`：原始 demo 內容指紋與已知來源版本。原始 demo 出口實際計算 SHA-1；`gameBuild` 取 demo header 的 build_num，`gamePatch` 取 patch_version，兩者分開保存；缺欄位維持 null，舊 artifact 可以沒有 gamePatch。map 內容版本未知時寫 null。模型路徑雜湊不能充當資源內容版本。
- `dependencies`：只記錄實際使用的输入內容指紋。煙霧依賴 scene artifact；動態幾何另依賴實際讀到的 glTF、buffer、幾何實作版本與查詢內容；姿態對齊依賴擷取記錄及 scene。Node 使用 SHA-256，指紋帶演算法前綴。這些是內容識別，不是來源真實性簽章。

來源版本未知不阻止離線診斷，但不得因此把 `assetVersionMatch` 從 unverified 改為 verified。格式相容也不代表來源遊戲版本相容。

歷史輸出採不覆寫方式保存。舊無版本研究輸出不自動套用新版本；新入口會拒絕，需重新擷取或日後加入明確的舊版讀取適配器。目前未實作分析快取排程器；未來以模組版本、參數及實際輸入指紋判斷重用，煙霧更新只重新產生煙霧及使用它的規則結果，保留可重用的幾何／姿態與歷史評分。

## 能力限制

碰撞並不等於遮蔽；玻璃、碰撞旗標與材質語意仍待驗證。煙霧 journal 不是渲染後的可見密度。附件座標不是已驗證的 hitbox 中心。這些模組保留 unknown／error，不能由規則以猜測值補足。

## 驗證

`cargo test -p demodesk-core`；`node --test scripts/test-analysis-contract.mjs scripts/test-analysis-geometry.mjs scripts/test-analysis-attachments.mjs`。涵蓋版本拒絕、獨立實作版本、替換 provider、幾何轉換及擷取紀錄完整性。未新增依賴。

附著點幾何與跨片段實測細節見 [crosshair-lock.md](crosshair-lock.md#跨片段與身體位置驗證2026-09-11)。新模組使用 `node --test scripts/test-attachment-geometry.mjs` 驗證局部／世界旋轉、縮放、四元數符號等價，以及不同模型或混合骨骼拒絕。


## 命中盒與資源版本

`analysis-hitboxes.mjs` 接收模型定義、實體觀察（resourceId、setIndex、scale、可空的 recordedContentFingerprint）及 `boneProvider(boneName)`。提供者回傳 `transformPoint(modelLocalPoint)`，負責實測骨骼的旋轉、平移與模型縮放；命中盒模組只負責轉換端點及縮放半徑，不依賴 HLAE 或某一角色名稱。

模型內容指紋與錄製時內容指紋明確相同才為 matched；後者不存在為 unknown，內容或模型 ID 不符直接停止重建。相同 build、patch 或路徑 ID 均不能替代內容指紋。即使內容 matched，渲染骨骼尚未證明等同伺服器命中盒時間，`eligibleForScoring` 仍為 false。

目前支援已核對的 capsule shape 2，translation-only 或其他 shape 另列未支援；缺少實測骨骼不採預設姿態。膠囊 min/max 是兩個端點，保留順序；center 僅是端點中點，不是額外偵測出的解剖部位。

## 應用程式整合與診斷輸入

`analysis_context` 匯出來源指紋、解析器回合、玩家身份與 tick rate。
`capture-analysis-attachments.mjs` 的 `includeIdentity: true` 使用引擎 controller/pawn
關係取得字串身份；無身份的舊 capture 仍可做幾何驗證，不能產生玩家評分輸入。
`build-crosshair-input.mjs` 以完整候選時間窗驗證對齊，依回合、玩家／實體生命週期、
模型與身體點切分軌跡；缺樣不補值，遮蔽一律 unknown，不用碰撞交點冒充不可見。

```powershell
cargo run -p demodesk-core --example analysis_context -- DEMO CONTEXT_JSON
node scripts/build-crosshair-input.mjs CAPTURE_LOG SCENE_JSON CONTEXT_JSON ISOLATED_DATA_DIR ANGULAR_RESOLUTION
cargo run -p demodesk-core --example scoring_app_check -- DEMO ISOLATED_DATA_DIR
```

角度精度必須明確提供；診斷預算不等於伺服器精度認證。輸出拒絕覆寫，重跑驗證使用
新的隔離目錄。實際驗證取得 9 名玩家的 400 段追蹤，應用引擎驗證 10 名玩家的自動
重用與手動追加歷史；36,640 筆測量未產生候選觀察。真實資料及身份均留在忽略的 out/。

尚待完成：歷史模型內容核對、伺服器姿態時間驗證、煙霧可見性／致盲資格、全場自動
測量產製，以及有獨立標註的正常、高技巧與外掛案例校準。這些條件未滿足前，不啟用
正式扣分；UI 顯示未評估原因，可查未校準觀察、門檻、回合時間與歷史版本。

## 正式資料方向：起始狀態＋變更紀錄

大量快照僅保留為離線對照，不作正式全場資料儲存格式。`analysis_journal` 在解析
出口逐 tick 交付最後一個封包狀態，只留前一狀態做差異；不累積整段 `Vec<SceneFrame>`。

```powershell
cargo run -p demodesk-core --example analysis_journal -- DEMO OUTPUT.ndjson FIRST LAST STEP
cargo run -p demodesk-core --example analysis_journal -- DEMO TRACKING.ndjson FIRST LAST STEP --tracking
```

兩種 profile 使用相同 `scene-journal` v1 契約。第一行保存来源／模組版本與 profile；
首次出現的實體記完整起始狀態，後續只記 create、remove、set、unset。
欄位路徑首次出現才寫進字典，變更以欄位編號參照。相同長度的陣列只寫改變的索引，
煙霧不再因一個 byte 改變重寫整個陣列；實體 serial 或 class 改變時重新建立狀態。
null 值與移除欄位分開表示。每個觀測 tick 仍有時間記錄，不能將缺幀冒充不變。
結尾含幀數及最後 tick，截斷檔案拒絕視為完整。

`analysis-journal.mjs` 逐行套用變更，未變動的實體及分支共用記憶體；保留較早幀時
不受後續更新污染。`buildInputWindows` 只保留 128 ticks 加上前後 4 ticks 的對齊窗，
交付有界的規則輸入。消費端必須延續規則狀態，不可把各窗各自評分後相加，也不應
永久保存展開後的玩家×目標×部位交叉資料。舊 `build-crosshair-input` 整批 JSON CLI
仍僅供相容性校準對照，並非全場正式儲存入口。

HLAE 擷取預設使用差異協定 v4：變動欄位、明確 reset／remove、每幀一次 tick，附件
名稱只宣告一次。原值可精確表示為 float32 時才使用 little-endian float32 的 Base64
編碼，其他值維持原 JSON 數值；缺值仍為 null。v1/v2 完整記錄與 v3 未壓縮差異記錄
仍可讀取。`stateChanges: false` 僅供舊格式對照。數值沒有量化、取整或插值。

真實同片段 4,104 個 packet frames 的無損對照結果：

| 資料 | 完整快照 | 變更紀錄 | 減少 |
| --- | ---: | ---: | ---: |
| 完整場景 | 755,944,802 bytes | 9,456,834 bytes | 98.7% |
| 玩家追蹤 | 28,270,739 bytes | 3,258,807 bytes | 88.5% |

完整場景逐幀核對包含煙霧、pose 欄位；數值須還原為相同原始 f32 位元。
獨立 Node 程序串流讀取玩家追蹤約 0.05 秒、JS heap 峰值約 11 MB；這是單次本機
量測，未包含 CS2 程序、原始 demo 解析及規則計算。32 個窗中 30 個通過對齊，另 2 個
明確拒絕；通過的 3 個窗位於回合結束後，故實際交付 27 個規則窗。

ServerInfo 已保留原始 tick_interval 與 manifest；解析快取版本升為 15，時脈缺失或
不一致會失敗，不再默認 64。實測來源 interval=0.015625；與擷取時脈相同。
`analysis-manifest.mjs` 依 Clarity 的 Resources/LZSS 格式解碼出 463 個資源、334 個模型，
包含全部 4 個實測玩家模型。這是路徑身份核對，不是歷史檔案內容雜湊證明。
來源：[Resources.java](https://github.com/skadistats/clarity/blob/master/src/main/java/skadistats/clarity/processor/resources/Resources.java)、[LZSS.java](https://github.com/skadistats/clarity/blob/master/src/main/java/skadistats/clarity/util/LZSS.java)。


同一段 4,096 render ticks 的引擎擷取實测：v2 完整文字記錄 58,790,216 bytes，
v3 狀態差異 48,193,301 bytes，v4 狀態差異＋無損 f32 編碼 26,622,606 bytes
（相較 v2 減少 54.7%）；v4 還原得到 21,619 筆玩家樣本、零缺幀，解碼約 0.86 秒。
這包含各身體附件實際持續移動的資料，因此縮減幅度不等同靜態場景。數字是這個片段
的測量結果，不宣稱整場有固定大小或固定壓縮率。

## 狀態變更接入 App 評分

以 `.ndjson` 場景輸入執行以下命令，直接寫共用 `body-measurement-journal` v1：

```powershell
node scripts/build-crosshair-input.mjs CAPTURE.log TRACKING.ndjson CONTEXT.json DATA_DIRECTORY 0.01
```

`0.01` 是明示的診斷角度解析度預算，並非已驗證的伺服器精度。JSON 場景入口只保留
舊格式比對用途。新格式位於 `analysis/body-measurements/<sha1(sourceFingerprint)>.ndjson`，
每場共用一份；來源 hash 與各依賴保存在 header。身體部位與軌跡身份只宣告一次，
部位以數字索引引用；每 tick 只寫變動的玩家視角、部位位置與有效軌跡清單。
沒有資料的 tick 不補值；有效軌跡在狀態中明確列出。尾端幀數／tick 檢查防止截斷資料
被當成完整測量。完整寫入且同步後才以不覆蓋方式發布。

Rust `analysis::body_journal` 獨立於規則，逐行重建當前狀態。App 原生讀取，不依赖 Node。
準星規則只展開當前選定玩家的當前 tick，跨窗延續短期取得軌跡與持續跟隨累計值，
不保留完整軌跡；最後才統一去重及套用上限。證據保存取得軌跡、鎖定起點與終點、
實際 `sampleCount` 及完整區間統計；中間原值可由有 hash 的來源紀錄追溯。
規則版號升為 `experimental-2`、ruleset 為 `2-experimental`，既有歷史不覆寫。

同一真實片段：舊 10 份評分輸入合計 125,581,949 bytes，新共用紀錄
14,240,148 bytes（減少 88.7%）；500 條軌跡、596,940 筆樣本逐筆完全相同。
App Engine 驗證 10 名玩家、自動重用及手動追加歷史；零候選不代表玩家正常的標註。
切分點回歸與 100,000 樣本持續跟隨測試通過，後者證據僅保留兩個端點且樣本數仍正確。
此格式目前只輸送身體測量；遮蔽仍是獨立分析模組，沒有證據時保持 unknown。
正式資料資格、獨立校準與全場擷取自動產製仍未完成，因此不啟用正式扣分。


## 精度實驗與預設儲存

保留精度的 gzip 已接入匯出器及原生讀取器，預設檔名為 `.ndjson.gz`；不先產出整份
未壓縮資料再壓縮。gzip CRC／完整結尾均驗證，原始 v1/v2 `.ndjson` 仍可讀。
body journal v2 在結尾保存各窗對齊結果，包含拒絕原因；整份資料完成後才發布。
評分來源 hash 指向實際保存的壓縮檔；精度沒有改變。

對同一 596,940 筆真實樣本實測（decimal MB，gzip 預設等級）：

| 位置小數位 | 視角 | JSON | gzip | 最大目標方向偏差 |
| --- | --- | ---: | ---: | ---: |
| 原精度 | 原精度 | 14.24 MB | 4.33 MB | 0 |
| 2 | 原精度 | 7.39 MB | 2.20 MB | 0.001748° |
| 3 | 原精度 | 8.08 MB | 2.63 MB | 0.000173° |
| 4 | 原精度 | 8.71 MB | 3.01 MB | 0.0000174° |

真實片段原本沒有候選，各精度亦沒有；因此這不能證明低精度不會誤判。另測 96 個
合成門檻邊界案例，距離 16／64／512／2048、不同方向與門檻上下偏移：僅位置保留
2／3／4 位分別有 27／15／3 個案例的候選或片段起訖改變；4 位中有 2 個新增候選。
同時捨入視角也會改變邊界候選。這些是刻意設計的敏感案例，不是外掛辨識效能或
實際玩家誤判率。不能僅靠「四位小數夠精準」就啟用捨入。

目前採無損 gzip，降低精度只留在實驗工具。若後續採用近似，必須記錄每種資料的
誤差界限及版本，門檻附近回查原精度／標記不確定；不能因近似增加遮蔽扣分。
煙霧與牆面邊界不是優先降低精度的對象：微小邊界偏移可能改變遮蔽分類。粗範圍
可以先排除明顯不相交的情況，邊界仍需精查。視角則直接影響準星吸附的核心測量。

重跑（只輸出摘要，不產出多份捨入後資料）：

```powershell
node scripts/measure-analysis-precision.mjs BODY.ndjson.gz SIZE_SUMMARY.json
cargo run -p demodesk-core --example crosshair_precision -- BODY.ndjson.gz DECISION_SUMMARY.json
node --test scripts/test-analysis-precision.mjs
```

## 分段擷取與單次場景掃描

`capture-analysis.ps1` 使用明確指定的 demo／context／引擎／HLAE／window hook／附件清單，
隔離設定、隱藏視窗；已有 CS2 時不啟動。預設依 context 回合範圍，每段最多 4096 ticks，
逐段寫入檔案，不把全場 console 放在記憶體。每段必須通過範圍、時脈、完整幀驗證，
才保存 log hash 檢查點；再次執行先核對來源與工具版本，重用已完成段。
失敗的 pending log 保留供診斷。全部完成後才產生 `captures.json`。

```powershell
./scripts/capture-analysis.ps1 -Demo DEMO -Context CONTEXT.json -Cs2 CS2.exe -Hlae HLAE.exe -Hook WINDOW_HOOK.dll -OutputDir out/capture -Attachments @('EXPLICIT_ATTACHMENT')
node scripts/build-crosshair-input.mjs --captures out/capture/captures.json TRACKING.ndjson CONTEXT.json DATA_DIRECTORY 0.01
```

manifest 每筆要求 log 路徑及 SHA-256。依序逐段載入，同一個場景 journal 只掃描一次；
來源變更、時脈不符、片段重疊／倒序、缺幀與截斷明確拒絕。較遠的下一片段只跳過
中間場景，不把整段空隙累积在記憶體。兩個相鄰 128-tick 真實擷取完成；重跑重用
檢查點。前述 4096-tick 已有擷取切成兩段，全部 3404 個有效狀態逐筆相同，32 個對齊窗
中 2 個拒絕原因仍保存。這些驗證涵蓋分段流程；整場驗證結果見下節，資料仍未具正式評分資格。


## 整場驗證（2026-09-11）

同一份本機 demo，23 回合，64 Hz，範圍 120,851 ticks（31 分 28 秒），分成 30 段。
實際來源及玩家資料只保存在忽略的 `out/scoring-full-match`，不納入儲存庫。

| 階段 | 實測 |
| --- | --- |
| 場景狀態變更 journal | 90,398,136 bytes；109.3 秒；峰值工作集 248.7 MB |
| 原始擷取（本次舊版未壓縮基準） | 939,934,332 bytes；兩次執行合計 2,182.8 秒；CS2 峰值工作集 4.65 GB |
| 整場分析 journal | 409,939,343 bytes → gzip 123,400,937 bytes；無損減少 69.9% |
| 分析匯出 | 82.37 秒；Node 峰值 RSS 870.5 MB |
| 有效量測 | 83,237 狀態、11,500 軌跡、19,890,760 樣本 |
| 對齊窗口 | 945 個：872 matched、60 mismatch、13 insufficientData；擷取窗口無缺口 |

有效量測包含 59 段連續 tick 區間；11,200 次軌跡中斷不能當作連續跟隨。
稽核確認量測位於正式回合及通過對齊的窗口內，不合格窗口不會混入評分輸入。
跨段偵測連續性的等價性由串流規則測試覆蓋；窗口拒絕原因保存在 journal footer。
完整擷取後再次執行，30 段檢查點全部重用，未重新啟動遊戲。

第二段初次驗證發現兩筆事件被引擎無換行日誌加上前綴，舊解析器因此漏讀。
解析器改為辨識事件命名空間並保留嚴格 JSON／完整幀檢查，已補回歸測試；
完整的 pending 檔經重新驗證後保留並續跑，沒有重錄或以假資料補幀。
解析 delta 時不再先建立整份 JSON 記錄陣列；同段驗證峰值 RSS 從 404.4 降至 301.2 MB。

新版擷取直接串流寫入 gzip，容量限制仍依未壓縮 bytes，完成 trailer 後才驗證及發布
檢查點；舊版純文字檢查點仍可重用。兩段各 128 ticks 的實機壓縮擷取及後續匯出通過。
其中一段 1,458,569 bytes 壓成 410,020 bytes；這不是整場壓縮率估計。

此結果只證明整場流程可完成，不能宣稱效率已適合自動互動分析：遊戲重播擷取仍約
36 分鐘，且原始日誌成本很高。範圍內約 492.2 秒不在正式回合，可作後續省略擷取的
明確對象；本次為維持基準一致未跳過。牆面、煙霧與閃光加重扣分仍須來源資格與
獨立校準，不能因串流或壓縮驗證成功就啟用正式扣分。

重跑摘要稽核與原子檢查點測試：

```powershell
node scripts/check-analysis-run.mjs BODY.ndjson.gz CONTEXT.json NEW_SUMMARY.json
./scripts/test-analysis-capture.ps1
```


App 實際 Engine 驗證：10 名玩家共 19,890,600 有效樣本、24 筆診斷觀察，
首次分析／自動歷史重用／手動追加重新評分全部通過，正式 score 仍為 null。
匯出與有效樣本相差的 160 筆，已逐軌跡核對為不足 3 ticks 的連續片段。
開發版（未最佳化 debug）首次每人 35.99–37.21 秒，重新分析 35.77–36.90 秒，
歷史重用 0.226–0.232 秒；20 次完整分析及 10 次重用合計 729.7 秒，
原生程序峰值工作集 331,112,448 bytes。此為 debug 基準，不代表 release 效能。
目前每人各自串流讀取共用 journal，避免記憶體展開但仍有重複掃描成本。
24 筆觀察不是 24 次作弊，也未驗證為誤判；需要獨立片段校準後才能啟用扣分。

本輪檢查：核心 87 通過／2 原有忽略；Clippy（沿用兩項既有 lint 例外）、
受影響 Node 測試、原子檢查點 PowerShell 測試、256 檔隱私模式檢查及 diff 格式檢查通過。


## 新效能目標與路線評估（2026-09-11）

> 此節為較早提案。最新使用者要求已改為「與 rules 無關的整場通用資料約 10 MB、轉換 ≤ 30 秒、所有 rules／所有玩家分析合計 ≤ 30 秒」。依單一 rule 候選決定轉換資料的方向已被取代。請先讀 [最新交接](handoff-analysis-performance.md)。

使用者要求：擷取約 5 分鐘內，挑戰 30 秒；每場分析資料約 10 MB。
下一輪以整場十名玩家、冷啟動／暖快取分開量測，暫按新增的永久分析資料合計
10,000,000 bytes 控制；不含原始 demo 和按版本共用的地圖資源，但兩者與暫存尖峰
仍另報，不能把日誌挪到另一個目錄冒充減量。5 分鐘為必要門檻，30 秒尚未證實。

已做兩個有界實驗：

- `capture-analysis.ps1 -FrameCap 0`：保留來源 64 Hz 的固定步長，只解除 FPS 上限。
  4096 ticks 完整、匯出仍 3404 有效狀態，但含啟動耗時 76.43 秒，沒有足以支撐
  5 分鐘整場的改善。預設仍維持舊設定；非預設 frame cap 記入 job，避免混用檢查點。
- 從既有 83,237 有效狀態投影共用玩家位置／視角，502,191 次更新：JSON gzip
  13,839,131 bytes；二進位 float64 gzip 9,502,509；原生 float32 gzip 8,699,653；
  float32 位元 XOR 差分 gzip 7,544,729。所有值均已證實原本可精確以 float32
  表示，兩個 float32 版本解碼後與 float64 基準逐值 hash 相同。此為容量實驗，
  缺少未對齊區域、完整生命週期、身體點和證據，不能稱為完整 10 MB 方案。

優先順序：

1. **直接解析封包並串流初篩**：只取位置、視角、身分／生命／隊伍／姿態狀態，
   不產出完整場景 JSON；同一次掃描處理所有玩家。保留逐 tick 角度，不以降採樣
   換速度。現有 parser 可取得這些欄位；完整骨骼／精確遮蔽仍非現成能力。
2. **保守空間初篩，再對候選精查**：用已驗證可包住所有部位與姿態的範圍，加上
   視線來源誤差，排除不可能接近目標的區間；不以開槍、可見、頭胸部作必要條件。
   保留吸附前完整 acquisition lookback、跟隨段與放開邊界。未知邊界不能排除。
   初篩必須保留舊基準全部 24 筆診斷觀察，並以短吸附、隔牆、煙霧及無射擊合成
   案例驗證；24 筆全保留仍不等於已證明所有真實作弊的召回率。
3. **只保存可重現的精簡資料與證據**：玩家值一次保存，身體點僅保留候選區間，
   規則算出的相對角度不全場物化；記錄來源 hash、模組版本、參數與完整性。
   固定欄位二進位、整數 tick／ID、無損位元差分優先於捨入。10 MB 若不足，不得
   靜默砍掉異常候選或把剩餘資料標成已評估；明確報告預算未達標。
4. **候選區間才使用遊戲還原**：作過渡方案，合併相鄰區間、共用單次遊戲啟動，
   將 seek、預熱、擷取及校驗全部計時。高候選密度時可能仍超過 5 分鐘；不是
   30 秒保證。候選太多時不能任意採前 N 個便給滿分。
5. **純離線骨架／可見性還原**：最有機會移除渲染成本，但動畫解碼與來源模型
   相容性尚未解決。先以短片段對照現有完整基準證明可行，不先建立整套引擎。
   地圖碰撞、煙霧、致盲保持共用且版本獨立，靜態地圖加速索引按內容 hash 共用。
6. **其餘措施**：release 編譯、取消中介檔反覆解碼、移除附件旋轉等未使用輸出、
   降低渲染解析度可個別量測；加速播放若跳 tick 不採用；僅略過非正式回合最多
   省本場 26% 時間，不能單獨達標；多個遊戲程序成本高，不列為主路線。

驗收必須分開報告耗時、永久容量、暫存尖峰、來源涵蓋率、候選保留率與判定差異。
新的快速初篩不能直接代替精查或當成證明玩家無外掛；保留當前正式扣分未啟用狀態。
