#![allow(clippy::needless_range_loop)] // Gaussian elimination reads clearer with indices.

//! Setting up a board in Mechabellum's Testing Grounds. The pure parts live here: where things
//! are on screen, and how board squares map to screen pixels. The Windows driver is in `win`.
//! See docs/testing-grounds.md.

pub mod calibration;
pub mod compare;
pub mod equipment;
#[cfg(windows)]
pub mod flow;
pub mod screen;
#[cfg(windows)]
pub mod win;

/// A point in physical pixels of the game window's client area.
pub type Px = (f64, f64);

/// Screen positions are measured on a 2560x1600 client area and scaled to the actual one.
pub const REFERENCE_SIZE: (f64, f64) = (2560.0, 1600.0);

/// A projective map from board squares (as in board code) to screen pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Projection {
    h: [f64; 8],
}

impl Projection {
    /// Least-squares fit from (board x, board y, screen x, screen y) pairs. Needs 4 or more.
    pub fn fit(points: &[(f64, f64, f64, f64)]) -> Option<Projection> {
        if points.len() < 4 {
            return None;
        }
        // Normal equations for the 8 unknowns (h33 = 1).
        let mut m = [[0.0f64; 9]; 8];
        let mut add = |row: [f64; 8], rhs: f64| {
            for i in 0..8 {
                for j in 0..8 {
                    m[i][j] += row[i] * row[j];
                }
                m[i][8] += row[i] * rhs;
            }
        };
        for &(x, y, u, v) in points {
            add([x, y, 1.0, 0.0, 0.0, 0.0, -u * x, -u * y], u);
            add([0.0, 0.0, 0.0, x, y, 1.0, -v * x, -v * y], v);
        }
        for c in 0..8 {
            let p = (c..8).max_by(|&a, &b| m[a][c].abs().total_cmp(&m[b][c].abs()))?;
            m.swap(c, p);
            if m[c][c].abs() < 1e-12 {
                return None;
            }
            for r in 0..8 {
                if r != c {
                    let f = m[r][c] / m[c][c];
                    for k in c..9 {
                        m[r][k] -= f * m[c][k];
                    }
                }
            }
        }
        let mut h = [0.0; 8];
        for i in 0..8 {
            h[i] = m[i][8] / m[i][i];
        }
        Some(Projection { h })
    }

    /// Board point (squares) under a screen position: the inverse of `to_screen`.
    pub fn to_board(&self, u: f64, v: f64) -> (f64, f64) {
        let h = &self.h;
        // Invert the 3x3 matrix [h0 h1 h2; h3 h4 h5; h6 h7 1] by cofactors.
        let (a, b, c, d, e, f, g, hh) = (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);
        let m = [
            [e - f * hh, c * hh - b, b * f - c * e],
            [f * g - d, a - c * g, c * d - a * f],
            [d * hh - e * g, b * g - a * hh, a * e - b * d],
        ];
        let x = m[0][0] * u + m[0][1] * v + m[0][2];
        let y = m[1][0] * u + m[1][1] * v + m[1][2];
        let w = m[2][0] * u + m[2][1] * v + m[2][2];
        (x / w, y / w)
    }

    /// Screen position of a board point, in squares.
    pub fn to_screen(&self, x: f64, y: f64) -> Px {
        let h = &self.h;
        let w = h[6] * x + h[7] * y + 1.0;
        (
            (h[0] * x + h[1] * y + h[2]) / w,
            (h[3] * x + h[4] * y + h[5]) / w,
        )
    }

    /// The same projection aiming at the board point (dx, dy) squares away: `to_screen(x, y)`
    /// gives what this one gives for (x + dx, y + dy).
    pub fn board_shifted(&self, dx: f64, dy: f64) -> Projection {
        let h = self.h;
        let k = 1.0 + h[6] * dx + h[7] * dy;
        Projection {
            h: [
                h[0] / k,
                h[1] / k,
                (h[2] + h[0] * dx + h[1] * dy) / k,
                h[3] / k,
                h[4] / k,
                (h[5] + h[3] * dx + h[4] * dy) / k,
                h[6] / k,
                h[7] / k,
            ],
        }
    }

    /// The same projection with the whole view moved by (dx, dy) pixels, for when the camera
    /// has panned since calibration.
    pub fn translated(&self, dx: f64, dy: f64) -> Projection {
        let h = self.h;
        Projection {
            h: [
                h[0] + dx * h[6],
                h[1] + dx * h[7],
                h[2] + dx,
                h[3] + dy * h[6],
                h[4] + dy * h[7],
                h[5] + dy,
                h[6],
                h[7],
            ],
        }
    }
}

/// Whether the tool keeps the game's starting camera tilt (`GROUNDS_VIEW=tilted`) instead of
/// looking straight down. Each view has its own calibrations.
pub fn tilted_view() -> bool {
    std::env::var("GROUNDS_VIEW").is_ok_and(|v| v.eq_ignore_ascii_case("tilted"))
}

/// A first guess for the straight-down view (see `flow::look_straight_down`), south half: the
/// zone's corners and the two towers as they look on Railway Town - Night, read off a
/// screenshot, so good to a square or so. `grounds calibrate` measures each map properly.
pub const TOP_DOWN_SOUTH: [(f64, f64, f64, f64); 6] = [
    (-30.0, -1.0, 640.0, 151.0),
    (30.0, -1.0, 2214.0, 819.0),
    (30.0, -31.0, 1920.0, 1536.0),
    (-30.0, -31.0, 333.0, 845.0),
    (-14.0, -17.0, 902.0, 653.0),
    (14.0, -17.0, 1632.0, 986.0),
];

/// The same for the north half: its camera looks from the other end, so the screen is the same
/// with the board turned half a turn.
pub const TOP_DOWN_NORTH: [(f64, f64, f64, f64); 6] = [
    (30.0, 1.0, 640.0, 151.0),
    (-30.0, 1.0, 2214.0, 819.0),
    (-30.0, 31.0, 1920.0, 1536.0),
    (30.0, 31.0, 333.0, 845.0),
    (14.0, 17.0, 902.0, 653.0),
    (-14.0, 17.0, 1632.0, 986.0),
];

/// Calibration for Railway Town, south half, seen from your own side with the camera zoomed all
/// the way out (60 wheel steps) and at its starting tilt, before the tool switched to looking
/// straight down (the tilted view lets a tower's model hide the ground in front of it). Units
/// dropped at these screen points were recorded at these board squares in the saved replay
/// (2026-09-27); fit error 2 to 10 px. Kept for the geometry tests.
pub const RAILWAY_TOWN_SOUTH: [(f64, f64, f64, f64); 6] = [
    (5.5, -12.0, 1472.0, 742.0),
    (22.5, -5.0, 1997.0, 762.0),
    (22.5, -18.5, 1862.0, 1024.0),
    (22.0, -24.0, 1792.0, 1126.0),
    (-5.5, -3.0, 1298.0, 500.0),
    (-12.5, -28.0, 803.0, 903.0),
];

/// Calibration for Railway Town, north half, seen from the north side after Switch Sides with
/// the camera zoomed all the way out (60 wheel steps). Measured 2026-09-27; fit error about 4 px.
pub const RAILWAY_TOWN_NORTH: [(f64, f64, f64, f64); 6] = [
    (15.5, 29.0, 715.0, 897.0),
    (-21.5, 12.0, 1911.0, 898.0),
    (18.5, 8.0, 917.0, 478.0),
    (-7.5, 28.0, 1326.0, 1089.0),
    (0.5, 18.0, 1239.0, 812.0),
    (0.5, 14.0, 1299.0, 725.0),
];

/// Screen areas covered by UI in the Testing Grounds view, where the board can't be clicked:
/// (left, top, right, bottom).
const UI_AREAS: [(f64, f64, f64, f64); 4] = [
    (0.0, 0.0, 2560.0, 200.0),       // top bar: HP, Start, Switch Sides
    (2140.0, 840.0, 2560.0, 1600.0), // shop and Testing Grounds Tools
    (0.0, 1380.0, 800.0, 1600.0),    // control hints (and the unit card when selected)
    (2500.0, 0.0, 2560.0, 300.0),    // menu buttons
];

/// Whether a board click at this reference-screen point is possible without panning.
pub fn clickable(p: Px) -> bool {
    let (w, h) = REFERENCE_SIZE;
    p.0 >= 10.0
        && p.1 >= 10.0
        && p.0 <= w - 10.0
        && p.1 <= h - 10.0
        && !UI_AREAS
            .iter()
            .any(|&(l, t, r, b)| p.0 >= l && p.0 <= r && p.1 >= t && p.1 <= b)
}

/// Buttons and panels, in reference pixels.
pub mod ui {
    use super::Px;

    /// Testing Grounds Tools: its tabs and the controls used here.
    pub const TOOLS_CLOSE_LEFT: Px = (358.0, 296.0);
    pub const TOOLS_CLOSE_RIGHT: Px = (2534.0, 317.0);
    pub const TOOLS_SQUAD_TAB: Px = (2099.0, 448.0);
    /// The Equipment tab: every item, by rank (see `equipment`).
    pub const TOOLS_EQUIPMENT_TAB: Px = (2099.0, 896.0);
    /// Where a granted item waits in your inventory, top left. Click it, then a unit.
    pub const INVENTORY_ITEM: Px = (64.0, 365.0);
    /// The Squad tab's Building section, above the units: three per row, three rows, scrolled on
    /// its own (6 wheel steps a row) with the cursor over it. Pick one, then click the spot.
    pub const BUILDING_SCROLL: Px = (2420.0, 640.0);
    pub const BUILDING_ROW_Y: f64 = 700.0;
    pub const BUILDING_COLUMNS: [f64; 3] = [2225.0, 2350.0, 2485.0];
    pub const BUILDING_STEPS_PER_ROW: i32 = 6;
    /// Building IDs by tile, row by row: all 9 checked against replays (`grounds
    /// buildings-check` and `grounds verify`, 2026-09-29).
    pub const BUILDINGS: [[u32; 3]; 3] = [[1, 4, 2], [3, 5, 6], [7, 8, 9]];

    /// The Devices row above the buildings, in the same columns: Shield Generator, Sentry
    /// Missile, Missile Interceptor.
    pub const DEVICE_ROW_Y: f64 = 510.0;
    pub const DEVICES: [u32; 3] = [10001, 20001, 30001];

    /// The Devices column for a device ID.
    pub fn device_tile(id: u32) -> Option<usize> {
        DEVICES.iter().position(|&d| d == id)
    }

    /// The Building tile (row, column) for a building ID.
    pub fn building_tile(id: u32) -> Option<(usize, usize)> {
        BUILDINGS
            .iter()
            .enumerate()
            .find_map(|(row, ids)| ids.iter().position(|&i| i == id).map(|col| (row, col)))
    }
    /// Left tools panel: empties the item inventory (equipped items stay on their units).
    pub const TOOLS_CLEAR_EQUIPMENT: Px = (192.0, 870.0);
    pub const TOOLS_UPGRADE: Px = (118.0, 1203.0);
    pub const TOOLS_SUPPLIES_1000: Px = (116.0, 745.0);
    /// The researchable tech buttons on a selected unit's card, left to right. They follow the
    /// unit's equipped techs in the order replays record them (`unitDatas`).
    pub const CARD_TECHS: [Px; 4] = [
        (260.0, 1453.0),
        (370.0, 1453.0),
        (480.0, 1453.0),
        (590.0, 1453.0),
    ];
    /// A point inside the Squad list, for scrolling it with the wheel.
    pub const PALETTE_SCROLL: Px = (2350.0, 1100.0);

    /// The Squad list in Testing Grounds Tools, three per row. Clicking one spawns it free, with
    /// no per-round limit.
    pub const PALETTE: [&str; 35] = [
        "Crawler",
        "Fang",
        "Hound",
        "Void Eye",
        "Arclight",
        "Marksman",
        "Vortex",
        "Mustang",
        "Sledgehammer",
        "Stormcaller",
        "Steel Ball",
        "Fire Badger",
        "Sabertooth",
        "Tarantula",
        "Rhino",
        "Hacker",
        "Wasp",
        "Phoenix",
        "Phantom Ray",
        "Centurion",
        "Wraith",
        "Scorpion",
        "Farseer",
        "Typhoon",
        "Vulcan",
        "Melting Point",
        "Fortress",
        "Sandworm",
        "Raiden",
        "Overlord",
        "War Factory",
        "Abyss",
        "Mountain",
        "Death Knell",
        "Experimental Death Knell",
    ];
    const PALETTE_ROWS: usize = 12;
    /// The top visible row when the list is scrolled to the bottom.
    const PALETTE_LAST_TOP_ROW: usize = 8;
    /// Wheel steps that scroll the list by one row.
    pub const PALETTE_STEPS_PER_ROW: i32 = 6;

    /// How to reach a unit in the Squad list: after scrolling to the very top (`None`) or to the
    /// very bottom (`Some(0)`), or from the top by some wheel steps (`Some(steps)`), click here.
    pub struct PaletteClick {
        pub scroll_down_steps: i32,
        pub at: Px,
    }

    pub fn palette(name: &str) -> Option<PaletteClick> {
        let key = |s: &str| {
            s.chars()
                .filter(|c| c.is_alphanumeric())
                .flat_map(char::to_lowercase)
                .collect::<String>()
        };
        let i = PALETTE.iter().position(|n| key(n) == key(name))?;
        let (row, col) = (i / 3, i % 3);
        // Visible slot in the list, and wheel steps down from the top to get there.
        let (steps, slot) = if row <= 2 {
            (0, row)
        } else if row >= PALETTE_LAST_TOP_ROW {
            // Scroll well past the end; the list stops at the bottom.
            (
                PALETTE_STEPS_PER_ROW * PALETTE_ROWS as i32 * 2,
                row - PALETTE_LAST_TOP_ROW,
            )
        } else {
            (PALETTE_STEPS_PER_ROW * (row as i32 - 1), 1)
        };
        const COL_X: [f64; 3] = [2218.0, 2349.0, 2479.0];
        Some(PaletteClick {
            scroll_down_steps: steps,
            at: (COL_X[col], (717.0 + 102.5 * slot as f64) * 1.28),
        })
    }

    pub const START: Px = (1280.0, 90.0);
    pub const SWITCH_SIDES: Px = (1280.0, 177.0);
    pub const MENU: Px = (2532.0, 28.0);
    pub const MENU_SAVE_REPLAY: Px = (1280.0, 922.0);
    pub const MENU_CLOSE: Px = (1280.0, 582.0);
    pub const TOOLS: Px = (2404.0, 872.0);
    /// Closes the Unlock Unit grid.
    pub const UNLOCK_CLOSE: Px = (2362.0, 265.0);
    /// Somewhere harmless to right-click (cancels a carried unit).
    pub const NEUTRAL: Px = (1920.0, 1152.0);
    /// Where units from the Squad list appear (the helipad), in the zoomed-out view of either side.
    pub const SPAWN_PAD: Px = (1285.0, 775.0);

    /// Shop card slots, filled in this order as unit types are unlocked. The Unlock Unit button
    /// takes the next free slot. Slots 0 and 1 are measured; 2 and 3 are inferred.
    pub const SHOP_SLOTS: [Px; 4] = [
        (2458.0, 1338.0),
        (2458.0, 1498.0),
        (2295.0, 1338.0),
        (2295.0, 1498.0),
    ];

    /// The Unlock Unit button's position when `unlocked` unit types are in the shop.
    pub fn unlock_button(unlocked: usize) -> Option<Px> {
        match unlocked {
            0 => Some((2458.0, 1505.0)),
            1 => Some((2458.0, 1505.0)),
            2 => Some((2295.0, 1505.0)),
            _ => None,
        }
    }

    /// Unit positions in the Unlock Unit grid, by tier rows.
    const GRID: [(&str, usize, usize); 32] = [
        ("Crawler", 0, 0),
        ("Fang", 0, 1),
        ("Hound", 0, 2),
        ("Void Eye", 0, 3),
        ("Arclight", 0, 4),
        ("Marksman", 0, 5),
        ("Vortex", 1, 0),
        ("Mustang", 1, 1),
        ("Sledgehammer", 1, 2),
        ("Steel Ball", 1, 3),
        ("Fire Badger", 1, 4),
        ("Tarantula", 1, 5),
        ("Sabertooth", 2, 0),
        ("Stormcaller", 3, 0),
        ("Rhino", 3, 1),
        ("Wasp", 3, 2),
        ("Phantom Ray", 3, 3),
        ("Phoenix", 3, 4),
        ("Hacker", 3, 5),
        ("Farseer", 4, 0),
        ("Scorpion", 4, 1),
        ("Typhoon", 4, 2),
        ("Centurion", 4, 3),
        ("Wraith", 4, 4),
        ("Vulcan", 5, 0),
        ("Melting Point", 5, 1),
        ("Fortress", 5, 2),
        ("Sandworm", 5, 3),
        ("Raiden", 5, 4),
        ("Overlord", 5, 5),
        ("War Factory", 6, 0),
        ("Mountain", 6, 1),
    ];
    const ROW_Y: [f64; 7] = [270.0, 405.0, 545.0, 705.0, 840.0, 995.0, 1150.0];

    /// Where to click in the Unlock Unit grid for a unit, by its game name.
    pub fn unlock_cell(name: &str) -> Option<Px> {
        let key = |s: &str| {
            s.chars()
                .filter(|c| c.is_alphanumeric())
                .flat_map(char::to_lowercase)
                .collect::<String>()
        };
        let (row, col) = if key(name) == "abyss" {
            (6, 2)
        } else {
            GRID.iter()
                .find(|(n, _, _)| key(n) == key(name))
                .map(|&(_, r, c)| (r, c))?
        };
        // Measured on a 2000-wide screenshot, then scaled to the 2560 reference.
        Some(((920.0 + 123.6 * col as f64) * 1.28, ROW_Y[row] * 1.28))
    }
}

/// A replay saved from Testing Grounds (or another offline game against the bot): its battle
/// ID is a timestamp (`26-09-27__12-10-57-082`) and the opponent is "Bot".
pub fn is_testing_grounds_replay(path: &std::path::Path) -> bool {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy())
        .unwrap_or_default();
    path.extension().is_some_and(|x| x == "grbr") && name.contains("__") && name.contains("VS[Bot]")
}

/// Equipped techs per unit type, in card order.
pub type Loadout = std::collections::BTreeMap<u32, Vec<(u32, String)>>;

/// Each player's equipped techs in card order, from the newest replay in `replay_dir`: yours
/// first, then the other side (in Testing Grounds, "Bot", which has its own loadout).
pub fn loadouts(replay_dir: &std::path::Path) -> anyhow::Result<(Loadout, Loadout)> {
    // Only Testing Grounds replays: a ranked match lists every unlocked tech, while Testing
    // Grounds lists the equipped four in card order.
    let newest = std::fs::read_dir(replay_dir)?
        .flatten()
        .filter(|e| is_testing_grounds_replay(&e.path()))
        .max_by_key(|e| e.metadata().and_then(|m| m.modified()).ok())
        .ok_or_else(|| {
            anyhow::anyhow!("no Testing Grounds replay in {} yet", replay_dir.display())
        })?;
    let record = mechabellum::BattleRecord::new_from_file(newest.path(), false)
        .map_err(|e| anyhow::anyhow!("reading {}: {e:?}", newest.path().display()))?;
    let detail = record.match_detail();
    let to_map = |p: &mechabellum::PlayerInfo| {
        let mut map = Loadout::new();
        // Star Expedition copies fold into the same unit ID; the regular unit comes first.
        for u in &p.loadout {
            map.entry(u.unit_id).or_insert_with(|| {
                u.techs
                    .iter()
                    .map(|t| {
                        (
                            t.id.parse().unwrap_or(0),
                            t.name.clone().unwrap_or_default(),
                        )
                    })
                    .collect()
            });
        }
        map
    };
    let mine = detail.players.iter().find(|p| p.name != "Bot");
    let other = detail
        .players
        .iter()
        .find(|p| Some(p.id.as_str()) != mine.map(|m| m.id.as_str()));
    Ok((
        mine.map(to_map).unwrap_or_default(),
        other.map(to_map).unwrap_or_default(),
    ))
}

/// Card button positions (0-based) for the techs a board code names, in the order given.
/// Errors name any tech that isn't equipped for the unit.
pub fn tech_buttons(
    unit: &str,
    wanted: &[String],
    equipped: &[(u32, String)],
) -> Result<Vec<usize>, String> {
    let key = |s: &str| {
        s.chars()
            .filter(|c| c.is_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect::<String>()
    };
    wanted
        .iter()
        .map(|w| {
            equipped
                .iter()
                .position(|(id, name)| key(name) == key(w) || id.to_string() == *w)
                .filter(|&i| i < 4)
                .ok_or_else(|| {
                    let have: Vec<&str> = equipped.iter().map(|(_, n)| n.as_str()).collect();
                    format!(
                        "{unit} has no equipped tech {w:?} (equipped: {})",
                        have.join(", ")
                    )
                })
        })
        .collect()
}

/// Where each unit of a side goes on screen.
#[derive(Debug, Clone)]
pub struct PlanStep {
    pub name: String,
    /// Where it goes: the code's spot, or moved inside [`FRONT_EDGE`] and [`SIDE_EDGE`].
    pub board: (f64, f64),
    pub screen: Px,
    pub clickable: bool,
    /// Squares it was moved to stay inside Testing Grounds' zone (see [`behind_front`]).
    pub moved_back: f64,
}

/// How close to the middle a unit may stand in Testing Grounds, in squares: one square farther
/// back than ranked games allow (measured 2026-09-29 on Railway Town - Dusk: a Fang aimed at 2
/// was moved to 3 or refused, and a turned one on the front line was put down unturned).
pub const FRONT_EDGE: f64 = 2.0;

/// How far from the middle across a unit may reach in Testing Grounds, in squares: one square in
/// from ranked's 30 on both sides (2026-10-01, Night round 2: Crawlers flush with x = -30 and
/// 30 were refused, and ones a square in placed exactly; the back edge is the same as ranked's).
pub const SIDE_EDGE: f64 = 29.0;

/// Each unit's spot, moved straight back where needed so every footprint stays behind
/// [`FRONT_EDGE`], and straight in from [`SIDE_EDGE`], with how far it moved. A unit moved
/// back pushes the ones right behind it (overlapping it across) back too, and one moved in
/// pushes the ones beside it, so a packed formation keeps its shape instead of the game
/// shoving the overlapping units aside. Units in the flank columns are left alone across.
pub fn behind_front(units: &[mechabellum::board::BoardUnit]) -> Vec<((f64, f64), f64)> {
    // Across (x), distance from the middle (t), width, depth.
    let boxes: Vec<(f64, f64, f64, f64)> = units
        .iter()
        .map(|u| {
            let (w, d) = mechabellum::footprint(u.unit_id).unwrap_or((2, 2));
            let (w, d) = if u.rotated { (d, w) } else { (w, d) };
            let (x, y) = (f64::from(u.x) / 10.0, f64::from(u.y) / 10.0);
            (x, y.abs(), f64::from(w), f64::from(d))
        })
        .collect();
    let mut order: Vec<usize> = (0..units.len()).collect();
    order.sort_by(|&a, &b| boxes[a].1.total_cmp(&boxes[b].1));
    let mut t = vec![0.0; units.len()];
    for (k, &i) in order.iter().enumerate() {
        let (x, ti, w, d) = boxes[i];
        let mut want = ti.max(FRONT_EDGE + d / 2.0);
        for &j in &order[..k] {
            let (xj, tj, wj, dj) = boxes[j];
            let across = (x - xj).abs() < (w + wj) / 2.0 - 1e-6;
            let was_behind = ti - d / 2.0 >= tj + dj / 2.0 - 1e-6;
            if across && was_behind {
                want = want.max(t[j] + (d + dj) / 2.0);
            }
        }
        t[i] = want;
    }
    // Then across, once from each side edge inward.
    let mut xs: Vec<f64> = boxes.iter().map(|b| b.0).collect();
    for dir in [1.0, -1.0] {
        // dir 1: in from the left edge (x = -SIDE_EDGE), units taken left to right.
        let mut order: Vec<usize> = (0..units.len())
            .filter(|&i| boxes[i].0.abs() <= 30.0)
            .collect();
        order.sort_by(|&a, &b| (dir * xs[a]).total_cmp(&(dir * xs[b])));
        let start = xs.clone();
        for (k, &i) in order.iter().enumerate() {
            let (_, _, w, d) = boxes[i];
            let mut want = (dir * start[i]).max(-SIDE_EDGE + w / 2.0);
            for &j in &order[..k] {
                let (_, _, wj, dj) = boxes[j];
                let level = (t[i] - t[j]).abs() < (d + dj) / 2.0 - 1e-6;
                let was_beside = dir * start[i] - w / 2.0 >= dir * start[j] + wj / 2.0 - 1e-6;
                if level && was_beside {
                    want = want.max(dir * xs[j] + (w + wj) / 2.0);
                }
            }
            xs[i] = dir * want;
        }
    }
    units
        .iter()
        .zip(&boxes)
        .zip(t)
        .zip(xs)
        .map(|(((u, &(x, ti, _, _)), t), xn)| {
            let sign = if u.y < 0 { -1.0 } else { 1.0 };
            ((xn, t * sign), t - ti + (xn - x).abs())
        })
        .collect()
}

/// Screen targets for a side's units, under a calibration.
pub fn plan(side: &mechabellum::board::Side, projection: &Projection) -> Vec<PlanStep> {
    side.units
        .iter()
        .zip(behind_front(&side.units))
        .map(|(u, (board, moved_back))| {
            let screen = projection.to_screen(board.0, board.1);
            PlanStep {
                name: u.name.clone(),
                board,
                screen,
                clickable: clickable(screen),
                moved_back,
            }
        })
        .collect()
}

/// The order to place a side's units in: as written, but big models (3x3 and up) last. A
/// click beside a big model can land on it and pick it up instead of dropping the unit being
/// placed (an Arclight beside a Tarantula did). Ordering by neighbours didn't help: with the
/// faces of a tower placed first, its corners failed more often (2026-10-02).
pub fn placement_order(units: &[mechabellum::board::BoardUnit]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..units.len()).collect();
    order.sort_by_key(|&i| {
        mechabellum::footprint(units[i].unit_id).is_some_and(|(w, d)| w >= 3 && d >= 3)
    });
    order
}

/// Mean RGB over a rectangle of a reference-size image.
pub fn mean_color(img: &image::RgbImage, (l, t, r, b): (u32, u32, u32, u32)) -> [f64; 3] {
    let mut sum = [0.0; 3];
    let mut n = 0.0f64;
    for y in t..b {
        for x in l..r {
            let p = img.get_pixel(x, y).0;
            for i in 0..3 {
                sum[i] += f64::from(p[i]);
            }
            n += 1.0;
        }
    }
    sum.map(|s| s / n.max(1.0))
}

/// Whether an item waits in the inventory, top left: its icon's frame is blue-grey, where the
/// ground behind it has little blue (measured: 105 to 113 with an item, 30 without on Railway
/// Town). Only meaningful with the tools closed, since the left panel covers the spot.
pub fn inventory_has_item(img: &image::RgbImage) -> bool {
    let [_, _, b] = mean_color(img, (30, 335, 100, 400));
    b > 90.0
}

/// Patches of open map on both sides of the board, clear of the HUD and tools (centers).
pub const OPEN_GROUND: [Px; 4] = [
    (680.0, 530.0),
    (1580.0, 530.0),
    (780.0, 1080.0),
    (1580.0, 1080.0),
];

/// How far the map in `now` has moved from where it was in `reference` (same camera zoom), in
/// reference pixels: the offset that best lines up square patches of ground centered on
/// `centers`. Pick things that look the same every round (towers, the spawn pad): the zone's
/// lit area grows by round and throws a whole-screen match off.
/// Also returns the match's mean difference per sample, lower being better.
pub fn camera_shift(
    reference: &image::RgbImage,
    now: &image::RgbImage,
    centers: &[Px],
) -> (Px, f64) {
    const SIZE: u32 = 160;
    let patches: Vec<(u32, u32)> = centers
        .iter()
        .map(|&(x, y)| {
            let half = f64::from(SIZE / 2);
            ((x - half).max(0.0) as u32, (y - half).max(0.0) as u32)
        })
        .collect();
    const STEP: u32 = 4;
    let grey = |img: &image::RgbImage, x: i64, y: i64| -> Option<f64> {
        if x < 0 || y < 0 || x >= i64::from(img.width()) || y >= i64::from(img.height()) {
            return None;
        }
        let p = img.get_pixel(x as u32, y as u32).0;
        Some(f64::from(p[0]) + f64::from(p[1]) + f64::from(p[2]))
    };
    let score = |dx: i64, dy: i64| -> f64 {
        let (mut sum, mut n) = (0.0, 0.0);
        for &(px, py) in &patches {
            for y in (0..SIZE).step_by(STEP as usize) {
                for x in (0..SIZE).step_by(STEP as usize) {
                    let (rx, ry) = (i64::from(px + x), i64::from(py + y));
                    if let (Some(a), Some(b)) =
                        (grey(reference, rx, ry), grey(now, rx + dx, ry + dy))
                    {
                        sum += (a - b).abs();
                        n += 1.0;
                    }
                }
            }
        }
        if n > 0.0 {
            sum / n
        } else {
            f64::MAX
        }
    };
    // Coarse, then fine around the best.
    let mut best = ((0i64, 0i64), score(0, 0));
    for dy in (-160..=160).step_by(8) {
        for dx in (-160..=160).step_by(8) {
            let s = score(dx, dy);
            if s < best.1 {
                best = ((dx, dy), s);
            }
        }
    }
    let ((cx, cy), _) = best;
    for dy in cy - 8..=cy + 8 {
        for dx in cx - 8..=cx + 8 {
            let s = score(dx, dy);
            if s < best.1 {
                best = ((dx, dy), s);
            }
        }
    }
    let ((dx, dy), s) = best;
    ((dx as f64, dy as f64), s)
}

/// Whether the left-hand Testing Grounds Tools panel is open, from its header's blue-grey
/// (65,69,87 measured; the ground there is 73,47,33 on Railway Town). Unlike the right panel's
/// header, a unit card pinned from the Squad list never covers it.
pub fn left_tools_open(img: &image::RgbImage) -> bool {
    let [r, _, b] = mean_color(img, (20, 290, 330, 325));
    b > r + 12.0 && b > 75.0
}

/// Whether the right-hand Testing Grounds Tools panel is open: its header is dark navy.
pub fn tools_open(img: &image::RgbImage) -> bool {
    let [r, g, b] = mean_color(img, (2200, 318, 2480, 345));
    b > r + 8.0 && r < 90.0 && g < 90.0
}

/// Centers of bright green blobs (the game tints units bought this round green) in an RGB
/// image, in its pixel coordinates, largest first.
pub fn green_blobs(img: &image::RgbImage) -> Vec<(Px, usize)> {
    const CELL: u32 = 16;
    let (w, h) = img.dimensions();
    let (cw, ch) = (w.div_ceil(CELL), h.div_ceil(CELL));
    let mut counts = vec![0usize; (cw * ch) as usize];
    for y in (0..h).step_by(2) {
        for x in (0..w).step_by(2) {
            let p = img.get_pixel(x, y).0;
            let (r, g, b) = (i32::from(p[0]), i32::from(p[1]), i32::from(p[2]));
            if g > 140 && g - r > 8 && g - b > 50 {
                counts[((y / CELL) * cw + x / CELL) as usize] += 1;
            }
        }
    }
    // Connected components over cells with enough green.
    let hot: Vec<bool> = counts.iter().map(|&c| c >= 4).collect();
    let mut seen = vec![false; hot.len()];
    let mut blobs = Vec::new();
    for start in 0..hot.len() {
        if !hot[start] || seen[start] {
            continue;
        }
        let (mut sx, mut sy, mut n, mut weight) = (0.0, 0.0, 0usize, 0usize);
        let mut stack = vec![start];
        seen[start] = true;
        while let Some(i) = stack.pop() {
            let (cx, cy) = (i as u32 % cw, i as u32 / cw);
            let c = counts[i];
            sx += (f64::from(cx * CELL) + f64::from(CELL) / 2.0) * c as f64;
            sy += (f64::from(cy * CELL) + f64::from(CELL) / 2.0) * c as f64;
            weight += c;
            n += 1;
            for (dx, dy) in [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)] {
                let (nx, ny) = (cx as i32 + dx, cy as i32 + dy);
                if nx < 0 || ny < 0 || nx >= cw as i32 || ny >= ch as i32 {
                    continue;
                }
                let j = (ny as u32 * cw + nx as u32) as usize;
                if hot[j] && !seen[j] {
                    seen[j] = true;
                    stack.push(j);
                }
            }
        }
        if n >= 3 {
            blobs.push(((sx / weight as f64, sy / weight as f64), weight));
        }
    }
    blobs.sort_by(|a, b| b.1.cmp(&a.1));
    blobs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_placement_order() {
        let board = mechabellum::board::Board::parse(
            "[board][side half=south][unit=Tarantula at=0,-10][unit=Arclight at=-16,-19][unit=Overlord at=0,-20][unit=Arclight at=-16,-15][/side][/board]",
        )
        .unwrap();
        assert_eq!(placement_order(&board.sides[0].units), vec![1, 3, 0, 2]);
    }

    #[test]
    fn test_behind_front() {
        let board = mechabellum::board::Board::parse(
            "[board version=2326]\n[side half=north]\n\
             [unit=Fang at=5,3.5 rotated]\n[unit=Fang at=5.5,2]\n[unit=Fang at=5.5,4]\n\
             [/side]\n[side half=south]\n[unit=Fang at=14,-3.5 rotated]\n\
             [unit=Vortex at=20,-5]\n[unit=Vortex at=14,-7]\n[/side]\n[/board]",
        )
        .unwrap();
        let [north, south] = [
            behind_front(&board.sides[0].units),
            behind_front(&board.sides[1].units),
        ];
        // A turned Fang is 5 deep: at 3.5 it reaches 1, so it goes back to 4.5.
        assert_eq!(north[0], ((5.0, 4.5), 1.0));
        // Two Fangs one behind the other, the front one short of the edge: both move back.
        assert_eq!(north[1], ((5.5, 3.0), 1.0));
        assert_eq!(north[2], ((5.5, 5.0), 1.0));
        assert_eq!(south[0], ((14.0, -4.5), 1.0));
        // Not in the way of the moved Fang: stays.
        assert_eq!(south[1], ((20.0, -5.0), 0.0));
        // Right behind it (as on Dusk round 2): pushed back with it.
        assert_eq!(south[2], ((14.0, -8.0), 1.0));
    }

    #[test]
    fn test_side_edges() {
        let board = mechabellum::board::Board::parse(
            "[board]
[side half=south]
             [unit=Crawler count=3 from=left]
[unit=Crawler at=27.5,-20]
             [unit=Crawler at=28.5,-26]
[unit=Crawler at=33,10]
[/side]
[/board]",
        )
        .unwrap();
        let moves = behind_front(&board.sides[0].units);
        // Crawlers are 5 wide. Flush with x = -30: the row shifts a square right, all three
        // together.
        assert_eq!(moves[0], ((-26.5, -16.0), 1.0));
        assert_eq!(moves[1], ((-21.5, -16.0), 1.0));
        assert_eq!(moves[2], ((-16.5, -16.0), 1.0));
        // Past 29 by a square, and by two.
        assert_eq!(moves[3], ((26.5, -20.0), 1.0));
        assert_eq!(moves[4], ((26.5, -26.0), 2.0));
        // In a flank column: left alone.
        assert_eq!(moves[5], ((33.0, 10.0), 0.0));
    }

    #[test]
    fn test_camera_shift_finds_a_pan() {
        // Smooth, non-repeating "terrain", like the map's ground.
        let pattern = |x: i64, y: i64| -> u8 {
            let (x, y) = (x as f64, y as f64);
            let v = 128.0
                + 45.0 * (x / 37.0 + (y / 53.0).sin()).sin()
                + 40.0 * (y / 29.0 + x / 71.0).cos()
                + 25.0 * ((x * 0.013 - y * 0.021).sin() * 3.1).sin();
            v.clamp(0.0, 255.0) as u8
        };
        let (dx, dy) = (21i64, -21i64);
        let make = |ox: i64, oy: i64| {
            image::RgbImage::from_fn(2560, 1600, |x, y| {
                let v = pattern(i64::from(x) - ox, i64::from(y) - oy);
                image::Rgb([v, v / 2, v / 3])
            })
        };
        let (shift, score) = camera_shift(&make(0, 0), &make(dx, dy), &OPEN_GROUND);
        assert_eq!(shift, (21.0, -21.0), "score {score}");
        assert!(score < 1.0, "{score}");
    }

    #[test]
    fn test_translated_projection_moves_every_point() {
        let p = Projection::fit(&RAILWAY_TOWN_SOUTH).unwrap();
        let q = p.translated(21.0, -21.0);
        for (x, y) in [(0.0, -10.0), (-25.0, -28.0), (20.0, -3.0)] {
            let (a, b) = (p.to_screen(x, y), q.to_screen(x, y));
            assert!((b.0 - a.0 - 21.0).abs() < 1e-6 && (b.1 - a.1 + 21.0).abs() < 1e-6);
        }
    }

    #[test]
    fn test_board_shifted_projection_aims_elsewhere() {
        let p = Projection::fit(&RAILWAY_TOWN_SOUTH).unwrap();
        let q = p.board_shifted(0.5, -1.0);
        for (x, y) in [(0.0, -10.0), (-25.0, -28.0), (20.0, -3.0)] {
            let (a, b) = (p.to_screen(x + 0.5, y - 1.0), q.to_screen(x, y));
            assert!((a.0 - b.0).abs() < 1e-6 && (a.1 - b.1).abs() < 1e-6);
        }
    }

    #[test]
    fn test_calibration_fits_and_predicts_the_zone() {
        let p = Projection::fit(&RAILWAY_TOWN_SOUTH).unwrap();
        for (x, y, u, v) in RAILWAY_TOWN_SOUTH {
            let (su, sv) = p.to_screen(x, y);
            assert!(
                (su - u).hypot(sv - v) < 12.0,
                "{x},{y}: {su},{sv} vs {u},{v}"
            );
        }
        // The zone's left corners, as seen on screen; the whole zone is in view.
        for ((x, y), (u, v)) in [
            ((-30.0, -31.0), (339.0, 806.0)),
            ((-30.0, -1.0), (717.0, 288.0)),
        ] {
            let (su, sv) = p.to_screen(x, y);
            assert!((su - u).hypot(sv - v) < 40.0, "{x},{y}: {su},{sv}");
            assert!(clickable((su, sv)));
        }
        let north = Projection::fit(&RAILWAY_TOWN_NORTH).unwrap();
        for (x, y, u, v) in RAILWAY_TOWN_NORTH {
            let (su, sv) = north.to_screen(x, y);
            assert!((su - u).hypot(sv - v) < 15.0);
        }
    }

    #[test]
    fn test_to_board_inverts_to_screen() {
        let p = Projection::fit(&RAILWAY_TOWN_SOUTH).unwrap();
        for (x, y) in [(0.0, -10.0), (-20.0, -25.0), (15.5, -3.0)] {
            let (u, v) = p.to_screen(x, y);
            let (bx, by) = p.to_board(u, v);
            assert!((bx - x).abs() < 1e-6 && (by - y).abs() < 1e-6, "{bx},{by}");
        }
    }

    #[test]
    fn test_tech_buttons() {
        let equipped = vec![
            (10510, "Mechanical Rage".to_string()),
            (180110, "Replicate".to_string()),
            (3510, "Loose Formation".to_string()),
            (10710, "Impact Drill".to_string()),
        ];
        assert_eq!(
            tech_buttons(
                "Crawler",
                &["impact drill".into(), "Mechanical Rage".into()],
                &equipped
            ),
            Ok(vec![3, 0])
        );
        assert!(tech_buttons("Crawler", &["Barrier".into()], &equipped)
            .unwrap_err()
            .contains("equipped"));
    }

    #[test]
    fn test_palette() {
        assert_eq!(ui::palette("Crawler").unwrap().scroll_down_steps, 0);
        assert!(
            ui::palette("Experimental Death Knell")
                .unwrap()
                .scroll_down_steps
                > 60
        );
        assert_eq!(ui::palette("Rhino").unwrap().scroll_down_steps, 18);
        assert!(ui::palette("Tank").is_none());
    }

    #[test]
    fn test_unlock_cells() {
        assert!(ui::unlock_cell("crawler").is_some());
        assert!(ui::unlock_cell("Fire Badger").is_some());
        assert!(ui::unlock_cell("Abyss").is_some());
        assert!(ui::unlock_cell("Tank").is_none());
    }

    #[test]
    fn test_green_blobs() {
        let mut img = image::RgbImage::from_pixel(400, 300, image::Rgb([120, 80, 60]));
        for y in 100..140 {
            for x in 200..260 {
                img.put_pixel(x, y, image::Rgb([120, 200, 60]));
            }
        }
        let blobs = green_blobs(&img);
        assert_eq!(blobs.len(), 1);
        let ((x, y), _) = blobs[0];
        assert!(
            (x - 230.0).abs() < 10.0 && (y - 120.0).abs() < 10.0,
            "{x},{y}"
        );
    }
}
