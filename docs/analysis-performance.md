# 整場異常行為分析：實作與驗收

> 2026-09-13 最新需求已改為逐規則行為次數，取消所有分數、扣分、頻率倍率及舊版相容。schema 2／10-match-behavior-counts 的現行定義見 [異常行為統計](scoring-rules.md)。以下舊分數與版本說明僅作歷史效能紀錄，不再是現行需求。

## 行為次數版本驗收（2026-09-13）

目前 `10-match-behavior-counts` 共 17 項獨立統計，結果 schema 2；計分器、扣分欄位與舊逐玩家評分入口已刪除。歷史以全員批次發布，UI 保留手動入口與單行三階段進度，基本 demo 自動分析不變。來源 producer 為 0.17.0，跳躍時鐘只在分析擷取解碼，不改基本 parser 欄位。

實際回報樣本 A：連跳保速分別 15／17 段、最長 14／11 跳；固定視角交替空中轉向 152／199 段、最長 32／23 步；其中一名玩家最後回合三次射擊同步急轉皆恢復。全場命中分別 31/31 與 24/27。樣本 B：63/195 命中、3 次穿煙擊殺、2 次穿透擊殺、受閃 8 發／2 命中、1 段短時間連殺。以上是行為量測，不能據此宣告作弊。

修正包括：直線程度至少三點、連跳比較相鄰起跳而非空中加速末幀、空中轉向區間不因共用邊界誤合併、致勝槍回合以正式 tick 範圍判定、未量測射擊保留分母。159 項 core tests 通過、2 項既有 ignored；Clippy、六語系一致性、TypeScript/build、完整 UI 與 privacy 檢查通過。portable 已重建，SHA256 與 release 執行檔相同。

最終 producer 0.17.0 實測（兩場皆重新產生通用檔，靜態地圖資源已有快取）：

| 樣本 | 每場通用資料 | 準備 | 全規則／10 人 | 暖分析 | 共用靜態資源 |
| --- | --- | --- | --- | --- | --- |
| A | 11,920,053 bytes | 6.256 秒 | 7.890 秒 | 7.796 秒 | 106,071,533 bytes |
| B | 40,519,524 bytes | 16.239 秒 | 24.524 秒 | 24.778 秒 | 104,327,723 bytes |

外部 body journal 均為 0 bytes。兩場本次準備與全員分析均低於 30 秒；不代表全新地圖靜態資源的冷啟動驗收。較長場通用資料仍 40.52 MB，高於約 35 MB 的容量方向，不能宣稱容量已達標。非交戰骨架重建略過後，兩場共 20 名玩家原有每項規則的完整輸出逐項 SHA256 比對一致。

新事件來源也確認樣本 B 有 6 發穿煙命中、6 發穿透命中（包括未擊殺）；受閃為 2/8。穿煙／穿牆仍只提供確認命中發數，空槍路徑未合格，完整命中率尚未完成。已有煙霧密度模擬不等於伺服器逐发 through_smoke 判定；沒有用固定球體、未套散布的準星射線或命中事件取代缺少的分母。

私有逐場驗證及原始量測位於 ignored `out/behavior-audit/`，不將玩家識別資訊提交至文件。

以下為已被取代的分數版本歷史。

## 小規則拆分（2026-09-13）

目前規則集為 `9-match-small-rules`，完整定義及實測見 [scoring-rules.md](scoring-rules.md)。啟用瞬間對準、快速直線轉向、固定點追蹤、視角震盪、表面穿透擊殺、穿煙擊殺、致盲旗標擊殺共七項；重疊事件只扣一次，去重後按勝出的小規則獨立計頻率。快速直線轉向每次基本 1 分、整場上限 15，避免普通轉向放大成主要重扣。

共用原生量測只掃一次；有效擊殺只迭代一次供全員三個情境規則使用。基本自動分析、手動信用入口、全員發布與歷史保留不變。純透視的遮蔽跟隨／動態預瞄已定義，但可靠視線資料尚未完成，未啟用這兩條扣分規則。

新版實際 App：先前回報樣本全員 10.735 秒、暖分析 10.650 秒；新增較長樣本全員 30.808 秒、暖分析 30.746 秒，後者未通過 30 秒門檻。通用資料分別 11.91／40.46 MB，靜態資源另計 106.07／104.33 MB，外部 journal 都是 0。兩場此輪均有快取，不能視為冷轉換驗證。新增較長樣本先前冷準備 33.753 秒仍未達標，不能宣稱全部效能目標完成。

## 信任分數：程度與頻率（2026-09-13）

使用者確認：容許偶發輕微誤判，依異常程度與整場發生頻率累積扣分。規則集升為 `8-match-frequency`；先做同組／同回合重疊事件去重，再按玩家的同類獨立事件順序加權：第 1–2 次 ×1、第 3–5 次 ×2、第 6–9 次 ×3、第 10 次起 ×4。既有規則的基本扣分表示程度，未知遮蔽仍不加重。

以每次基本 2 分為例，1／2／4／10 次合計扣 2／4／12／48 分。偶發輕微事件保留 20 分總上限；第 3 次起的重複行為不再被偶發輕微上限卡住，仍受規則／類別上限限制，總分最低 0。事件次數與倍率寫入扣分明細，計分政策寫入本次來源紀錄，舊版歷史保留。

本次調整共用計分器，不放寬身體誤差門檻，不引入反應時間或按姓名判定。全場 generic 轉換、原生共享掃描與手動入口不變。分數依量測到的行為累加，並不保證正常玩家只會出現 1–2 次，也不把分數當成作弊機率。


驗證：142 個 core tests 通過、2 個既有 ignored；Clippy（保留既有影片編碼參數數量豁免）、TypeScript、UI、privacy 通過。實際 App 路徑重算回報樣本：兩位玩家為 40／88，兩次輕微事件為 96、六次為 78；全員分析 10.548 秒，暖重算分析 10.471 秒，歷史重用 0.188 秒。容量與資料來源不變，沒有逐玩家掃描。證據 `out/score13-frequency-result.txt`，新次數加權並不證明此前未命中的行為已被偵測。新版 portable 已重新打包並驗證與 release 執行檔一致。

## 回報樣本修正（2026-09-13）

此節覆寫以下歷史紀錄中的「只有診斷、不扣分」行為。規則集為 `7-match-scored`；這是實驗性行為分數，沒有獨立人類／作弊資料集校準，不代表作弊機率，也不能用分數降低證明辨識正確。

- 修正 adapter：測量結果原先全部被轉成 0 分 observations／Unavailable，現在交給共用計分器去重、限制輕微扣分與規則／類別上限。完成且無命中仍 100，真正失敗仍無分數。
- 原生資料新增實際 pawn 模型 MDAT 的固定 hitbox 中心，保留原骨點；依 recorded model/set 與同一骨骼變換，沒有最近點切換、估計高度或依 rule 篩選轉換。資源快取 FORMAT 3；generic schema 7 / producer 0.15.0 不變。
- 新增持續視角震盪規則，與身體追蹤共用同一次全場掃描。判讀原始 pitch/yaw 通道，非極點附近的球面準星速度：至少 1 秒、32 steps、每步 wrapped yaw 至少 30°、兩端 |pitch| 至少 88°、反向比例至少 0.5。每段提議 20，規則／類別上限 60。缺 tick、生命、身份、回合與視角中斷不串接。
- 分析只使用正式回合 freezeEndTick 到 endTick；基本 demo 自動分析不改。App 不再由舊 body journal／legacy 檔隱式取代原生分析；離線診斷 API 保留。
- Windows 大型資源目錄 rename 出現 AccessDenied 時，逐檔驗證發布，最後才提交 manifest；中途失敗可重試，不把半成品視作有效快取。

回報樣本重新跑實際 App engine：全新資料目錄、沒有 generic 或靜態資源快取，generic **11,913,168 bytes**，準備 **24.473 秒**，兩規則／10 玩家 **10.489 秒**，首次合計 **34.967 秒**。暖快取準備 **1.809 秒**、分析 **10.388 秒**；歷史重用 **0.191 秒**，不重算。靜態共用資源 **106,071,533 bytes**，外部 journal **0 bytes**。本地證據 `out/score13-cold-final-result.txt`；不能用這個較短樣本取代所有長度比賽的效能保證。

較長的既有樣本：generic 36,175,721 bytes，舊 generic 快取＋首次 FORMAT3 靜態資源準備 23.496 秒，全部規則 25.470 秒，暖分析 25.620 秒；資源 110,689,746 bytes。另一次**完全冷快取測試失敗**：準備 53.814 秒、分析 41.700 秒，暖分析也達 52.436 秒（`out/score13-long-cold-final-result.txt`）。當時整機 CPU 91%，CS2 與其他程式活動中，顯示時間有明顯負載影響；不能把慢值刪掉或宣稱長樣本冷啟動已通過。本輪回報樣本的冷啟動達標，較長樣本的完全冷啟動仍需在可比負載下重測。

驗證：141 個 core tests 通過、2 個既有 ignored；UI 手動入口／單行進度／全員分數／歷史測試、TypeScript、前端 build、privacy checks 通過。嚴格 Clippy 被既有 `render/encode.rs` 的 8 參數函式阻擋；僅豁免該既有 lint 後重新檢查通過，本輪不改動影片編碼介面。`npm run app:build` 成功，portable 執行檔與 release 執行檔 SHA-256 一致。

回報的第一位玩家由 100 變為 **40**：第 14 回合三段視角震盪，最長 11.375 秒。第二位由 100 變為 **92**：四段輕微吸附，每段 2；沒有同類視角震盪。其他玩家仍有 88–100 分，故第二位的 92 **不構成可靠區分作弊的證據**。不能為符合使用者標籤而按名稱、命中率或 KD 扣分，也不能將這場當成獨立校準。

固定身體點診斷重建 7,665,284 個 hitbox 中心，hitboxUnavailable 為空；整體仍有 3,165 個 pose 缺值（cached pose、topology、packet timestamp、version 0），與幾何缺漏分開計數。新增中心僅小幅改善第二位擊殺前角誤差，不能把原先漏檢全部歸因於未轉完。


> 最新使用者覆寫：本階段接受目前約 35 MB 的整場共用資料；10 MB 延後優化，不再是本階段驗收阻擋。轉換 ≤ 30 秒、全部 rules／全部玩家合計 ≤ 30 秒，以及手動信用分析／保留基本自動分析的要求不變。容量仍如實統計，不因放寬容量而放寬身體量測完整性。

> 2026-09-12 最新計分定義：依使用者要求，完成分析後由 100 分開始，沒有符合扣分條件的資料（含規則沒有可用樣本）維持 100。規則缺值與未校準狀態保留於收合的分析細節；執行失敗不給分。ruleset 為 `4-match-baseline`，舊歷史不改寫。進度為「準備資料 → 分析玩家 → 評分」，僅顯示目前步驟一行。結果頁列出全員分數，每人有獨立 rules 結果、扣分及歷史；僅資料準備與來源掃描共用。以下先前效能數字與資料產製限制仍然適用。


更新：2026-09-12。手動整場原生分析、全員獨立分數、三步進度與本機效能驗收完成。實驗規則的獨立校準仍未認證，觀察不作正式扣分；缺失來源影格保留診斷，不補假資料。

## 首次實體 baseline 漏轉修正（2026-09-12）

producer `0.15.0` 修正第一批武器的初始狀態。真實 tick 65：CC4 class 19 在 wire `instancebaseline` 有 423 bytes，舊統計解析器卻沒有該 class 的 baseline；分析因此缺少 entity 75／serial 31 的模型。正確字串表解碼後，其 model `17040132134709990891` 已由原始來源還原，沒有預設模型或事後補值。

信用分析啟用時，實體初始化使用當下正確 wire baseline，保留 sparse slot、無 key 的 value update 與表格更新位置。基本解析未啟用此模式，既有行為與自動觸發維持不變。baseline 已展開成通用實體欄位，沒有再重複儲存 raw baseline payload；舊通用快取不重用。

## App 效能驗收（2026-09-12）

真正 `Engine::score_match`，全新隔離資料目錄 `out/scoring-native-app-20260912-d`，沒有外部 body journal：

| 項目 | 結果 |
| --- | ---: |
| 每場共用通用資料 | 36,175,721 bytes（約 36.18 MB） |
| 首次準備（含共用動畫資源冷建置） | 23.771 秒 |
| 全部啟用規則／10 位玩家合計 | 24.372 秒 |
| 首次完整評分 | 48.148 秒 |
| 暖快取重新評分 | 27.443 秒 |
| 讀取評分歷史 | 0.387 秒，規則不重算 |
| 共用靜態動畫資源 | 105,003,058 bytes，另列 |
| 外部量測 journal | 0 bytes |
| 有效規則樣本／實驗觀察 | 213,554,965／302 |

兩段 30 秒 gate 均通過。baseline 修正後，841,899／845,151 個活體 pawn frames 有原生姿勢，共 54,723,435 個身體點；原先 6,847 個 missing secondary model 已全數消除。剩餘 3,054 個 recipe 內 cached pose 缺值、35 個初始 topology 缺值與 163 個 version 0 明確列入 coverage，不補造姿勢。全員獨立 checks、全員歷史原子追加、首次與重算 checks 完全一致，基本 demo 解析過程未觸發信用評分。步驟事件僅 1→2→3；歷史重用無分析。100 是無正式扣分的基準，非身體資料不存在的替代證明。

原有基本分析自動觸發保持不變。開啟 detail 或分頁只讀既有結果；右上角手動按鈕才開始整場信用分析。僅顯示目前進度一行，結果列出全部玩家分數；詳細規則與歷史收合。

驗證：core 133 tests passed、2 ignored；前端建置、UI 互動回歸與隱私檢查通過。Clippy 保留既有 render 引數數量與 store 測試 vec 警告，未添加新依賴。產生 portable executable，無提交或版本標籤變更。開發 profile 也對 workspace core 啟用最佳化，避免 app:dev 執行未最佳化的整場動畫運算。

## 完整動畫來源與原生還原更新（2026-09-12）

通用 producer `0.14.0`／schema 7 保留所有動畫實體及各次欄位狀態，以字串字典避免重複儲存欄位名稱。獨立轉換 36,167,313 bytes、10.908 秒；解碼 1.283 秒。schema 6 完整來源與 schema 7 的 321,352 條欄位時間線、123,570 ticks 及 18,934 events 摘要完全相同；沒有依玩家、rule 或候選區間刪資料。35 MB 為本階段約略容量目標，實際 bytes 如實回報，10 MB 延後。

原生還原已完成 AimCS（軀幹、頭部、武器與手臂）、SnapWeapon primary 骨架、FootIK、ModelSpaceBlend，以及既有 sampling／blend／mask 任務。secondary skeleton 由記錄的 entity handle／serial → model → skeleton 字典解析；地圖 brush 不會再被當成動畫依賴。缺失的原始資料不補值。

- 41 個實際捕獲姿勢的 82 個手部位置：最大誤差 0.004918 遊戲單位；頭部案例 <0.001。
- 移動中的 FootIK：最大附件誤差 0.000161 遊戲單位；關閉求解的控制組誤差升至 0.037630，非 no-op 驗證。
- 原生整場檢查：835,052／845,151 活體 pawn frames 有已還原姿勢，54,278,380 個身體點，原生消費 21.930 秒（後續 FK／規則查表優化另以 App 驗收）。
- 還未解出的來源仍列 coverage：recipe 內缺 cached pose、缺 secondary 模型、初始 topology／version。未解析 trailer 原樣保留，不推測跨封包 cache 生命週期。

單次整場掃描先還原各 pawn，再把同一組身體點交給全部玩家的規則。IK 僅重建必要祖先鏈；規則每組觀察者／目標查表一次，骨骼狀態直接索引。對照完整 FK 與完整規則串流的測試保證優化不改結果。

ruleset `6-match-native` 隔離先前部分原生結果；舊歷史不改寫。分數仍是完成檢查後的 100 分基準，實驗觀察不冒充正式扣分。獨立規則校準與歷史遊戲資源相容性尚未認證，不能用 100 分宣稱未作弊。

開發模式在既有通用／資源快取上的同路徑驗證：準備 3.256 秒、全員規則 28.710 秒；重算 29.136 秒，結果相同。這是暖快取驗證，沒有把它當成首次冷建置時間。

以下按時間保留較早的驗收與失敗證據；其中「尚未接入」等描述是歷史狀態。

## 原生 App 路徑與首次冷啟動驗收（2026-09-12）

`Engine::score_match` 已接入原生動畫資源、recipe、body 與 match rules；不再把通用 frame 掃描丟棄後等待外部 body journal。基本自動分析保持原路徑，信用分析仍只在手動入口啟動。ruleset `5-match-native` 隔離先前沒有原生量測的歷史。

新隔離資料目錄實際執行 `scoring_app_check`（一般模式，沒有 `--allow-diagnostics`）：
- 共用每場資料 35,375,472 bytes；外部量測 journal 0 bytes。
- 首次準備 25.954 秒；所有啟用 rules、10 位玩家合計 25.486 秒；合計 51.443 秒。
- 暖快取重新評分：準備 3.976 秒、分析 25.161 秒；歷史重用 0.505 秒。
- 規則有效樣本 23,309,398、未校準觀察 32；全員各自 100 分、無正式扣分。重算追加歷史，首次與重算 checks 完全相等。
- 靜態動畫共用快取 79,190,553 bytes，另列，未藏進每場容量。909 個 recorded clips 加主 skeleton：首次資源準備 12.830 秒、暖讀 0.963 秒。
- 這份驗收證明原生路徑、全員分數與時間，不代表全骨骼／所有自訂任務已還原，也不代表規則已獨立校準。當時原生還原 447,208／845,151 個活體 pawn frames，部分骨骼仍未知；後續 ModelSpaceBlend／FootIK／Aim 工作仍在進行。未解析 trailer 與歷史資源相容性保持未認證，不會拿未知值補零、以 pawn origin 冒充身體點或給出正式扣分。

通用 producer `0.12.0` 修正 sendtable 同名欄位碰撞：速度與 view offset 的 `m_vecX/Y/Z` 依原始 send node、wire path 分開擷取，刪除混淆的重複 alias。基本解析的 ID／名稱／數值行为未改。獨立轉換 35,373,308 bytes、10.938 秒；123570 ticks、18934 events 完整。純讀解碼及原生消費另改為借用完整 pose payload，避免逐幀建立 byte-index 樹；缺 byte 仍明確缺值。

原生 clip 直接解 u16 編碼並還原 sample interpolation／model-space chain；VRF DATA 的浮點文字有限精度明列為資源精度限制。203×74 個原始 clip frames 與 VRF ReadFrame oracle 的最大 component 誤差 8.35e-7；tick10031 三個 render attachment 驗證誤差均 <0.001 遊戲單位。App 不依賴研究用 .NET helper／私人 runtime。

## Indexed 動畫來源與原始 clip 驗證（2026-09-12）

Producer 0.10.0 再修正動畫來源陣列：`m_vecExternalGraphIds`、`m_vecExternalClipIds`、`m_vecSecondarySkeletons`、`m_vecSecondarySkeletonSlotIDs` 原本經 property ID map 覆蓋成最後一個值；現在在 scoring-only capture 保留各 sendtable path、長度、字串／整數型別與移除。一般 demo 解析仍不啟用此 capture。快取版本更新，舊資料不覆寫。

完整 release 轉換：35,292,192 bytes、9.644 秒；解壓 2.331 秒、123570 個狀態，終點 tick 123570。Core 97 tests passed、2 ignored；Clippy core／Tauri all-targets 通過，原有兩項 warning 未改。

離線原型已直接呼叫既有 VRF `AnimationClip.ReadFrame`，得到 idle、default idle、breathing 的 6／6／191 frames，各 74 根骨骼；沒有經過 DMX exporter 的座標／微小位移修正。原型位於忽略的 `out/pose-native-research/vrf-managed`，包含私人 .NET10 runtime、已抽出的既有 VRF managed assemblies 與 JSON 輸出，共 223,031,436 bytes。這是研究成本，不是已整合、達標的產品共用資源；沒有全域 runtime 安裝或修改遊戲檔案。

`python -X utf8 out/pose-native-research/validate-idle-body.py` 驗證 tick10031 的單一 idle pose：原始 base clip FK + 網路 origin／yaw + 記錄的 root bone offset，與既有獨立 render 參考的 eholster／pistol／knife 附著點誤差分別為 0.0000311／0.0003087／0.0000260 遊戲單位。這只是三個點、單一姿態的通過，並非整場／全身還原。

當前 `client.dll`（SHA256 `a0c195f0b6ec00915ef08c548200a010ebbe7982d3a4bc468cad939b67c8c4e3`）的 AimCS／SnapWeapon 寫入範圍已靜態核對：spine、neck/head、weapon、arm/hand 與部分 secondary pose；pelvis／legs 不在寫入集合。此結論有版本與 hierarchy 前提，其他任務或版本不可直接沿用。完整 CS2 任務執行、時間對齊與原生 body → rules 接入仍未完成，沒有用這個部分驗證繞過原生验收的零樣本失敗。

額外無損容量實驗：全欄位時間線分組後 gzip6 約 28.26 MB，LZMA6 約 21.02 MB；後者僅壓縮即 19.24 秒。欄位 alias 去重沒有顯著改善。這些是研究上界，沒有證明 10 MB 不可能，也沒有作為新格式投入產品。

## 原生來源漏轉修正與失敗驗收（2026-09-12）

新增的 scoring-only 字串表擷取保留 `AnimTaskTypes` 與 `AnimAssetData`；正常 demo 基本解析不啟用它。修正只更新 value 的封包、非連續索引的 `+2` 增量，以及只包含部分表格的 full-packet snapshot。截斷輸入回傳錯誤，固定長度資料按實際 bits 讀取。索引與位元語意參照 [Clarity S2StringTableEmitter](https://github.com/skadistats/clarity/blob/master/src/main/java/skadistats/clarity/processor/stringtables/S2StringTableEmitter.java)。

`match-state` producer 0.9.0 以同一份差分串流保存這些字典，也保留 secondary skeleton 計數，不依 rule 或玩家篩選。實際 demo 完整走到 tick 123570：16 種動畫任務、38 筆資源表項，共 108 個名稱／資料欄位。直接來源與 compact 解壓後的最終字典摘要同為 `5f54fcd7af6037491197edc562949c487eeefadf`。這是字典還原驗證，**不是身體座標還原驗證**。

獨立 compact release 測量：35,243,527 bytes，轉換 9.673 秒、解壓驗證 2.509 秒；123570 狀態、18934 事件，容量仍超出 10 MB。Engine 使用其實際來源 metadata 與事件設定生成 35,262,154 bytes，兩個入口的產物不當成位元相同。

新的 `scoring_app_check` 預設檢查有效樣本、沒有外部 body journal 與兩段 30 秒預算；容量如實回報，10 MB 已延後，不阻擋本階段驗收。`--allow-diagnostics` 僅供重跑既有診斷 benchmark，仍要求有效樣本；不可當成原生功能驗收。

本次另開空資料目錄跑真正 Engine：10 位玩家、`diagnosticBytes=0`、`samples=0`、`observations=0`，程式以 exit 1 回報 `acceptance failed: no measured player samples reached the rules`。執行期間曾與 Clippy 重疊，不將該次時間用作效能驗收。100 分是產品的無扣分基準，不能讓這個失敗 gate 通過。

尚未實作完成的是將網路動畫配方與對應模型／動畫資源還原成身體座標，再直接送入全員 rules。Engine 現在仍只驗證 compact，規則身體輸入仍來自另備的 body journal。CS2 專用動畫任務與姿態時間語意尚未完成還原驗證；不得以 pawn origin、固定高度或猜測骨骼替代。

驗證：parser 字串表回歸 1 項通過；core 96 項通過、2 項 ignored；Clippy core／Tauri all-targets 通過但有既有 render 參數數量及 store 測試 vec 警告，`-D warnings` 因這兩項失敗。沒有改動基本分析的自動觸發或正常解析選項。

## 互動

Demo 原有的自動基本分析維持不變。開啟 detail、信用分數分頁、切換玩家或讀取歷史，都不會啟動信用分數計算。

按 detail 右上角「分數分析」圖示才呼叫 `score_match`，一次處理整場全部玩家。按鈕有 accessible label、tooltip、spinner 與執行中停用狀態。切換分頁不重新派工作，過期回應不覆蓋另一份 demo，失敗保留已完成結果。

完成後切換玩家直接選取回傳結果；再次按鈕會整場重新評估並追加全員歷史。結果以單一批次檔原子發布，不留下部分玩家的新結果，舊逐玩家歷史仍可讀。

## 既有診斷 journal 的 release 實測（不是完整功能驗收）

Windows，AMD Ryzen 7 9800X3D，8 核／8 個邏輯處理器，約 31.1 GiB RAM。Rust release（opt-level 3、thin LTO）。同一份交接指定的 23 回合／64 Hz demo；完整 packet 來源 tick 1–123570，共 123570 個狀態與 18934 筆 parser 支援的遊戲事件，沒有依射擊／可見／rule 候選裁切。

以下為真實 `Engine::score_match` 驗證，測量時沒有並行編譯或其他效能測試：

| 項目 | 實測 |
| --- | ---: |
| 無 match-state 快取的共用資料準備 | 10.341 秒 |
| 全部已啟用 rules × 全部 10 位玩家 | 15.337 秒 |
| 首次上述流程合計 | 25.678 秒 |
| 暖快取準備／全部玩家分析 | 0.416／15.408 秒 |
| 暖快取重新分析合計 | 15.824 秒 |
| 相同來源／ruleset 歷史重用 | 0.419 秒 |
| App Engine 峰值 working set | 334643200 bytes（334.6 MB） |
| 通用 match-state | **35216231 bytes（35.22 MB，未達約 10 MB）** |
| 另外使用的既有 body 診斷 journal | **123400937 bytes（123.40 MB）** |
| 本次兩份輸入合計 | **158617168 bytes（158.62 MB）** |

這不是「只有 demo 到合格身體／遮蔽分析」的冷啟動驗收。測試重用先前的 HLAE body 診斷 journal；原有擷取 2182.8 秒、匯出 82.37 秒的成本仍存在。沒有 body journal 的 demo 仍會產生通用資料，但目前準星吸附沒有可用身體樣本。總分依使用者定義顯示 100，這不代表資料還原已完成。

無 match-state 快取代表重新做 entity 解碼／壓縮，不代表清除 OS 檔案快取。基本解析快取讀取約 0.539 秒另列。首次歷史地圖／模型準備尚未驗收，因為沒有可驗證的對應資源，不能稱其成本為零。

新轉換串流寫入暫存檔，完成後原子改名；單次暫存檔最大約等於 35.22 MB 產物，沒有完整逐幀 JSON 暫存。原始 demo、既有擷取、診斷 journal、舊格式研究產物另存並保留，沒有用移動／刪除它們冒充容量改善。

## 契約與流程

- `analysis/compact.rs`：`match-state` schema 6、producer 0.10.0；ruleset `4-match-baseline`，準星吸附演算法仍為 `experimental-2`。
- 快取：`analysis/match-state/<sha1(sourceFingerprint)>-v6-0.10.0.gz`。Demo 指紋、game build／patch、map content fingerprint、格式及實作版本分開保存。
- Scoring-only `AnalysisChanges` 在正常基本解析中為 `None`。啟用後記錄 property、pose、array removal 與 entity lifecycle 的實際變更；一個 entity 解碼 pass 處理全部人物。Schema/header pass、來源雜湊仍存在，成本已包含。
- 保留身分連結、隊伍、生命、位置／視角／姿態、網路碰撞範圍、閃光、煙霧 bytes、動態障礙、parser 支援的全部遊戲事件。選擇不依賴 scoring rule。
- 保留 entity serial、建立／移除、tick／net tick；缺值不補零，缺 tick 不插值。原生數值保留位元，不捨入。
- 狀態變更、排序索引差分、位元 XOR；PoseRecipe 網路 byte array 使用長度＋presence bitmap＋bytes，避免每個 byte 一筆整數更新。缺 slot 與已知零可區分。
- `compact::visit` 提供目前狀態、changed IDs 與事件回呼；`pose_bytes` 還原長度與已知 slot。檢查 footer、gzip CRC、長度、索引、版本與來源。
- 舊研究格式 2–5 有明確 reader 分支供稽核，未知版本拒絕；新 cache 不直接沿用舊格式。
- Shared body journal 只解壓／解碼一次，active tracks 分配給各玩家的既有 Stream；不逐玩家重新掃描。Check 仍經既有統一計分器。
- 通用／診斷檔的實際內容雜湊、版本、來源與容量保存於 provenance；壓縮檔另有一次內容雜湊讀取，不是額外逐玩家解碼。
- 新歷史：`scoring/matches/<sha1(demoId)>/match-<run>.json`。舊 `scoring/<demo+player hash>/` 相容；基本解析重跑不刪除歷史。

## 還原證明與未完成項目

在 0.7.0 profile 的表示法比較中，packed arrays 展開後，245102 個邏輯欄位的 tick／值／移除摘要、時鐘與事件摘要，和未合併版本完全一致，包括姿態、煙霧與非玩家環境。

0.8.0 另外修正欄位匹配：依實際 property 名稱選擇基礎資料，不讓 PlayerPawn／AnimGraph 類別前綴誤選全部內部欄位。Controller pawn handle 仍明確保留；graph／serialization／時間與比賽狀態為獨立通用來源欄位。新增回歸防止摩擦等內部屬性因類別名再次被全部匯出。這是來源 profile 變更，不宣稱其欄位摘要與舊 profile 完全相同，也沒有用某條 rule 初篩。

診斷仍為 **19890600 個有效樣本、24 筆觀察**；首次與暖快取 Check 完全一致。正式 score 全為 null；`rapid || follows`、缺 tick 中斷、去重與上限均未調整。

仍未完成：

1. 10 MB 容量優化已延後；目前 producer 0.10.0 為 35.29 MB，本階段接受。
2. 直接從通用姿態還原可靠身體量測，替代 123.40 MB HLAE 診斷資料；目前通用檔不能單獨重現 body 診斷。
3. 經核實的歷史模型／地圖內容、骨骼時間、伺服器身體範圍、透明牆面、煙霧可見濃度；碰撞面或煙霧 seed 不等於可靠遮蔽。
4. 來源資格與獨立校準完成前的正式信用扣分。容量／速度測試不能取代這些要求。

## 驗證與重現

- `cargo test -p demodesk-core --lib -q`：95 通過，2 個原有忽略。
- `cargo clippy -p demodesk-core -p demodesk --all-targets -- -D warnings -A clippy::too_many_arguments -A clippy::useless_vec`：通過。
- `npm run build` 與建置產物的 `npm run test:ui`：通過。
- UI：開啟／切人不自動算、一次全員、重複請求停用、計算中切分頁／玩家、全員歷史追加、錯誤保留結果、重新載入不派工作。
- 核心：signed zero／整數精度、缺 slot／已知零、截斷／CRC／未知版本、批次結果等價、原子歷史追加。

```powershell
cargo run -p demodesk-core --release --example analysis_compact -- DEMO NEW_OUTPUT.gz
cargo run -p demodesk-core --release --example analysis_compact -- --digest OUTPUT.gz
cargo run -p demodesk-core --release --example scoring_app_check -- DEMO ISOLATED_DATA_DIRECTORY
```

`--inspect OUTPUT.gz` 回報 coverage 與未壓縮欄位更新量，不是模組壓縮大小；`--pose-budget` 是獨立編碼實驗，不能冒充整場容量。

最新 App 驗證為 `app-profile.stdout`、`app-profile-process.json`；下列較早編碼實驗均保留作比較。私人驗證位於 ignored `out/scoring-match-validation/`：`app-final.stdout`、`app-process.json`、`compact-v8.json`、`digest-v3/v4/v6/v7/v8.json`。真實識別、路徑與擷取不放入 repo。
