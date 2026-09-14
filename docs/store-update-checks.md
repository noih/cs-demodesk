# Microsoft Store 更新檢查

研究日期：2026-09-15。本次只分析，未接入 Store API，也未執行安裝或更新。

## 現況與可行性

`crates/demodesk-core/src/updates.rs` 遇到 package identity 就回傳 `Packaged`，沒有查詢 Store；portable 才查 GitHub。`src-tauri/src/lib.rs` 在 `spawn_blocking` 呼叫此函式。[現行發佈約定](../scripts/README.md#microsoft-store-msix)也刻意分開兩個來源，避免 GitHub 已發佈但 Store 尚未上架時誤報。

Microsoft 支援由 Store 發佈的 MSIX 桌面程式使用 `StoreContext.GetAppAndOptionalStorePackageUpdatesAsync()`；不限定 UWP。此 API 需要 package identity，portable 不適用。本專案 [manifest](../packaging/msix/AppxManifest.xml) 是 FullTrust MSIX，最低 Windows 10 build 19041，高於 API 所需版本，因此具備接入條件。[Microsoft：Store package updates](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/package-updates-from-store)

有 identity 不代表一定從 Store 安裝：側載 MSIX 也有 identity。建議把它視為「可以嘗試 Store 查詢」，不要視為 Store 安裝證明；側載、未關聯商店、服務異常等情況保留「無法確認」。Store 文件亦要求正確產品關聯，否則相關 API 可能回傳錯誤；其授權測試流程不可當作側載更新必然成功的保證。[Microsoft：StoreContext 設定及測試](https://learn.microsoft.com/en-us/windows/uwp/monetize/in-app-purchases-and-trials)

## 最小接入方式

1. 保留 portable 的 GitHub 查詢。MSIX 路徑建立 `StoreContext`，以桌面視窗 HWND 經 `IInitializeWithWindow.Initialize` 綁定 owner；Microsoft 對桌面 StoreContext 明列此設定。[桌面設定](https://learn.microsoft.com/en-us/windows/uwp/monetize/in-app-purchases-and-trials#using-the-storecontext-class-with-the-desktop-bridge)
2. 在 UI thread 發起查詢，以非阻塞方式等待完成；不能直接把 WinRT 呼叫塞進現有 `spawn_blocking`。方法文件明列非 UI thread 可能造成 `ERROR_INVALID_WINDOW_HANDLE`。檢查該執行緒 WinRT/COM apartment 初始化，避免切換既有 apartment；自行成功呼叫 `RoInitialize` 時須配對 `RoUninitialize`。[查詢 API](https://learn.microsoft.com/en-us/uwp/api/windows.services.store.storecontext.getappandoptionalstorepackageupdatesasync?view=winrt-26100)、[RoInitialize](https://learn.microsoft.com/en-us/windows/win32/api/roapi/nf-roapi-roinitialize)
3. 回傳集合包含 optional packages。建議只在更新項目的 `Package.Id.FamilyName` 等於目前 app 的 family name 時，亮起 app 更新提示；不要僅以集合非空判斷主程式更新。這是本專案的篩選建議。[集合範圍](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/package-updates-from-store)、[FamilyName](https://learn.microsoft.com/en-us/uwp/api/windows.applicationmodel.packageid.familyname?view=winrt-26100)
4. 查詢成功且找到主程式更新：Store 連結變綠並顯示「有可用更新」。成功但無主程式更新：顯示「目前未偵測到更新」。例外、逾時或查詢未完成：保留「無法確認／由 Store 管理」，不能當作已是最新版。查詢依賴 Store、服務及網路，而且有快取／頻率限制，故空集合也不是全球最新版保證。[API 環境與更新可用性](https://learn.microsoft.com/en-us/uwp/api/windows.services.store.storecontext.getappandoptionalstorepackageupdatesasync?view=winrt-26100)
5. 點擊開啟 `ms-windows-store://pdp/?ProductId=9N5G4VXSDGS5`，由使用者在 Store 更新；開啟失敗可回退既有 HTTPS 商品頁。也可使用 `ms-windows-store://downloadsandupdates` 開啟更新頁。不需要自行下載、覆寫 EXE 或接入安裝 API。[Microsoft：Store URI](https://learn.microsoft.com/en-us/windows/apps/develop/launch/launch-store-app)

既有 core 已使用 `windows` 0.61，但沒有 `Services_Store`、`ApplicationModel` 或相關 WinRT/視窗 interop features；實作時可沿用此套件並補所需 features，Tauri 層負責視窗與執行緒。

## 版本號限制與驗證

**不能把 `StorePackageUpdate.Package.Id.Version` 直接標成新版本。** 官方只定義 `Package` 為「有更新可用的套件」，`PackageId.Version` 為該套件版本；並未承諾它是下載目標的版本。查到的公開契約不足以斷言此欄位必為已安裝版或必為目標版；`StorePackageUpdate` 也沒有獨立的 target-version 欄位。因此最小可靠 UI 是「有可用更新」，目標版本號留待有明確支援的資料來源與實機驗證後再加，不能用 GitHub tag 代替。[Package](https://learn.microsoft.com/en-us/uwp/api/windows.services.store.storepackageupdate.package?view=winrt-26100)、[PackageId.Version](https://learn.microsoft.com/en-us/uwp/api/windows.applicationmodel.packageid.version?view=winrt-26100)、[StorePackageUpdate](https://learn.microsoft.com/en-us/uwp/api/windows.services.store.storepackageupdate?view=winrt-26100)

接入後需以真正 Store 安裝的舊版驗證「有更新」，並確認最新版、側載、斷網／服務不可用及 optional-only 更新的狀態；portable 測試或 mocked IPC 只能驗證分流／畫面，不能證明 Store 查詢正常。
