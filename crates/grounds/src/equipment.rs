//! The Testing Grounds Tools' Equipment tab: every item, grouped by rank, three per row. Clicking
//! an item puts it in your inventory (top left); click it there, then a unit, to equip it.
//!
//! The catalog lists each of the game's 79 item IDs once. Several names appear more than once
//! (Heavy Armor in ranks 1 and 2, three Dominion Cores), as different IDs. `grounds items-check`
//! equips cells and reads the IDs back from a replay; `CONFIRMED` lists the ones checked. Among
//! the 1303xxxx items, rank 1 turned out to hold the *higher* IDs (Heavy Armor 13030013 in rank
//! 1, 13030002 in rank 2), and the unchecked pairs in that family follow the same pattern.

use crate::Px;

/// The catalog top to bottom: each rank's rows, three cells per row, as (item ID, name).
pub const CATALOG: [&[&[(u32, &str)]]; 4] = [
    // Rank 1
    &[
        &[
            (13030005, "Haste Module"),
            (13030012, "Laser Sights"),
            (13030013, "Heavy Armor"),
        ],
        &[
            (13030014, "Improved Firepower Control System"),
            (13030015, "Enhancement Module"),
            (13030016, "Haste Module"),
        ],
        &[
            (13030018, "Amplifying Core"),
            (13030020, "Dominion Core"),
            (13030021, "Secondary Laser Sight"),
        ],
        &[
            (13030022, "Secondary Fire Control System"),
            (13030023, "Secondary Heavy Armor"),
            (13030024, "Small Amplifying Core"),
        ],
        &[
            (13030029, "Dominion Core"),
            (1305003, "Photon Coating"),
            (1305013, "Photon Coating"),
        ],
        &[
            (1309001, "Absorption Module"),
            (13040001, "Deployment Module"),
            (1307001, "Barrier"),
        ],
        &[
            (13150001, "Advanced HE Shells"),
            (13150002, "Explosive Ammo"),
            (13150003, "Improved Burst Ammunition"),
        ],
        &[
            (13020001, "Nano Repair Kit"),
            (13010001, "Portable Shield"),
            (1308001, "Anti-Interference Module"),
        ],
        &[(13130001, "Optical Camouflage")],
    ],
    // Rank 2
    &[
        &[
            (13030001, "Laser Sights"),
            (13030002, "Heavy Armor"),
            (13030003, "Improved Firepower Control System"),
        ],
        &[
            (13030011, "Rapid Autoloader"),
            (13030017, "Super Heavy Armor"),
            (13030019, "Advanced Firepower Control System"),
        ],
        &[
            (1305004, "Advanced EMP Ammo"),
            (1305005, "Advanced Parasitic Ammo"),
            (1305006, "Advanced Suppression Ammo"),
        ],
        &[
            (1305007, "Combat Evolution System"),
            (1305008, "Photon Emission Device"),
            (1305009, "Advanced Acid Ammo"),
        ],
        &[
            (1305010, "Advanced Guidance Radar"),
            (1305014, "High-Incendiary Ammunition"),
            (1307002, "Super Barrier"),
        ],
        &[
            (1306004, "Abyss Beacon"),
            (1306001, "Tank Production Line"),
            (1306002, "Mustang Production Line"),
        ],
        &[
            (1306003, "Steel Ball Production Line"),
            (13110001, "Splitter Core"),
            (13120001, "Emergency Capacitor"),
        ],
        &[
            (13130002, "Optical Camouflage"),
            (13100001, "Experimental Mountain Armor"),
            (13140001, "Heavy Targeting System"),
        ],
    ],
    // Rank 3
    &[
        &[
            (13030007, "Amplifying Core"),
            (13030009, "Small Amplifying Core"),
            (13030010, "Dominion Core"),
        ],
        &[
            (13030521, "Secondary Laser Sight"),
            (13030522, "Secondary Fire Control System"),
            (13030523, "Secondary Heavy Armor"),
        ],
        &[
            (1305001, "Charged Ammo"),
            (1305011, "Charged Ammo"),
            (1309002, "Absorption Module"),
        ],
        &[
            (1307003, "Barrier"),
            (13150101, "Explosive Ammo"),
            (13020002, "Nano Repair Kit"),
        ],
        &[
            (13010002, "Portable Shield"),
            (13130101, "Optical Camouflage"),
        ],
    ],
    // Rank 4
    &[
        &[
            (13030004, "Enhancement Module"),
            (13030006, "Super Heavy Armor"),
            (13030008, "Advanced Firepower Control System"),
        ],
        &[
            (13030025, "Improved Laser Sight"),
            (13030026, "Improved Heavy Armor"),
            (13030027, "Improved Fire Control System"),
        ],
        &[
            (13030028, "Improved Amplification Core"),
            (1305002, "Parasitic Ammo"),
            (1305012, "Parasitic Ammo"),
        ],
        &[
            (1306005, "Tank Production Line"),
            (1306006, "Mustang Production Line"),
            (1306007, "Steel Ball Production Line"),
        ],
        &[
            (13020003, "Improved Nano Repair Kit"),
            (13130003, "Improved Optical Camouflage"),
            (13140002, "Heavy Targeting System"),
        ],
        &[(13140003, "Lightweight Lock System")],
    ],
];

/// IDs checked against a replay by equipping them from their catalog cell. The rest are the
/// best reading of the catalog.
pub const CONFIRMED: &[u32] = &[
    // 2026-09-28, on Fortresses: rank 1's first 13 cells (Amplifying Core not placed).
    13030005, 13030012, 13030013, 13030014, 13030015, 13030016, 13030020, 13030021, 13030022,
    13030023, 13030024, 13030029,
];

/// Cell centers across the panel.
const COLUMNS: [f64; 3] = [2227.0, 2352.0, 2476.0];
/// Where the first row's icons sit with the list scrolled to the top.
const FIRST_ROW_Y: f64 = 530.0;
/// Rows are this far apart, and each rank header after the first adds this much.
const ROW_PITCH: f64 = 176.9;
const HEADER_EXTRA: f64 = 46.0;
/// One wheel step scrolls this far (250 px per 6 steps, measured).
pub const PX_PER_STEP: f64 = 250.0 / 6.0;
/// The list can't scroll past this many steps from the top.
const MAX_STEPS: i32 = 101;
/// Aim to bring a cell to about here, well inside the visible part of the list.
const TARGET_Y: f64 = 800.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cell {
    pub id: u32,
    pub name: &'static str,
    /// 1-based, as the catalog labels it.
    pub rank: usize,
    /// Wheel steps down from the top of the list, then where to click.
    pub scroll_down_steps: i32,
    pub at: Px,
}

/// Every cell, top to bottom.
pub fn cells() -> Vec<Cell> {
    let mut out = Vec::new();
    let mut row_index = 0usize;
    for (rank, rows) in CATALOG.iter().enumerate() {
        for row in rows.iter() {
            let y = FIRST_ROW_Y + ROW_PITCH * row_index as f64 + HEADER_EXTRA * rank as f64;
            let steps = (((y - TARGET_Y) / PX_PER_STEP).round() as i32).clamp(0, MAX_STEPS);
            for (col, &(id, name)) in row.iter().enumerate() {
                out.push(Cell {
                    id,
                    name,
                    rank: rank + 1,
                    scroll_down_steps: steps,
                    at: (COLUMNS[col], y - steps as f64 * PX_PER_STEP),
                });
            }
            row_index += 1;
        }
    }
    out
}

/// The cell for an item ID.
pub fn cell(id: u32) -> Option<Cell> {
    cells().into_iter().find(|c| c.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_catalog_covers_every_item_once() {
        let names = mechabellum::GameNames::bundled();
        let all = names.ids(mechabellum::NameKind::Equipment);
        let cells = cells();
        assert_eq!(cells.len(), 79);
        assert_eq!(cells.len(), all.len());
        for c in &cells {
            assert!(all.contains(&u64::from(c.id)), "{} {}", c.id, c.name);
            assert_eq!(
                names.resolve(mechabellum::NameKind::Equipment, u64::from(c.id)),
                Some(c.name),
                "{}",
                c.id
            );
        }
        let mut ids: Vec<u32> = cells.iter().map(|c| c.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), 79, "an ID is listed twice");
    }

    #[test]
    fn test_cells_land_inside_the_list() {
        for c in cells() {
            assert!(
                (430.0..=1320.0).contains(&c.at.1),
                "{} at {:?} after {} steps",
                c.name,
                c.at,
                c.scroll_down_steps
            );
        }
        // Measured: rank 2's first row at y 668 after 36 steps.
        let laser2 = cell(13030001).unwrap();
        let y = laser2.at.1 + (laser2.scroll_down_steps - 36) as f64 * PX_PER_STEP;
        assert!((y - 668.0).abs() < 8.0, "{y}");
    }
}
