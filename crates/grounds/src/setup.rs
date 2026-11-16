//! Setting up whole boards: getting into Testing Grounds, placing every unit, building, device
//! and item of a board code, and checking the result against the replay the game saves. The
//! `grounds` program and the desktop app both run these. Progress goes to a [`Report`], not to
//! standard output, and a run stops when the user moves the mouse (see [`crate::stop`]).

use crate::calibration::Calibrations;
use crate::{plan, ui, Projection};

use anyhow::{Context, Result};
use mechabellum::board::{Board, Half};

use std::path::PathBuf;

/// Where a run's progress goes.
pub trait Report {
    /// A line of the running account, e.g. "  Crawler at 3.0,-12.0 level 1: placed".
    fn line(&self, text: &str);
    /// Units placed so far, of all the board's units.
    fn units(&self, _done: usize, _total: usize) {}
}

/// Prints each line (the `grounds` program).
pub struct Print;

impl Report for Print {
    fn line(&self, text: &str) {
        println!("{text}");
    }
}

/// How a setup treats what Testing Grounds can't reproduce.
#[derive(Debug, Clone, Copy, Default)]
pub struct Options {
    /// Leave and re-enter Testing Grounds first: a clean board, round 1, and a reset camera.
    /// (A code that names its map always gets a fresh session on that map.)
    pub fresh: bool,
    /// Leave out what Testing Grounds can't reproduce, and say so, instead of refusing the code:
    /// a map it doesn't have (Railway Town stands in), units it can't click to, techs not in
    /// the loadout, items not in the Equipment tab, buildings and devices it has no tile for.
    /// For setting up rounds of real matches.
    pub leave_out: bool,
}

/// What a setup put on the board.
#[derive(Debug, Clone)]
pub struct SetUp {
    /// The board as set up: the code, less anything left out.
    pub board: Board,
    /// What was left out and why, one line each.
    pub left_out: Vec<String>,
}

/// What a check found.
#[derive(Debug, Clone)]
pub struct Checked {
    pub set_up: SetUp,
    /// The replay the fight was saved to.
    pub replay: PathBuf,
    /// The board that replay recorded.
    pub recorded: Board,
    /// Expected differences, each starting with [`crate::compare::NOTE`]: a unit Testing
    /// Grounds moved off the front edge, say.
    pub notes: Vec<String>,
    /// Where the replay disagrees with the board as set up. Empty when it matches.
    pub differences: Vec<String>,
}

/// The map a board code names; Railway Town (Testing Grounds' default) when it names none.
pub fn board_map(board: &Board) -> &str {
    board.map.as_deref().unwrap_or("Railway Town")
}

/// Screen projections (south, north) for a map.
pub fn projections(calibrations: &Calibrations, map: &str) -> Result<(Projection, Projection)> {
    calibrations.projections(map).with_context(|| {
        format!("{map} isn't calibrated yet; run: grounds calibrate --map \"{map}\"")
    })
}

pub fn half_name(half: Half) -> &'static str {
    match half {
        Half::South => "south",
        Half::North => "north",
    }
}

/// The game's replay folder: the one Steam's libraries have, else the default install's.
pub fn replay_dir() -> PathBuf {
    mechabellum::install::find_replay_dir().unwrap_or_else(|| {
        mechabellum::install::default_game_dir()
            .join("ProjectDatas")
            .join("Replay")
    })
}

fn same_map(a: &str, b: &str) -> bool {
    let key = |s: &str| s.to_lowercase().replace([' ', '-'], "");
    key(a) == key(b)
}

/// For a map Testing Grounds doesn't have, or one with no calibration, the map to use instead
/// (Railway Town) and a line saying so. `None` when the board's own map will do.
pub fn stand_in_map(board: &Board, calibrations: &Calibrations) -> Option<(String, String)> {
    let named = board_map(board);
    let offered = crate::MAPS.iter().any(|m| same_map(m, named));
    if offered && calibrations.projections(named).is_some() {
        return None;
    }
    Some((
        "Railway Town".to_owned(),
        format!(
            "the map: Testing Grounds has no {named}, so it's set up on Railway Town (the towers \
             stand in the same places on every map, but the ground between them differs)"
        ),
    ))
}

/// Remove what can't be placed from the projections and tiles alone: units outside the
/// calibrated view or missing from the Squad list, items not in the Equipment tab, buildings and
/// devices outside the view or without a tile. Returns a line for each thing left out.
pub fn leave_out_unplaceable(
    board: &mut Board,
    south: &Projection,
    north: &Projection,
) -> Vec<String> {
    let at = |x: i32, y: i32| format!("{:.1},{:.1}", f64::from(x) / 10.0, f64::from(y) / 10.0);
    let mut dropped = Vec::new();
    for side in &mut board.sides {
        let projection = if side.half == Some(Half::North) {
            north
        } else {
            south
        };
        let steps = plan(side, projection);
        let mut keep = steps
            .iter()
            .map(|s| s.clickable && ui::palette(&s.name).is_some());
        side.units.retain(|u| {
            let ok = keep.next().unwrap_or(false);
            if !ok {
                dropped.push(format!("{} at {} (can't be placed)", u.name, at(u.x, u.y)));
            }
            ok
        });
        for u in &mut side.units {
            let before = u.items.len();
            u.items.retain(|i| {
                mechabellum::board::resolve_item(i)
                    .is_ok_and(|id| crate::equipment::cell(id).is_some())
            });
            if u.items.len() < before {
                dropped.push(format!(
                    "some items on {} (not in the Equipment tab)",
                    u.name
                ));
            }
        }
        let clickable = |x: i32, y: i32| {
            crate::clickable(projection.to_screen(f64::from(x) / 10.0, f64::from(y) / 10.0))
        };
        side.buildings.retain(|b| {
            let ok = clickable(b.x, b.y) && ui::building_tile(b.building_id).is_some();
            if !ok {
                dropped.push(format!("{} at {} (can't be placed)", b.name, at(b.x, b.y)));
            }
            ok
        });
        side.devices.retain(|d| {
            let ok = clickable(d.x, d.y) && ui::device_tile(d.building_id).is_some();
            if !ok {
                dropped.push(format!("{} at {} (can't be placed)", d.name, at(d.x, d.y)));
            }
            ok
        });
    }
    dropped.sort();
    dropped.dedup();
    dropped
}

/// Remove techs that aren't in the side's Testing Grounds loadout (yours for the south half,
/// the Bot's for the north). Returns a line for each unit that lost some.
pub fn leave_out_techs(
    board: &mut Board,
    mine: &crate::Loadout,
    theirs: &crate::Loadout,
) -> Vec<String> {
    let mut dropped = Vec::new();
    for side in &mut board.sides {
        let loadout = if side.half == Some(Half::North) {
            theirs
        } else {
            mine
        };
        for u in &mut side.units {
            let equipped = loadout.get(&u.unit_id).map(Vec::as_slice).unwrap_or(&[]);
            let before = u.techs.len();
            u.techs.retain(|t| {
                crate::tech_buttons(&u.name, std::slice::from_ref(t), equipped).is_ok()
            });
            if u.techs.len() < before {
                dropped.push(format!(
                    "some techs on {} (not in the Testing Grounds loadout)",
                    u.name
                ));
            }
        }
    }
    dropped.sort();
    dropped.dedup();
    dropped
}

/// Command Center and Research Center on the south half, in squares (the same on every 2.0 map;
/// mirrored for the north). They look the same every round, so they anchor camera matching.
pub const TOWER: (f64, f64) = (14.0, -17.0);

#[cfg(any(windows, target_os = "macos"))]
pub use driving::*;

#[cfg(any(windows, target_os = "macos"))]
mod driving {
    use super::*;
    use crate::driver::Game;
    use crate::flow;
    use crate::{green_blobs, Px};

    use anyhow::bail;

    use std::path::Path;

    /// Get into Testing Grounds on the board's map, clear both sides, reach the board's round,
    /// and place it. Leaves the game on your own side, ready for Start.
    pub fn set_up(
        board: &Board,
        calibrations: &Calibrations,
        replays: &Path,
        options: Options,
        report: &dyn Report,
    ) -> Result<(Game, SetUp)> {
        let mut board = board.clone();
        let mut left_out = Vec::new();
        if options.leave_out {
            if let Some((map, why)) = stand_in_map(&board, calibrations) {
                board.map = Some(map);
                left_out.push(why);
            }
        }
        let (south, north) = projections(calibrations, board_map(&board))?;
        if options.leave_out {
            left_out.extend(leave_out_unplaceable(&mut board, &south, &north));
        }

        let mut game = Game::find_or_launch()?;
        // A code for a named map starts a fresh session on that map; otherwise use what's open.
        if board.map.is_some() {
            flow::enter_map(&mut game, board_map(&board))?;
        } else {
            flow::enter(&mut game, options.fresh)?;
        }
        // Clearing also switches sides and back, which settles your own side's camera where
        // calibration measured it: straight after entering, it sits about a square off.
        let reference = flow::clear_both(&mut game)?;
        // Researching techs needs each side's loadout, which only a Testing Grounds replay
        // records. Without one, fight an empty round to make one.
        let wants_techs = |b: &Board| {
            b.sides
                .iter()
                .flat_map(|s| &s.units)
                .any(|u| !u.techs.is_empty())
        };
        let mut fought = false;
        if wants_techs(&board) && crate::loadouts(replays).is_err() {
            report.line(
                "no Testing Grounds replay yet; running an empty round to record the loadouts",
            );
            flow::run_round_and_save(&mut game, replays)?;
            fought = true;
        }
        if options.leave_out && wants_techs(&board) {
            let (mine, theirs) = crate::loadouts(replays).unwrap_or_default();
            left_out.extend(leave_out_techs(&mut board, &mine, &theirs));
        }
        for line in &left_out {
            report.line(&format!("left out: {line}"));
        }
        // The deployment zone grows with the round: in round 1 its front stops about 5 squares
        // from the middle, and flank columns open in round 3. Fight empty rounds up to the code's.
        let round = board.round.unwrap_or(1).clamp(1, 12);
        let now_round = if fought { 2 } else { 1 };
        if round > now_round {
            report.line(&format!("advancing Testing Grounds to round {round}"));
            for _ in now_round..round {
                flow::run_round(&mut game)?;
            }
        }
        // Fights and side switches move the camera by about a square (measured 2026-09-29 on
        // Railway Town: after fights one half's units landed a square toward the middle, the
        // other's exactly, and which one changed with the switching). place() lines each half
        // up with its view from just after clearing, when placements land exactly.
        place(
            &mut game,
            &board,
            &south,
            &north,
            None,
            Some(&reference),
            replays,
            report,
        )?;
        Ok((game, SetUp { board, left_out }))
    }

    /// Set up `board` on a clean Testing Grounds board, fight the round, and compare the saved
    /// replay with the board as set up.
    pub fn verify(
        board: &Board,
        calibrations: &Calibrations,
        replays: &Path,
        options: Options,
        report: &dyn Report,
    ) -> Result<Checked> {
        let (mut game, set_up) = set_up(board, calibrations, replays, options, report)?;
        let file = flow::run_round_and_save(&mut game, replays)?;
        report.line(&format!("saved {}", file.display()));
        let record = mechabellum::BattleRecord::new_from_file(&file, false)
            .map_err(|e| anyhow::anyhow!("reading {}: {e:?}", file.display()))?;
        let detail = record.match_detail();
        let last = detail
            .rounds
            .len()
            .checked_sub(1)
            .context("the replay has no rounds")?;
        let recorded = Board::from_round(&detail, last).context("no board in the last round")?;
        report.line(&format!("recorded:\n{recorded}"));
        // Compare with where the units were meant to go here: moved back behind the front edge.
        let mut meant = set_up.board.clone();
        let mut notes = Vec::new();
        for side in &mut meant.sides {
            let moves = crate::behind_front(&side.units);
            for (u, ((x, y), moved)) in side.units.iter_mut().zip(moves) {
                if moved > 0.0 {
                    notes.push(format!(
                        "{}{} at {:.1},{:.1} was moved {moved:.1} squares, to clear Testing Grounds' edges (2 from the middle, 29 across) or a unit moved in its way",
                        crate::compare::NOTE,
                        u.name,
                        f64::from(u.x) / 10.0,
                        f64::from(u.y) / 10.0
                    ));
                    u.x = (x * 10.0).round() as i32;
                    u.y = (y * 10.0).round() as i32;
                }
            }
        }
        let mut differences = Vec::new();
        for d in crate::compare::differences(&meant, &recorded) {
            if d.starts_with(crate::compare::NOTE) {
                notes.push(d);
            } else {
                differences.push(d);
            }
        }
        Ok(Checked {
            set_up,
            replay: file,
            recorded,
            notes,
            differences,
        })
    }

    /// Switch to the side being edited, if it isn't already, and zoom all the way out.
    pub fn switch_to(game: &mut Game, half: Half, current: &mut Half) {
        if *current != half {
            game.right_click(ui::NEUTRAL, 400);
            game.click(ui::SWITCH_SIDES, 2500);
            *current = half;
        }
        // Zoom all the way out: a fixed view, whatever the zoom was.
        game.wheel(-60);
    }

    /// Open Testing Grounds Tools (the button toggles, so look first).
    fn open_tools(game: &mut Game) -> Result<()> {
        for _ in 0..3 {
            if crate::tools_open(&game.screenshot()?) {
                return Ok(());
            }
            game.click(ui::TOOLS, 1200);
        }
        bail!("couldn't open Testing Grounds Tools")
    }

    fn close_tools(game: &mut Game) {
        game.click(ui::TOOLS_CLOSE_LEFT, 500);
        game.click(ui::TOOLS_CLOSE_RIGHT, 700);
    }

    /// Place the board's units, buildings and devices in the running Testing Grounds, side by
    /// side, with their levels, techs, items and turns. `reference` is each half's empty view
    /// (south, north) from just after clearing, to correct camera drift against; `limit` stops
    /// after that many units.
    #[allow(clippy::too_many_arguments)]
    pub fn place(
        game: &mut Game,
        board: &Board,
        south: &Projection,
        north: &Projection,
        limit: Option<usize>,
        reference: Option<&(image::RgbImage, image::RgbImage)>,
        replays: &Path,
        report: &dyn Report,
    ) -> Result<()> {
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
            for b in &side.buildings {
                let (x, y) = (f64::from(b.x) / 10.0, f64::from(b.y) / 10.0);
                if !crate::clickable(projection.to_screen(x, y)) {
                    bail!("{} at {x},{y} is outside the calibrated view", b.name);
                }
                if ui::building_tile(b.building_id).is_none() {
                    bail!("{} ({}) isn't in the Building tiles", b.name, b.building_id);
                }
            }
            for d in &side.devices {
                let (x, y) = (f64::from(d.x) / 10.0, f64::from(d.y) / 10.0);
                if !crate::clickable(projection.to_screen(x, y)) {
                    bail!("{} at {x},{y} is outside the calibrated view", d.name);
                }
                if ui::device_tile(d.building_id).is_none() {
                    bail!("{} ({}) isn't in the Devices row", d.name, d.building_id);
                }
            }
        }

        // Techs are clicked by position on each unit's card, in loadout order.
        let wants_techs = board
            .sides
            .iter()
            .flat_map(|s| &s.units)
            .any(|u| !u.techs.is_empty());
        let (mine, theirs) = if wants_techs {
            crate::loadouts(replays)?
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
                crate::tech_buttons(&unit.name, &unit.techs, equipped)
                    .map_err(|e| anyhow::anyhow!(e))?,
            );
        }
        let mut buttons = buttons.into_iter();
        // Items by ID, each from a cell of the Equipment tab; check them all before starting.
        let mut items: Vec<Vec<u32>> = Vec::new();
        for unit in board.sides.iter().flat_map(|s| &s.units) {
            let mut ids = Vec::new();
            for entry in &unit.items {
                let id = mechabellum::board::resolve_item(entry)
                    .map_err(|e| anyhow::anyhow!("{} at {}: {e}", unit.name, entry))?;
                if crate::equipment::cell(id).is_none() {
                    bail!("{entry} on {} isn't in the Equipment tab", unit.name);
                }
                ids.push(id);
            }
            items.push(ids);
        }
        let mut items = items.into_iter();

        let total = board
            .sides
            .iter()
            .map(|s| s.units.len())
            .sum::<usize>()
            .min(limit.unwrap_or(usize::MAX));
        report.units(0, total);
        // Testing Grounds starts on your own (south) side.
        let mut current = Half::South;
        let mut placed = 0;
        for side in &board.sides {
            let half = side.half.unwrap_or(Half::South);
            switch_to(game, half, &mut current);
            let projection = if half == Half::North { north } else { south };
            // Where the camera sits drifts by a square or so with fights and side switches.
            // Line up the towers and the spawn pad with this half's reference view (where
            // placements land exactly), and aim that much off.
            let corrected;
            let projection = match reference {
                Some((ref_south, ref_north)) => {
                    let reference = if half == Half::North {
                        ref_north
                    } else {
                        ref_south
                    };
                    let flip = if half == Half::North { -1.0 } else { 1.0 };
                    let landmarks = [
                        projection.to_screen(-TOWER.0, TOWER.1 * flip),
                        projection.to_screen(TOWER.0, TOWER.1 * flip),
                        ui::SPAWN_PAD,
                    ];
                    std::thread::sleep(std::time::Duration::from_millis(800));
                    let ((dx, dy), fit) =
                        crate::camera_shift(reference, &game.screenshot()?, &landmarks);
                    if dx.hypot(dy) > 80.0 {
                        report.line(&format!(
                            "  (camera seems {dx},{dy} px off, too far to trust; not correcting)"
                        ));
                        projection
                    } else {
                        if dx != 0.0 || dy != 0.0 {
                            report.line(&format!(
                                "  (camera {dx},{dy} px off its reference, match {fit:.1}; correcting)"
                            ));
                        }
                        corrected = projection.translated(dx, dy);
                        &corrected
                    }
                }
                None => projection,
            };
            // Research costs supplies, and each side has its own; add plenty.
            if side.units.iter().any(|u| !u.techs.is_empty()) {
                game.click(ui::TOOLS, 1200);
                for _ in 0..5 {
                    game.click(ui::TOOLS_SUPPLIES_1000, 300);
                }
                close_tools(game);
            }
            // Items wait in an inventory before they're equipped; start this side with it
            // empty, or a leftover would be equipped instead.
            if side.units.iter().any(|u| !u.items.is_empty()) {
                clear_inventory(game);
            }
            report.line(&format!(
                "{}:",
                side.name.as_deref().unwrap_or(half_name(half))
            ));
            let steps = plan(side, projection);
            let side_buttons: Vec<Vec<usize>> = buttons.by_ref().take(side.units.len()).collect();
            let side_items: Vec<Vec<u32>> = items.by_ref().take(side.units.len()).collect();
            let order = crate::placement_order(&side.units);
            for i in order {
                let (step, unit) = (&steps[i], &side.units[i]);
                if limit.is_some_and(|l| placed >= l) {
                    break;
                }
                // Stop here rather than on the next screenshot, before a unit is half done.
                crate::stop::check()?;
                placed += 1;
                // The north half's own frame is the board turned half a turn.
                let flip = if half == Half::North { -1.0 } else { 1.0 };
                // Where each try should land, and where to click for it.
                let mut tries: Vec<((f64, f64), (f64, f64))> = vec![(step.board, step.board)];
                // Beside a tower the click itself can land on the tower (the Command Center
                // takes clicks about 2.4 squares out from its center), though the ground is
                // free. A drop snaps to the grid, so a click up to half a square off lands on
                // the same spot: aim farther from the tower, on the axes facing it.
                let (ox, oy) = (step.board.0 * flip, step.board.1 * flip);
                let tower = [(-TOWER.0, TOWER.1), (TOWER.0, TOWER.1)]
                    .into_iter()
                    .find(|t| (ox - t.0).abs() <= 4.0 && (oy - t.1).abs() <= 4.0);
                if let Some(t) = tower {
                    let away = |d: f64| if d.abs() < 1.0 { 0.0 } else { d.signum() };
                    let (sx, sy) = (away(ox - t.0), away(oy - t.1));
                    // One axis at a time (shifting both at a corner overshot a square),
                    // forward or back first.
                    for k in [0.25, 0.4] {
                        for (ax, ay) in [(0.0, sy), (sx, 0.0), (sx, sy)] {
                            if ax != 0.0 || ay != 0.0 {
                                let aim = ((ox + ax * k) * flip, (oy + ay * k) * flip);
                                tries.push((step.board, aim));
                            }
                        }
                    }
                    tries.dedup();
                }
                tries.extend(NUDGES.iter().skip(1).map(|(dx, dy)| {
                    let p = (step.board.0 + dx * flip, step.board.1 + dy * flip);
                    (p, p)
                }));
                tries.retain(|&(_, (x, y))| {
                    // Stay in the side's zone and in view.
                    (x * flip).abs() <= 30.0
                        && (-40.0..=-1.0).contains(&(y * flip))
                        && crate::clickable(projection.to_screen(x, y))
                });
                let spots: Vec<(f64, f64)> = tries.iter().map(|t| t.0).collect();
                let screens: Vec<_> = tries
                    .iter()
                    .map(|&(_, (x, y))| projection.to_screen(x, y))
                    .collect();
                // Turn a unit 4 squares behind its spot (toward its own edge), where the
                // unturned footprint fits even when the spot is on the front line.
                let turn_at = unit
                    .rotated
                    .then(|| projection.to_screen(step.board.0, step.board.1 - 4.0 * flip));
                // One unit that won't spawn shouldn't sink the rest of the board; the
                // comparison afterwards reports it missing. A stop does end the run.
                let landed = match spawn_and_place(game, &step.name, &screens, turn_at) {
                    Ok(landed) => landed,
                    Err(e) if crate::stop::is_stop(&e).is_some() => return Err(e),
                    Err(e) => {
                        report.line(&format!(
                            "  {:<14} at {:>5.1},{:>5.1}: NOT placed ({e:#})",
                            step.name, step.board.0, step.board.1
                        ));
                        game.right_click(ui::NEUTRAL, 600);
                        report.units(placed, total);
                        continue;
                    }
                };
                let at = landed.map_or(step.screen, |i| screens[i]);
                // Techs: select it (a click picks it up), click its card's buttons, then put
                // it back.
                let techs = side_buttons[i].clone();
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
                    close_tools(game);
                    game.right_click(ui::NEUTRAL, 500);
                }
                // Items: grant each into the inventory, then click it and the unit. One the
                // unit can't wear stays in the inventory; clear it.
                let mut refused_items = Vec::new();
                for &id in &side_items[i] {
                    grant(game, id)?;
                    game.click(ui::INVENTORY_ITEM, 700);
                    game.click(at, 900);
                    game.right_click(ui::NEUTRAL, 600);
                    if crate::inventory_has_item(&game.screenshot()?) {
                        clear_inventory(game);
                        refused_items.push(id);
                    }
                }
                report.line(&format!(
                    "  {:<14} at {:>5.1},{:>5.1} level {}: {}{}",
                    step.name,
                    step.board.0,
                    step.board.1,
                    unit.level,
                    match landed.map(|i| spots[i]) {
                        Some((x, y)) if (x, y) == step.board && step.moved_back > 0.0 => format!(
                            "moved {:.1} squares, to {x:.1},{y:.1} (Testing Grounds' zone is a square smaller at the front and sides)",
                            step.moved_back
                        ),
                        Some((x, y))
                            if (x, y) == step.board
                                && landed.is_some_and(|i| tries[i].1 != step.board) =>
                        {
                            // Only a click aimed off the spot took; from here the calibration's
                            // error can snap it a square out (2026-10-02, x = +-12 by the
                            // towers on Railway Town landed at +-11). The verify step has the
                            // last word.
                            "placed by aiming off the tower (may be a square out)".to_owned()
                        }
                        Some((x, y)) if (x, y) == step.board => "placed".to_owned(),
                        Some((x, y)) => format!("refused there; placed at {x:.1},{y:.1}"),
                        None => "NOT confirmed (check the screen)".to_owned(),
                    },
                    if refused_items.is_empty() {
                        String::new()
                    } else {
                        format!("; it can't wear item {refused_items:?}")
                    }
                ));
                report.units(placed, total);
            }
            // Buildings last: one on the spawn pad would block the units' spawns.
            for b in &side.buildings {
                let (row, col) = ui::building_tile(b.building_id)
                    .with_context(|| format!("{} isn't in the Building tiles", b.name))?;
                let (x, y) = (f64::from(b.x) / 10.0, f64::from(b.y) / 10.0);
                put_building(game, row, col, projection.to_screen(x, y))?;
                game.wheel(-60);
                report.line(&format!("  {:<14} at {x:>5.1},{y:>5.1}: built", b.name));
            }
            for d in &side.devices {
                let col = ui::device_tile(d.building_id)
                    .with_context(|| format!("{} isn't in the Devices row", d.name))?;
                let (x, y) = (f64::from(d.x) / 10.0, f64::from(d.y) / 10.0);
                put_device(game, col, projection.to_screen(x, y))?;
                game.wheel(-60);
                report.line(&format!("  {:<14} at {x:>5.1},{y:>5.1}: set up", d.name));
            }
        }
        // Leave the game on your own side.
        switch_to(game, Half::South, &mut current);
        crate::stop::check()?;
        Ok(())
    }

    /// Board offsets (squares, in the side's own frame: +y toward the middle) to try when the
    /// game refuses a drop. From this camera the Research Center's model covers the ground about
    /// 2 to 3 squares in front of it on screen, and a click there hits the tower, so the game
    /// refuses it (the ground itself is free; panning or zooming doesn't change the camera's
    /// angle, so neither uncovers it). One square farther forward is usually clear. A refused
    /// unit would sit on the spawn pad and block the next spawn, so a nearby spot beats none.
    const NUDGES: [(f64, f64); 6] = [
        (0.0, 0.0),
        (0.0, 1.0),
        (0.0, 2.0),
        (-3.0, 0.0),
        (3.0, 0.0),
        (0.0, 4.0),
    ];

    /// Pick a tile from the Squad tab's Building section (row and column from 0) and put it at a
    /// screen point, then close the tools.
    pub fn put_building(game: &mut Game, row: usize, col: usize, at: Px) -> Result<()> {
        game.right_click(ui::NEUTRAL, 400);
        open_tools(game)?;
        game.click(ui::TOOLS_SQUAD_TAB, 800);
        game.wheel_at(ui::BUILDING_SCROLL, 30);
        if row > 0 {
            game.wheel_at(
                ui::BUILDING_SCROLL,
                -ui::BUILDING_STEPS_PER_ROW * row as i32,
            );
        }
        game.click((ui::BUILDING_COLUMNS[col], ui::BUILDING_ROW_Y), 900);
        game.click(at, 900);
        close_tools(game);
        Ok(())
    }

    /// Pick a device from the Squad tab's Devices row (column from 0) and put it at a screen
    /// point, then close the tools.
    pub fn put_device(game: &mut Game, col: usize, at: Px) -> Result<()> {
        game.right_click(ui::NEUTRAL, 400);
        open_tools(game)?;
        game.click(ui::TOOLS_SQUAD_TAB, 800);
        game.click((ui::BUILDING_COLUMNS[col], ui::DEVICE_ROW_Y), 900);
        game.click(at, 900);
        close_tools(game);
        Ok(())
    }

    /// Empty the item inventory of the side being edited (equipped items stay on their units).
    pub fn clear_inventory(game: &mut Game) {
        game.right_click(ui::NEUTRAL, 400);
        game.click(ui::TOOLS, 1200);
        game.click(ui::TOOLS_CLEAR_EQUIPMENT, 800);
        close_tools(game);
    }

    /// Put an item in your inventory from the tools' Equipment tab, and close the tools.
    pub fn grant(game: &mut Game, id: u32) -> Result<()> {
        let cell = crate::equipment::cell(id)
            .with_context(|| format!("item {id} isn't in the Equipment tab"))?;
        game.right_click(ui::NEUTRAL, 400);
        open_tools(game)?;
        game.click(ui::TOOLS_EQUIPMENT_TAB, 800);
        // To the top, then down to the cell's row. Only scroll with the list under the cursor.
        game.wheel_at(ui::PALETTE_SCROLL, 150);
        if cell.scroll_down_steps > 0 {
            game.wheel_at(ui::PALETTE_SCROLL, -cell.scroll_down_steps);
        }
        game.click(cell.at, 900);
        close_tools(game);
        Ok(())
    }

    /// Spawn a unit from the Squad list and move it to the first of `targets` the game accepts.
    /// Returns the index of the target it landed on, if a unit was seen there afterwards.
    pub fn spawn_and_place(
        game: &mut Game,
        name: &str,
        targets: &[Px],
        turn_at: Option<Px>,
    ) -> Result<Option<usize>> {
        let (spawn, grown, pad_before) = spawn_unit(game, name)?;
        move_unit(game, name, spawn, grown, pad_before, targets, turn_at)
    }

    /// How much green shows within 60 pixels of `at` (the biggest blob there).
    fn blob_weight(blobs: &[(Px, usize)], at: Px) -> usize {
        blobs
            .iter()
            .filter(|(p, _)| (p.0 - at.0).hypot(p.1 - at.1) < 60.0)
            .map(|(_, w)| *w)
            .max()
            .unwrap_or(0)
    }

    /// Spawn a unit from the Squad list. Returns where it showed up on the pad, how much green
    /// shows there with it, and how much showed there before it (a unit left on the pad
    /// earlier).
    pub fn spawn_unit(game: &mut Game, name: &str) -> Result<(Px, usize, usize)> {
        let near = |a: Px, b: Px, d: f64| (a.0 - b.0).hypot(a.1 - b.1) < d;
        let cell = ui::palette(name).with_context(|| format!("{name} isn't in the Squad list"))?;

        // Keep the known camera view: zooming all the way out is harmless if it already is.
        game.wheel(-60);
        // Spawn it, and find it: the only new green blob.
        let before = green_blobs(&game.screenshot()?);
        // Open the tools (the button toggles), and never scroll unless the list is under the
        // cursor: over the map the wheel zooms the camera instead.
        open_tools(game)?;
        game.click(ui::TOOLS_SQUAD_TAB, 800);
        game.wheel_at(ui::PALETTE_SCROLL, 80);
        if cell.scroll_down_steps > 0 {
            game.wheel_at(ui::PALETTE_SCROLL, -cell.scroll_down_steps);
        }
        game.click(cell.at, 1000);
        close_tools(game);
        if crate::tools_open(&game.screenshot()?) {
            game.click(ui::TOOLS_CLOSE_RIGHT, 700);
        }
        // The new unit is the blob that appeared, or the one that grew most (a spawn on top of
        // an earlier unit merges with it).
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
        // A big unit's model hides much of its tile, splitting it into small pieces, so add up
        // the growth over the whole pad and take its weighted center.
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
        Ok((
            spawn,
            blob_weight(&after, spawn),
            blob_weight(&before, spawn),
        ))
    }

    /// Move the unit standing at `spawn` (with `grown` green showing there) to the first of
    /// `targets` the game accepts. Returns the index of the target it landed on, if a unit was
    /// seen there afterwards; a refused unit is left where it was.
    fn move_unit(
        game: &mut Game,
        name: &str,
        spawn: Px,
        grown: usize,
        pad_before: usize,
        targets: &[Px],
        turn_at: Option<Px>,
    ) -> Result<Option<usize>> {
        let near = |a: Px, b: Px, d: f64| (a.0 - b.0).hypot(a.1 - b.1) < d;
        // Pick it up, drop it, confirm, deselect. It has moved when the blob that grew at the
        // spawn point has shrunk back and a blob sits at the target. A refused drop leaves the
        // unit carried, so cancel with a right-click and try again.
        // A middle click on the ground while carrying turns the unit 90 degrees. Turning is a
        // toggle and a refused drop keeps the turn, so turn only once. It's done at `turn_at`, a
        // spot where the unturned footprint fits: at the target itself, one that only fits
        // turned (a Sledgehammer on the front line) is refused before the turn applies.
        let mut turned = false;
        // Each target gets two tries; the first gets three, as a drop sometimes just doesn't
        // take.
        let attempts = targets
            .iter()
            .enumerate()
            .flat_map(|(i, t)| std::iter::repeat_n((i, *t), if i == 0 { 3 } else { 2 }));
        for (i, target) in attempts {
            game.click(spawn, 700);
            if let (Some(spot), false) = (turn_at, turned) {
                game.click(spot, 700);
                game.middle_click(spot, 700);
                turned = true;
            }
            game.click(target, 700);
            if let Ok(dir) = std::env::var("GROUNDS_DEBUG") {
                // Still carried if the drop was refused: the game draws the footprint it tried.
                let n = std::fs::read_dir(&dir).map(|d| d.count()).unwrap_or(0);
                if let Ok(shot) = game.screenshot_here() {
                    let _ = shot.save(format!("{dir}/carry{n:03}.png"));
                }
            }
            game.double_click(target, 700);
            game.right_click(target, 900);
            let shot = game.screenshot()?;
            let now = green_blobs(&shot);
            if let Ok(dir) = std::env::var("GROUNDS_DEBUG") {
                let n = std::fs::read_dir(&dir).map(|d| d.count()).unwrap_or(0);
                let _ = shot.save(format!("{dir}/drop{n:03}.png"));
                eprintln!(
                    "  debug drop{n:03}: {name} target {:.0},{:.0} spawn {:.0},{:.0} weight {} of {}; blobs {:?}",
                    target.0,
                    target.1,
                    spawn.0,
                    spawn.1,
                    blob_weight(&now, spawn),
                    grown,
                    now.iter()
                        .map(|(p, w)| (p.0 as i32, p.1 as i32, *w))
                        .collect::<Vec<_>>()
                );
            }
            // A target on the pad itself (it covers a few squares) can't show the unit leaving
            // it. Otherwise the pad must be back to how it was: after a real move it reads 0,
            // after a refused drop 51 to 354 (2026-10-02). Relative tests (under 80%, then 30%
            // of the green measured at spawn) let refused drops beside other units read as
            // placed: green is always near the target there, and the spawn measure itself swung
            // from 78 to 355. A unit left on the pad earlier stays there, so compare with the
            // pad before the spawn.
            let left_spawn =
                near(target, spawn, 120.0) || blob_weight(&now, spawn) < pad_before + 20;
            let at_target = now.iter().any(|(p, _)| near(*p, target, 80.0));
            if left_spawn && at_target {
                return Ok(Some(i));
            }
            game.right_click(ui::NEUTRAL, 600);
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn board(text: &str) -> Board {
        Board::parse(text).unwrap()
    }

    #[test]
    fn test_stand_in_map() {
        let all = Calibrations::bundled();
        // A Testing Grounds map stays.
        let forest =
            board("[board map=\"Forest Eye\"][side][unit=Crawler at=0,-10][/side][/board]");
        assert!(stand_in_map(&forest, &all).is_none());
        // No map named: Railway Town, which Testing Grounds has.
        let unnamed = board("[board][side][unit=Crawler at=0,-10][/side][/board]");
        assert!(stand_in_map(&unnamed, &all).is_none());
        // A ranked map Testing Grounds doesn't offer gets Railway Town, with a reason.
        let other =
            board("[board map=\"Nowhere Valley\"][side][unit=Crawler at=0,-10][/side][/board]");
        let (map, why) = stand_in_map(&other, &all).unwrap();
        assert_eq!(map, "Railway Town");
        assert!(why.contains("Nowhere Valley"), "{why}");
    }

    #[test]
    fn test_leave_out_unplaceable_keeps_what_fits() {
        let all = Calibrations::bundled();
        let (south, north) = projections(&all, "Railway Town").unwrap();
        // Two units in the zone, one of them a kind the Squad list doesn't have (the parser
        // would refuse an unknown name, so it's renamed after parsing), and an item that
        // doesn't resolve.
        let mut b = board(
            "[board][side half=south]\
             [unit=Crawler at=0,-10 items=\"No Such Item\"]\
             [unit=Fang at=6,-12]\
             [/side][/board]",
        );
        b.sides[0].units[1].name = "Nonesuch".to_owned();
        let dropped = leave_out_unplaceable(&mut b, &south, &north);
        let names: Vec<&str> = b.sides[0].units.iter().map(|u| u.name.as_str()).collect();
        assert_eq!(names, ["Crawler"]);
        assert!(b.sides[0].units[0].items.is_empty());
        assert!(
            dropped
                .iter()
                .any(|d| d == "Nonesuch at 6.0,-12.0 (can't be placed)"),
            "{dropped:?}"
        );
        assert!(
            dropped.iter().any(|d| d.contains("items on Crawler")),
            "{dropped:?}"
        );
    }

    #[test]
    fn test_leave_out_techs_by_side() {
        let mut b = board(
            "[board][side half=south][unit=Crawler at=0,-10 techs=\"Barrier, Acid Crawler\"][/side]\
             [side half=north][unit=Crawler at=0,10 techs=\"Barrier\"][/side][/board]",
        );
        let crawler = b.sides[0].units[0].unit_id;
        let mut mine = crate::Loadout::new();
        mine.insert(crawler, vec![(1, "Barrier".to_owned())]);
        // The Bot's loadout has no Crawler techs at all.
        let theirs = crate::Loadout::new();
        let dropped = leave_out_techs(&mut b, &mine, &theirs);
        assert_eq!(b.sides[0].units[0].techs, ["Barrier"]);
        assert!(b.sides[1].units[0].techs.is_empty());
        assert_eq!(dropped.len(), 1, "{dropped:?}");
    }
}
