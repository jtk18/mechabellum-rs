//! Desktop client: runs the stats server in-process on localhost, watches the game's replay
//! folder, and stays in the system tray when its window is closed.

mod art;
mod gamelog;
mod steam;
mod sync;
mod watcher;

use mechabellum_server::{
    Facets, GameStats, MatchRow, Stats, StatsFilter, Store, UnitTechStats, UploadAuth,
    VersionTrends,
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
    syncer: sync::Syncer,
    /// Wakes the sync worker (new matches, new settings).
    sync_wake: Mutex<std::sync::mpsc::Sender<()>>,
    art: Arc<Mutex<Option<art::ArtManifest>>>,
    /// What art extraction is doing, or why it failed.
    art_status: Arc<Mutex<Option<String>>>,
    /// Extraction progress while it runs, for the splash screen.
    art_progress: Arc<Mutex<Option<art::Progress>>>,
}

#[derive(Serialize)]
struct ArtInfo {
    dir: PathBuf,
    manifest: Option<art::ArtManifest>,
    status: Option<String>,
    /// Set while extraction runs.
    progress: Option<art::Progress>,
    /// Display name -> image file, for views that only know names (the stats tables).
    units_by_name: std::collections::BTreeMap<String, String>,
    techs_by_name: std::collections::BTreeMap<String, String>,
    specialists_by_name: std::collections::BTreeMap<String, String>,
}

impl ArtInfo {
    fn new(
        dir: PathBuf,
        manifest: Option<art::ArtManifest>,
        status: Option<String>,
        progress: Option<art::Progress>,
    ) -> Self {
        use mechabellum::{GameNames, NameKind};
        let names = GameNames::bundled();
        let mut info = ArtInfo {
            dir,
            manifest: None,
            status,
            progress,
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
        .filter(|d| steam::unity_data_dir(d).is_some())
        .or_else(steam::find_game_dir)
}

/// Extract game art in the background, telling the UI as it goes.
fn start_art_extraction(app: &AppHandle, state: &AppState) {
    let Some(game) = game_dir(state) else {
        *state.art_status.lock().unwrap() = Some(
            "Game art needs the Mechabellum install, which wasn't found in any Steam library."
                .into(),
        );
        return;
    };
    let art_dir = state.art_dir.clone();
    let (art_slot, status) = (state.art.clone(), state.art_status.clone());
    let progress_slot = state.art_progress.clone();
    let app = app.clone();
    std::thread::spawn(move || {
        let report = |message: &str| {
            *status.lock().unwrap() = Some(message.to_owned());
            let _ = app.emit("art-status", message);
        };
        // Sprite extraction reports every image from several threads; pass on a step change
        // or each half percent, not all of them.
        let step_started = Mutex::new((0, std::time::Instant::now()));
        let progress = |p: art::Progress| {
            {
                let mut started = step_started.lock().unwrap();
                if p.step != started.0 {
                    log::info!(
                        "art: {} took {:.1?}",
                        art::STEPS[started.0],
                        started.1.elapsed()
                    );
                    *started = (p.step, std::time::Instant::now());
                }
            }
            let mut slot = progress_slot.lock().unwrap();
            let due = slot.as_ref().is_none_or(|last| {
                p.step != last.step || p.fraction - last.fraction >= 0.005 || p.fraction >= 1.0
            });
            if due {
                let _ = app.emit("art-progress", &p);
                *slot = Some(p);
            }
        };
        report("Extracting game art");
        let result = art::extract(&game, &art_dir, &progress);
        {
            let started = step_started.lock().unwrap();
            log::info!(
                "art: {} took {:.1?}",
                art::STEPS[started.0],
                started.1.elapsed()
            );
        }
        *progress_slot.lock().unwrap() = None;
        match result {
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

/// A round of a stored match as board code (docs/board-code.md). `round_index` is 0-based.
#[tauri::command]
async fn board_code(
    state: State<'_, AppState>,
    battle_id: String,
    round_index: usize,
) -> Result<String, String> {
    let store = state.store.clone();
    let detail = tauri::async_runtime::spawn_blocking(move || store.match_detail(&battle_id))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("{e:#}"))?
        .ok_or("That match isn't stored.")?;
    mechabellum::board::Board::from_round(&detail, round_index)
        .map(|board| board.to_string())
        .ok_or_else(|| "That round isn't in the replay.".into())
}

/// The techs each unit has unlocked in Unit Modification, from the game's GameData.grdb next to
/// the replay folder. Empty when it can't be found.
#[tauri::command]
fn loadout(state: State<'_, AppState>) -> Vec<mechabellum::profile::UnitLoadout> {
    let Some(replays) = state.watching.lock().unwrap().clone() else {
        return Vec::new();
    };
    let Some(file) = replays.parent().map(|p| p.join("GameData.grdb")) else {
        return Vec::new();
    };
    match std::fs::read(&file) {
        Ok(bytes) => mechabellum::profile::unlocked_techs(&String::from_utf8_lossy(&bytes)),
        Err(e) => {
            log::info!("no loadout from {}: {e}", file.display());
            Vec::new()
        }
    }
}

/// Put text on the clipboard. Done here rather than in the page, which may not have focus.
#[tauri::command]
fn copy_text(app: AppHandle, text: String) -> Result<(), String> {
    use tauri_plugin_clipboard_manager::ClipboardExt;
    app.clipboard().write_text(text).map_err(|e| e.to_string())
}

/// Each player's rating at the start of a match, when the game log had it.
#[tauri::command]
async fn match_ratings(
    state: State<'_, AppState>,
    battle_id: String,
) -> Result<Vec<mechabellum_server::Rating>, String> {
    let store = state.store.clone();
    tauri::async_runtime::spawn_blocking(move || store.ratings(&battle_id))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
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
fn sync_now(state: State<'_, AppState>) -> Result<(), String> {
    // A manual run also retries matches the server refused before; an update may have fixed
    // the reason.
    let server = sync::base(&state.settings.lock().unwrap().sync.server);
    if !server.is_empty() {
        state
            .store
            .clear_sync_rejections(&server)
            .map_err(|e| e.to_string())?;
    }
    let _ = state.sync_wake.lock().unwrap().send(());
    Ok(())
}

#[tauri::command]
fn sync_status(state: State<'_, AppState>) -> sync::SyncStatus {
    state.syncer.status.lock().unwrap().clone()
}

/// Matches the configured server refused, with its reasons.
#[tauri::command]
async fn sync_rejections(
    state: State<'_, AppState>,
) -> Result<Vec<mechabellum_server::SyncRejection>, String> {
    let server = sync::base(&state.settings.lock().unwrap().sync.server);
    if server.is_empty() {
        return Ok(Vec::new());
    }
    let store = state.store.clone();
    tauri::async_runtime::spawn_blocking(move || store.sync_rejections(&server))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn art_info(state: State<'_, AppState>) -> ArtInfo {
    ArtInfo::new(
        state.art_dir.clone(),
        state.art.lock().unwrap().clone(),
        state.art_status.lock().unwrap().clone(),
        state.art_progress.lock().unwrap().clone(),
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

/// Whole-match stats: deployments, match lengths, winners and losers, every unit used.
#[tauri::command]
async fn game_stats(state: State<'_, AppState>, filter: StatsFilter) -> Result<GameStats, String> {
    let store = state.store.clone();
    tauri::async_runtime::spawn_blocking(move || store.game_stats(&filter))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("{e:#}"))
}

/// One match from each player's side, in `match_detail`'s player order.
#[tauri::command]
async fn player_games(
    state: State<'_, AppState>,
    battle_id: String,
) -> Result<Vec<mechabellum::PlayerGame>, String> {
    let store = state.store.clone();
    tauri::async_runtime::spawn_blocking(move || store.player_games(&battle_id))
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

/// Passed when the app starts with the computer: stay in the tray instead of opening a window.
const STARTED_HIDDEN: &str = "--hidden";

/// Whether the app starts when the user signs in.
#[tauri::command]
fn autostart_enabled(app: AppHandle) -> Result<bool, String> {
    use tauri_plugin_autostart::ManagerExt;
    app.autolaunch().is_enabled().map_err(|e| e.to_string())
}

#[tauri::command]
fn set_autostart(app: AppHandle, enabled: bool) -> Result<(), String> {
    use tauri_plugin_autostart::ManagerExt;
    let launcher = app.autolaunch();
    if enabled {
        launcher.enable()
    } else {
        launcher.disable()
    }
    .map_err(|e| e.to_string())
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
        // Must come first: a second launch just brings the running app forward.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main_window(app)
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![STARTED_HIDDEN]),
        ))
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .build(),
        )
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
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
                art_dir,
                art: Arc::new(Mutex::new(manifest.clone())),
                art_status: Arc::new(Mutex::new(None)),
                art_progress: Arc::new(Mutex::new(None)),
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

            // No replay folder yet (the game creates it on first launch): keep looking, and
            // start watching once Steam's install has one. The UI polls status, so it follows.
            // Spawned only after `manage`, since it reads AppState straight away.
            {
                let app = app.handle().clone();
                std::thread::Builder::new()
                    .name("replay-finder".into())
                    .spawn(move || loop {
                        let state = app.state::<AppState>();
                        if state.watching.lock().unwrap().is_some() {
                            // Found, or set by hand; a later "choose folder" handles itself.
                            std::thread::sleep(std::time::Duration::from_secs(5));
                            continue;
                        }
                        if steam::find_replay_dir().is_some() {
                            log::info!("replay folder appeared");
                            start_watching(&app, &state);
                        }
                        std::thread::sleep(std::time::Duration::from_secs(5));
                    })?;
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

            // The window starts hidden (see tauri.conf.json) so a start with the computer can
            // stay in the tray.
            if !std::env::args().any(|a| a == STARTED_HIDDEN) {
                show_main_window(app.handle());
            }
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
            game_stats,
            player_games,
            art_info,
            extract_art,
            sync_settings,
            set_sync_settings,
            sync_now,
            sync_status,
            sync_rejections,
            autostart_enabled,
            set_autostart,
            match_ratings,
            board_code,
            copy_text,
            loadout,
            set_replay_dir
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
