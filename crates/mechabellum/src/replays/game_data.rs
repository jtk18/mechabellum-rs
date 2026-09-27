//! Unit facts that replays don't carry, inferred from the replays themselves (game version 2.0).
//!
//! Positions in replays are unit centers on a grid of 10-unit squares, snapped to half squares.
//!
//! - **Footprints:** a center on a half square means an odd width in squares, and a center on a
//!   grid line means an even width. How tightly players pack copies of a unit gives the size.
//!   Anchored to the published sizes: giants are 4x4 squares, and Overlord has been 5x5 since
//!   Season 2. These are approximate.
//! - **XP:** `Exp` is progress within the current level and resets on level-up. Units parked at
//!   a full bar give exact thresholds: each unit has a base, and every unit shares the same
//!   multiplier per level.

/// Size of one grid square in replay coordinates.
pub const GRID_SQUARE: i32 = 10;

/// XP needed to leave each level, as a multiple of the unit's base. Beyond this, the replays
/// don't have enough units to measure it.
const LEVEL_MULTIPLIERS: [f64; 4] = [1.0, 2.2533, 3.5617, 4.49];

/// Base unit ID: AsyncBattle copies (5001-5032, 5201-5203) share their original's data.
fn base_id(id: u32) -> u32 {
    match id {
        5001..=5099 => id - 5000,
        5201 => 2001,
        5202 => 2002,
        5203 => 4001,
        _ => id,
    }
}

/// Footprint in grid squares, (width, depth), before rotation. `None` for unknown units.
pub fn footprint(unit_id: u32) -> Option<(u8, u8)> {
    Some(match base_id(unit_id) {
        1 => (4, 4),    // Fortress
        2 => (2, 2),    // Marksman
        3 => (4, 4),    // Vulcan
        4 => (4, 4),    // Melting Point
        5 => (3, 3),    // Rhino
        6 => (3, 2),    // Wasp
        7 => (5, 2),    // Mustang
        8 => (5, 2),    // Steel Ball
        9 => (4, 3),    // Fang
        10 => (2, 3),   // Crawler
        11 => (5, 5),   // Overlord
        12 => (3, 2),   // Stormcaller
        13 => (5, 2),   // Sledgehammer
        14 => (3, 3),   // Hacker
        15 => (2, 2),   // Arclight
        16 => (2, 4),   // Phoenix
        17 => (5, 5),   // War Factory
        18 => (5, 5),   // Wraith
        19 => (3, 3),   // Scorpion
        20 => (5, 2),   // Fire Badger
        21 => (3, 3),   // Sabertooth
        22 => (2, 4),   // Typhoon
        23 => (4, 4),   // Sandworm
        24 => (5, 3),   // Tarantula
        25 => (2, 2),   // Phantom Ray
        26 => (3, 3),   // Farseer
        27 => (4, 4),   // Raiden
        28 => (4, 2),   // Hound
        29 => (7, 7),   // Abyss
        30 => (4, 2),   // Void Eye
        31 => (2, 2),   // Vortex
        32 => (5, 5),   // Centurion
        2001 => (5, 5), // Death Knell
        2002 => (7, 7), // Mountain
        4001 => (5, 5), // Experimental Death Knell
        _ => return None,
    })
}

/// XP a unit needs at level 0 to reach level 1.
fn base_xp(unit_id: u32) -> Option<u32> {
    Some(match base_id(unit_id) {
        1 => 2000,
        2 => 650,
        3 => 2500,
        4 => 1750,
        5 => 1650,
        6 => 1350,
        7 => 1200,
        8 => 1000,
        9 => 600,
        10 => 450,
        11 => 1750,
        12 => 1650,
        13 => 1300,
        14 => 560,
        15 => 750,
        16 => 1950,
        17 => 4000,
        18 => 2250,
        19 => 1500,
        20 => 1400,
        21 => 1300,
        22 => 1950,
        23 => 2500,
        24 => 1500,
        25 => 1500,
        26 => 1800,
        27 => 3000,
        28 => 750,
        29 => 6000,
        30 => 650,
        31 => 750,
        32 => 1500,
        2002 => 4000,
        _ => return None,
    })
}

/// XP needed to leave `level` (0-based, as stored in replays). `None` when unknown.
pub fn xp_to_next_level(unit_id: u32, level: u32) -> Option<u32> {
    let multiplier = LEVEL_MULTIPLIERS.get(level as usize)?;
    Some((base_xp(unit_id)? as f64 * multiplier).round() as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_xp_thresholds_match_observed_caps() {
        // Units observed sitting at exactly these values with a full bar.
        assert_eq!(xp_to_next_level(2, 0), Some(650));
        assert_eq!(xp_to_next_level(2, 1), Some(1465));
        assert_eq!(xp_to_next_level(2, 2), Some(2315));
        assert_eq!(xp_to_next_level(30, 3), Some(2919));
        assert_eq!(xp_to_next_level(21, 1), Some(2929));
        assert_eq!(xp_to_next_level(5021, 1), Some(2929), "AsyncBattle copy");
        assert_eq!(xp_to_next_level(2, 4), None, "not measured");
        assert_eq!(xp_to_next_level(999, 0), None);
    }

    #[test]
    fn test_footprints() {
        assert_eq!(footprint(11), Some((5, 5)), "Overlord, published");
        assert_eq!(footprint(1), Some((4, 4)), "giant, published");
        assert_eq!(footprint(5010), footprint(10));
        assert_eq!(footprint(999), None);
    }
}
