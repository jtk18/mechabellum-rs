//! Desktop client: runs the stats server in-process on localhost, watches the game's replay
//! folder, and stays in the system tray when its window is closed.

mod art;
mod gamelog;
mod steam;
mod sync;
mod watcher;

use mechabellum_server::{
    Facets, MatchRow, Stats, StatsFilter, Store, UnitTechStats, UploadAuth, VersionTrends,
};
use serde::{Deserialize, Serialize};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};
use watcher::{EventSink, ReplayWatcher, WatchEvent};

use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

const DEFAULT_PORT: u16 = 8765;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct Settings {
    /// None means "detect through Steam".
    replay_dir: Option<PathBuf>,
    port: u16,
    /// Sharing matches with a community server.
    sync: sync::SyncSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            replay_dir: None,
            port: DEFAULT_PORT,
            sync: sync::SyncSettings::default(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize)]
struct Backfill {
    done: usize,
    total: usize,
    finished: bool,
}

struct AppState {
    store: Arc<Store>,
    db_path: PathBuf,
    settings_path: PathBuf,
    settings: Mutex<Settings>,
    watcher: Mutex<Option<ReplayWatcher>>,
    watching: Mutex<Option<PathBuf>>,
    backfill: Arc<Mutex<Backfill>>,
    http_url: Mutex<Option<String>>,
    last_error: Arc<Mutex<Option<String>>>,
    art_dir: PathBuf,
    tools_dir: PathBuf,
    syncer: sync::Syncer,
    /// Wakes the sync worker (new matches, new settings).
    sync_wake: Mutex<std::sync::mpsc::Sender<()>>,
    art: Arc<Mutex<Option<art::ArtManifest>>>,
    /// What art extraction is doing, or why it failed.
    art_status: Arc<Mutex<Option<String>>>,
}

#[derive(Serialize)]
struct ArtInfo {
    dir: PathBuf,
    manifest: Option<art::ArtManifest>,
    status: Option<String>,
    /// Display name -> image file, for views that only know names (the stats tables).
    units_by_name: std::collections::BTreeMap<String, String>,
    techs_by_name: std::collections::BTreeMap<String, String>,
    specialists_by_name: std::collections::BTreeMap<String, String>,
}

impl ArtInfo {
    fn new(dir: PathBuf, manifest: Option<art::ArtManifest>, status: Option<String>) -> Self {
        use mechabellum::{GameNames, NameKind};
        let names = GameNames::bundled();
        let mut info = ArtInfo {
            dir,
            manifest: None,
            status,
            units_by_name: Default::default(),
            techs_by_name: Default::default(),
            specialists_by_name: Default::default(),
        };
        if let Some(m) = &manifest {
            // Lowest ID first, so an original unit wins over its AsyncBattle copy.
            for (id, unit) in &m.units {
                if let (Some(name), Some(icon)) =
                    (names.resolve(NameKind::Unit, *id as u64), &unit.icon)
                {
                    info.units_by_name
                        .entry(name.to_owned())
                        .or_insert_with(|| icon.clone());
                }
            }
            for (id, file) in &m.techs {
                if let Some(name) = names.resolve(NameKind::Tech, *id) {
                    info.techs_by_name
                        .entry(name.to_owned())
                        .or_insert_with(|| file.clone());
                }
            }
            for (id, file) in &m.specialists {
                if let Some(name) = names.resolve(NameKind::Officer, *id) {
                    info.specialists_by_name
                        .entry(name.to_owned())
                        .or_insert_with(|| file.clone());
                }
            }
        }
        info.manifest = manifest;
        info
    }
}

#[derive(Serialize)]
struct Status {
    /// Stats are being rebuilt for a newer version of the app.
    upgrading: bool,
    replay_dir: Option<PathBuf>,
    replay_dir_detected: bool,
    db_path: PathBuf,
    http_url: Option<String>,
    matches: u64,
    /// Matches that ended before the first fight, so they have no result.
    unfinished: u64,
    backfill: Backfill,
    last_error: Option<String>,
}

fn load_settings(path: &PathBuf) -> Settings {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_settings(path: &PathBuf, settings: &Settings) -> Result<(), String> {
    let json = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| e.to_string())
}

/// (Re)start the watcher on the configured folder, or the one Steam knows about.
fn start_watching(app: &AppHandle, state: &AppState) {
    let configured = state.settings.lock().unwrap().replay_dir.clone();
    let dir = configured.or_else(steam::find_replay_dir);

    let mut slot = state.watcher.lock().unwrap();
    *slot = None;
    *state.backfill.lock().unwrap() = Backfill::default();
    *state.watching.lock().unwrap() = dir.clone();
    let Some(dir) = dir else {
        *state.last_error.lock().unwrap() =
            Some("Couldn't find Mechabellum's replay folder. Choose it below.".into());
        return;
    };

    let app_handle = app.clone();
    let backfill = state.backfill.clone();
    let last_error = state.last_error.clone();
    let sink: EventSink = Arc::new(move |event: WatchEvent| {
        match &event {
            WatchEvent::BackfillProgress { done, total } => {
                *backfill.lock().unwrap() = Backfill {
                    done: *done,
                    total: *total,
                    finished: done == total,
                };
            }
            WatchEvent::Failed { file, error } => {
                log::warn!("{file}: {error}");
                *last_error.lock().unwrap() = Some(format!("{file}: {error}"));
            }
            WatchEvent::Ingested {
                outcome: mechabellum_server::IngestOutcome::Added { .. },
                ..
            } => {
                let _ = app_handle
                    .state::<AppState>()
                    .sync_wake
                    .lock()
                    .unwrap()
                    .send(());
            }
            WatchEvent::Ingested { .. } => {}
        }
        let _ = app_handle.emit("replay-event", &event);
    });

    match ReplayWatcher::start(&dir, state.store.clone(), sink) {
        Ok(watcher) => {
            log::info!("watching {}", dir.display());
            *state.last_error.lock().unwrap() = None;
            *slot = Some(watcher);
        }
        Err(e) => {
            *state.last_error.lock().unwrap() = Some(format!("watching {}: {e:#}", dir.display()));
        }
    }
}

/// The game install that holds the watched replay folder (`<game>/ProjectDatas/Replay`), or
/// the one Steam knows about.
fn game_dir(state: &AppState) -> Option<PathBuf> {
    let from_replays = state
        .watching
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|dir| Some(dir.parent()?.parent()?.to_path_buf()));
    from_replays
        .filter(|d| d.join("ProjectDatas").is_dir())
        .or_else(|| Some(steam::find_replay_dir()?.parent()?.parent()?.to_path_buf()))
}

/// Extract game art in the background, telling the UI as it goes.
fn start_art_extraction(app: &AppHandle, state: &AppState) {
    let Some(game) = game_dir(state) else {
        *state.art_status.lock().unwrap() =
            Some("Game art needs the Mechabellum install; choose the replay folder first.".into());
        return;
    };
    let (art_dir, tools_dir) = (state.art_dir.clone(), state.tools_dir.clone());
    let (art_slot, status) = (state.art.clone(), state.art_status.clone());
    let app = app.clone();
    std::thread::spawn(move || {
        let report = |message: &str| {
            *status.lock().unwrap() = Some(message.to_owned());
            let _ = app.emit("art-status", message);
        };
        match art::extract(&game, &art_dir, &tools_dir, &report) {
            Ok(manifest) => {
                log::info!(
                    "art: {} units, {} techs, {} specialists",
                    manifest.units.len(),
                    manifest.techs.len(),
                    manifest.specialists.len()
                );
                *art_slot.lock().unwrap() = Some(manifest);
                *status.lock().unwrap() = None;
                let _ = app.emit("art-status", "ready");
            }
            Err(e) => {
                log::warn!("art extraction: {e:#}");
                report(&format!("Couldn't extract game art: {e:#}"));
            }
        }
    });
}

/// Serve the stats API on localhost so browsers and other tools can use it too.
fn start_http(app: &AppHandle, store: Arc<Store>, port: u16) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
        let state = app.state::<AppState>();
        match tokio::net::TcpListener::bind(addr).await {
            Ok(listener) => {
                *state.http_url.lock().unwrap() = Some(format!("http://{addr}"));
                log::info!("stats API on http://{addr}");
                let router = mechabellum_server::router(store, UploadAuth::Open);
                if let Err(e) = axum::serve(listener, router).await {
                    *state.last_error.lock().unwrap() = Some(format!("stats API stopped: {e}"));
                }
            }
            Err(e) => {
                *state.last_error.lock().unwrap() =
                    Some(format!("Couldn't start the stats API on port {port}: {e}"));
            }
        }
    });
}

#[tauri::command]
fn status(state: State<'_, AppState>) -> Result<Status, String> {
    let configured = state.settings.lock().unwrap().replay_dir.is_some();
    let replay_dir = state.watching.lock().unwrap().clone();
    Ok(Status {
        upgrading: state.store.needs_upgrade().unwrap_or(false),
        replay_dir_detected: replay_dir.is_some() && !configured,
        replay_dir,
        db_path: state.db_path.clone(),
        http_url: state.http_url.lock().unwrap().clone(),
        matches: state.store.match_count().map_err(|e| e.to_string())?,
        unfinished: state.store.unfinished_count().map_err(|e| e.to_string())?,
        backfill: state.backfill.lock().unwrap().clone(),
        last_error: state.last_error.lock().unwrap().clone(),
    })
}

#[tauri::command]
async fn stats(state: State<'_, AppState>, filter: StatsFilter) -> Result<Stats, String> {
    let store = state.store.clone();
    tauri::async_runtime::spawn_blocking(move || store.stats(&filter))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("{e:#}"))
}

#[tauri::command]
async fn recent_matches(state: State<'_, AppState>, limit: u32) -> Result<Vec<MatchRow>, String> {
    let store = state.store.clone();
    tauri::async_runtime::spawn_blocking(move || store.recent_matches(limit))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("{e:#}"))
}

#[tauri::command]
async fn version_trends(
    state: State<'_, AppState>,
    filter: StatsFilter,
) -> Result<VersionTrends, String> {
    let store = state.store.clone();
    tauri::async_runtime::spawn_blocking(move || store.version_trends(&filter))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("{e:#}"))
}

#[tauri::command]
fn sync_settings(state: State<'_, AppState>) -> sync::SyncSettings {
    state.settings.lock().unwrap().sync.clone()
}

/// Save sharing settings and sync right away if they're complete.
#[tauri::command]
fn set_sync_settings(state: State<'_, AppState>, sync: sync::SyncSettings) -> Result<(), String> {
    {
        let mut settings = state.settings.lock().unwrap();
        settings.sync = sync.clone();
        save_settings(&state.settings_path, &settings)?;
    }
    *state.syncer.settings.lock().unwrap() = sync;
    let _ = state.sync_wake.lock().unwrap().send(());
    Ok(())
}

#[tauri::command]
fn sync_now(state: State<'_, AppState>) {
    let _ = state.sync_wake.lock().unwrap().send(());
}

#[tauri::command]
fn sync_status(state: State<'_, AppState>) -> sync::SyncStatus {
    state.syncer.status.lock().unwrap().clone()
}

#[tauri::command]
fn art_info(state: State<'_, AppState>) -> ArtInfo {
    ArtInfo::new(
        state.art_dir.clone(),
        state.art.lock().unwrap().clone(),
        state.art_status.lock().unwrap().clone(),
    )
}

/// Extract the game's art again, e.g. after a patch.
#[tauri::command]
fn extract_art(app: AppHandle, state: State<'_, AppState>) {
    start_art_extraction(&app, &state);
}

#[tauri::command]
async fn tech_stats(
    state: State<'_, AppState>,
    filter: StatsFilter,
) -> Result<Vec<UnitTechStats>, String> {
    let store = state.store.clone();
    tauri::async_runtime::spawn_blocking(move || store.tech_stats(&filter))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("{e:#}"))
}

#[tauri::command]
async fn match_detail(
    state: State<'_, AppState>,
    battle_id: String,
) -> Result<mechabellum::MatchDetail, String> {
    let store = state.store.clone();
    tauri::async_runtime::spawn_blocking(move || store.match_detail(&battle_id))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("{e:#}"))?
        .ok_or_else(|| "That match is no longer stored.".to_owned())
}

#[tauri::command]
async fn facets(state: State<'_, AppState>) -> Result<Facets, String> {
    let store = state.store.clone();
    tauri::async_runtime::spawn_blocking(move || store.facets())
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("{e:#}"))
}

/// Pass `None` to go back to detecting the folder through Steam.
#[tauri::command]
fn set_replay_dir(
    app: AppHandle,
    state: State<'_, AppState>,
    dir: Option<PathBuf>,
) -> Result<(), String> {
    if let Some(dir) = &dir {
        if !dir.is_dir() {
            return Err(format!("{} is not a folder", dir.display()));
        }
    }
    {
        let mut settings = state.settings.lock().unwrap();
        settings.replay_dir = dir;
        save_settings(&state.settings_path, &settings)?;
    }
    start_watching(&app, &state);
    Ok(())
}

fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .build(),
        )
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let db_path = data_dir.join("mechabellum.db");
            let settings_path = data_dir.join("settings.json");
            let settings = load_settings(&settings_path);
            let store = Arc::new(Store::open(&db_path)?);
            let art_dir = data_dir.join("art");
            let manifest = art::ArtManifest::load(&art_dir);

            // Ratings (MMR) from the game's log, which replays don't carry.
            {
                let store = store.clone();
                let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
                std::thread::Builder::new()
                    .name("game-log".into())
                    .spawn(move || {
                        gamelog::follow(gamelog::log_dirs(&steam::libraries()), store, stop)
                    })?;
            }

            // Sharing with a community server, when turned on.
            let (sync_wake, wake_rx) = std::sync::mpsc::channel();
            let progress_app = app.handle().clone();
            let syncer = sync::Syncer::start(
                store.clone(),
                settings.sync.clone(),
                wake_rx,
                Arc::new(move |status: &sync::SyncStatus| {
                    let _ = progress_app.emit("sync-status", status);
                }),
            );

            app.manage(AppState {
                store: store.clone(),
                db_path,
                settings_path,
                settings: Mutex::new(settings.clone()),
                watcher: Mutex::new(None),
                watching: Mutex::new(None),
                backfill: Arc::new(Mutex::new(Backfill::default())),
                http_url: Mutex::new(None),
                last_error: Arc::new(Mutex::new(None)),
                tools_dir: data_dir.join("tools"),
                art_dir,
                art: Arc::new(Mutex::new(manifest.clone())),
                art_status: Arc::new(Mutex::new(None)),
                syncer,
                sync_wake: Mutex::new(sync_wake),
            });
            // Rebuild outdated stats in the background so the window opens right away.
            if store.needs_upgrade().unwrap_or(false) {
                let (store, app) = (store.clone(), app.handle().clone());
                std::thread::spawn(move || {
                    let _ = app.emit("stats-upgrade", "running");
                    match store.upgrade() {
                        Ok(_) => {
                            let _ = app.emit("stats-upgrade", "done");
                        }
                        Err(e) => log::warn!("stats upgrade: {e:#}"),
                    }
                });
            }
            start_http(app.handle(), store, settings.port);
            start_watching(app.handle(), &app.state::<AppState>());
            if manifest.is_none() {
                start_art_extraction(app.handle(), &app.state::<AppState>());
            }

            let show = MenuItem::with_id(app, "show", "Show stats", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &quit])?;
            let mut tray = TrayIconBuilder::with_id("main")
                .tooltip("Mechabellum Stats")
                .menu(&menu)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "show" => show_main_window(app),
                    "quit" => app.exit(0),
                    _ => {}
                });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            // Keep running in the tray so new replays are still picked up.
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            status,
            stats,
            recent_matches,
            facets,
            version_trends,
            tech_stats,
            match_detail,
            art_info,
            extract_art,
            sync_settings,
            set_sync_settings,
            sync_now,
            sync_status,
            set_replay_dir
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
