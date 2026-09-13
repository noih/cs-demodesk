> 現行 App 輸出逐規則次數與量測，不再計分；本文件的舊分數敘述已停用。現行定義見 [異常行為統計](scoring-rules.md)。

# 準星吸附：首版核心與資料契約

目前已實作離線規則核心、共用場景差異紀錄與診斷輸入，並接上評分 UI 及歷史保存；正式扣分仍未啟用。`score` 固定為 null。`provisionalScore` 與候選扣分只供測量／校準，不是可發布的信用分數。

## 已確認的行為

- 進入吸附：從未貼合到貼合敵人身體固定位置的軌跡、角度移動速度及突然跳轉。
- 維持吸附：對同一身體位置持續跟隨的角度誤差與時間，不限頭、胸、腹或腳等部位。
- 不計敵人出現後的反應時間，不要求射擊、壓槍或敵人變向。
- 靜止對準本身不觸發；此前若有快速吸向動作，仍保留進入吸附證據。
- 單次可以少量扣分，持續跟隨與多次獨立片段可增加扣分；輕微合計及單一類別都不能扣光 100 分。
- 隔牆、隔煙、致盲可加重；正常可見時已開始、遮蔽後仍繼續的吸附也適用，不設剛進入掩體的豁免。
- 同段多種遮蔽取最高強度，不累加。未知遮蔽不視為可見，也不加重。

## 執行

```powershell
cargo test -p demodesk-core --lib scoring::crosshair_lock
cargo run -p demodesk-core --example crosshair_lock -- --synthetic out/crosshair-lock-synthetic.json
cargo run -p demodesk-core --example crosshair_lock -- out/measured-input.json out/crosshair-lock-report.json
cargo run -p demodesk-core --example crosshair_lock -- out/measured-input.json out/crosshair-lock-report.json out/parameters.json
```

輸出目錄必須存在；輸出檔案不可已存在。範例以同目錄暫存檔寫入，完成後才保存，拒絕覆寫已有證據。驗證失敗以錯誤退出，不產生新的成功報告。真實玩家與 demo 衍生資料僅留在忽略的本機 `out/`。

## 輸入契約

`scoring::crosshair_lock::Input` 使用 camelCase JSON。每個輸入只對應單場 demo 的一位受評玩家。

| 欄位 | 意義 |
| --- | --- |
| demoFingerprint / playerId / measurementSource | 來源內容識別、受評玩家及測量來源。 |
| tickRate / sampleStepTicks | 已驗證的 tick rate 及預期取樣間隔。 |
| angularResolutionDegrees | 原始視角角度解析度；不能填插值後的假精度。 |
| tracks | 同一回合、敵人、固定身體局部位置的取樣序列。 |
| track.round / targetId / pointId / enemy | 1-based 回合、目標及局部位置的穩定識別、經驗證的敵我關係。 |
| sample.tick / eye / view / target | tick、觀察者眼睛世界座標、原始 pitch/yaw 度數、同一身體局部位置在當下的世界座標。 |
| sample.obstruction | unknown（預設）、visible、smoke、blind、wall。僅能填獨立驗證的情境。 |

`pointId` 不是每一幀離準星最近的部位。資料生產端必須固定局部位置，再依骨架／姿勢變換到每個時點的世界座標；不能以玩家原點、固定身高或逐幀改選最近骨骼替代。

同一回合、目標與 pointId 只能有一條 track；缺樣以 tick 缺口呈現。死亡、重生、隊伍或部位身份改變必須切斷序列，重新建立穩定身份，不能跨越這些界線追蹤。樣本 tick 必須嚴格遞增。

## 測量與合併

- 使用 3D 方向夾角計算貼合誤差，yaw 跨越 ±180° 不會產生假 360° 跳轉。
- 每個符合貼合誤差的連續片段，向前搜尋有限時間窗內的非貼合軌跡；記錄起始 tick、到達 tick、角位移、時間、平均角速度與直線程度（端點角距／角路徑總長）。
- 高速度，或速度與直線程度同時達標，可形成快速吸向候選；不是只看直線程度就觸發。
- 持續跟隨需時間與角度位移皆達標；目標相對視線及準星都必須有足夠移動，排除雙方靜止對準。
- 原始 tick 缺口切斷所有測量，不插值、不跨缺口計算瞬移或持續時間。粗取樣與不足的角度精度回傳 `insufficientData`。
- 遮蔽加重要求同一已確認情境有足夠連續取樣支持。此版本使用跟隨最低時間作為情境最低時間，是待校準參數，並非掩體後反應豁免。
- 同回合的證據時間窗有交集即合併，包含進入吸附的時間窗；連鎖重疊也視為同組。不同目標或部位的重疊候選保守合併，最高候選扣分勝出，其他證據保留 `mergedInto`。
- 依候選扣分高至低分配上限，同分使用固定回合／tick／目標／部位排序。每筆保存 proposedPoints 與 allocatedPoints，後者才參與實驗分數對帳。
- 單一部位的多次取樣不是多次扣分；解除後重新進入才可能形成下一段。規則不包含可見後反應時間。

## 實驗參數，尚未校準

預設：最大取樣間隔 20 ms、貼合誤差 0.15°、進入時間窗 200 ms、最小進入角位移 5°、速度 180°/s 配合直線程度 0.98，或快速跳轉速度 720°/s。持續跟隨至少 150 ms 且移動 1°；750 ms 以上的持續跟隨提高候選扣分。

扣分暫用 2（基礎）、5（較長跟隨）、8（煙霧或致盲）、12（牆壁）；輕微合計上限 20、準星吸附類別上限 70。這些只用於合成驗證及參數探索，沒有證據支持它們是人類與外掛的分界，不可直接上線。每份報告保存完整參數與 `experimental-2` 規則版本。

## 接到 demo 前的必要工作

目前 `DemoParser::ticks` 能要求 pitch/yaw，但現有核心沒有匯出連續眼睛世界座標與姿勢對齊的身體局部位置。2D replay 另外有間隔取樣、位置與 yaw 取整，且其玩家列沒有 pitch，不能直接作為本規則證據。

1. 建立並實測身體局部位置與眼睛座標的資料來源；包含蹲姿、動畫、移動、死亡及跨版本處理。
2. 確認連續視角的時間對齊、解析度及觀察者位置，避免把錄製／網路缺口當吸附。
3. 建立可信的牆、煙霧、致盲情境判定；不能用 spotted、附近有煙或穿煙擊殺旗標代替。未確認時仍可用軌跡，但不加重情境分數。
4. 用正常甩槍、高手跟槍、預瞄、不同距離及受控吸附樣本校準，再以獨立樣本驗證誤觸發。合成測試只能證明程式符合公式，不能證明能辨識外掛。
5. 校準完成後接上 [本場評分計畫](match-behavior-scoring-plan.md) 的註冊清單、正式計分、來源指紋、歷史保存、自動／手動評分與回放 UI。目前沒有把未校準的核心掛進這些入口。

## 真實 demo 資料匯出

已新增 `crosshair_data`，直接解析每個 tick 的原始位置與 pitch/yaw，不重用 16 FPS 的回放資料。保留生命／隊伍狀態、蹲姿量、flash_duration、flash_max_alpha、hitbox set、root bone offset、duck view offset、view punch，以及致盲、死亡、回合開始、煙霧引爆／結束事件。欄位缺值保留 null，並輸出 decodedCounts，區分 schema 中有欄位與實際取得值。

```powershell
cargo run -p demodesk-core --example crosshair_data -- <demo.dem> out/crosshair-data.json <first-tick> <last-tick> <verified-tick-rate>
```

原始測量範例仍接受明確 tick rate；正式來源時脈已從 `svc_ServerInfo.tick_interval` 讀取並驗證一致性，不再默認 64。

致盲剩餘事件時間由 `player_blind` 的開始 tick 與 duration 推導；死亡、回合開始及已知死亡狀態會清除，部分範圍匯出也會處理範圍開始前的事件。相互重疊的事件保留各自區間的聯集。`flash_duration` 不是倒數計時；alpha 欄位是最大強度，不代表每個 tick 畫面仍然全白，因此尚不自動轉成規則的 Blind 情境。

實測本機站姿校準 demo 的 2,001 筆樣本均讀到位置、視角、生命狀態、flashMaxAlpha、hitboxSet、rootBoneOffset、duckViewOffset 與 viewPunch。這些偏移不等於眼睛／各部位的精確世界座標；raw 輸出仍不能直接冒充規則 Input。另以真實對戰 demo 匯出 20,490 筆玩家樣本，上述欄位全數可讀，其中 266 筆落在已記錄的致盲事件區間。對戰樣本輸出與本機資料都保留在忽略的 out。

## 靜態地圖碰撞查詢

已使用現有 Source 2 Viewer CLI 成功抽取 Dust2 的 `maps/de_dust2/world_physics.vmdl_c`，再將其 PHYS 區塊匯出為 glTF。此能力由 [Source 2 Viewer 官方格式說明](https://s2v.app/ValveResourceFormat/guides/format-support.html)記載；不另行實作 Source 2 二進位幾何解碼器。

```powershell
<Source2Viewer-CLI.exe> -i <map.vpk> -f maps/<map>/world_physics.vmdl_c -o out/world_physics.vmdl_c
<Source2Viewer-CLI.exe> -i out/world_physics.vmdl_c -o out/world.gltf -d --gltf_export_format gltf --gltf_export_extras
node scripts/trace-analysis-geometry.mjs out/world_physics.gltf out/ray.json
node --test scripts/test-analysis-geometry.mjs
```

實際輸出 physics 檔名可能由工具追加 `_physics`；以上模型名的匯出會產生 `world_physics.gltf` 與同名 `.bin`。查詢 JSON 為 `{"start":[x,y,z],"end":[x,y,z]}`，使用匯出器轉換前的局部座標（此世界碰撞匯出為 CS2 遊戲單位）。工具要求平坦場景、所有節點具有相同轉換矩陣，拒絕含不同個別模型變換的輸入，不直接把 glTF 公尺座標與 demo 遊戲單位混用。

測試線段在 Dust2 435,649 個三角形中命中 concrete 表面；輸出每個幾何群組的最近命中比例、SurfaceProperty 與 InteractAs，`visibility` 維持 unknown。玻璃、碰撞工具體、可破壞物、動態門與地圖版本匹配仍未解決，不能把每個碰撞都算成隔牆。線段沒有命中也不保證可見。

這是單條線段的離線線性掃描，不能直接放進每個 tick × 玩家 × 身體部位的計分迴圈；正式串接前需要空間索引及已驗證的遮蔽分類。煙霧事件另有下節的位元組／更新日誌輸出，沒有用近似球體冒充實際煙霧體積。


## 逐封包場景、動畫與煙霧日誌

```powershell
cargo run -p demodesk-core --example crosshair_scene -- <demo.dem> out/crosshair-scene.json <first-tick> <last-tick> <step-ticks>
```

從 demo 開頭依序解碼，保留選定 tick 最後一個一般網路封包的狀態，不插值。`step-ticks=1` 保留逐 tick 快照；實測用 64 只用於資料盤點，不能作為吸附的高頻樣本。每格同時保留 demo `tick` 與封包 `netTick`，煙霧的 `m_nSmokeEffectTickBegin` 屬於伺服器時間域，不可直接與 demo tick 相減。

- 修正動態類別未登錄欄位的問題，輸出門、可破壞／物理模型、FuncBrush 等類別的模型引用、座標分量、旋轉、父物件、碰撞及渲染旗標。`entityId + serial` 區分實體索引被重新使用；模型引用仍是資源識別值，不是已解析的世界碰撞網格。
- `CUtlBinaryBlock` 改以無損位元組值保存，避免 UTF-8 替代字元破壞動畫資料。`poseFields` 另存欄位名稱、完整 field path 與值，保留動畫 slot／array 索引，移除同名覆寫後的誤導性 scalar。這是網路姿態 recipe，尚未解成骨架矩陣；outer `DemAnimationData` 仍未解碼。
- 煙霧固定陣列各索引獨立保存，依 `m_nVoxelFrameDataSize` 裁切有效長度，尚未收到的位元組為 null。原始場景不執行煙霧解碼。另用 `analysis_smoke` 將完整資料拆成獨立 journal artifact；缺值、截斷、錯誤序列或不支援旗標會留下該實體的 `error`。
- 更新日誌保留序號、heartbeat、種子格座標與 state，以及尚未解碼的 density/palette payload。[格式研究來源](https://github.com/osztenkurden/cs2parser/blob/master/docs/smoke-voxel-format.md)指出，種子格不等於渲染雲霧，因此輸出不宣稱能判定視線穿煙。

本機真實對戰片段抽取 33 格：330 筆玩家、99 筆 FuncBrush、33 筆物理模型及 56 筆煙霧實體快照。48 筆具有非空煙霧資料，全部位元組齊全且日誌解碼成功，共含 4,062 筆日誌記錄（跨快照會重複，並非獨立事件）；保留 34,473 筆姿態欄位值。此片段 demo 與伺服器 tick 差值為 3,420，僅是該檔案的實測值，不硬編碼成全域偏移。

回歸涵蓋二進位高位元組、煙霧陣列邊界、缺值與截斷、種子格 state／未解碼內容保留。此步未接上正式扣分：精確骨架／眼睛座標、模型資源與動態碰撞組合、煙霧密度及致盲畫面仍需要還原與驗證。

本輪驗證：核心 78 passed／2 ignored、幾何 2 passed、前端 build 與 privacy 檢查通過。Clippy 通過（只排除既有 `render/encode.rs` 的 `too_many_arguments`）。額外姿態索引保存僅在場景擷取模式啟用，一般 demo 解析沿用原有資料量。


## 模型識別與動態碰撞查詢

`resolve-analysis-models.mjs` 使用 [MurmurHash64B](https://github.com/aappleby/smhasher/blob/master/src/MurmurHash2.cpp) 與 [Source 2 資源 seed](https://github.com/ValveResourceFormat/ValveResourceFormat#list-of-supported-magics)，將明確指定 VPK 中的未編譯模型路徑對回 demo 的 64-bit model ID。全程使用字串／BigInt，不經 JavaScript Number。保留候選 VPK、路徑、CRC 與位元組長度；同 ID 有多個候選時不擅自選擇。

```powershell
node scripts/resolve-analysis-models.mjs <Source2Viewer-CLI.exe> out/crosshair-scene.json <map.vpk> <pak01_dir.vpk>
```

本機片段的 9 個非零模型 ID 對上 8 個，包括玩家模型、煙霧彈、足球與兩個 FuncBrush；另一個 FuncBrush 在本次指定資源包中找不到。模型 ID 是路徑識別，不保證資源內容與 demo 錄製版本相同，輸出仍標示 `assetVersionMatch: unverified`。

以既有 Source 2 Viewer 將選定模型抽取並匯出 `model_physics.gltf` 後，可使用同一支線段查詢工具查動態場景：

```json
{
  "tick": 100,
  "start": [0, 0, 0],
  "end": [100, 0, 0],
  "modelFiles": { "11435759149502318708": "soccerball/model_physics.gltf" }
}
```

```powershell
node scripts/trace-analysis-geometry.mjs out/crosshair-scene.json out/dynamic-query.json
node --test scripts/test-analysis-geometry.mjs
```

`modelFiles` 路徑相對於 query 檔案。tick 必須精確命中一格，沒有最近 tick 替代。工具將世界線段依當格座標、pitch/yaw/roll 及正等比縮放反變換到模型空間，再沿用既有三角形求交；命中點轉回世界座標，並保留 entityId、serial、材質與碰撞／渲染旗標。座標公式重用 vendored parser 的 cell × 512 − 16384 + offset 慣例，角度慣例參考 [Valve AngleMatrix](https://github.com/ValveSoftware/source-sdk-2013/blob/master/src/mathlib/mathlib_base.cpp)。

父物件／附著關係、非零 root bone offset、啟用的動畫或缺少必要變換不被猜測；它們和缺失模型列入 `unavailable`。損壞的幾何檔案直接報錯。結果只表示碰撞面交點：玻璃、隱藏工具碰撞、模型版本及視覺不透明度尚未完成驗證，`visibility` 仍為 unknown，未接入加重扣分。

真實片段同格查詢 3 個動態物件，共 548 個三角形，跨過足球中心的線段命中 soccerball 表面，交點距模型中心約 7.9 遊戲單位，符合其匯出碰撞邊界；第四個模型缺失明確列出。足球原點與 45° yaw 亦和地圖實體資料相符。新增回歸驗證雜湊、64-bit 精度、重複資源候選、三軸角度與縮放、移動後不再命中舊位置、缺值與父物件拒絕；目前幾何相關 5 項測試通過。


## 引擎附著點取樣與時間對齊

已使用 [HLAE Entity.getAttachment](https://github.com/advancedfx/advancedfx/blob/main/misc/mirv-script/src/types/mirv.d.ts) 直接取得回放引擎計算的模型附著點世界位置與四元數。`scripts/capture-analysis-attachments.mjs` 是 mirv-script 模組，載入後由呼叫端指定 tick 範圍及已確認的模型附著點；它不啟動回放，也不改變視角。

在隔離回放的啟動模組中呼叫：

```javascript
import {startAttachmentCapture} from '../../scripts/capture-analysis-attachments.mjs';
const stop = startAttachmentCapture({
  firstTick: 100, lastTick: 115,
  attachments: ['clip_limit', 'eholster', 'grenade0', 'c4', 'pistol', 'knife', 'weapon_hand_r', 'weapon_hand_l'],
  renderStage: 12,
});
// 需要提前取消時呼叫 stop()。
```

模組路徑依啟動檔位置調整，使用 `mirv_script_load "<啟動模組.mjs>"` 載入。上述附著點名稱來自實際玩家模型的 MDAT 區塊，分別影響頭部、骨盆、脊椎、腿及手部；附著點不是 hitbox 中心，不可直接將名稱換成頭／胸的假座標。其他模型沒有該附著點時輸出 null。

取樣在 [FRAME_RENDER_PASS](https://github.com/advancedfx/advancedfx/blob/main/misc/mirv-script/src/types/prop.d.ts) 完成後進行。stage 12 已用本機 CS2 patch 14181 / HLAE 2.191.1 驗證，版本變更後需重新確認，不能依賴所有版本都具有相同 stage 編號。保留之前的 frame callback，結束、取消或失敗時還原；僅列存活玩家，重複 render tick 不重複取樣，範圍最多 4,096 ticks。

每行以 `DEMODESK_POSE ` 開頭，後接短 JSON tuple，避免遊戲主控台截斷長行：

- `ready`：範圍、附著點數、取樣階段。
- `frame`：demo tick、demoTime、引擎 curTime。
- `player`：tick、entity index、controller handle、health、team。
- `origin` / `eye` / `view`：tick、entity index、三維向量（view 為角度）。
- `point` / `rotation`：tick、entity index、附著點名稱、向量或 null；rotation 次序為 x/y/z/w。
- `end`：tick、玩家數、附著點數；`done`：完成取樣的 frame 數。

消費端必須核對每格 begin/end、玩家及附著點記錄數；`DEMODESK_POSE_ERROR`、遺失行或缺少 done 不得作為完整證據。缺少 tick 不插值，缺值不補零；輸出是渲染來源，尚未自動轉成規則 Input。

實測隔離回放 16 個連續 ticks、每格 10 名玩家，取得 1,280 組附著點位置／旋轉，共 3,234 行協議記錄，無截斷、缺值或擷取錯誤。回放使用獨立設定與隱藏視窗 hook；視窗觀測沒有前景事件或大型可見視窗，結束後關閉受控程序。

時間對齊實驗發現：在 view-setup 階段混用同 tick 封包資料，160 筆位置最大誤差 5.592 遊戲單位，角度最大誤差 1.401°。改在 render-pass 完成後讀取，位置與視角一致對應到本片段前 2 個封包 ticks；可重疊的 140 筆比較中，位置最大差 0.000031 遊戲單位，角度最大差 0.00000265°。這是單一片段／工具版本的驗證，**未把 2 tick 偏移寫成通用補償**，也尚未證明所有動畫骨骼與碰撞狀態都能用同一偏移還原。

因此後續的明確工作是跨片段／版本驗證時間映射，將附著點變換和模型 hitbox／骨骼關係對上，再接入實驗規則。完整骨架、模型版本一致性、煙霧渲染密度及正式校準仍未完成；這些限制不被轉成成功評分。

本輪最終檢查：`npm run test:core` 78 passed／2 ignored；`node --test scripts/test-analysis-attachments.mjs scripts/test-analysis-geometry.mjs` 6 passed；`npm run test:privacy` 與 `git diff --check` 通過。此輪未修改前端，未重跑前端 build。


## 共用資料邊界

場景、玩家量測、物理幾何、煙霧及姿態均獨立於 `scoring`，介面與版號規則見 [analysis-data.md](analysis-data.md)。`crosshair_scene` / `crosshair_data` 保留既有命令名稱，但輸出改為版本化 artifact，原本 payload 位於 `data`。舊無版本研究檔保留作歷史證據；新工具拒絕直接載入，需由原始 demo 重新產生，不能把未知版本補成目前版本。

```sh
cargo run -p demodesk-core --example analysis_smoke -- out/scene.json out/smoke.json
node --test scripts/test-analysis-contract.mjs scripts/test-analysis-geometry.mjs scripts/test-analysis-attachments.mjs
```

`analyze-attachment-capture.mjs` 驗證 render-pass-after 記錄完整性，再測量位置／視角的 packet tick 偏移。來源、時鐘或樣本不足時不產生可用偏移；測得偏移也不等於骨骼時間及遊戲資源版本已驗證。


## 跨片段與身體位置驗證（2026-09-11）

本機同一遊戲／工具版本下，使用兩段不重疊片段；各自擷取範圍前後補 4 ticks 原始封包，所有候選偏移都必須具有完整比較樣本。

| 片段 | 連續渲染 ticks | 位置／角度比較數 | 唯一對齊偏移 | 最大位置差（game units） | 最大角度差（度） |
| --- | --- | --- | --- | --- | --- |
| A | 64 | 640 | −2 ticks | 0.00003435 | 0.000006055 |
| B | 128 | 970 | −2 ticks | 0.00004431 | 0.000007325 |

偏移的定義為 `packetTick = renderTick + packetTickShift`。這兩段通過 0.001 game units／0.0001 度的對齊容許值；仍逐次測量，沒有將 −2 ticks 當作所有遊戲版本或回放情境的固定值。此驗證只覆蓋目前同一 demo、同一遊戲 build 的兩段資料。

另一次獨立啟動重播片段 B，額外取得 `grenade0`、`grenade1`、`grenade2` 三個同屬 `spine_0` 的附著點。從實際模型 MDAT 取得局部位置與旋轉，先確認各重複定義一致、單一骨骼權重為 1、沒有 root-transform 或 ignore-rotation 特例，再由第一點反推骨骼變換、預測其他兩點的世界變換。

- 只評估模型 ID 相符且模型縮放已知的 527 筆玩家樣本；其他模型的 443 筆明列未評估。
- 1,054 組獨立附件對照全部通過；最大位置誤差 0.00015537 game units，最大旋轉誤差 0.00010925 度。容許值為 0.001 game units／0.01 度。
- 兩次獨立回放共同的 7,760 組附著點全部存在，世界位置差為零；旋轉角度差最多 0.00000419 度（四元數比較的浮點誤差量級）。
- 隔離回放未觀測到前景或可見遊戲視窗，受控 CS2 程序已結束。真實資料與報告保留在 ignored `out/`。

### 可重複執行的入口

```sh
node scripts/analyze-attachment-capture.mjs CONSOLE.log SCENE.json ALIGNMENT.json
node scripts/validate-attachment-geometry.mjs CONSOLE.log SCENE.json ATTACHMENT_MODEL.json GEOMETRY.json
node --test scripts/test-analysis-attachments.mjs scripts/test-attachment-geometry.mjs scripts/test-analysis-contract.mjs scripts/test-analysis-geometry.mjs
```

`ATTACHMENT_MODEL.json` 使用 `attachment-definitions` schema 1 的共用 artifact；`data` 包含 `resourceId`、實際模型 bytes 的 `contentFingerprint`、`definitions`。每個定義包含 `bone`、`offset`、x/y/z/w `rotation`、`weights`、`rootTransforms`、`influences`、`ignoreRotation`，由實際模型資料產生。不能用猜測的角色路徑或手填身體座標替代。此工具驗證單一模型、同一根骨骼的兩個以上附件；其他模型明確未評估。

`analysis-attachment-geometry.mjs` 是純資料模組，不依賴 scoring、HLAE 或 glTF；CLI 負責組裝模型、封包與引擎取樣。輸出保存來源及三份輸入指紋，重跑不能覆寫歷史檔案。資料不足、不符合模型或不支援骨骼形式時，不產生成功驗證。

**已確認的是渲染位置／視角時間映射，以及選定模型附件間的幾何一致性與重現性。** 尚未確認歷史 demo 與目前遊戲模型內容版本一致、完整骨骼的伺服器時間語意、所有部位的 hitbox 或牆／煙霧遮蔽。因此這批資料尚未自動送入正式扣分。下一步需建立可核對來源版本的骨骼／hitbox 對照，並補另一個 demo 或遊戲版本的驗證。


## 模型版本與命中盒重建（2026-09-11）

本輪將已驗證的附件骨骼變換抽成共用函式，並新增 `analysis-hitboxes.mjs`：透過骨骼 provider 接收實測變換，不依賴角色名稱、HLAE 或 scoring。支援模型 shape 2 的膠囊；其 min/max 為端點，不作各軸排序，依據 [Source 2 Viewer Hitbox 定義](https://github.com/ValveResourceFormat/ValveResourceFormat/blob/master/ValveResourceFormat/Resource/ResourceTypes/ModelData/Hitbox.cs) 與 [模型匯出實作](https://github.com/ValveResourceFormat/ValveResourceFormat/blob/master/ValveResourceFormat/IO/ModelExtract.ValveModel.cs)。

實際模型具 17 個附件定義、19 個命中盒，重複 MDAT 區塊的資料一致；demo 觀察到 hitbox set 0。既有取樣可直接對應頭、骨盆、spine_0、spine_3 及左右大腿的骨骼，因此 527 筆相符模型樣本重建 3,162 個世界膠囊。其餘 6,851 個命中盒因未量測骨骼列為 unavailable；其他模型的 443 筆樣本也保留未評估。骨骼名稱採精確匹配，沒有自行把大小寫不同的名稱當成同一骨骼。

```sh
node scripts/reconstruct-analysis-hitboxes.mjs CONSOLE.log SCENE.json PLAYER_MODEL.json OUTPUT.json
node --test scripts/test-analysis-hitboxes.mjs scripts/test-attachment-geometry.mjs
```

模型輸入為 `player-model` schema 1 artifact：沿用 resourceId、contentFingerprint、definitions，另加 `hitboxSets` 陣列，每組含 name、hitboxes，每盒含 index、bone、min、max、radius、shape、translationOnly。觀察中的 setIndex 必須有對應資料；沒有該組、沒有骨骼或 shape 尚未支援時，不以其他組或預設姿態補齊。

### 來源核對

`demo_source` 現在重用安全 varint 讀取與現有 protobuf 定義，直接取得 header 的 build_num 與 patch_version，分開存為 `gameBuild`、`gamePatch`。截斷、錯誤 frame 或目前不支援的壓縮 header 明確報錯；缺少版本欄位維持 null。場景／玩家量測實作版號升至 0.2.0，schema 保持 1，gamePatch 為向後相容的可選來源欄位。

本次實際 demo 為 `gameBuild = 10896`、`gamePatch = 14178`。本機安裝資料分開保存 `ClientVersion = 2000908`、`PatchVersion = 1.41.8.1`、`SourceRevision = 10981323`，不可把上述不同欄位互相當作同一種版號。先前文字中的「14181」是 patch 表示，不是 header build_num。

重新從本機 VPK 抽出的模型與之前使用的模型內容 SHA-256 相同；新版來源匯出的 136 格 packet scene 與舊資料完全一致，重建幾何也完全一致。模型 artifact 保存模型 bytes、MDAT 匯出與安裝版本檔的輸入指紋，舊檔案不覆寫。

**錄製當時的模型內容指紋尚未取得，故相容性仍是 unknown。** 不把路徑 ID、能播放、局部幾何吻合或安裝版本相同當作錄製時模型內容已驗證。內容指紋相同才能令 compatibility 為 matched；不同則直接拒絕重建。即使內容 matched，目前渲染骨骼的伺服器時間語意仍未验证，所以 `eligibleForScoring` 固定為 false。

本輪驗證：核心 80 passed／2 ignored，分析工具 11 passed；涵蓋版本欄位分離、截斷拒絕、膠囊反向端點、旋轉／縮放、缺骨骼、不支援形狀與模型內容不符。未變更前端。
