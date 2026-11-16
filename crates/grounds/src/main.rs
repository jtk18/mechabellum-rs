//! `grounds`: set up a board from board code in Testing Grounds.
//!
//! Start the game, enter Testing Grounds (Railway Town, the default map), then run
//! `grounds place board.txt`. Runs on Windows and macOS. See docs/testing-grounds.md.

use mechabellum::board::{Board, Half};
use mechabellum_grounds::calibration::{default_path, Calibrations};
use mechabellum_grounds::setup::{
    self, board_map, half_name, projections, replay_dir, Options, Print,
};
use mechabellum_grounds::{plan, Projection, TOP_DOWN_NORTH, TOP_DOWN_SOUTH};

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
    /// Place the board code's units, both sides, in the running game
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
    /// Fight the round as it's set up, save the replay, and print the board it recorded
    Fight,
    /// How far the map moved between two screenshots (reference first), in pixels
    Shift { reference: PathBuf, now: PathBuf },
    /// Place each of the 9 Building tiles on your half, fight, and print which building each is
    BuildingsCheck,
    /// Equip catalog items on units, fight, and check each item's ID in the replay
    ItemsCheck {
        /// First catalog cell to check (0-based, top to bottom)
        #[arg(long, default_value_t = 0)]
        from: usize,
        /// How many cells to check in one fight (at most 12: one per spot on the half)
        #[arg(long, default_value_t = 12)]
        count: usize,
        /// The unit to equip; some items only fit some units (Laser Sights won't go on a Crawler)
        #[arg(long, default_value = "Fortress")]
        unit: String,
        /// Only cells whose IDs aren't confirmed yet (`from` counts among those)
        #[arg(long)]
        unconfirmed: bool,
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
    /// Try a unit at spots around a tower, one click each, and print what the game did: where
    /// it may stand, where it's refused, and where the click goes to something else (the
    /// tower's model). Run on the half being shown.
    ClickMap {
        #[arg(long, default_value = "Railway Town - Night")]
        map: String,
        #[arg(long)]
        north: bool,
        /// cc or rc
        #[arg(long, default_value = "cc")]
        tower: String,
        /// Squares out from the tower's center to test, each way
        #[arg(long, default_value_t = 4.0)]
        reach: f64,
        /// Squares between spots
        #[arg(long, default_value_t = 1.0)]
        step: f64,
        /// Save each hover's screenshot here
        #[arg(long)]
        out: Option<PathBuf>,
        /// The unit to try
        #[arg(long, default_value = "Arclight")]
        unit: String,
    },
    /// Measure a map's screen positions for both halves and save them
    Calibrate {
        /// Map name as the game shows it, e.g. "Forest Eye"
        #[arg(long)]
        map: String,
        /// Keep the replay it saves (normally deleted after reading)
        #[arg(long)]
        keep: bool,
        /// Start over instead of pooling with the map's earlier points
        #[arg(long)]
        replace: bool,
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
    /// Turn the mouse wheel at the middle of the window (positive zooms in)
    Zoom {
        #[arg(allow_hyphen_values = true)]
        steps: i32,
    },
    /// Pan the camera: drag with the right button between two screen points (reference pixels)
    Pan {
        #[arg(value_parser = parse_point)]
        from: (u32, u32),
        #[arg(value_parser = parse_point)]
        to: (u32, u32),
    },
    /// Press Switch Sides this many times
    Switch {
        #[arg(default_value_t = 1)]
        times: u32,
    },
    /// Hold a camera key (Q, E, R or F) for some milliseconds
    Key {
        letter: char,
        #[arg(default_value_t = 1000)]
        ms: u64,
    },
    /// Turn the camera: drag with the middle button between two screen points
    Turn {
        #[arg(value_parser = parse_point)]
        from: (u32, u32),
        #[arg(value_parser = parse_point)]
        to: (u32, u32),
    },
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

/// The compiled-in calibrations, with any saved by `grounds calibrate` in their place.
fn calibrations() -> Calibrations {
    Calibrations::load(&default_path())
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Plan { code } => {
            let board = load(&code)?;
            let (south, north) = projections(&calibrations(), board_map(&board))?;
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
        #[cfg(any(windows, target_os = "macos"))]
        Command::ClickMap {
            map,
            north,
            tower,
            reach,
            step,
            out,
            unit,
        } => click_map(&map, north, &tower, reach, step, out.as_deref(), &unit)?,
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
        Command::Shift { reference, now } => {
            let a = image::open(&reference)?.to_rgb8();
            let b = image::open(&now)?.to_rgb8();
            let ((dx, dy), s) =
                mechabellum_grounds::camera_shift(&a, &b, &mechabellum_grounds::OPEN_GROUND);
            println!("the map moved {dx},{dy} px (mean difference {s:.1})");
        }
        Command::Blobs { png, at: None } => {
            let img = image::open(&png)?.to_rgb8();
            for ((x, y), weight) in mechabellum_grounds::green_blobs(&img) {
                println!("{x:.0},{y:.0} weight {weight}");
            }
        }
        #[cfg(any(windows, target_os = "macos"))]
        Command::Drop { unit, points } => {
            let mut game = mechabellum_grounds::driver::Game::find()?;
            for (x, y) in points {
                let target = (f64::from(x), f64::from(y));
                let ok = setup::spawn_and_place(&mut game, &unit, &[target], None)?;
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
        #[cfg(any(windows, target_os = "macos"))]
        Command::Enter { fresh } => {
            let mut game = mechabellum_grounds::driver::Game::find_or_launch()?;
            mechabellum_grounds::flow::enter(&mut game, fresh)?;
            println!("in Testing Grounds");
        }
        #[cfg(any(windows, target_os = "macos"))]
        Command::Calibrate { map, keep, replace } => calibrate(&map, keep, replace)?,
        #[cfg(any(windows, target_os = "macos"))]
        Command::Tidy => {
            println!(
                "removed {} practice replays",
                mechabellum_grounds::flow::tidy_replays()
            );
        }
        #[cfg(any(windows, target_os = "macos"))]
        Command::Lobby => {
            let mut game = mechabellum_grounds::driver::Game::find()?;
            mechabellum_grounds::flow::leave_to_lobby(&mut game)?;
        }
        #[cfg(any(windows, target_os = "macos"))]
        Command::Go { map } => {
            let mut game = mechabellum_grounds::driver::Game::find_or_launch()?;
            mechabellum_grounds::flow::enter_map(&mut game, &map)?;
        }
        #[cfg(any(windows, target_os = "macos"))]
        Command::Replay { file, round } => replay(&file, round)?,
        #[cfg(any(windows, target_os = "macos"))]
        Command::BuildingsCheck => buildings_check()?,
        #[cfg(any(windows, target_os = "macos"))]
        Command::Fight => {
            let mut game = mechabellum_grounds::driver::Game::find()?;
            let file = mechabellum_grounds::flow::run_round_and_save(&mut game, &replay_dir())?;
            let detail = mechabellum::BattleRecord::new_from_file(&file, false)
                .map_err(|e| anyhow::anyhow!("reading {}: {e:?}", file.display()))?
                .match_detail();
            let last = detail.rounds.len().checked_sub(1).context("no rounds")?;
            let board = Board::from_round(&detail, last).context("no board")?;
            println!("{board}");
        }
        #[cfg(any(windows, target_os = "macos"))]
        Command::Grant { id } => {
            let mut game = mechabellum_grounds::driver::Game::find()?;
            setup::grant(&mut game, id)?;
            println!("granted {id}; it's in your inventory, top left");
        }
        #[cfg(any(windows, target_os = "macos"))]
        Command::ItemsCheck {
            from,
            count,
            unit,
            unconfirmed,
        } => items_check(from, count, &unit, unconfirmed)?,
        #[cfg(any(windows, target_os = "macos"))]
        Command::Clear => {
            let mut game = mechabellum_grounds::driver::Game::find()?;
            mechabellum_grounds::flow::clear_both(&mut game)?;
        }
        #[cfg(any(windows, target_os = "macos"))]
        Command::Verify { code, fresh } => {
            let board = load(&code)?;
            let options = Options {
                fresh,
                leave_out: false,
            };
            let checked = setup::verify(&board, &calibrations(), &replay_dir(), options, &Print)?;
            mechabellum_grounds::flow::tidy_replays();
            for note in &checked.notes {
                println!("{note}");
            }
            if checked.differences.is_empty() {
                println!("OK: the replay matches the code");
            } else {
                for p in &checked.differences {
                    println!("MISMATCH {p}");
                }
                std::process::exit(2);
            }
        }
        #[cfg(any(windows, target_os = "macos"))]
        Command::Zoom { steps } => {
            let mut game = mechabellum_grounds::driver::Game::find()?;
            game.wheel(steps);
        }
        #[cfg(any(windows, target_os = "macos"))]
        Command::Key { letter, ms } => {
            if !"QERFqerf".contains(letter) {
                bail!("only the camera keys Q, E, R and F");
            }
            let mut game = mechabellum_grounds::driver::Game::find()?;
            game.hold_key(letter, ms);
        }
        #[cfg(any(windows, target_os = "macos"))]
        Command::Switch { times } => {
            let mut game = mechabellum_grounds::driver::Game::find()?;
            for _ in 0..times {
                game.right_click(mechabellum_grounds::ui::NEUTRAL, 400);
                game.click(mechabellum_grounds::ui::SWITCH_SIDES, 2500);
            }
            game.wheel(-60);
        }
        #[cfg(any(windows, target_os = "macos"))]
        Command::Turn { from, to } => {
            let mut game = mechabellum_grounds::driver::Game::find()?;
            let px = |(x, y): (u32, u32)| (f64::from(x), f64::from(y));
            game.middle_drag(px(from), px(to), 1500);
        }
        #[cfg(any(windows, target_os = "macos"))]
        Command::Pan { from, to } => {
            let mut game = mechabellum_grounds::driver::Game::find()?;
            let px = |(x, y): (u32, u32)| (f64::from(x), f64::from(y));
            game.right_drag(px(from), px(to), 1500);
        }
        #[cfg(any(windows, target_os = "macos"))]
        Command::Shot { out } => {
            let mut game = mechabellum_grounds::driver::Game::find()?;
            game.screenshot()?.save(&out)?;
            println!("saved {}", out.display());
        }
        #[cfg(any(windows, target_os = "macos"))]
        Command::Place {
            code,
            limit,
            unlocked,
        } => {
            let _ = unlocked;
            let board = load(&code)?;
            let (south, north) = projections(&calibrations(), board_map(&board))?;
            let mut game = mechabellum_grounds::driver::Game::find()?;
            setup::place(
                &mut game,
                &board,
                &south,
                &north,
                limit,
                None,
                &replay_dir(),
                &Print,
            )?
        }
        #[cfg(not(any(windows, target_os = "macos")))]
        Command::Shot { .. }
        | Command::Zoom { .. }
        | Command::Pan { .. }
        | Command::Turn { .. }
        | Command::Switch { .. }
        | Command::Key { .. }
        | Command::Place { .. }
        | Command::Drop { .. }
        | Command::Enter { .. }
        | Command::Clear
        | Command::Replay { .. }
        | Command::Go { .. }
        | Command::Lobby
        | Command::Tidy
        | Command::Grant { .. }
        | Command::Fight
        | Command::BuildingsCheck
        | Command::ItemsCheck { .. }
        | Command::Calibrate { .. }
        | Command::ClickMap { .. }
        | Command::Verify { .. } => {
            bail!("driving the game needs Windows or macOS")
        }
    }
    Ok(())
}

/// Board squares to drop calibration units at, in a side's own frame (x right, y toward the
/// middle; the south half's coordinates). Spread over the zone, and deliberately not symmetric,
/// so a fit can't match them rotated. They keep clear of the towers (±14,-17) and the
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

/// Place each of the Building section's 9 tiles on your half, fight, and print which building
/// the replay recorded at each spot.
#[cfg(any(windows, target_os = "macos"))]
fn buildings_check() -> Result<()> {
    use mechabellum_grounds::flow;
    let map = "Railway Town";
    let (south, _) = mechabellum_grounds::calibration::Calibrations::load(
        &mechabellum_grounds::calibration::default_path(),
    )
    .projections(map)
    .context("Railway Town isn't calibrated")?;
    let spots: Vec<(f64, f64)> = CALIBRATION_TARGETS
        .iter()
        .copied()
        .filter(|&t| t != (-4.0, -3.0) && t != (9.0, -4.0))
        .collect();
    let mut game = mechabellum_grounds::driver::Game::find_or_launch()?;
    flow::enter_map(&mut game, map)?;
    flow::clear_both(&mut game)?;
    let mut tried = Vec::new();
    for (i, &(x, y)) in spots.iter().take(9).enumerate() {
        setup::put_building(&mut game, i / 3, i % 3, south.to_screen(x, y))?;
        tried.push((i / 3, i % 3, (x, y)));
    }
    // A round with no units isn't recorded; add one out of the way.
    let (ux, uy) = spots[9];
    setup::spawn_and_place(&mut game, "Crawler", &[south.to_screen(ux, uy)], None)?;
    let file = flow::run_round_and_save(&mut game, &replay_dir())?;
    let detail = mechabellum::BattleRecord::new_from_file(&file, false)
        .map_err(|e| anyhow::anyhow!("reading {}: {e:?}", file.display()))?
        .match_detail();
    let last = detail.rounds.len().checked_sub(1).context("no rounds")?;
    let built: Vec<&mechabellum::PlacedBuilding> = detail.rounds[last]
        .sides
        .iter()
        .flatten()
        .flat_map(|s| &s.buildings)
        .filter(|b| b.y < 0)
        .collect();
    for (row, col, (x, y)) in tried {
        let near = built.iter().min_by(|a, b| {
            let d = |p: &&&mechabellum::PlacedBuilding| {
                (f64::from(p.x) / 10.0 - x).hypot(f64::from(p.y) / 10.0 - y)
            };
            d(a).total_cmp(&d(b))
        });
        match near {
            Some(b) if (f64::from(b.x) / 10.0 - x).hypot(f64::from(b.y) / 10.0 - y) < 4.0 => {
                println!(
                    "  row {row} col {col}: {} ({}) at {},{}",
                    b.name.as_deref().unwrap_or("?"),
                    b.id,
                    f64::from(b.x) / 10.0,
                    f64::from(b.y) / 10.0
                )
            }
            _ => println!("  row {row} col {col}: nothing recorded near {x},{y}"),
        }
    }
    flow::tidy_replays();
    Ok(())
}

/// Equip catalog cells `from..from + count` on units across your half, fight, and compare
/// each unit's item in the replay with the cell's ID.
#[cfg(any(windows, target_os = "macos"))]
fn items_check(from: usize, count: usize, unit: &str, unconfirmed: bool) -> Result<()> {
    use mechabellum_grounds::{equipment, flow, ui};

    let cells: Vec<_> = equipment::cells()
        .into_iter()
        .filter(|c| !unconfirmed || !equipment::CONFIRMED.contains(&c.id))
        .skip(from)
        .take(count)
        .collect();
    if cells.is_empty() {
        bail!("no catalog cells from {from}");
    }
    // Railway Town refuses drops at (-4,-3), and a Fortress at (9,-4), both near the pad.
    let targets: Vec<(f64, f64)> = CALIBRATION_TARGETS
        .iter()
        .copied()
        .filter(|&t| t != (-4.0, -3.0) && t != (9.0, -4.0))
        .take(cells.len())
        .collect();
    let map = "Railway Town";
    let (south, _) = mechabellum_grounds::calibration::Calibrations::load(
        &mechabellum_grounds::calibration::default_path(),
    )
    .projections(map)
    .context("Railway Town isn't calibrated")?;

    let mut game = mechabellum_grounds::driver::Game::find_or_launch()?;
    flow::enter_map(&mut game, map)?;
    flow::clear_both(&mut game)?;
    // Start with an empty inventory: a leftover item would be picked up instead of the new one.
    setup::clear_inventory(&mut game);
    let mut placed = Vec::new();
    let mut refused = Vec::new();
    for (cell, &(x, y)) in cells.iter().zip(&targets) {
        let at = south.to_screen(x, y);
        match setup::spawn_and_place(&mut game, unit, &[at], None) {
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
        setup::grant(&mut game, cell.id)?;
        game.click(ui::INVENTORY_ITEM, 700);
        game.click(at, 900);
        game.right_click(ui::NEUTRAL, 600);
        // A unit that can't wear the item leaves it in the inventory, where it would be picked
        // up again next time; clear it.
        if mechabellum_grounds::inventory_has_item(&game.screenshot()?) {
            refused.push(*cell);
            setup::clear_inventory(&mut game);
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

/// See `Command::ClickMap`. Each probe: pick the unit up from the pad, click the spot, read the
/// screen, right-click (a placed unit goes back to the pad, a carried one is let go).
#[cfg(any(windows, target_os = "macos"))]
fn click_map(
    map: &str,
    north: bool,
    tower: &str,
    reach: f64,
    step: f64,
    out: Option<&std::path::Path>,
    unit: &str,
) -> Result<()> {
    use mechabellum_grounds::setup::TOWER;
    let (s, n) = calibrations()
        .projections(map)
        .with_context(|| format!("{map} isn't calibrated"))?;
    let projection = if north { n } else { s };
    let flip = if north { -1.0 } else { 1.0 };
    let tx = match tower {
        "cc" => -TOWER.0,
        "rc" => TOWER.0,
        _ => bail!("--tower is cc or rc"),
    };
    // A point in the side's own frame (x right, y forward) to the screen.
    let screen = |x: f64, y: f64| projection.to_screen(x * flip, y * flip);
    let mut game = mechabellum_grounds::driver::Game::find()?;
    // The unit is moved from spot to spot: picked up where it stands, dropped and confirmed at
    // the next spot. Where it ends up (its green tint) says whether the spot took it.
    let (mut at, _, _) = setup::spawn_unit(&mut game, unit)?;
    let near = |a: (f64, f64), b: (f64, f64), d: f64| (a.0 - b.0).hypot(a.1 - b.1) < d;
    println!(
        "{unit} centered at each spot near the {tower} (own {tx},{}): G it stood there, . it didn't",
        TOWER.1
    );
    // GROUNDS_STOP_AFTER=n leaves the unit where the nth probe put it (to fight and check).
    let stop_after: Option<usize> = std::env::var("GROUNDS_STOP_AFTER")
        .ok()
        .and_then(|v| v.parse().ok());
    let mut probes = 0;
    let k = (reach / step).round() as i32;
    print!("      ");
    for i in -k..=k {
        print!("{:>5}", f64::from(i) * step);
    }
    println!("   <- right of the tower's center");
    for j in (-k..=k).rev() {
        let dy = f64::from(j) * step;
        print!("{dy:>5} ");
        for i in -k..=k {
            let dx = f64::from(i) * step;
            let p = screen(tx + dx, TOWER.1 + dy);
            game.click(at, 600);
            game.click(p, 600);
            game.double_click(p, 600);
            game.right_click(mechabellum_grounds::ui::NEUTRAL, 700);
            let shot = game.screenshot()?;
            if let Some(dir) = out {
                let _ = shot.save(dir.join(format!("click_{dx}_{dy}.png")));
            }
            let blobs = mechabellum_grounds::green_blobs(&shot);
            let placed = blobs.iter().any(|(b, _)| near(*b, p, 20.0));
            if placed {
                at = p;
            } else if let Some((b, _)) = blobs.iter().min_by(|x, y| {
                let d = |q: &(f64, f64)| (q.0 - at.0).hypot(q.1 - at.1);
                d(&x.0).total_cmp(&d(&y.0))
            }) {
                // Not there: it's wherever its tint is now (usually where it was).
                at = *b;
            }
            print!("{:>5}", if placed { "G" } else { "." });
            probes += 1;
            if stop_after == Some(probes) {
                println!();
                return Ok(());
            }
        }
        println!();
    }
    game.park();
    Ok(())
}

/// Drop Crawlers across both halves of `map`, fight a round, read where the replay recorded
/// them, and save a fitted calibration for the map.
#[cfg(any(windows, target_os = "macos"))]
fn calibrate(map: &str, keep: bool, replace: bool) -> Result<()> {
    use mechabellum_grounds::calibration::{match_by_guess, match_points, MapCalibration};
    use mechabellum_grounds::{flow, ui};

    let mut game = mechabellum_grounds::driver::Game::find_or_launch()?;
    flow::enter_map(&mut game, map)?;
    flow::clear_both(&mut game)?;
    // First guesses at screen positions: the map's own calibration if it has one, else Railway
    // Town's. Other maps' layouts are similar enough that these land inside their zones; the
    // fit uses where they actually land.
    let (guess_south, guess_north) = match Calibrations::load(&default_path()).projections(map) {
        Some(p) => p,
        None => (
            Projection::fit(&TOP_DOWN_SOUTH).context("calibration")?,
            Projection::fit(&TOP_DOWN_NORTH).context("calibration")?,
        ),
    };
    let known = Calibrations::load(&default_path())
        .projections(map)
        .is_some();
    let mut dropped = [Vec::new(), Vec::new()];
    let mut current = Half::South;
    for (i, half) in [Half::South, Half::North].into_iter().enumerate() {
        setup::switch_to(&mut game, half, &mut current);
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
            match setup::spawn_and_place(&mut game, "Crawler", &[target], None) {
                Ok(Some(_)) => {
                    // Looking straight down, the unit's green tile lies flat on the ground
                    // around its center: measure that, not the click, which the game rounds to
                    // the grid (by up to half a square).
                    let blobs = mechabellum_grounds::green_blobs(&game.screenshot()?);
                    let tile = blobs
                        .iter()
                        .map(|b| b.0)
                        .filter(|p| (p.0 - target.0).hypot(p.1 - target.1) < 45.0)
                        .min_by(|a, b| {
                            let d = |p: &(f64, f64)| (p.0 - target.0).hypot(p.1 - target.1);
                            d(a).total_cmp(&d(b))
                        });
                    dropped[i].push(tile.unwrap_or(target));
                }
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
    setup::switch_to(&mut game, Half::South, &mut current);
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
    let earlier = if replace {
        MapCalibration::default()
    } else {
        all.maps.get(map).cloned().unwrap_or_default()
    };
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
#[cfg(any(windows, target_os = "macos"))]
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
    let board = Board::from_round(&detail, index).context("that round isn't in the replay")?;
    println!("round {} of {}:\n{board}", index + 1, file.display());
    // Testing Grounds can't reproduce everything; set_up says what it left out.
    let options = Options {
        fresh: true,
        leave_out: true,
    };
    let checked = setup::verify(&board, &calibrations(), &replay_dir(), options, &Print)?;
    for p in checked.notes.iter().chain(&checked.differences) {
        println!("setup differs: {p}");
    }
    mechabellum_grounds::flow::tidy_replays();
    let again = mechabellum::BattleRecord::new_from_file(&checked.replay, false)
        .map_err(|e| anyhow::anyhow!("reading {}: {e:?}", checked.replay.display()))?
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
