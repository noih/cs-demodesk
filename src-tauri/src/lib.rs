//! CS DemoDesk — Tauri glue: exposes the engine as commands and forwards engine events to the
//! window. All real logic lives in `demodesk-core`.

use demodesk_core::engine::{Detected, Engine, Event, Notify, SetupState};
use demodesk_core::radar::MapAssets;
use demodesk_core::render::{DoctorReport, RenderOptions, SetupTool};
use demodesk_core::store::{DemoMeta, RenderJob, Settings};
use demodesk_core::KillEvent;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};

mod data_directory;
#[cfg(windows)]
mod webview_runtime;
use data_directory::DataDirectory;
type Directory = Arc<DataDirectory>;

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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Status {
    ok: bool,
    missing_render_tools: Vec<&'static str>,
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
    setup: std::collections::HashMap<SetupTool, SetupState>,
    tool_checks:
        std::collections::HashMap<SetupTool, demodesk_core::render::diagnostics::ToolCheck>,
    data_dir: PathBuf,
    data_dir_override: Option<PathBuf>,
    default_data_dir: PathBuf,
    restart_required: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StorageBytes {
    /// size of the stored parse results (demodesk-data/parsed)
    parsed_bytes: u64,
    anomaly_bytes: u64,
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

fn settings_response(engine: &Engine, directory: &DataDirectory) -> SettingsResponse {
    let selected = directory.selected();
    let restart_required = selected.as_ref().unwrap_or(&directory.default) != &directory.active;
    SettingsResponse {
        data_dir_override: selected,
        default_data_dir: directory.default.clone(),
        restart_required,
        settings: engine.settings(),
        detected: engine.detected(),
        doctor: engine.doctor(),
        setup: engine.setup_state(),
        tool_checks: engine.tool_checks(),
        data_dir: engine.data_dir().to_path_buf(),
    }
}

/// Every engine call does file I/O (settings, parse results, job records) or
/// more; run it on the blocking pool so the async runtime stays responsive.
async fn blocking<T: Send + 'static>(
    engine: &Eng,
    f: impl FnOnce(&Eng) -> CmdResult<T> + Send + 'static,
) -> CmdResult<T> {
    let engine = engine.clone();
    tauri::async_runtime::spawn_blocking(move || f(&engine))
        .await
        .map_err(err)?
}

#[tauri::command]
async fn check_for_updates() -> CmdResult<demodesk_core::updates::Update> {
    tauri::async_runtime::spawn_blocking(|| demodesk_core::updates::check().map_err(err))
        .await
        .map_err(err)?
}
fn existing_browse_directory(path: &Path) -> Option<PathBuf> {
    if !path.is_absolute() {
        return None;
    }
    if path.is_dir() {
        return Some(path.to_path_buf());
    }
    path.parent()
        .filter(|parent| parent.is_dir())
        .map(Path::to_path_buf)
}

#[tauri::command]
fn browse_directory(app: AppHandle, path: Option<String>) -> CmdResult<PathBuf> {
    if let Some(directory) = path
        .as_deref()
        .and_then(|value| existing_browse_directory(Path::new(value.trim())))
    {
        return Ok(directory);
    }
    app.path().desktop_dir().map_err(err)
}

#[cfg(test)]
mod browse_tests {
    #[test]
    fn existing_file_directory_and_missing_path() {
        let exe = std::env::current_exe().unwrap();
        let parent = exe.parent().unwrap();
        assert_eq!(
            super::existing_browse_directory(&exe),
            Some(parent.to_path_buf())
        );
        assert_eq!(
            super::existing_browse_directory(parent),
            Some(parent.to_path_buf())
        );
        assert_eq!(
            super::existing_browse_directory(
                &parent.join("missing-browse-test-dir").join("tool.exe")
            ),
            None
        );
        assert_eq!(
            super::existing_browse_directory(std::path::Path::new("relative.exe")),
            None
        );
    }
}

#[tauri::command]
async fn get_status(engine: State<'_, Eng>) -> CmdResult<Status> {
    blocking(&engine, |e| {
        let d = e.doctor();
        let mut missing_render_tools = Vec::new();
        if d.paths.steam_dir.is_none() {
            missing_render_tools.push("Steam");
        }
        if d.paths.cs2_exe.is_none() {
            missing_render_tools.push("CS2");
        }
        if d.paths.hlae_exe.is_none() || d.paths.hlae_dll.is_none() {
            missing_render_tools.push("HLAE");
        }
        if d.paths.ffmpeg_exe.is_none() {
            missing_render_tools.push("ffmpeg");
        }
        Ok(Status {
            missing_render_tools,
            ok: d.ok,
            problems: d.problems,
            data_dir: e.data_dir().to_path_buf(),
            active_render: e.active_job_id(),
            version: env!("CARGO_PKG_VERSION").into(),
        })
    })
    .await
}

#[tauri::command]
async fn get_storage_bytes(engine: State<'_, Eng>) -> CmdResult<StorageBytes> {
    blocking(&engine, |e| {
        let (parsed_bytes, clips_bytes, radar_bytes, anomaly_bytes) = e.storage_bytes();
        Ok(StorageBytes {
            parsed_bytes,
            clips_bytes,
            radar_bytes,
            anomaly_bytes,
        })
    })
    .await
}

#[tauri::command]
async fn get_settings(
    engine: State<'_, Eng>,
    directory: State<'_, Directory>,
) -> CmdResult<SettingsResponse> {
    let directory = directory.inner().clone();
    blocking(&engine, move |e| Ok(settings_response(e, &directory))).await
}

fn validate_settings(engine: &Engine, settings: &Settings) -> Vec<String> {
    let mut problems = engine.validate_settings(settings);
    match DataDirectory::settings_use_app_data(settings) {
        Ok(true) => problems.push("Choose settings directories outside AppData.".into()),
        Err(error) => problems.push(error),
        Ok(false) => {}
    }
    for path in [&settings.hlae_exe, &settings.ffmpeg_exe, &settings.vrf_exe]
        .into_iter()
        .flatten()
    {
        let directory = demodesk_core::render::paths::installation_directory(Path::new(path));
        if let Err(error) = DataDirectory::validate_tool_directory(&directory) {
            problems.push(error);
        }
    }
    problems
}

#[tauri::command]
async fn preview_settings(
    engine: State<'_, Eng>,
    directory: State<'_, Directory>,
    settings: Settings,
    original_settings: Settings,
    data_dir_override: Option<String>,
) -> CmdResult<serde_json::Value> {
    let directory = directory.inner().clone();
    blocking(&engine, move |e| {
        let target = directory
            .validate(data_dir_override)?
            .ok_or("Missing data directory")?;
        let merged = directory.target_settings(&target, &original_settings, &settings)?;
        let problems = validate_settings(e, &merged);
        if !problems.is_empty() {
            return Err(problems.join("\n"));
        }
        Ok(serde_json::json!({ "target": target, "restartRequired": directory.changes(&target) }))
    })
    .await
}

#[tauri::command]
async fn save_settings(
    engine: State<'_, Eng>,
    directory: State<'_, Directory>,
    settings: Settings,
    original_settings: Settings,
    data_dir_override: Option<String>,
    change_confirmed: bool,
) -> CmdResult<SettingsResponse> {
    let directory = directory.inner().clone();
    blocking(&engine, move |e| {
        let target = directory
            .validate(data_dir_override)?
            .ok_or("Missing data directory")?;
        let merged = directory.target_settings(&target, &original_settings, &settings)?;
        let problems = validate_settings(e, &merged);
        if !problems.is_empty() {
            return Err(problems.join("\n"));
        }
        if directory.changes(&target) {
            if !change_confirmed {
                return Err("Confirm the data directory change before saving.".into());
            }
            directory.commit_switch(target, &merged)?;
        } else {
            e.save_settings(merged)
                .map_err(|problems| problems.join("\n"))?;
            directory.save(Some(target))?;
        }
        Ok(settings_response(e, &directory))
    })
    .await
}

#[tauri::command]
async fn run_setup(
    engine: State<'_, Eng>,
    tool: SetupTool,
    force: bool,
    directory: Option<String>,
) -> CmdResult<bool> {
    blocking(&engine, move |e| {
        let directory = directory
            .filter(|value| !value.trim().is_empty())
            .map(|value| DataDirectory::validate_tool_directory(Path::new(value.trim())))
            .transpose()?;
        Ok(e.start_setup(tool, force, directory))
    })
    .await
}

#[tauri::command]
fn cancel_setup(engine: State<'_, Eng>, tool: SetupTool) {
    engine.cancel_setup(tool);
}

#[tauri::command]
async fn check_tools(
    engine: State<'_, Eng>,
    directory: State<'_, Directory>,
) -> CmdResult<SettingsResponse> {
    let directory = directory.inner().clone();
    blocking(&engine, move |e| {
        e.check_tools();
        Ok(settings_response(e, &directory))
    })
    .await
}

#[tauri::command]
async fn tool_diagnostics(engine: State<'_, Eng>) -> CmdResult<String> {
    blocking(&engine, |e| {
        serde_json::to_string_pretty(&e.tool_diagnostics()).map_err(err)
    })
    .await
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
        Ok(DemoResponse {
            meta,
            parsed: parsed.map(|p| p.without_kills()),
        })
    })
    .await
}

#[tauri::command]
async fn scoring_history(
    engine: State<'_, Eng>,
    id: String,
) -> CmdResult<std::collections::BTreeMap<String, Vec<demodesk_core::scoring::Assessment>>> {
    blocking(&engine, move |e| e.scoring_match_history(&id).map_err(err)).await
}

#[tauri::command]
async fn score_match(
    engine: State<'_, Eng>,
    id: String,
    force: bool,
) -> CmdResult<demodesk_core::scoring::queue::Job> {
    blocking(&engine, move |e| {
        e.enqueue_analysis(&id, force).map_err(err)
    })
    .await
}

#[tauri::command]
async fn analysis_jobs(
    engine: State<'_, Eng>,
) -> CmdResult<Vec<demodesk_core::scoring::queue::Job>> {
    blocking(&engine, |e| Ok(e.analysis_jobs())).await
}

#[tauri::command]
async fn get_kills(engine: State<'_, Eng>, id: String) -> CmdResult<Vec<KillEvent>> {
    blocking(&engine, move |e| {
        e.parsed(&id)
            .map(|p| p.kills.clone())
            .ok_or_else(|| "demo not parsed".into())
    })
    .await
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
async fn clear_match_anomaly(engine: State<'_, Eng>, id: String) -> CmdResult<()> {
    blocking(&engine, move |e| e.clear_match_anomaly(&id).map_err(err)).await
}

#[tauri::command]
async fn clear_anomaly_data(engine: State<'_, Eng>) -> CmdResult<u64> {
    blocking(&engine, |e| e.clear_anomaly_data().map_err(err)).await
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
async fn analysis_clips(
    engine: State<'_, Eng>,
    demo_id: String,
    selection: demodesk_core::scoring::clips::Selection,
) -> CmdResult<Vec<demodesk_core::scoring::clips::RuleClips>> {
    blocking(&engine, move |e| {
        e.analysis_clips(&demo_id, &selection).map_err(err)
    })
    .await
}
#[tauri::command]
async fn start_analysis_render(
    engine: State<'_, Eng>,
    demo_id: String,
    selection: demodesk_core::scoring::clips::Selection,
    options: RenderOptions,
) -> CmdResult<Vec<RenderJob>> {
    blocking(&engine, move |e| {
        e.enqueue_analysis_render(&demo_id, selection, options)
            .map_err(err)
    })
    .await
}

#[tauri::command]
async fn start_render(
    engine: State<'_, Eng>,
    demo_id: String,
    highlight_ids: Vec<String>,
    options: RenderOptions,
) -> CmdResult<RenderJob> {
    blocking(&engine, move |e| {
        e.enqueue_render(&demo_id, highlight_ids, options)
            .map_err(err)
    })
    .await
}

#[tauri::command]
async fn list_jobs(engine: State<'_, Eng>) -> CmdResult<Vec<RenderJob>> {
    blocking(&engine, |e| e.list_jobs().map_err(err)).await
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

struct StartupError(Option<String>);

#[tauri::command]
fn get_startup_error(state: State<'_, StartupError>) -> Option<String> {
    state.0.clone()
}

#[tauri::command]
fn preferences_need_reset(directory: State<'_, Directory>) -> bool {
    directory.reset_preferences()
}

#[tauri::command]
fn acknowledge_preferences_reset(directory: State<'_, Directory>) -> CmdResult<()> {
    directory.acknowledge_preferences()
}

#[tauri::command]
fn retry_startup(app: AppHandle, state: State<'_, StartupError>) -> CmdResult<()> {
    if state.0.is_none() {
        return Err("The app is already running.".into());
    }
    app.request_restart();
    Ok(())
}

#[tauri::command]
fn recover_data_directory(
    app: AppHandle,
    directory: State<'_, Directory>,
    state: State<'_, StartupError>,
    path: Option<String>,
) -> CmdResult<()> {
    if state.0.is_none() {
        return Err("Recovery is only available before startup.".into());
    }
    directory.save(directory.validate(path)?)?;
    // Deliver Exit so the single-instance plugin releases its lock before relaunch.
    app.request_restart();
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    if let Some(ok) = data_directory::probe_command() {
        std::process::exit(if ok { 0 } else { 2 });
    }
    #[cfg(windows)]
    if !webview_runtime::ready() {
        return;
    }
    let builder = tauri::Builder::default();
    // Register first: duplicate launches must exit before opening the data store.
    #[cfg(desktop)]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, _, _| {
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.show();
            let _ = window.unminimize();
            let _ = window.set_focus();
        }
    }));
    builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let handle = app.handle().clone();
            let base = app.path().home_dir()?.join(".noih");
            let config = base.join("data-directory.json");
            let default = base.join("demodesk-data");
            let legacy_config = app.path().app_config_dir()?.join("data-directory.json");
            let legacy_default = app.path().app_local_data_dir()?.join("demodesk-data");
            let (directory, initialized) = match DataDirectory::load_outside(
                config.clone(),
                &default,
                legacy_config,
                legacy_default,
            ) {
                Ok(mut directory) => {
                    let result = directory.initialize();
                    (directory, result)
                }
                Err(error) => (DataDirectory::recovery(config, default), Err(error)),
            };
            let directory = Arc::new(directory);
            let data_dir = directory.active.clone();
            let startup = initialized.and_then(|_| {
                app.asset_protocol_scope()
                    .allow_directory(&data_dir, true)
                    .map_err(err)?;
                Engine::new(data_dir.clone(), Arc::new(TauriNotify(handle.clone()))).map_err(err)
            });
            let error = match startup {
                Ok(engine) => {
                    app.manage(engine);
                    None
                }
                Err(error) => Some(error),
            };
            app.manage(StartupError(error));
            app.manage(directory);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            browse_directory,
            check_for_updates,
            get_startup_error,
            preferences_need_reset,
            acknowledge_preferences_reset,
            retry_startup,
            recover_data_directory,
            get_status,
            get_settings,
            get_storage_bytes,
            save_settings,
            preview_settings,
            run_setup,
            cancel_setup,
            check_tools,
            tool_diagnostics,
            list_demos,
            register_demo,
            parse_demo,
            get_demo,
            score_match,
            analysis_jobs,
            scoring_history,
            get_kills,
            get_replay,
            get_map_assets,
            clear_radar,
            clear_analysis,
            clear_match_anomaly,
            clear_all_analysis,
            clear_anomaly_data,
            remove_demo,
            start_render,
            analysis_clips,
            start_analysis_render,
            list_jobs,
            cancel_job,
            delete_job,
            clear_all_clips,
            reveal_path,
            open_path,
            open_url
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if matches!(event, tauri::RunEvent::Exit) {
                if let Some(directory) = app.try_state::<Directory>() {
                    directory.release_locks();
                }
            }
        });
}
