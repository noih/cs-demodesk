//! CS DemoDesk — Tauri glue: exposes the engine as commands and forwards engine events to the
//! window. All real logic lives in `demodesk-core`.

use demodesk_core::engine::{Detected, Engine, Event, Notify, SetupState};
use demodesk_core::radar::MapAssets;
use demodesk_core::render::{DoctorReport, RenderOptions};
use demodesk_core::store::{DemoMeta, RenderJob, Settings};
use demodesk_core::KillEvent;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};

pub const EVENT_NAME: &str = "demodesk://event";

struct TauriNotify(AppHandle);
impl Notify for TauriNotify {
    fn notify(&self, event: Event) {
        let _ = self.0.emit(EVENT_NAME, event);
    }
}

type Eng = Arc<Engine>;
type CmdResult<T> = Result<T, String>;

fn err<E: std::fmt::Display>(e: E) -> String {
    format!("{e:#}")
}

/// Portable layout: keep everything next to the executable when that folder is
/// writable, otherwise fall back to the per-user app data folder.
fn choose_data_dir(app: &AppHandle) -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join("demodesk-data");
            let probe = candidate.join(".write-test");
            if std::fs::create_dir_all(&candidate).is_ok() && std::fs::write(&probe, b"ok").is_ok() {
                let _ = std::fs::remove_file(&probe);
                return candidate;
            }
        }
    }
    app.path().app_data_dir().map(|p| p.join("demodesk-data")).unwrap_or_else(|_| PathBuf::from("demodesk-data"))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Status {
    ok: bool,
    problems: Vec<String>,
    data_dir: PathBuf,
    active_render: Option<String>,
    version: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SettingsResponse {
    settings: Settings,
    detected: Detected,
    doctor: DoctorReport,
    setup: SetupState,
    data_dir: PathBuf,
    /// size of the stored parse results (demodesk-data/parsed)
    parsed_bytes: u64,
    /// size of the rendered videos (demodesk-data/clips)
    clips_bytes: u64,
    /// size of the extracted radar images (demodesk-data/radar)
    radar_bytes: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DemoResponse {
    meta: DemoMeta,
    /// ParsedDemo without the (large) kills array
    parsed: Option<serde_json::Value>,
}

fn settings_response(engine: &Engine) -> SettingsResponse {
    let (parsed_bytes, clips_bytes, radar_bytes) = engine.storage_bytes();
    SettingsResponse { settings: engine.settings(), detected: engine.detected(), doctor: engine.doctor(), setup: engine.setup_state(), data_dir: engine.data_dir().to_path_buf(), parsed_bytes, clips_bytes, radar_bytes }
}

/// Every engine call does file I/O (settings, parse results, job records) or
/// more; run it on the blocking pool so the async runtime stays responsive.
async fn blocking<T: Send + 'static>(engine: &Eng, f: impl FnOnce(&Eng) -> CmdResult<T> + Send + 'static) -> CmdResult<T> {
    let engine = engine.clone();
    tauri::async_runtime::spawn_blocking(move || f(&engine)).await.map_err(err)?
}

#[tauri::command]
async fn get_status(engine: State<'_, Eng>) -> CmdResult<Status> {
    blocking(&engine, |e| {
        let d = e.doctor();
        Ok(Status { ok: d.ok, problems: d.problems, data_dir: e.data_dir().to_path_buf(), active_render: e.active_job_id(), version: env!("CARGO_PKG_VERSION").into() })
    })
    .await
}

#[tauri::command]
async fn get_settings(engine: State<'_, Eng>) -> CmdResult<SettingsResponse> {
    blocking(&engine, |e| Ok(settings_response(e))).await
}

#[tauri::command]
async fn save_settings(engine: State<'_, Eng>, settings: Settings) -> CmdResult<SettingsResponse> {
    blocking(&engine, move |e| {
        e.save_settings(settings).map_err(|problems| problems.join("\n"))?;
        Ok(settings_response(e))
    })
    .await
}

#[tauri::command]
async fn run_setup(engine: State<'_, Eng>, force: bool) -> CmdResult<bool> {
    blocking(&engine, move |e| Ok(e.start_setup(force))).await
}

#[tauri::command]
async fn list_demos(engine: State<'_, Eng>) -> CmdResult<Vec<DemoMeta>> {
    blocking(&engine, |e| Ok(e.list_demos())).await
}

#[tauri::command]
async fn register_demo(engine: State<'_, Eng>, path: String) -> CmdResult<DemoMeta> {
    blocking(&engine, move |e| e.add_demo(Path::new(&path)).map_err(err)).await
}

#[tauri::command]
async fn parse_demo(engine: State<'_, Eng>, id: String) -> CmdResult<()> {
    blocking(&engine, move |e| e.parse_demo(&id).map_err(err)).await
}

#[tauri::command]
async fn get_demo(engine: State<'_, Eng>, id: String) -> CmdResult<DemoResponse> {
    blocking(&engine, move |e| {
        let (meta, parsed) = e.get_demo(&id).ok_or("demo not found")?;
        Ok(DemoResponse { meta, parsed: parsed.map(|p| p.without_kills()) })
    })
    .await
}

#[tauri::command]
async fn get_kills(engine: State<'_, Eng>, id: String) -> CmdResult<Vec<KillEvent>> {
    blocking(&engine, move |e| e.parsed(&id).map(|p| p.kills.clone()).ok_or_else(|| "demo not parsed".into())).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReplayFile {
    path: PathBuf,
    bytes: u64,
}

/// Position stream for the 2D replay; built on first call (a few seconds).
#[tauri::command]
async fn get_replay(engine: State<'_, Eng>, id: String) -> CmdResult<ReplayFile> {
    blocking(&engine, move |e| {
        let path = e.replay_file(&id).map_err(err)?;
        let bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        Ok(ReplayFile { path, bytes })
    })
    .await
}

/// Radar image(s) for a map, extracted from the game files on first call.
#[tauri::command]
async fn get_map_assets(engine: State<'_, Eng>, map_name: String) -> CmdResult<MapAssets> {
    blocking(&engine, move |e| e.map_assets(&map_name).map_err(err)).await
}

#[tauri::command]
async fn clear_radar(engine: State<'_, Eng>) -> CmdResult<u64> {
    blocking(&engine, |e| Ok(e.clear_radar())).await
}

#[tauri::command]
async fn clear_analysis(engine: State<'_, Eng>, id: String) -> CmdResult<()> {
    blocking(&engine, move |e| e.clear_analysis(&id).map_err(err)).await
}

#[tauri::command]
async fn clear_all_analysis(engine: State<'_, Eng>) -> CmdResult<u64> {
    blocking(&engine, |e| e.clear_all_analysis().map_err(err)).await
}

#[tauri::command]
async fn remove_demo(engine: State<'_, Eng>, id: String) -> CmdResult<()> {
    blocking(&engine, move |e| e.remove_demo(&id).map_err(err)).await
}

#[tauri::command]
async fn start_render(engine: State<'_, Eng>, demo_id: String, highlight_ids: Vec<String>, options: RenderOptions) -> CmdResult<RenderJob> {
    blocking(&engine, move |e| e.enqueue_render(&demo_id, highlight_ids, options).map_err(err)).await
}

#[tauri::command]
async fn list_jobs(engine: State<'_, Eng>) -> CmdResult<Vec<RenderJob>> {
    blocking(&engine, |e| Ok(e.list_jobs())).await
}

#[tauri::command]
async fn cancel_job(engine: State<'_, Eng>, id: String) -> CmdResult<bool> {
    blocking(&engine, move |e| Ok(e.cancel_job(&id))).await
}

#[tauri::command]
async fn delete_job(engine: State<'_, Eng>, id: String) -> CmdResult<()> {
    blocking(&engine, move |e| e.delete_job(&id).map_err(err)).await
}

#[tauri::command]
async fn clear_all_clips(engine: State<'_, Eng>) -> CmdResult<u64> {
    blocking(&engine, |e| e.clear_all_clips().map_err(err)).await
}

#[tauri::command]
async fn reveal_path(path: String) -> CmdResult<()> {
    tauri_plugin_opener::reveal_item_in_dir(&path).map_err(err)
}

#[tauri::command]
async fn open_path(path: String) -> CmdResult<()> {
    tauri_plugin_opener::open_path(&path, None::<&str>).map_err(err)
}

/// Open a web page in the default browser (only https://github.com/… is ever passed).
#[tauri::command]
async fn open_url(url: String) -> CmdResult<()> {
    if !url.starts_with("https://") {
        return Err("only https urls".into());
    }
    tauri_plugin_opener::open_url(&url, None::<&str>).map_err(err)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let handle = app.handle().clone();
            let data_dir = choose_data_dir(&handle);
            let engine = Engine::new(data_dir.clone(), Arc::new(TauriNotify(handle.clone())))?;
            // Let the webview play videos from the data folder.
            let _ = app.asset_protocol_scope().allow_directory(&data_dir, true);
            app.manage(engine);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_status,
            get_settings,
            save_settings,
            run_setup,
            list_demos,
            register_demo,
            parse_demo,
            get_demo,
            get_kills,
            get_replay,
            get_map_assets,
            clear_radar,
            clear_analysis,
            clear_all_analysis,
            remove_demo,
            start_render,
            list_jobs,
            cancel_job,
            delete_job,
            clear_all_clips,
            reveal_path,
            open_path,
            open_url
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
