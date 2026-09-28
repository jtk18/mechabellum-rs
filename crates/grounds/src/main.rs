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
    /// Remove every unit on both sides
    Clear,
    /// Leave the sandbox for the Testing Grounds presets
    Lobby,
    /// Delete the practice replays this tool saved, except the newest
    Tidy,
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
                let ok = spawn_and_place(&mut game, &unit, target, false)?;
                println!(
                    "{unit} to {x},{y}: {}",
                    if ok { "placed" } else { "NOT confirmed" }
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
        | Command::Lobby
        | Command::Tidy
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
        if !fresh {
            flow::clear_both(&mut game)?;
        }
    }
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
/// so a fit can't match them rotated.
const CALIBRATION_TARGETS: [(f64, f64); 7] = [
    (-22.0, -6.0),
    (18.0, -9.0),
    (-4.0, -3.0),
    (-25.0, -27.0),
    (24.0, -22.0),
    (7.0, -28.0),
    (-12.0, -14.0),
];

/// Drop Crawlers across both halves of `map`, fight a round, read where the replay recorded
/// them, and save a fitted calibration for the map.
#[cfg(windows)]
fn calibrate(map: &str, keep: bool) -> Result<()> {
    use mechabellum_grounds::calibration::{
        default_path, match_points, Calibrations, MapCalibration,
    };
    use mechabellum_grounds::{flow, ui};

    let mut game = mechabellum_grounds::win::Game::find_or_launch()?;
    flow::enter_map(&mut game, map)?;
    flow::clear_both(&mut game)?;
    // First guesses at screen positions: Railway Town's calibration. The real map's layout is
    // similar enough that these land inside its zones; the fit uses where they actually land.
    let guess_south = Projection::fit(&RAILWAY_TOWN_SOUTH).context("calibration")?;
    let guess_north = Projection::fit(&RAILWAY_TOWN_NORTH).context("calibration")?;
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
            match spawn_and_place(&mut game, "Crawler", target, false) {
                Ok(true) => dropped[i].push(target),
                Ok(false) => {
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
    let mut result = MapCalibration::default();
    for (i, half) in [Half::South, Half::North].into_iter().enumerate() {
        let (points, err) = match_points(&dropped[i], &recorded(half))
            .with_context(|| format!("{half:?}: too few units to fit (need 4)"))?;
        println!(
            "{half:?}: fit from {} points, worst error {err:.1} px",
            points.len()
        );
        if err > 25.0 {
            bail!("{half:?}: the fit is poor ({err:.0} px); not saving");
        }
        match half {
            Half::South => result.south = points,
            Half::North => result.north = points,
        }
    }
    let path = default_path();
    let mut all = Calibrations::load(&path);
    all.maps.insert(map.to_owned(), result);
    all.save(&path)?;
    println!("saved {map} to {}", path.display());
    flow::clear_both(&mut game)?;
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
            let landed = spawn_and_place(&mut game, &step.name, step.screen, unit.rotated)
                .with_context(|| format!("{} at {},{}", step.name, step.board.0, step.board.1))?;
            // Techs: select it (a click picks it up), click its card's buttons, then put it back.
            let techs = buttons.next().unwrap_or_default();
            if !techs.is_empty() {
                game.click(step.screen, 900);
                for b in &techs {
                    game.click(ui::CARD_TECHS[*b], 900);
                }
                game.right_click(ui::NEUTRAL, 700);
            }
            // Level: select it, then Upgrade in the tools, once per level.
            if unit.level > 1 {
                game.click(step.screen, 700);
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
                if landed {
                    "placed"
                } else {
                    "NOT confirmed (check the screen)"
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
    Ok(())
}

fn half_name(half: Half) -> &'static str {
    match half {
        Half::South => "south",
        Half::North => "north",
    }
}

/// Spawn a unit from the Squad list and move it to `target`. Returns whether a unit was seen
/// there afterwards.
#[cfg(windows)]
fn spawn_and_place(
    game: &mut mechabellum_grounds::win::Game,
    name: &str,
    target: mechabellum_grounds::Px,
    rotated: bool,
) -> Result<bool> {
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
    // Units can take a moment to appear.
    let mut found = None;
    for _ in 0..4 {
        found = after
            .iter()
            .filter(|b| growth(b) > 100 && near(b.0, ui::SPAWN_PAD, 170.0))
            .max_by_key(|b| growth(b))
            .map(|b| b.0);
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
    for _ in 0..3 {
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
        let left_spawn = (spawn_weight(&now) as f64) < grown as f64 * 0.8;
        let at_target = now.iter().any(|(p, _)| near(*p, target, 80.0));
        if left_spawn && at_target {
            return Ok(true);
        }
        game.right_click(ui::NEUTRAL, 600);
    }
    Ok(false)
}
