//! Getting around the game: into Testing Grounds from wherever it is, clearing the board,
//! running a round, and saving its replay. Every step checks the screen it expects first, and
//! nothing here goes near Multiplayer or matchmaking.

use crate::screen::{classify, Screen};
use crate::ui;
use crate::win::Game;
use crate::Px;

use anyhow::{bail, Result};

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

/// Main menu: "Standard Versus".
const MAIN_STANDARD_VERSUS: Px = (512.0, 560.0);
/// Mode lobby: the "Solo" tab.
const LOBBY_SOLO_TAB: Px = (843.0, 69.0);
/// Solo tab: the Testing Grounds card.
const SOLO_TESTING_GROUNDS: Px = (2176.0, 768.0);
/// Presets: "Enter the Testing Grounds".
const ENTER_TESTING_GROUNDS: Px = (2284.0, 1475.0);
/// In-game menu: "Return to lobby".
const MENU_RETURN_TO_LOBBY: Px = (1280.0, 739.0);
/// Tools, bottom left: "Clear all", and its confirmation.
const TOOLS_CLEAR_ALL: Px = (298.0, 1498.0);
const CLEAR_CONFIRM: Px = (1116.0, 956.0);

pub fn current(game: &mut Game) -> Result<Screen> {
    Ok(classify(&game.screenshot()?))
}

/// Poll until `want` shows, or give up after `timeout`.
pub fn wait_for(game: &mut Game, want: Screen, timeout: Duration) -> Result<()> {
    let start = Instant::now();
    loop {
        let now = current(game)?;
        if now == want {
            return Ok(());
        }
        if start.elapsed() > timeout {
            bail!("waited {timeout:?} for {want:?}, but the game shows {now:?}");
        }
        std::thread::sleep(Duration::from_millis(1500));
    }
}

/// Get to the Testing Grounds sandbox, ready to deploy, with the camera in its known view.
/// With `fresh`, leave and re-enter first: a clean board, round 1, and a reset camera.
pub fn enter(game: &mut Game, fresh: bool) -> Result<()> {
    if fresh && matches!(current(game)?, Screen::Deploying | Screen::Battle) {
        leave_to_lobby(game)?;
    }
    for _ in 0..8 {
        match current(game)? {
            Screen::Deploying => {
                game.right_click(ui::NEUTRAL, 400);
                game.wheel(-60);
                return Ok(());
            }
            Screen::Battle => wait_for(game, Screen::Deploying, Duration::from_secs(200))?,
            Screen::Menu => game.click(ui::MENU_CLOSE, 1000),
            Screen::Lobby => {
                game.click(ENTER_TESTING_GROUNDS, 3000);
                wait_for(game, Screen::Deploying, Duration::from_secs(60))?;
            }
            Screen::MainMenu => game.click(MAIN_STANDARD_VERSUS, 3000),
            Screen::ModeTabs => {
                game.click(LOBBY_SOLO_TAB, 2500);
                game.click(SOLO_TESTING_GROUNDS, 3000);
            }
            // Loading, a dialog, or something unknown: wait rather than click blindly.
            Screen::Other => std::thread::sleep(Duration::from_secs(3)),
        }
    }
    bail!("couldn't get into Testing Grounds")
}

/// Menu, Return to lobby.
pub fn leave_to_lobby(game: &mut Game) -> Result<()> {
    if current(game)? == Screen::Battle {
        wait_for(game, Screen::Deploying, Duration::from_secs(200))?;
    }
    game.right_click(ui::NEUTRAL, 400);
    game.click(ui::MENU, 1500);
    if current(game)? != Screen::Menu {
        bail!("the in-game menu didn't open");
    }
    game.click(MENU_RETURN_TO_LOBBY, 4000);
    wait_for(game, Screen::Lobby, Duration::from_secs(60))
}

/// Remove every unit on the side being edited.
fn clear_side(game: &mut Game) -> Result<()> {
    game.right_click(ui::NEUTRAL, 400);
    game.click(ui::TOOLS, 1300);
    game.click(TOOLS_CLEAR_ALL, 900);
    game.click(CLEAR_CONFIRM, 900);
    game.click(ui::TOOLS_CLOSE_LEFT, 500);
    game.click(ui::TOOLS_CLOSE_RIGHT, 800);
    Ok(())
}

/// Clear both sides and come back to your own, zoomed out.
pub fn clear_both(game: &mut Game) -> Result<()> {
    clear_side(game)?;
    game.click(ui::SWITCH_SIDES, 2500);
    clear_side(game)?;
    game.click(ui::SWITCH_SIDES, 2500);
    game.wheel(-60);
    Ok(())
}

/// The newest replay file in `dir` and when it was written.
fn newest_replay(dir: &Path) -> Option<(PathBuf, SystemTime)> {
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "grbr"))
        .filter_map(|e| Some((e.path(), e.metadata().ok()?.modified().ok()?)))
        .max_by_key(|(_, t)| *t)
}

/// Press Start, wait for the fight to end, and save the replay. Returns the replay file.
pub fn run_round_and_save(game: &mut Game, replay_dir: &Path) -> Result<PathBuf> {
    if current(game)? != Screen::Deploying {
        bail!("not ready to start a round");
    }
    game.right_click(ui::NEUTRAL, 400);
    game.click(ui::START, 3000);
    // The fight lasts up to the preset's combat phase (120 s by default).
    wait_for(game, Screen::Deploying, Duration::from_secs(240))?;
    std::thread::sleep(Duration::from_secs(2));
    let before = SystemTime::now();
    game.click(ui::MENU, 1500);
    if current(game)? != Screen::Menu {
        bail!("the in-game menu didn't open");
    }
    game.click(ui::MENU_SAVE_REPLAY, 3000);
    game.click(ui::MENU_CLOSE, 1000);
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(20) {
        if let Some((path, when)) = newest_replay(replay_dir) {
            if when + Duration::from_secs(1) >= before {
                return Ok(path);
            }
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    bail!("no replay was saved in {}", replay_dir.display())
}
