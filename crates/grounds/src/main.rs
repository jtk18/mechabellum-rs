//! `grounds`: set up a board from board code in Testing Grounds.
//!
//! Start the game, enter Testing Grounds (Railway Town, the default map), then run
//! `grounds place board.txt`. Only your own side, and only the part of the zone the default
//! camera shows, for now. See docs/testing-grounds.md.

use mechabellum::board::{Board, Half};
use mechabellum_grounds::{plan, Projection, RAILWAY_TOWN_NORTH, RAILWAY_TOWN_SOUTH};

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};

use std::path::PathBuf;

#[derive(Parser)]
#[command(about = "Set up boards in Mechabellum's Testing Grounds")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Show where each unit of the first side would be clicked, without touching the game
    Plan { code: PathBuf },
    /// Place the first side's units in the running game
    Place {
        code: PathBuf,
        /// Stop after this many units
        #[arg(long)]
        limit: Option<usize>,
        /// Unit types already in the shop, in shop order (e.g. Crawler,Fang)
        #[arg(long, value_delimiter = ',')]
        unlocked: Vec<String>,
    },
    /// Get into the Testing Grounds sandbox from the main menu, the lobby, or a round
    Enter {
        /// Leave and re-enter first: a clean board, round 1, and a reset camera
        #[arg(long)]
        fresh: bool,
    },
    /// Open Testing Grounds on a map (fresh session)
    Go {
        #[arg(long)]
        map: String,
    },
    /// Re-fight a round from a real replay in Testing Grounds and compare the outcome
    Replay {
        /// A .grbr replay
        file: PathBuf,
        /// 1-based round; the last round by default
        #[arg(long)]
        round: Option<usize>,
    },
    /// Remove every unit on both sides
    Clear,
    /// Leave the sandbox for the Testing Grounds presets
    Lobby,
    /// Delete the practice replays this tool saved, except the newest
    Tidy,
    /// Put an item from the Equipment tab in your inventory, by item ID
    Grant { id: u32 },
    /// Equip catalog items on units, fight, and check each item's ID in the replay
    ItemsCheck {
        /// First catalog cell to check (0-based, top to bottom)
        #[arg(long, default_value_t = 0)]
        from: usize,
        /// How many cells to check in one fight (at most 13)
        #[arg(long, default_value_t = 13)]
        count: usize,
        /// The unit to equip; some items only fit some units (Laser Sights won't go on a Crawler)
        #[arg(long, default_value = "Fortress")]
        unit: String,
    },
    /// Board squares under screen points (reference pixels, "x,y"), for a map and half
    Where {
        #[arg(long, default_value = "Railway Town")]
        map: String,
        #[arg(long)]
        north: bool,
        #[arg(value_parser = parse_point, required = true)]
        points: Vec<(u32, u32)>,
    },
    /// Measure a map's screen positions for both halves and save them
    Calibrate {
        /// Map name as the game shows it, e.g. "Forest Eye"
        #[arg(long)]
        map: String,
        /// Keep the replay it saves (normally deleted after reading)
        #[arg(long)]
        keep: bool,
    },
    /// Set up a board code, fight the round, save the replay, and check it matches the code
    Verify {
        code: PathBuf,
        /// Start from a fresh Testing Grounds session
        #[arg(long)]
        fresh: bool,
    },
    /// Spawn a unit and drop it at each screen point (reference pixels, "x,y"), for calibration
    Drop {
        #[arg(long, default_value = "Crawler")]
        unit: String,
        #[arg(value_parser = parse_point, required = true)]
        points: Vec<(u32, u32)>,
    },
    /// Print whether the tools panel is open in a screenshot
    Panel { png: PathBuf },
    /// Print which screen each screenshot shows
    Which {
        #[arg(required = true)]
        pngs: Vec<PathBuf>,
    },
    /// Mean color of rectangles (left,top,right,bottom) in screenshots, for tuning
    Color {
        #[arg(required = true)]
        pngs: Vec<PathBuf>,
        #[arg(long, value_delimiter = ';')]
        rect: Vec<String>,
    },
    /// Save a screenshot of the game window
    Shot { out: PathBuf },
    /// List green blobs (units bought this round) in a screenshot
    Blobs {
        png: PathBuf,
        /// Instead, print the greenest colors around x,y (for tuning)
        #[arg(long, value_parser = parse_point)]
        at: Option<(u32, u32)>,
    },
}

fn parse_point(s: &str) -> Result<(u32, u32), String> {
    let (x, y) = s.split_once(',').ok_or("x,y")?;
    Ok((
        x.trim().parse().map_err(|_| "x")?,
        y.trim().parse().map_err(|_| "y")?,
    ))
}

fn load(path: &PathBuf) -> Result<Board> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    Ok(Board::parse(&text)?)
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Plan { code } => {
            let board = load(&code)?;
            let (south, north) = projections(&board)?;
            let projection_for = |half: Option<Half>| {
                if half == Some(Half::North) {
                    north
                } else {
                    south
                }
            };
            for side in &board.sides {
                println!(
                    "{} ({} half):",
                    side.name.as_deref().unwrap_or("side"),
                    if side.half == Some(Half::North) {
                        "north"
                    } else {
                        "south"
                    }
                );
                for step in plan(side, &projection_for(side.half)) {
                    println!(
                        "{:<14} at {:>6.1},{:>6.1} -> screen {:>5.0},{:>5.0}{}",
                        step.name,
                        step.board.0,
                        step.board.1,
                        step.screen.0,
                        step.screen.1,
                        if step.clickable {
                            ""
                        } else {
                            "  (off screen: needs the camera moved)"
                        }
                    );
                }
            }
        }
        Command::Blobs {
            png,
            at: Some((cx, cy)),
        } => {
            let img = image::open(&png)?.to_rgb8();
            let mut hist = std::collections::BTreeMap::<(u8, u8, u8), usize>::new();
            for y in cy.saturating_sub(40)..cy + 40 {
                for x in cx.saturating_sub(40)..cx + 40 {
                    let p = img.get_pixel(x, y).0;
                    if p[1] > p[0] && p[1] > p[2] {
                        *hist
                            .entry((p[0] / 10 * 10, p[1] / 10 * 10, p[2] / 10 * 10))
                            .or_default() += 1;
                    }
                }
            }
            let mut top: Vec<_> = hist.into_iter().collect();
            top.sort_by(|a, b| b.1.cmp(&a.1));
            for (c, n) in top.into_iter().take(12) {
                println!("{c:?} x{n}");
            }
        }
        Command::Where { map, north, points } => {
            let (s, n) = mechabellum_grounds::calibration::Calibrations::load(
                &mechabellum_grounds::calibration::default_path(),
            )
            .projections(&map)
            .with_context(|| format!("{map} isn't calibrated"))?;
            let p = if north { n } else { s };
            for (x, y) in points {
                let (bx, by) = p.to_board(f64::from(x), f64::from(y));
                println!("{x},{y} -> board {bx:.1},{by:.1}");
            }
        }
        Command::Color { pngs, rect } => {
            for png in pngs {
                let img = image::open(&png)?.to_rgb8();
                let cols: Vec<String> = rect
                    .iter()
                    .map(|r| {
                        let v: Vec<u32> =
                            r.split(',').filter_map(|x| x.trim().parse().ok()).collect();
                        let c = mechabellum_grounds::mean_color(&img, (v[0], v[1], v[2], v[3]));
                        format!("{:>3.0},{:>3.0},{:>3.0}", c[0], c[1], c[2])
                    })
                    .collect();
                println!(
                    "{:<14} {}",
                    png.file_stem().unwrap().to_string_lossy(),
                    cols.join("  |  ")
                );
            }
        }
        Command::Which { pngs } => {
            for png in pngs {
                let img = image::open(&png)?.to_rgb8();
                println!(
                    "{:<14} {:?}",
                    png.file_stem().unwrap().to_string_lossy(),
                    mechabellum_grounds::screen::classify(&img)
                );
            }
        }
        Command::Panel { png } => {
            let img = image::open(&png)?.to_rgb8();
            println!(
                "{:?} open={}",
                mechabellum_grounds::mean_color(&img, (2200, 318, 2480, 345)),
                mechabellum_grounds::tools_open(&img)
            );
        }
        Command::Blobs { png, at: None } => {
            let img = image::open(&png)?.to_rgb8();
            for ((x, y), weight) in mechabellum_grounds::green_blobs(&img) {
                println!("{x:.0},{y:.0} weight {weight}");
            }
        }
        #[cfg(windows)]
        Command::Drop { unit, points } => {
            let mut game = mechabellum_grounds::win::Game::find()?;
            for (x, y) in points {
                let target = (f64::from(x), f64::from(y));
                let ok = spawn_and_place(&mut game, &unit, &[target], false)?;
                println!(
                    "{unit} to {x},{y}: {}",
                    if ok.is_some() {
                        "placed"
                    } else {
                        "NOT confirmed"
                    }
                );
            }
        }
        #[cfg(windows)]
        Command::Enter { fresh } => {
            let mut game = mechabellum_grounds::win::Game::find_or_launch()?;
            mechabellum_grounds::flow::enter(&mut game, fresh)?;
            println!("in Testing Grounds");
        }
        #[cfg(windows)]
        Command::Calibrate { map, keep } => calibrate(&map, keep)?,
        #[cfg(windows)]
        Command::Tidy => {
            println!(
                "removed {} practice replays",
                mechabellum_grounds::flow::tidy_replays()
            );
        }
        #[cfg(windows)]
        Command::Lobby => {
            let mut game = mechabellum_grounds::win::Game::find()?;
            mechabellum_grounds::flow::leave_to_lobby(&mut game)?;
        }
        #[cfg(windows)]
        Command::Go { map } => {
            let mut game = mechabellum_grounds::win::Game::find_or_launch()?;
            mechabellum_grounds::flow::enter_map(&mut game, &map)?;
        }
        #[cfg(windows)]
        Command::Replay { file, round } => replay(&file, round)?,
        #[cfg(windows)]
        Command::Grant { id } => {
            let mut game = mechabellum_grounds::win::Game::find()?;
            grant(&mut game, id)?;
            println!("granted {id}; it's in your inventory, top left");
        }
        #[cfg(windows)]
        Command::ItemsCheck { from, count, unit } => items_check(from, count, &unit)?,
        #[cfg(windows)]
        Command::Clear => {
            let mut game = mechabellum_grounds::win::Game::find()?;
            mechabellum_grounds::flow::clear_both(&mut game)?;
        }
        #[cfg(windows)]
        Command::Verify { code, fresh } => {
            let board = load(&code)?;
            let (south, north) = projections(&board)?;
            let problems = verify(&board, &south, &north, fresh)?;
            mechabellum_grounds::flow::tidy_replays();
            if problems.is_empty() {
                println!("OK: the replay matches the code");
            } else {
                for p in &problems {
                    println!("MISMATCH {p}");
                }
                std::process::exit(2);
            }
        }
        #[cfg(windows)]
        Command::Shot { out } => {
            let mut game = mechabellum_grounds::win::Game::find()?;
            game.screenshot()?.save(&out)?;
            println!("saved {}", out.display());
        }
        #[cfg(windows)]
        Command::Place {
            code,
            limit,
            unlocked,
        } => {
            let _ = unlocked;
            let board = load(&code)?;
            let (south, north) = projections(&board)?;
            place(&board, &south, &north, limit)?
        }
        #[cfg(not(windows))]
        Command::Shot { .. }
        | Command::Place { .. }
        | Command::Drop { .. }
        | Command::Enter { .. }
        | Command::Clear
        | Command::Replay { .. }
        | Command::Go { .. }
        | Command::Lobby
        | Command::Tidy
        | Command::Grant { .. }
        | Command::ItemsCheck { .. }
        | Command::Calibrate { .. }
        | Command::Verify { .. } => {
            bail!("driving the game needs Windows")
        }
    }
    Ok(())
}

/// The map a board code names; Railway Town (Testing Grounds' default) when it names none.
fn board_map(board: &Board) -> &str {
    board.map.as_deref().unwrap_or("Railway Town")
}

/// Screen projections (south, north) for the board's map.
fn projections(board: &Board) -> Result<(Projection, Projection)> {
    use mechabellum_grounds::calibration::{default_path, Calibrations};
    let map = board_map(board);
    Calibrations::load(&default_path())
        .projections(map)
        .with_context(|| {
            format!("{map} isn't calibrated yet; run: grounds calibrate --map \"{map}\"")
        })
}

fn replay_dir() -> PathBuf {
    mechabellum::install::find_replay_dir().unwrap_or_else(|| {
        mechabellum::install::default_game_dir()
            .join("ProjectDatas")
            .join("Replay")
    })
}

/// Set up `board` on a clean Testing Grounds board, fight the round, and compare the saved
/// replay with the code. Returns the differences.
#[cfg(windows)]
fn verify(
    board: &Board,
    south: &Projection,
    north: &Projection,
    fresh: bool,
) -> Result<Vec<String>> {
    use mechabellum_grounds::flow;
    let mut game = mechabellum_grounds::win::Game::find_or_launch()?;
    // A code for a named map starts a fresh session on that map; otherwise use what's open.
    if board.map.is_some() {
        flow::enter_map(&mut game, board_map(board))?;
    } else {
        flow::enter(&mut game, fresh)?;
    }
    // Clearing also switches sides and back, which settles your own side's camera where
    // calibration measured it: straight after entering, it sits about a square off.
    flow::clear_both(&mut game)?;
    // Researching techs needs each side's loadout, which only a Testing Grounds replay records.
    // Without one, fight an empty round to make one.
    let wants_techs = board
        .sides
        .iter()
        .flat_map(|s| &s.units)
        .any(|u| !u.techs.is_empty());
    if wants_techs && mechabellum_grounds::loadouts(&replay_dir()).is_err() {
        println!("no Testing Grounds replay yet; running an empty round to record the loadouts");
        flow::run_round_and_save(&mut game, &replay_dir())?;
    }
    place(board, south, north, None)?;
    let mut game = mechabellum_grounds::win::Game::find()?;
    let file = flow::run_round_and_save(&mut game, &replay_dir())?;
    println!("saved {}", file.display());
    let record = mechabellum::BattleRecord::new_from_file(&file, false)
        .map_err(|e| anyhow::anyhow!("reading {}: {e:?}", file.display()))?;
    let detail = record.match_detail();
    let last = detail
        .rounds
        .len()
        .checked_sub(1)
        .context("the replay has no rounds")?;
    let recorded = Board::from_round(&detail, last).context("no board in the last round")?;
    println!(
        "recorded:
{recorded}"
    );
    Ok(mechabellum_grounds::compare::differences(board, &recorded))
}

/// Board squares to drop calibration units at, in a side's own frame (x right, y toward the
/// middle; the south half's coordinates). Spread over the zone, and deliberately not symmetric,
/// so a fit can't match them rotated. They keep clear of the towers (about ±14,-17.5) and the
/// spawn pad (the middle), where drops are refused. Each drop snaps to the grid, so a fit needs
/// many points to average that out.
const CALIBRATION_TARGETS: [(f64, f64); 14] = [
    (-22.0, -6.0),
    (18.0, -9.0),
    (-4.0, -3.0),
    (-25.0, -27.0),
    (24.0, -22.0),
    (7.0, -28.0),
    (-6.0, -10.0),
    (9.0, -4.0),
    (-27.0, -15.0),
    (27.0, -12.0),
    (-17.0, -25.0),
    (3.0, -23.0),
    (-9.0, -29.0),
    (15.0, -27.0),
];

/// Put an item in your inventory from the tools' Equipment tab, and close the tools.
#[cfg(windows)]
fn grant(game: &mut mechabellum_grounds::win::Game, id: u32) -> Result<()> {
    use mechabellum_grounds::ui;
    let cell = mechabellum_grounds::equipment::cell(id)
        .with_context(|| format!("item {id} isn't in the Equipment tab"))?;
    game.right_click(ui::NEUTRAL, 400);
    let mut opened = false;
    for _ in 0..3 {
        if mechabellum_grounds::tools_open(&game.screenshot()?) {
            opened = true;
            break;
        }
        game.click(ui::TOOLS, 1200);
    }
    if !opened {
        bail!("couldn't open Testing Grounds Tools");
    }
    game.click(ui::TOOLS_EQUIPMENT_TAB, 800);
    // To the top, then down to the cell's row. Only scroll with the list under the cursor.
    game.wheel_at(ui::PALETTE_SCROLL, 150);
    if cell.scroll_down_steps > 0 {
        game.wheel_at(ui::PALETTE_SCROLL, -cell.scroll_down_steps);
    }
    game.click(cell.at, 900);
    game.click(ui::TOOLS_CLOSE_LEFT, 500);
    game.click(ui::TOOLS_CLOSE_RIGHT, 700);
    Ok(())
}

/// Equip catalog cells `from..from + count` on units across your half, fight, and compare
/// each unit's item in the replay with the cell's ID.
#[cfg(windows)]
fn items_check(from: usize, count: usize, unit: &str) -> Result<()> {
    use mechabellum_grounds::{equipment, flow, ui};

    let cells: Vec<_> = equipment::cells()
        .into_iter()
        .skip(from)
        .take(count)
        .collect();
    if cells.is_empty() {
        bail!("no catalog cells from {from}");
    }
    // Railway Town refuses drops at (-4,-3), right in front of the pad.
    let targets: Vec<(f64, f64)> = CALIBRATION_TARGETS
        .iter()
        .copied()
        .filter(|&t| t != (-4.0, -3.0))
        .take(cells.len())
        .collect();
    let map = "Railway Town";
    let (south, _) = mechabellum_grounds::calibration::Calibrations::load(
        &mechabellum_grounds::calibration::default_path(),
    )
    .projections(map)
    .context("Railway Town isn't calibrated")?;

    let mut game = mechabellum_grounds::win::Game::find_or_launch()?;
    flow::enter_map(&mut game, map)?;
    flow::clear_both(&mut game)?;
    let mut placed = Vec::new();
    let mut refused = Vec::new();
    for (cell, &(x, y)) in cells.iter().zip(&targets) {
        let at = south.to_screen(x, y);
        match spawn_and_place(&mut game, unit, &[at], false) {
            Ok(Some(_)) => {}
            Ok(None) => {
                println!("  {unit} at {x},{y} wasn't placed; skipping {}", cell.name);
                continue;
            }
            Err(e) => {
                println!("  {unit} at {x},{y}: {e:#}; skipping {}", cell.name);
                game.right_click(ui::NEUTRAL, 600);
                continue;
            }
        }
        grant(&mut game, cell.id)?;
        game.click(ui::INVENTORY_ITEM, 700);
        game.click(at, 900);
        game.right_click(ui::NEUTRAL, 600);
        // A unit that can't wear the item leaves it in the inventory, where it would be picked
        // up again next time; clear it.
        if mechabellum_grounds::inventory_has_item(&game.screenshot()?) {
            refused.push(*cell);
            game.click(ui::TOOLS, 1200);
            game.click(ui::TOOLS_CLEAR_EQUIPMENT, 800);
            game.click(ui::TOOLS_CLOSE_LEFT, 500);
            game.click(ui::TOOLS_CLOSE_RIGHT, 700);
            continue;
        }
        placed.push((cell, (x, y)));
    }
    for cell in &refused {
        println!(
            "  a {unit} can't wear rank {} {} ({})",
            cell.rank, cell.name, cell.id
        );
    }
    let file = flow::run_round_and_save(&mut game, &replay_dir())?;
    let record = mechabellum::BattleRecord::new_from_file(&file, false)
        .map_err(|e| anyhow::anyhow!("reading {}: {e:?}", file.display()))?;
    let detail = record.match_detail();
    let last = detail.rounds.len().checked_sub(1).context("no rounds")?;
    let units: Vec<&mechabellum::PlacedUnit> = detail.rounds[last]
        .sides
        .iter()
        .flatten()
        .flat_map(|s| &s.units)
        .filter(|u| u.y < 0)
        .collect();
    let (mut ok, mut wrong) = (0, 0);
    for (cell, (x, y)) in placed {
        let near = units.iter().min_by(|a, b| {
            let d = |u: &&&mechabellum::PlacedUnit| {
                (f64::from(u.x) / 10.0 - x).hypot(f64::from(u.y) / 10.0 - y)
            };
            d(a).total_cmp(&d(b))
        });
        let worn: Vec<String> = near
            .map(|u| u.equipment.iter().map(|e| e.id.clone()).collect())
            .unwrap_or_default();
        let label = format!("rank {} {} ({})", cell.rank, cell.name, cell.id);
        if worn.iter().any(|id| id.trim() == cell.id.to_string()) {
            ok += 1;
            println!("  confirmed {label}");
        } else {
            wrong += 1;
            let names: Vec<String> = worn
                .iter()
                .map(|id| {
                    let name = mechabellum::resolve_name(mechabellum::NameKind::Equipment, id)
                        .unwrap_or("?");
                    format!("{name} ({id})")
                })
                .collect();
            println!(
                "  MISMATCH {label}: the {unit} wore {}",
                if names.is_empty() {
                    "nothing".to_owned()
                } else {
                    names.join(", ")
                }
            );
        }
    }
    println!("{ok} confirmed, {wrong} not");
    flow::tidy_replays();
    Ok(())
}

/// The worst distance, in pixels, between measured points and a fit through them.
fn worst_error(points: &mechabellum_grounds::calibration::Points) -> Option<f64> {
    let p = Projection::fit(points)?;
    Some(
        points
            .iter()
            .map(|&(x, y, u, v)| {
                let (su, sv) = p.to_screen(x, y);
                (su - u).hypot(sv - v)
            })
            .fold(0.0, f64::max),
    )
}

/// Drop Crawlers across both halves of `map`, fight a round, read where the replay recorded
/// them, and save a fitted calibration for the map.
#[cfg(windows)]
fn calibrate(map: &str, keep: bool) -> Result<()> {
    use mechabellum_grounds::calibration::{
        default_path, match_by_guess, match_points, Calibrations, MapCalibration,
    };
    use mechabellum_grounds::{flow, ui};

    let mut game = mechabellum_grounds::win::Game::find_or_launch()?;
    flow::enter_map(&mut game, map)?;
    flow::clear_both(&mut game)?;
    // First guesses at screen positions: the map's own calibration if it has one, else Railway
    // Town's. Other maps' layouts are similar enough that these land inside their zones; the
    // fit uses where they actually land.
    let (guess_south, guess_north) = match Calibrations::load(&default_path()).projections(map) {
        Some(p) => p,
        None => (
            Projection::fit(&RAILWAY_TOWN_SOUTH).context("calibration")?,
            Projection::fit(&RAILWAY_TOWN_NORTH).context("calibration")?,
        ),
    };
    let known = Calibrations::load(&default_path())
        .projections(map)
        .is_some();
    let mut dropped = [Vec::new(), Vec::new()];
    let mut current = Half::South;
    for (i, half) in [Half::South, Half::North].into_iter().enumerate() {
        switch_to(&mut game, half, &mut current)?;
        let guess = if half == Half::North {
            &guess_north
        } else {
            &guess_south
        };
        for &(x, y) in &CALIBRATION_TARGETS {
            let (bx, by) = if half == Half::North {
                (-x, -y)
            } else {
                (x, y)
            };
            let target = guess.to_screen(bx, by);
            if !mechabellum_grounds::clickable(target) {
                continue;
            }
            match spawn_and_place(&mut game, "Crawler", &[target], false) {
                Ok(Some(_)) => dropped[i].push(target),
                Ok(None) => {
                    // Refused (a building, the zone's edge): the unit may still be carried.
                    game.right_click(ui::NEUTRAL, 600);
                    println!("  drop at {:.0},{:.0} was refused", target.0, target.1);
                }
                Err(e) => println!("  drop at {:.0},{:.0}: {e:#}", target.0, target.1),
            }
        }
        println!("{half:?}: {} units placed", dropped[i].len());
    }
    switch_to(&mut game, Half::South, &mut current)?;
    let file = flow::run_round_and_save(&mut game, &replay_dir())?;
    let record = mechabellum::BattleRecord::new_from_file(&file, false)
        .map_err(|e| anyhow::anyhow!("reading {}: {e:?}", file.display()))?;
    if !keep {
        let _ = std::fs::remove_file(&file);
    }
    let detail = record.match_detail();
    let last = detail
        .rounds
        .len()
        .checked_sub(1)
        .context("the replay has no rounds")?;
    let board = Board::from_round(&detail, last).context("no board in the replay")?;
    let recorded = |half: Half| -> Vec<(f64, f64)> {
        board
            .sides
            .iter()
            .filter(|s| s.half == Some(half))
            .flat_map(|s| &s.units)
            .map(|u| (f64::from(u.x) / 10.0, f64::from(u.y) / 10.0))
            .collect()
    };
    let path = default_path();
    let mut all = Calibrations::load(&path);
    // Earlier runs' points for this map, pooled with this run's so grid snapping averages out.
    let earlier = all.maps.get(map).cloned().unwrap_or_default();
    let mut result = MapCalibration::default();
    for (i, half) in [Half::South, Half::North].into_iter().enumerate() {
        // With a close calibration already, match each drop to the unit recorded nearest where
        // it predicts; else rely on placement order.
        let guess = if half == Half::North {
            &guess_north
        } else {
            &guess_south
        };
        let by_guess = known
            .then(|| match_by_guess(&dropped[i], &recorded(half), guess))
            .flatten()
            .filter(|(_, e)| *e <= 25.0);
        let (mut points, err) = by_guess
            .or_else(|| match_points(&dropped[i], &recorded(half)))
            .with_context(|| format!("{half:?}: too few units to fit (need 4)"))?;
        println!(
            "{half:?}: fit from {} points, worst error {err:.1} px",
            points.len()
        );
        if err > 25.0 {
            bail!("{half:?}: the fit is poor ({err:.0} px); not saving");
        }
        let old = match half {
            Half::South => &earlier.south,
            Half::North => &earlier.north,
        };
        let mut pooled = old.clone();
        pooled.extend_from_slice(&points);
        let pooled_err = worst_error(&pooled);
        if !old.is_empty() && pooled_err.is_some_and(|e| e <= 25.0) {
            println!(
                "{half:?}: pooled with {} earlier points, worst error {:.1} px",
                old.len(),
                pooled_err.unwrap_or_default()
            );
            points = pooled;
        } else if !old.is_empty() {
            println!("{half:?}: earlier points disagree; replacing them");
        }
        match half {
            Half::South => result.south = points,
            Half::North => result.north = points,
        }
    }
    all.maps.insert(map.to_owned(), result);
    all.save(&path)?;
    println!("saved {map} to {}", path.display());
    flow::clear_both(&mut game)?;
    Ok(())
}

/// Each side's fight result in one round, and the HP it lost when the next round records it:
/// (half, result, hp lost).
fn outcome(detail: &mechabellum::MatchDetail, index: usize) -> Vec<(Half, String, Option<i32>)> {
    let hp_after = |s: usize| {
        detail
            .rounds
            .get(index + 1)
            .and_then(|r| r.sides.get(s))
            .and_then(Option::as_ref)
            .map(|side| side.hp)
    };
    detail.rounds[index]
        .sides
        .iter()
        .enumerate()
        .filter_map(|(s, side)| {
            let side = side.as_ref()?;
            let mut ys: Vec<i32> = side.units.iter().map(|u| u.y).collect();
            ys.sort_unstable();
            let half = if ys.get(ys.len() / 2).is_some_and(|&y| y > 0) {
                Half::North
            } else {
                Half::South
            };
            let lost = hp_after(s).map(|after| side.hp - after);
            Some((half, side.result.clone(), lost))
        })
        .collect()
}

/// Set up a round of a real match and fight it again; report how the outcome compares.
#[cfg(windows)]
fn replay(file: &std::path::Path, round: Option<usize>) -> Result<()> {
    let record = mechabellum::BattleRecord::new_from_file(file, false)
        .map_err(|e| anyhow::anyhow!("reading {}: {e:?}", file.display()))?;
    let detail = record.match_detail();
    let index = match round {
        Some(r) => r.checked_sub(1).context("rounds start at 1")?,
        None => detail
            .rounds
            .len()
            .checked_sub(1)
            .context("the replay has no rounds")?,
    };
    let mut board = Board::from_round(&detail, index).context("that round isn't in the replay")?;
    let (south, north) = projections(&board)?;

    // Keep what Testing Grounds can reproduce, and say what was left out.
    let (mine, theirs) = mechabellum_grounds::loadouts(&replay_dir()).unwrap_or_default();
    let mut dropped = Vec::new();
    for side in &mut board.sides {
        let north_half = side.half == Some(Half::North);
        let projection = if north_half { north } else { south };
        let loadout = if north_half { &theirs } else { &mine };
        side.units.retain(|u| {
            let screen = projection.to_screen(f64::from(u.x) / 10.0, f64::from(u.y) / 10.0);
            let ok = mechabellum_grounds::clickable(screen)
                && mechabellum_grounds::ui::palette(&u.name).is_some();
            if !ok {
                dropped.push(format!(
                    "{} at {:.1},{:.1} (can't be placed)",
                    u.name,
                    f64::from(u.x) / 10.0,
                    f64::from(u.y) / 10.0
                ));
            }
            ok
        });
        for u in &mut side.units {
            let equipped = loadout.get(&u.unit_id).map(Vec::as_slice).unwrap_or(&[]);
            let before = u.techs.len();
            u.techs.retain(|t| {
                mechabellum_grounds::tech_buttons(&u.name, std::slice::from_ref(t), equipped)
                    .is_ok()
            });
            if u.techs.len() < before {
                dropped.push(format!(
                    "some techs on {} (not in the Testing Grounds loadout)",
                    u.name
                ));
            }
            if !u.items.is_empty() {
                dropped.push(format!("items on {}: {}", u.name, u.items.join(", ")));
                u.items.clear();
            }
        }
        for b in side.buildings.drain(..) {
            dropped.push(format!(
                "{} at {},{} (buildings aren't placed yet)",
                b.name,
                f64::from(b.x) / 10.0,
                f64::from(b.y) / 10.0
            ));
        }
    }
    println!(
        "round {} of {}:
{board}",
        index + 1,
        file.display()
    );
    dropped.sort();
    dropped.dedup();
    for d in &dropped {
        println!("left out: {d}");
    }

    let started_problems = verify(&board, &south, &north, true)?;
    for p in &started_problems {
        println!("setup differs: {p}");
    }
    mechabellum_grounds::flow::tidy_replays();

    // The fight just saved is the newest practice replay.
    let newest = std::fs::read_dir(replay_dir())?
        .flatten()
        .map(|e| e.path())
        .filter(|p| mechabellum_grounds::is_testing_grounds_replay(p))
        .max_by_key(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok())
        .context("no practice replay saved")?;
    let again = mechabellum::BattleRecord::new_from_file(&newest, false)
        .map_err(|e| anyhow::anyhow!("reading {}: {e:?}", newest.display()))?
        .match_detail();
    let real = outcome(&detail, index);
    let redo = outcome(&again, again.rounds.len() - 1);
    println!("\n{:<8} {:>18} {:>18}", "half", "real", "testing grounds");
    for half in [Half::South, Half::North] {
        let r = real.iter().find(|o| o.0 == half);
        let t = redo.iter().find(|o| o.0 == half);
        let show = |o: Option<&(Half, String, Option<i32>)>| match o {
            Some((_, result, Some(lost))) => format!("{result} (-{lost} HP)"),
            Some((_, result, None)) => result.clone(),
            None => "-".to_owned(),
        };
        println!("{:<8} {:>18} {:>18}", half_name(half), show(r), show(t));
    }
    Ok(())
}

/// Which side the game currently lets you edit: the name in the top-left box.
#[cfg(windows)]
fn switch_to(
    game: &mut mechabellum_grounds::win::Game,
    half: Half,
    current: &mut Half,
) -> Result<()> {
    use mechabellum_grounds::ui;
    if *current != half {
        game.right_click(ui::NEUTRAL, 400);
        game.click(ui::SWITCH_SIDES, 2500);
        *current = half;
    }
    // Zoom all the way out: a fixed view, whatever the zoom was.
    game.wheel(-60);
    Ok(())
}

#[cfg(windows)]
fn place(
    board: &Board,
    south: &Projection,
    north: &Projection,
    limit: Option<usize>,
) -> Result<()> {
    use mechabellum_grounds::ui;
    use mechabellum_grounds::win::Game;

    if board.sides.is_empty() {
        bail!("the code has no sides");
    }
    // Units spawn on a pad in the middle of the zone, and one standing there hides the next
    // spawn and blocks it. So place the units farthest from the pad first.
    let mut board = board.clone();
    for side in &mut board.sides {
        let projection = if side.half == Some(Half::North) {
            north
        } else {
            south
        };
        let from_pad = |u: &mechabellum::board::BoardUnit| {
            let (x, y) = projection.to_screen(f64::from(u.x) / 10.0, f64::from(u.y) / 10.0);
            (x - ui::SPAWN_PAD.0).hypot(y - ui::SPAWN_PAD.1)
        };
        side.units
            .sort_by(|a, b| from_pad(b).total_cmp(&from_pad(a)));
    }
    let board = &board;
    // Check every side fits before touching the game.
    for side in &board.sides {
        if side.half.is_none() && board.sides.len() > 1 {
            bail!("with two sides, give each one a half (half=south or half=north)");
        }
        let projection = if side.half == Some(Half::North) {
            north
        } else {
            south
        };
        if let Some(bad) = plan(side, projection).iter().find(|s| !s.clickable) {
            bail!(
                "{} at {},{} is outside the calibrated view",
                bad.name,
                bad.board.0,
                bad.board.1
            );
        }
    }

    // Techs are clicked by position on each unit's card, in loadout order.
    let replays = replay_dir();
    let replays = replays.as_path();
    let wants_techs = board
        .sides
        .iter()
        .flat_map(|s| &s.units)
        .any(|u| !u.techs.is_empty());
    let (mine, theirs) = if wants_techs {
        mechabellum_grounds::loadouts(replays)?
    } else {
        Default::default()
    };
    let mut buttons: Vec<Vec<usize>> = Vec::new();
    for (side, unit) in board
        .sides
        .iter()
        .flat_map(|s| s.units.iter().map(move |u| (s, u)))
    {
        let loadout = if side.half == Some(Half::North) {
            &theirs
        } else {
            &mine
        };
        let equipped = loadout.get(&unit.unit_id).map(Vec::as_slice).unwrap_or(&[]);
        buttons.push(
            mechabellum_grounds::tech_buttons(&unit.name, &unit.techs, equipped)
                .map_err(|e| anyhow::anyhow!(e))?,
        );
    }
    let mut buttons = buttons.into_iter();

    let mut game = Game::find()?;
    // Testing Grounds starts on your own (south) side.
    let mut current = Half::South;
    let mut placed = 0;
    for side in &board.sides {
        let half = side.half.unwrap_or(Half::South);
        switch_to(&mut game, half, &mut current)?;
        let projection = if half == Half::North { north } else { south };
        // Research costs supplies, and each side has its own; add plenty.
        if side.units.iter().any(|u| !u.techs.is_empty()) {
            game.click(ui::TOOLS, 1200);
            for _ in 0..5 {
                game.click(ui::TOOLS_SUPPLIES_1000, 300);
            }
            game.click(ui::TOOLS_CLOSE_LEFT, 500);
            game.click(ui::TOOLS_CLOSE_RIGHT, 700);
        }
        println!("{}:", side.name.as_deref().unwrap_or(half_name(half)));
        for (step, unit) in plan(side, projection).iter().zip(&side.units) {
            if limit.is_some_and(|l| placed >= l) {
                break;
            }
            placed += 1;
            // The north half's own frame is the board turned half a turn.
            let flip = if half == Half::North { -1.0 } else { 1.0 };
            let spots: Vec<(f64, f64)> = NUDGES
                .iter()
                .map(|(dx, dy)| (step.board.0 + dx * flip, step.board.1 + dy * flip))
                .filter(|&(x, y)| {
                    // Stay in the side's zone and in view.
                    (x * flip).abs() <= 30.0
                        && (-40.0..=-1.0).contains(&(y * flip))
                        && mechabellum_grounds::clickable(projection.to_screen(x, y))
                })
                .collect();
            let screens: Vec<_> = spots
                .iter()
                .map(|&(x, y)| projection.to_screen(x, y))
                .collect();
            let landed = spawn_and_place(&mut game, &step.name, &screens, unit.rotated)
                .with_context(|| format!("{} at {},{}", step.name, step.board.0, step.board.1))?;
            let at = landed.map_or(step.screen, |i| screens[i]);
            // Techs: select it (a click picks it up), click its card's buttons, then put it back.
            let techs = buttons.next().unwrap_or_default();
            if !techs.is_empty() {
                game.click(at, 900);
                for b in &techs {
                    game.click(ui::CARD_TECHS[*b], 900);
                }
                game.right_click(ui::NEUTRAL, 700);
            }
            // Level: select it, then Upgrade in the tools, once per level.
            if unit.level > 1 {
                game.click(at, 700);
                game.click(ui::TOOLS, 1200);
                for _ in 1..unit.level {
                    game.click(ui::TOOLS_UPGRADE, 500);
                }
                game.click(ui::TOOLS_CLOSE_LEFT, 500);
                game.click(ui::TOOLS_CLOSE_RIGHT, 700);
                game.right_click(ui::NEUTRAL, 500);
            }
            println!(
                "  {:<14} at {:>5.1},{:>5.1} level {}: {}",
                step.name,
                step.board.0,
                step.board.1,
                unit.level,
                match landed.map(|i| spots[i]) {
                    Some((x, y)) if (x, y) == step.board => "placed".to_owned(),
                    Some((x, y)) => format!("refused there; placed at {x:.1},{y:.1}"),
                    None => "NOT confirmed (check the screen)".to_owned(),
                }
            );
        }
    }
    // Leave the game on your own side.
    switch_to(&mut game, Half::South, &mut current)?;

    let skipped: Vec<&str> = board
        .sides
        .iter()
        .flat_map(|s| &s.units)
        .filter(|u| !u.items.is_empty())
        .map(|u| u.name.as_str())
        .collect();
    if !skipped.is_empty() {
        println!("Not set yet (items): {}", skipped.join(", "));
    }
    let buildings: Vec<&str> = board
        .sides
        .iter()
        .flat_map(|s| &s.buildings)
        .map(|b| b.name.as_str())
        .collect();
    if !buildings.is_empty() {
        println!("Not set yet (buildings): {}", buildings.join(", "));
    }
    Ok(())
}

fn half_name(half: Half) -> &'static str {
    match half {
        Half::South => "south",
        Half::North => "north",
    }
}

/// Board offsets (squares, in the side's own frame: +y toward the middle) to try when the game
/// refuses a drop, e.g. when a tower's model is in front of the spot. A refused unit would sit
/// on the spawn pad and block the next spawn, so a nearby spot beats none.
const NUDGES: [(f64, f64); 5] = [(0.0, 0.0), (0.0, 2.0), (-3.0, 0.0), (3.0, 0.0), (0.0, 4.0)];

/// Spawn a unit from the Squad list and move it to the first of `targets` the game accepts.
/// Returns the index of the target it landed on, if a unit was seen there afterwards.
#[cfg(windows)]
fn spawn_and_place(
    game: &mut mechabellum_grounds::win::Game,
    name: &str,
    targets: &[mechabellum_grounds::Px],
    rotated: bool,
) -> Result<Option<usize>> {
    use mechabellum_grounds::{green_blobs, ui, Px};

    let near = |a: Px, b: Px, d: f64| (a.0 - b.0).hypot(a.1 - b.1) < d;
    let cell = ui::palette(name).with_context(|| format!("{name} isn't in the Squad list"))?;

    // Keep the known camera view: zooming all the way out is harmless if it already is.
    game.wheel(-60);
    // Spawn it, and find it: the only new green blob.
    let before = green_blobs(&game.screenshot()?);
    // Open the tools (the button toggles), and never scroll unless the list is under the cursor:
    // over the map the wheel zooms the camera instead.
    let mut opened = false;
    for _ in 0..3 {
        if mechabellum_grounds::tools_open(&game.screenshot()?) {
            opened = true;
            break;
        }
        game.click(ui::TOOLS, 1200);
    }
    if !opened {
        bail!("couldn't open Testing Grounds Tools");
    }
    game.click(ui::TOOLS_SQUAD_TAB, 800);
    game.wheel_at(ui::PALETTE_SCROLL, 80);
    if cell.scroll_down_steps > 0 {
        game.wheel_at(ui::PALETTE_SCROLL, -cell.scroll_down_steps);
    }
    game.click(cell.at, 1000);
    game.click(ui::TOOLS_CLOSE_LEFT, 500);
    game.click(ui::TOOLS_CLOSE_RIGHT, 700);
    if mechabellum_grounds::tools_open(&game.screenshot()?) {
        game.click(ui::TOOLS_CLOSE_RIGHT, 700);
    }
    // The new unit is the blob that appeared, or the one that grew most (a spawn on top of an
    // earlier unit merges with it).
    let mut after = green_blobs(&game.screenshot()?);
    let growth = |(p, w): &(Px, usize)| {
        let old = before
            .iter()
            .filter(|(q, _)| near(*p, *q, 60.0))
            .map(|(_, w)| *w)
            .max()
            .unwrap_or(0);
        *w as i64 - old as i64
    };
    // A big unit's model hides much of its tile, splitting it into small pieces, so add up the
    // growth over the whole pad and take its weighted center.
    let pad_growth = |after: &[(Px, usize)]| -> Option<Px> {
        let grown: Vec<(Px, f64)> = after
            .iter()
            .filter(|b| near(b.0, ui::SPAWN_PAD, 170.0))
            .map(|b| (b.0, growth(b).max(0) as f64))
            .filter(|(_, g)| *g > 0.0)
            .collect();
        let total: f64 = grown.iter().map(|(_, g)| g).sum();
        (total > 100.0).then(|| {
            let x = grown.iter().map(|(p, g)| p.0 * g).sum::<f64>() / total;
            let y = grown.iter().map(|(p, g)| p.1 * g).sum::<f64>() / total;
            (x, y)
        })
    };
    // Units can take a moment to appear.
    let mut found = None;
    for _ in 0..4 {
        found = pad_growth(&after);
        if found.is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(700));
        after = green_blobs(&game.screenshot()?);
    }
    let Some(spawn) = found else {
        bail!("nothing appeared after spawning {name}");
    };

    // Pick it up, drop it, confirm, deselect. It has moved when the blob that grew at the spawn
    // point has shrunk back and a blob sits at the target. A refused drop leaves the unit carried,
    // so cancel with a right-click and try again.
    let spawn_weight = |blobs: &[(Px, usize)]| {
        blobs
            .iter()
            .filter(|(p, _)| near(*p, spawn, 60.0))
            .map(|(_, w)| *w)
            .max()
            .unwrap_or(0)
    };
    let grown = spawn_weight(&after);
    // Each target gets two tries; the first gets three, as a drop sometimes just doesn't take.
    let attempts = targets
        .iter()
        .enumerate()
        .flat_map(|(i, t)| std::iter::repeat_n((i, *t), if i == 0 { 3 } else { 2 }));
    for (i, target) in attempts {
        game.click(spawn, 700);
        game.click(target, 700);
        // Between the first click at the spot and the confirming double click, a middle click
        // turns the unit 90 degrees.
        if rotated {
            game.middle_click(target, 700);
        }
        game.double_click(target, 700);
        game.right_click(target, 900);
        let now = green_blobs(&game.screenshot()?);
        // A target on the pad itself can't show the unit leaving it.
        let left_spawn =
            near(target, spawn, 60.0) || (spawn_weight(&now) as f64) < grown as f64 * 0.8;
        let at_target = now.iter().any(|(p, _)| near(*p, target, 80.0));
        if left_spawn && at_target {
            return Ok(Some(i));
        }
        game.right_click(ui::NEUTRAL, 600);
    }
    Ok(None)
}
