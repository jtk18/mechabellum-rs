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
///
/// Fang and Crawler were corrected 2026-09-27 from (4, 3) and (2, 3): in 2.0 replays their
/// centers sit on half squares across (odd width) and on grid lines front to back (even depth)
/// in 30 of 33 Fangs and 9 of 9 Crawlers, and Fangs stack 2 squares apart. Fang is 5 wide by
/// 2 deep (J, from play). Crawler is 5 wide (2026-10-01): side by side in a row, 2.0 Crawlers
/// stand exactly 5 squares apart, never closer (28 pairs; turned ones 5 or more front to back),
/// and Testing Grounds refused one dropped 4 squares from another.
///
/// 2026-09-30, from 2,821 replay boards (`replay-parse check-board`): Centurion, Wraith, and
/// Tarantula are 3×3, not 5×5 or 5×3 (units stand closer than half that to the zone's edges
/// and to each other), and Phoenix and Typhoon are 4 wide by 2 deep, not 2 by 4. With these
/// every unit type fits its board without overlaps; every footprint is at most what the
/// zone's edges allow.
///
/// 2026-10-02: Phantom Ray is 5 wide by 2 deep, not 2 by 2. Its centers sit on half squares across
/// and whole squares front to back (1,211 of 1,272 unturned), which only an odd width and even
/// depth allow, and side by side in a row they stand 5 squares apart or more, never closer (62
/// pairs at 5).
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
        9 => (5, 2),    // Fang
        10 => (5, 2),   // Crawler
        11 => (5, 5),   // Overlord
        12 => (3, 2),   // Stormcaller
        13 => (5, 2),   // Sledgehammer
        14 => (3, 3),   // Hacker
        15 => (2, 2),   // Arclight
        16 => (4, 2),   // Phoenix
        17 => (5, 5),   // War Factory
        18 => (3, 3),   // Wraith
        19 => (3, 3),   // Scorpion
        20 => (5, 2),   // Fire Badger
        21 => (3, 3),   // Sabertooth
        22 => (4, 2),   // Typhoon
        23 => (4, 4),   // Sandworm
        24 => (3, 3),   // Tarantula
        25 => (5, 2),   // Phantom Ray
        26 => (3, 3),   // Farseer
        27 => (4, 4),   // Raiden
        28 => (4, 2),   // Hound
        29 => (7, 7),   // Abyss
        30 => (4, 2),   // Void Eye
        31 => (2, 2),   // Vortex
        32 => (3, 3),   // Centurion
        2001 => (5, 5), // Death Knell
        2002 => (7, 7), // Mountain
        4001 => (5, 5), // Experimental Death Knell
        _ => return None,
    })
}

/// Newest version whose XP caps were checked against replays. Balance patches change caps, so
/// newer versions shouldn't be judged by this table until it's re-measured.
pub const XP_TABLE_VERIFIED_THROUGH: u32 = 2324;

/// XP a unit needs at level 0 to reach level 1, measured from 2.0 replays (versions 2259 and
/// 2324): units with a full bar sit exactly at `base * multiplier`. Star Expedition copies
/// (5001-5032, 5201-5203) level at their own, mostly lower, rates.
fn base_xp(unit_id: u32) -> Option<u32> {
    Some(match unit_id {
        1 => 1600,
        2 => 650,
        3 => 2200,
        4 => 1400,
        5 => 1150,
        6 => 900,
        7 => 1200,
        8 => 1000,
        9 => 600,
        10 => 450,
        11 => 1750,
        12 => 1100,
        13 => 1300,
        14 => 560,
        15 => 750,
        16 => 1300,
        17 => 3200,
        18 => 2250,
        19 => 1500,
        20 => 1400,
        21 => 1300,
        22 => 1950,
        23 => 2000,
        24 => 1500,
        25 => 1000,
        26 => 1800,
        27 => 2400,
        28 => 750,
        29 => 4800,
        30 => 650,
        31 => 750,
        32 => 2250,

        5001 => 2000,
        5002 => 450,
        5003 => 2500,
        5004 => 1750,
        5005 => 1650,
        5006 => 1350,
        5007 => 900,
        5008 => 1000,
        5009 => 450,
        5010 => 350,
        5011 => 1750,
        5012 => 1650,
        5013 => 1000,
        5015 => 500,
        5016 => 1950,
        5017 => 4000,
        5019 => 1500,
        5020 => 900,
        5021 => 900,
        5022 => 1500,
        5023 => 2500,
        5024 => 1000,
        5025 => 1500,
        5026 => 1800,
        5027 => 3000,
        5028 => 400,
        5029 => 6000,
        5030 => 400,
        5031 => 400,
        5032 => 1500,
        5202 => 4000,
        5203 => 1200,
        _ => return None,
    })
}

/// XP needed to leave `level` (0-based, as stored in replays). `None` when unknown.
pub fn xp_to_next_level(unit_id: u32, level: u32) -> Option<u32> {
    // Units whose caps don't follow the shared multipliers. The Vulcan's later levels fit a
    // base of 2420, but its first level-up comes sooner.
    if unit_id == 3 {
        return [2200, 5453, 8620].get(level as usize).copied();
    }
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
        assert_eq!(
            xp_to_next_level(5021, 0),
            Some(900),
            "Star Expedition copy levels faster"
        );
        assert_eq!(xp_to_next_level(32, 0), Some(2250), "Centurion, 2324");
        assert_eq!(xp_to_next_level(32, 1), Some(5070));
        assert_eq!(xp_to_next_level(32, 2), Some(8014));
        assert_eq!(xp_to_next_level(1, 0), Some(1600));
        assert_eq!(xp_to_next_level(3, 0), Some(2200), "Vulcan");
        assert_eq!(xp_to_next_level(3, 1), Some(5453));
        assert_eq!(xp_to_next_level(2, 4), None, "not measured");
        assert_eq!(xp_to_next_level(999, 0), None);
    }

    #[test]
    fn test_footprints() {
        assert_eq!(footprint(11), Some((5, 5)), "Overlord, published");
        assert_eq!(footprint(1), Some((4, 4)), "giant, published");
        assert_eq!(footprint(5010), footprint(10));
        assert_eq!(footprint(9), Some((5, 2)), "Fang, from play");
        assert_eq!(footprint(999), None);
    }
}
