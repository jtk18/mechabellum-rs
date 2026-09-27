//! `grounds`: set up a board from board code in Testing Grounds.
//!
//! Start the game, enter Testing Grounds (Railway Town, the default map), then run
//! `grounds place board.txt`. Only your own side, and only the part of the zone the default
//! camera shows, for now. See docs/testing-grounds.md.

use mechabellum::board::{Board, Half};
use mechabellum_grounds::{plan, Projection, RAILWAY_TOWN_SOUTH};

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
    let projection = Projection::fit(&RAILWAY_TOWN_SOUTH).context("calibration")?;
    match cli.command {
        Command::Plan { code } => {
            let board = load(&code)?;
            let side = board.sides.first().context("the code has no sides")?;
            if side.half == Some(Half::North) {
                bail!("only the south half is calibrated so far");
            }
            for step in plan(side, &projection) {
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
        Command::Blobs { png, at: None } => {
            let img = image::open(&png)?.to_rgb8();
            for ((x, y), weight) in mechabellum_grounds::green_blobs(&img) {
                println!("{x:.0},{y:.0} weight {weight}");
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
        } => place(&load(&code)?, &projection, limit, unlocked)?,
        #[cfg(not(windows))]
        Command::Shot { .. } | Command::Place { .. } => bail!("driving the game needs Windows"),
    }
    Ok(())
}

#[cfg(windows)]
fn place(
    board: &Board,
    projection: &Projection,
    limit: Option<usize>,
    _already: Vec<String>,
) -> Result<()> {
    use mechabellum_grounds::win::Game;
    use mechabellum_grounds::{green_blobs, ui, Px};

    let side = board.sides.first().context("the code has no sides")?;
    if side.half == Some(Half::North) {
        bail!("only the south half is calibrated so far");
    }
    let steps = plan(side, projection);
    if let Some(bad) = steps.iter().find(|s| !s.clickable) {
        bail!(
            "{} at {},{} is off screen in the calibrated view; move it right or forward",
            bad.name,
            bad.board.0,
            bad.board.1
        );
    }
    let mut game = Game::find()?;
    game.wheel(-15);
    game.right_click(ui::NEUTRAL, 400);

    let near = |a: Px, b: Px, d: f64| (a.0 - b.0).hypot(a.1 - b.1) < d;
    let open_tools = |game: &mut Game| {
        game.click(ui::TOOLS, 1200);
        game.click(ui::TOOLS_SQUAD_TAB, 800);
    };
    let close_tools = |game: &mut Game| {
        game.click(ui::TOOLS_CLOSE_LEFT, 500);
        game.click(ui::TOOLS_CLOSE_RIGHT, 700);
    };

    for (i, (step, unit)) in steps
        .iter()
        .zip(&side.units)
        .take(limit.unwrap_or(usize::MAX))
        .enumerate()
    {
        let cell = ui::palette(&step.name)
            .with_context(|| format!("{} isn't in the Squad list", step.name))?;

        // Spawn it from the Squad list, and find it: the only new green blob.
        let before: Vec<Px> = green_blobs(&game.screenshot()?)
            .into_iter()
            .map(|b| b.0)
            .collect();
        open_tools(&mut game);
        game.wheel_at(ui::PALETTE_SCROLL, 80);
        if cell.scroll_down_steps > 0 {
            game.wheel_at(ui::PALETTE_SCROLL, -cell.scroll_down_steps);
        }
        game.click(cell.at, 1000);
        close_tools(&mut game);
        let after = green_blobs(&game.screenshot()?);
        let Some(&(spawn, _)) = after
            .iter()
            .find(|(p, _)| !before.iter().any(|&q| near(*p, q, 60.0)))
        else {
            bail!(
                "unit {} ({}): nothing appeared after spawning it",
                i + 1,
                step.name
            );
        };

        // Pick it up, drop it, confirm, deselect.
        game.click(spawn, 700);
        game.click(step.screen, 700);
        game.double_click(step.screen, 700);
        game.right_click(step.screen, 900);

        // Level: select it, then Upgrade in the tools.
        if unit.level > 1 {
            game.click(step.screen, 700);
            game.click(ui::TOOLS, 1200);
            for _ in 1..unit.level {
                game.click(ui::TOOLS_UPGRADE, 500);
            }
            close_tools(&mut game);
            game.right_click(ui::NEUTRAL, 500);
        }

        let landed = green_blobs(&game.screenshot()?)
            .into_iter()
            .any(|(p, _)| near(p, step.screen, 80.0));
        println!(
            "{:>2}. {:<14} at {:>5.1},{:>5.1} level {}: {}",
            i + 1,
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
    let skipped: Vec<&str> = side
        .units
        .iter()
        .filter(|u| !u.techs.is_empty() || !u.items.is_empty() || u.rotated)
        .map(|u| u.name.as_str())
        .collect();
    if !skipped.is_empty() {
        println!(
            "Not set yet (techs, items, rotation): {}",
            skipped.join(", ")
        );
    }
    Ok(())
}
