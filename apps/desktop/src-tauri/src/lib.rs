//! Desktop client: runs the stats server in-process on localhost, watches the game's replay
//! folder, and stays in the system tray when its window is closed.

mod steam;
mod watcher;

use mechabellum_server::{Facets, MatchRow, Stats, StatsFilter, Store, UploadAuth};
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
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            replay_dir: None,
            port: DEFAULT_PORT,
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
}

#[derive(Serialize)]
struct Status {
    replay_dir: Option<PathBuf>,
    replay_dir_detected: bool,
    db_path: PathBuf,
    http_url: Option<String>,
    matches: u64,
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
        replay_dir_detected: replay_dir.is_some() && !configured,
        replay_dir,
        db_path: state.db_path.clone(),
        http_url: state.http_url.lock().unwrap().clone(),
        matches: state.store.match_count().map_err(|e| e.to_string())?,
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
            });
            start_http(app.handle(), store, settings.port);
            start_watching(app.handle(), &app.state::<AppState>());

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
            set_replay_dir
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
