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
/// Presets: the map dropdown, and its entries top to bottom.
const MAP_DROPDOWN: Px = (2260.0, 1172.0);
pub const MAPS: [&str; 5] = [
    "Railway Town",
    "Forest Eye",
    "Training Base",
    "Railway Town - Dusk",
    "Railway Town - Night",
];
const MAP_ENTRY_X: f64 = 2211.0;
const MAP_ENTRY_FIRST_Y: f64 = 1224.0;
const MAP_ENTRY_STEP: f64 = 49.9;
/// In-game menu: "Return to lobby".
const MENU_RETURN_TO_LOBBY: Px = (1280.0, 739.0);
/// Somewhere with nothing to hover over on any screen: below the board, above the hints.
const HOVER_CLEAR: Px = (1280.0, 1380.0);
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
                if crate::tools_open(&game.screenshot()?) {
                    game.click(ui::TOOLS_CLOSE_LEFT, 500);
                    game.click(ui::TOOLS_CLOSE_RIGHT, 800);
                }
                game.right_click(ui::NEUTRAL, 400);
                look_straight_down(game);
                return Ok(());
            }
            Screen::Battle => wait_for(game, Screen::Deploying, Duration::from_secs(200))?,
            Screen::Menu => game.click(ui::MENU_CLOSE, 1000),
            Screen::Lobby => {
                game.click(ENTER_TESTING_GROUNDS, 3000);
                wait_for(game, Screen::Deploying, Duration::from_secs(60))?;
                // The camera flies in after loading; zooming out before it settles gets undone.
                std::thread::sleep(Duration::from_secs(6));
            }
            Screen::MainMenu => game.click(MAIN_STANDARD_VERSUS, 3000),
            Screen::ModeTabs => {
                game.click(LOBBY_SOLO_TAB, 2500);
                game.click(SOLO_TESTING_GROUNDS, 3000);
            }
            // Loading, a dialog, or something unknown: wait rather than click blindly. The
            // cursor may be resting on the tools list, whose hover card covers the banner, so
            // move it (without clicking) to open ground below the board first.
            Screen::Other => {
                // A unit card pinned from the Squad list covers the banner; closing the tools
                // takes it away.
                if crate::left_tools_open(&game.screenshot()?) {
                    game.click(ui::TOOLS_CLOSE_LEFT, 500);
                    game.click(ui::TOOLS_CLOSE_RIGHT, 800);
                    continue;
                }
                game.glide(HOVER_CLEAR);
                std::thread::sleep(Duration::from_secs(3));
            }
        }
    }
    bail!("couldn't get into Testing Grounds")
}

/// Tilt the camera to look straight down and zoom all the way out. Both stop at a limit, so
/// the view is the same every time, and switching sides keeps the tilt. From the starting tilt a
/// tower's model hides the ground about 2 squares in front of it, so a click there hits the
/// tower; from above nothing is hidden.
pub fn look_straight_down(game: &mut Game) {
    if !crate::tilted_view() {
        game.hold_key('R', 4000);
    }
    game.wheel(-60);
}

/// Choose the map in the Testing Grounds presets. Must be on the presets screen.
pub fn select_map(game: &mut Game, name: &str) -> Result<()> {
    let key = |s: &str| s.to_lowercase().replace([' ', '-'], "");
    let Some(i) = MAPS.iter().position(|m| key(m) == key(name)) else {
        bail!(
            "unknown map {name:?}; Testing Grounds has {}",
            MAPS.join(", ")
        );
    };
    if current(game)? != Screen::Lobby {
        bail!("not on the Testing Grounds presets");
    }
    game.click(MAP_DROPDOWN, 1200);
    game.click(
        (MAP_ENTRY_X, MAP_ENTRY_FIRST_Y + MAP_ENTRY_STEP * i as f64),
        1200,
    );
    Ok(())
}

/// Get into Testing Grounds on a given map, starting fresh.
pub fn enter_map(game: &mut Game, name: &str) -> Result<()> {
    match current(game)? {
        Screen::Deploying | Screen::Battle | Screen::Menu => leave_to_lobby(game)?,
        Screen::Lobby => {}
        _ => {
            // Get to the presets by way of the sandbox route, then leave.
            enter(game, false)?;
            leave_to_lobby(game)?;
        }
    }
    select_map(game, name)?;
    enter(game, false)
}

/// Menu, Return to lobby.
pub fn leave_to_lobby(game: &mut Game) -> Result<()> {
    match current(game)? {
        Screen::Lobby => return Ok(()),
        Screen::Battle => wait_for(game, Screen::Deploying, Duration::from_secs(200))?,
        _ => {}
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

/// Clear both sides and come back to your own, zoomed out. Returns each half's empty view
/// (south, north), as a reference for where the camera sits when placements land exactly.
pub fn clear_both(game: &mut Game) -> Result<(image::RgbImage, image::RgbImage)> {
    clear_side(game)?;
    game.click(ui::SWITCH_SIDES, 2500);
    clear_side(game)?;
    game.wheel(-60);
    std::thread::sleep(Duration::from_millis(800));
    let north = game.screenshot()?;
    game.click(ui::SWITCH_SIDES, 2500);
    game.wheel(-60);
    std::thread::sleep(Duration::from_millis(800));
    let south = game.screenshot()?;
    Ok((south, north))
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

/// Press Start and wait for the fight to end and the next deployment to begin.
pub fn run_round(game: &mut Game) -> Result<()> {
    if current(game)? != Screen::Deploying {
        bail!("not ready to start a round");
    }
    game.right_click(ui::NEUTRAL, 400);
    game.click(ui::START, 3000);
    // The fight lasts up to the preset's combat phase (120 s by default).
    wait_for(game, Screen::Deploying, Duration::from_secs(240))?;
    std::thread::sleep(Duration::from_secs(2));
    Ok(())
}

/// Press Start, wait for the fight to end, and save the replay. Returns the replay file.
pub fn run_round_and_save(game: &mut Game, replay_dir: &Path) -> Result<PathBuf> {
    run_round(game)?;
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
                remember(&path);
                return Ok(path);
            }
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    bail!("no replay was saved in {}", replay_dir.display())
}

/// The list of replays this tool saved, so tidying never touches the player's own.
fn ledger() -> PathBuf {
    std::env::temp_dir().join("mechabellum-grounds-replays.txt")
}

fn remember(path: &Path) {
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(ledger())
    {
        let _ = writeln!(f, "{}", path.display());
    }
}

/// Delete the replays this tool saved, except the newest: that one records the loadouts that
/// researching techs needs. Returns how many were removed.
pub fn tidy_replays() -> usize {
    let text = std::fs::read_to_string(ledger()).unwrap_or_default();
    let mut saved: Vec<(PathBuf, SystemTime)> = text
        .lines()
        .map(PathBuf::from)
        .filter(|p| crate::is_testing_grounds_replay(p))
        .filter_map(|p| {
            let t = std::fs::metadata(&p).ok()?.modified().ok()?;
            Some((p, t))
        })
        .collect();
    saved.sort_by_key(|(_, t)| *t);
    saved.dedup_by(|a, b| a.0 == b.0);
    let newest = saved.pop();
    let removed = saved
        .iter()
        .filter(|(p, _)| std::fs::remove_file(p).is_ok())
        .count();
    let _ = std::fs::write(
        ledger(),
        newest
            .map(|(p, _)| format!("{}\n", p.display()))
            .unwrap_or_default(),
    );
    removed
}
