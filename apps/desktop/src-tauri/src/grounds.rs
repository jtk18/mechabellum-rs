//! Setting up a stored match's round in the game's Testing Grounds by driving the game (the
//! `mechabellum-grounds` crate): Windows and macOS. One run at a time, on its own thread. It
//! reports to the window through events (`grounds-line`, `grounds-units`, `grounds-finished`),
//! and stops when the user moves the mouse or presses Stop.

use crate::AppState;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use std::sync::atomic::{AtomicBool, Ordering};

/// Whether a run is going.
#[derive(Default)]
pub struct Grounds {
    running: AtomicBool,
}

/// What the window needs to offer a run.
#[derive(Serialize)]
pub struct Info {
    /// Whether this platform can drive the game at all (not Linux).
    supported: bool,
    /// macOS: whether this app may send mouse and key events. `None` elsewhere.
    accessibility: Option<bool>,
    /// macOS: whether this app may capture the game's window. `None` elsewhere.
    screen_recording: Option<bool>,
    running: bool,
}

fn info(grounds: &Grounds) -> Info {
    #[cfg(target_os = "macos")]
    let (accessibility, screen_recording) = {
        let a = mechabellum_grounds::mac::access();
        (Some(a.accessibility), Some(a.screen_recording))
    };
    #[cfg(not(target_os = "macos"))]
    let (accessibility, screen_recording) = (None, None);
    Info {
        supported: cfg!(any(windows, target_os = "macos")),
        accessibility,
        screen_recording,
        running: grounds.running.load(Ordering::SeqCst),
    }
}

#[tauri::command]
pub fn grounds_info(grounds: State<'_, Grounds>) -> Info {
    info(&grounds)
}

/// macOS: ask for Accessibility and Screen Recording. The system adds the app to each list,
/// switched off, for the user to switch on.
#[tauri::command]
pub fn grounds_request_access(grounds: State<'_, Grounds>) -> Info {
    #[cfg(target_os = "macos")]
    mechabellum_grounds::mac::request_access();
    info(&grounds)
}

/// Stop the run in progress.
#[tauri::command]
pub fn grounds_stop() {
    mechabellum_grounds::stop::request();
}

/// Restart the app: macOS applies a new Screen Recording permission only to a fresh process.
#[tauri::command]
pub fn grounds_restart(app: AppHandle, grounds: State<'_, Grounds>) -> Result<(), String> {
    if grounds.running.load(Ordering::SeqCst) {
        return Err("A Testing Grounds setup is running.".into());
    }
    app.restart()
}

/// How a run ended.
#[derive(Debug, Clone, Serialize)]
pub struct Finished {
    /// "done", "stopped" or "failed".
    outcome: &'static str,
    /// What happened, in a sentence.
    message: String,
    /// What Testing Grounds couldn't reproduce and was left out.
    left_out: Vec<String>,
    /// After a fight: where the saved replay disagrees with the board (empty: it matches).
    /// `None` when there was no fight.
    differences: Option<Vec<String>>,
    /// After a fight: expected differences, like a unit moved off the front edge.
    notes: Vec<String>,
}

#[derive(Clone, Serialize)]
struct Units {
    done: usize,
    total: usize,
}

/// Sends a run's progress to the window.
struct Events(AppHandle);

impl mechabellum_grounds::setup::Report for Events {
    fn line(&self, text: &str) {
        log::info!("grounds: {text}");
        let _ = self.0.emit("grounds-line", text);
    }
    fn units(&self, done: usize, total: usize) {
        let _ = self.0.emit("grounds-units", Units { done, total });
    }
}

/// Set up round `round_index` (0-based) of a stored match in Testing Grounds, and with `fight`,
/// fight it and compare the saved replay. Returns once the run has started; `grounds-finished`
/// says how it ended.
#[tauri::command]
pub async fn grounds_start(
    app: AppHandle,
    state: State<'_, AppState>,
    grounds: State<'_, Grounds>,
    battle_id: String,
    round_index: usize,
    fight: bool,
) -> Result<(), String> {
    if !cfg!(any(windows, target_os = "macos")) {
        return Err("Setting up Testing Grounds needs Windows or macOS.".into());
    }
    let store = state.store.clone();
    let detail = tauri::async_runtime::spawn_blocking(move || store.match_detail(&battle_id))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("{e:#}"))?
        .ok_or("That match isn't stored.")?;
    let board = mechabellum::board::Board::from_round(&detail, round_index)
        .ok_or("That round isn't in the replay.")?;
    let replays = state
        .watching
        .lock()
        .unwrap()
        .clone()
        .or_else(crate::steam::find_replay_dir)
        .ok_or("The game's replay folder wasn't found; Testing Grounds saves its fights there.")?;
    if grounds
        .running
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err("A Testing Grounds setup is already running.".into());
    }
    let handle = app.clone();
    let spawned = std::thread::Builder::new()
        .name("grounds".into())
        .spawn(move || {
            let finished = run(&handle, &board, &replays, fight);
            log::info!("grounds: {}", finished.message);
            handle
                .state::<Grounds>()
                .running
                .store(false, Ordering::SeqCst);
            let _ = handle.emit("grounds-finished", finished);
        });
    if let Err(e) = spawned {
        grounds.running.store(false, Ordering::SeqCst);
        return Err(e.to_string());
    }
    Ok(())
}

#[cfg(any(windows, target_os = "macos"))]
fn run(
    app: &AppHandle,
    board: &mechabellum::board::Board,
    replays: &std::path::Path,
    fight: bool,
) -> Finished {
    use mechabellum_grounds::{calibration::Calibrations, flow, setup, stop};

    stop::reset();
    let report = Events(app.clone());
    // A round of a real match: leave out what Testing Grounds can't reproduce, and say so.
    let options = setup::Options {
        fresh: true,
        leave_out: true,
    };
    let calibrations = Calibrations::bundled();
    let result = if fight {
        setup::verify(board, &calibrations, replays, options, &report).map(|checked| {
            let matches = checked.differences.is_empty();
            Finished {
                outcome: "done",
                message: if matches {
                    "Set up and fought; the saved replay matches the board.".into()
                } else {
                    "Set up and fought; the saved replay differs from the board.".into()
                },
                left_out: checked.set_up.left_out,
                differences: Some(checked.differences),
                notes: checked.notes,
            }
        })
    } else {
        setup::set_up(board, &calibrations, replays, options, &report).map(|(_, set_up)| Finished {
            outcome: "done",
            message: "Set up. Press Start in the game to fight it.".into(),
            left_out: set_up.left_out,
            differences: None,
            notes: Vec::new(),
        })
    };
    // The fights this run saved, except the newest (it records the loadouts techs need).
    flow::tidy_replays();
    result.unwrap_or_else(|e| {
        let (outcome, message) = match stop::is_stop(&e) {
            Some(stop::Reason::MouseMoved) => ("stopped", "Stopped: the mouse moved.".to_owned()),
            Some(stop::Reason::Asked) => ("stopped", "Stopped.".to_owned()),
            None => ("failed", format!("{e:#}")),
        };
        Finished {
            outcome,
            message,
            left_out: Vec::new(),
            differences: None,
            notes: Vec::new(),
        }
    })
}

#[cfg(not(any(windows, target_os = "macos")))]
fn run(
    _app: &AppHandle,
    _board: &mechabellum::board::Board,
    _replays: &std::path::Path,
    _fight: bool,
) -> Finished {
    Finished {
        outcome: "failed",
        message: "Setting up Testing Grounds needs Windows or macOS.".into(),
        left_out: Vec::new(),
        differences: None,
        notes: Vec::new(),
    }
}
