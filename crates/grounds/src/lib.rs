#![allow(clippy::needless_range_loop)] // Gaussian elimination reads clearer with indices.

//! Setting up a board in Mechabellum's Testing Grounds. The pure parts live here: where things
//! are on screen, and how board squares map to screen pixels. The Windows driver is in `win`.
//! See docs/testing-grounds.md.

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

    /// Screen position of a board point, in squares.
    pub fn to_screen(&self, x: f64, y: f64) -> Px {
        let h = &self.h;
        let w = h[6] * x + h[7] * y + 1.0;
        (
            (h[0] * x + h[1] * y + h[2]) / w,
            (h[3] * x + h[4] * y + h[5]) / w,
        )
    }
}

/// Calibration for Railway Town, south half, seen from your own side with the camera zoomed all
/// the way out (60 wheel steps). Units dropped at these screen points were recorded at these board
/// squares in the saved replay (2026-09-27); fit error 2 to 10 px. The whole zone is on screen.
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

/// Equipped techs per unit type, in card order.
pub type Loadout = std::collections::BTreeMap<u32, Vec<(u32, String)>>;

/// Each player's equipped techs in card order, from the newest replay in `replay_dir`: yours
/// first, then the other side (in Testing Grounds, "Bot", which has its own loadout).
pub fn loadouts(replay_dir: &std::path::Path) -> anyhow::Result<(Loadout, Loadout)> {
    let newest = std::fs::read_dir(replay_dir)?
        .flatten()
        .filter(|e| {
            e.path()
                .extension()
                .is_some_and(|x| x == "grbr" || x == "xml")
        })
        .max_by_key(|e| e.metadata().and_then(|m| m.modified()).ok())
        .ok_or_else(|| anyhow::anyhow!("no replays in {}", replay_dir.display()))?;
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
    pub board: (f64, f64),
    pub screen: Px,
    pub clickable: bool,
}

/// Screen targets for a side's units, under a calibration.
pub fn plan(side: &mechabellum::board::Side, projection: &Projection) -> Vec<PlanStep> {
    side.units
        .iter()
        .map(|u| {
            let board = (f64::from(u.x) / 10.0, f64::from(u.y) / 10.0);
            let screen = projection.to_screen(board.0, board.1);
            PlanStep {
                name: u.name.clone(),
                board,
                screen,
                clickable: clickable(screen),
            }
        })
        .collect()
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
