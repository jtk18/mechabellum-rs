//! A whole match from one player's side, for match cards and whole-game stats: the army's size
//! and levels, the units used and sold, and how each fight's army was deployed.
//!
//! Everything here is derived from `MatchDetail`, so it follows the same round model: a fight's
//! army is the one that fought it, and the last fight's is rebuilt from its actions.

use super::detail::{MatchDetail, PlacedUnit, SideLayout};
use super::game_data::GRID_SQUARE;
use crate::board::{Half, TOWER_SIZE, TOWER_Y, ZONE_FRONT, ZONE_HALF_WIDTH};

use serde::Serialize;

use std::collections::BTreeMap;

/// Earliest build whose deployment geometry this module knows: the 2.0 board, measured on
/// versions 2259 and later. Older replays use a larger board with no towers (coordinates past
/// ±1,000 in 1.x replays against ±350 in 2.0), so their deployments aren't classified.
pub const DEPLOYMENT_GEOMETRY_FROM: u32 = 2259;

/// A unit is aggressive when its front edge is within this of the zone's front edge (J: "within
/// 2-3 squares from the line").
const AGGRESSIVE_REACH: i32 = 3 * GRID_SQUARE;
/// Midboard is the ground right in front of the towers (J): from the towers' centers forward to
/// this far in front of their front faces. Behind the towers' centers is backboard.
const MIDBOARD_REACH: i32 = 5 * GRID_SQUARE;
/// An army is aggressive, midboard or backboard when that band holds the largest share of it
/// and at least this much; otherwise it's standard. Without a floor, an army spread evenly
/// (25/27/24/24) changed class from fight to fight on a percentage point.
const DISTINCT_SHARE: f32 = 0.4;
/// An aggressive deployment is asymmetric when its forward group leans at least this far to one
/// side: (right - left) / total, by footprint area. 0.5 is three quarters on one side.
const ASYM_LEAN: f32 = 0.5;

/// How far forward a unit, or most of an army, stands. Front to back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Depth {
    /// At the line: front edge within 3 squares of it, or in a flank column.
    Aggressive,
    /// Between the aggressive band and midboard. Also the class of an army that no other band
    /// leads with `DISTINCT_SHARE` of it.
    Standard,
    /// Right in front of the towers, up to level with their centers.
    Midboard,
    /// Behind the towers' centers.
    Backboard,
}

impl Depth {
    const ALL: [Depth; 4] = [
        Depth::Aggressive,
        Depth::Standard,
        Depth::Midboard,
        Depth::Backboard,
    ];

    fn name(self) -> &'static str {
        match self {
            Depth::Aggressive => "Aggressive",
            Depth::Standard => "Standard",
            Depth::Midboard => "Midboard",
            Depth::Backboard => "Backboard",
        }
    }
}

/// How one player's army was deployed for one fight.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Deployment {
    /// "Standard", "Sym aggressive", "Asym aggressive", "Midboard" or "Backboard".
    pub label: String,
    /// The band holding the largest share of the army (ties go to the one further forward), when
    /// that's aggressive, midboard or backboard with at least 40% of it; otherwise standard.
    pub depth: Depth,
    /// For an aggressive deployment, how its forward group leans: -1 all on the player's own
    /// left, 1 all on their right, by footprint area.
    pub lean: Option<f32>,
    /// Share of the army in each band, front to back, by footprint area.
    pub bands: [f32; 4],
    /// Units in a flank column (they open along the edges of the opponent's half from round 3).
    pub flank: u32,
}

/// One fight from one player's side.
#[derive(Debug, Clone, Serialize)]
pub struct FightSummary {
    /// 1-based, as the app counts fights.
    pub fight: u32,
    /// "Win", "Lose" or "Deuce". For a replay older than 2340, the last fight's is the match
    /// result.
    pub result: String,
    /// Reactor core HP going into the fight.
    pub hp: i32,
    pub units: u32,
    /// `None` before the 2.0 board, outside 1v1, or for an empty army.
    pub deployment: Option<Deployment>,
    /// The army as it fought, in the board's shared frame, for drawing it: `[unit_id, x, y,
    /// width, depth, level, band]`, with positions in replay units (10 per square), the
    /// footprint in squares after rotation, the level 1-based, and the unit's `Depth` band as
    /// 0-3 front to back (-1 when the deployment isn't classified). Units with no known
    /// footprint are 2x2.
    pub board: Vec<[i32; 7]>,
}

/// One unit type over a match.
#[derive(Debug, Clone, Serialize)]
pub struct UnitUse {
    pub unit_id: u32,
    pub name: String,
    /// Copies added over the match: the starter pack's, purchases, and cards'. 0 in AsyncBattle
    /// (see `PlayerGame::units_added`).
    pub added: u32,
    /// Copies sold. 0 in AsyncBattle.
    pub sold: u32,
    /// Most copies fielded in one fight.
    pub peak: u32,
    /// Copies in the last fight.
    pub last: u32,
    /// Highest level a copy reached, 1-based.
    pub max_level: u32,
    /// The first fight it was fielded in.
    pub first_fight: u32,
}

/// A whole match from one player's side. It names no player, so it can be published as is:
/// `MatchDetail::player_games` returns one per player, in `players` order.
#[derive(Debug, Clone, Serialize)]
pub struct PlayerGame {
    /// The half this player deployed on. Boards are in the shared frame, so a card needs it to
    /// turn each army to face the right way.
    pub half: Option<Half>,
    pub fights_won: u32,
    /// Units in the last fight.
    pub final_units: u32,
    /// Most units fielded in one fight.
    pub peak_units: u32,
    /// Mean level of the last fight's army, 1-based.
    pub mean_level: f32,
    pub max_level: u32,
    /// Levels gained over the match, counting units later sold.
    pub level_ups: u32,
    /// Units added over the match (starter pack, purchases, cards). `None` in AsyncBattle, whose
    /// opponents' rounds come from different games and don't follow on from each other.
    pub units_added: Option<u32>,
    /// Units sold over the match: in one fight's army and gone from the next. `None` in
    /// AsyncBattle, as for `units_added`.
    pub units_sold: Option<u32>,
    /// Techs researched by the last fight.
    pub techs: u32,
    /// Cards picked between rounds.
    pub cards: u32,
    /// Round 1's deployment label: how the match is classed for this player (J, 2026-10-27:
    /// judge the layout by round 1's positioning, not the last round's). `None` when round 1
    /// isn't recorded or isn't classified; a later round never stands in for it.
    pub opening: Option<String>,
    /// Most fielded first.
    pub units: Vec<UnitUse>,
    pub fights: Vec<FightSummary>,
}

/// A unit's position turned into its own side's frame: x to the player's right, y toward the
/// middle of the board.
fn own_frame(half: Half, x: i32, y: i32) -> (i32, i32) {
    match half {
        Half::South => (x, y),
        Half::North => (-x, -y),
    }
}

fn in_flank_column(x: i32) -> bool {
    x.abs() > ZONE_HALF_WIDTH
}

/// Width and depth in replay units, after rotation. On the 2.0 board a flank column runs front
/// to back and units stand in it on end, so there `rotated` reads the other way (as the desktop
/// app draws them; see `board.rs`).
fn extent(unit: &PlacedUnit, board_2: bool) -> (i32, i32) {
    let (w, d) = unit.footprint.unwrap_or((2, 2));
    let turned = unit.rotated != (board_2 && in_flank_column(unit.x));
    let (w, d) = if turned { (d, w) } else { (w, d) };
    (i32::from(w) * GRID_SQUARE, i32::from(d) * GRID_SQUARE)
}

/// Which half an army stands on: the side of the middle its median unit is on, leaving out
/// flank columns, which reach into the opponent's half.
fn half_of(units: &[PlacedUnit]) -> Option<Half> {
    let mut ys: Vec<i32> = units
        .iter()
        .filter(|u| !in_flank_column(u.x))
        .map(|u| u.y)
        .collect();
    if ys.is_empty() {
        ys = units.iter().map(|u| u.y).collect();
    }
    ys.sort_unstable();
    let y = *ys.get(ys.len() / 2)?;
    Some(if y < 0 { Half::South } else { Half::North })
}

/// The band a unit stands in, from its position in its own side's frame.
fn depth_of(x: i32, y: i32, depth: i32) -> Depth {
    let front_edge = y + depth / 2;
    let tower_front = TOWER_Y + TOWER_SIZE / 2;
    if in_flank_column(x) || front_edge >= ZONE_FRONT - AGGRESSIVE_REACH {
        Depth::Aggressive
    } else if y > tower_front + MIDBOARD_REACH {
        Depth::Standard
    } else if y >= TOWER_Y {
        Depth::Midboard
    } else {
        Depth::Backboard
    }
}

/// The band one unit stands in.
fn unit_depth(unit: &PlacedUnit, half: Half) -> Depth {
    let (x, y) = own_frame(half, unit.x, unit.y);
    depth_of(x, y, extent(unit, true).1)
}

/// Classify one army on the 2.0 board. Each unit counts by its footprint's area, so a 7x7 Abyss weighs what it
/// covers rather than what a Marksman does.
pub fn classify(units: &[PlacedUnit], half: Half) -> Option<Deployment> {
    let mut bands = [0f32; 4];
    let (mut left, mut right) = (0f32, 0f32);
    let mut flank = 0;
    for unit in units {
        let (x, _) = own_frame(half, unit.x, unit.y);
        let (w, d) = extent(unit, true);
        let area = (w * d) as f32;
        let depth = unit_depth(unit, half);
        bands[depth as usize] += area;
        flank += u32::from(in_flank_column(x));
        if depth == Depth::Aggressive {
            match x.signum() {
                -1 => left += area,
                1 => right += area,
                _ => {
                    left += area / 2.0;
                    right += area / 2.0;
                }
            }
        }
    }
    let total: f32 = bands.iter().sum();
    if total <= 0.0 {
        return None;
    }
    for share in &mut bands {
        *share /= total;
    }
    // Back to front, so a tie keeps the band further forward.
    let lead = Depth::ALL
        .into_iter()
        .rev()
        .max_by(|a, b| bands[*a as usize].total_cmp(&bands[*b as usize]))?;
    let depth = if bands[lead as usize] >= DISTINCT_SHARE {
        lead
    } else {
        Depth::Standard
    };
    let lean = (left + right > 0.0).then(|| (right - left) / (right + left));
    let label = match (depth, lean) {
        (Depth::Aggressive, Some(lean)) if lean.abs() >= ASYM_LEAN => "Asym aggressive".to_owned(),
        (Depth::Aggressive, _) => "Sym aggressive".to_owned(),
        (other, _) => other.name().to_owned(),
    };
    Some(Deployment {
        label,
        depth,
        lean: (depth == Depth::Aggressive).then_some(lean).flatten(),
        bands,
        flank,
    })
}

impl MatchDetail {
    /// Whether deployments in this match can be classified: a 1v1 on the 2.0 board. Other modes'
    /// zones haven't been measured.
    pub fn deployments_known(&self) -> bool {
        self.version >= DEPLOYMENT_GEOMETRY_FROM && self.match_mode == "VS_1_1"
    }

    /// The match from each player's side, in `players` order.
    pub fn player_games(&self) -> Vec<PlayerGame> {
        let classify_deployments = self.deployments_known();
        let comparable = self.game_mode != "AsyncBattle";
        (0..self.players.len())
            .map(|i| {
                let sides: Vec<(u32, &SideLayout)> = self
                    .rounds
                    .iter()
                    .filter_map(|r| Some((r.round + 1, r.sides.get(i)?.as_ref()?)))
                    .filter(|(_, side)| !side.units.is_empty())
                    .collect();
                let half = sides.first().and_then(|(_, side)| half_of(&side.units));

                let mut uses: BTreeMap<u32, UnitUse> = BTreeMap::new();
                // Highest level seen per copy, by index and type, to count level-ups once.
                let mut levels: BTreeMap<(u32, u32), u32> = BTreeMap::new();
                let mut sold = 0;
                let mut added = 0;
                for (n, (fight, side)) in sides.iter().enumerate() {
                    let mut counts: BTreeMap<u32, u32> = BTreeMap::new();
                    for unit in &side.units {
                        *counts.entry(unit.unit_id).or_default() += 1;
                        let level = levels.entry((unit.index, unit.unit_id)).or_default();
                        *level = (*level).max(unit.level);
                        let entry = uses.entry(unit.unit_id).or_insert_with(|| UnitUse {
                            unit_id: unit.unit_id,
                            name: unit.name.clone(),
                            added: 0,
                            sold: 0,
                            peak: 0,
                            last: 0,
                            max_level: 0,
                            first_fight: *fight,
                        });
                        entry.max_level = entry.max_level.max(unit.level + 1);
                        if unit.is_new {
                            entry.added += 1;
                            added += 1;
                        }
                    }
                    for (unit_id, count) in counts {
                        if let Some(entry) = uses.get_mut(&unit_id) {
                            entry.peak = entry.peak.max(count);
                        }
                    }
                    // Units don't die between fights, so a copy missing from the next fight's
                    // army was sold (or recycled by a skill).
                    if let Some((next_fight, next)) = sides.get(n + 1) {
                        if *next_fight == fight + 1 {
                            for unit in &side.units {
                                let kept = next
                                    .units
                                    .iter()
                                    .any(|u| u.index == unit.index && u.unit_id == unit.unit_id);
                                if !kept {
                                    sold += 1;
                                    if let Some(entry) = uses.get_mut(&unit.unit_id) {
                                        entry.sold += 1;
                                    }
                                }
                            }
                        }
                    }
                }
                let last = sides.last().map(|(_, side)| *side);
                if let Some(last) = last {
                    for unit in &last.units {
                        if let Some(entry) = uses.get_mut(&unit.unit_id) {
                            entry.last += 1;
                        }
                    }
                }
                let mut units: Vec<UnitUse> = uses.into_values().collect();
                units.sort_by(|a, b| {
                    (b.peak, b.last, b.added)
                        .cmp(&(a.peak, a.last, a.added))
                        .then_with(|| a.name.cmp(&b.name))
                });

                let fights: Vec<FightSummary> = sides
                    .iter()
                    .map(|(fight, side)| {
                        let half = half.filter(|_| classify_deployments);
                        FightSummary {
                            fight: *fight,
                            result: side.result.clone(),
                            hp: side.hp,
                            units: side.units.len() as u32,
                            deployment: half.and_then(|half| classify(&side.units, half)),
                            board: side
                                .units
                                .iter()
                                .map(|u| {
                                    let (w, d) = extent(u, classify_deployments);
                                    [
                                        u.unit_id as i32,
                                        u.x,
                                        u.y,
                                        w / GRID_SQUARE,
                                        d / GRID_SQUARE,
                                        u.level as i32 + 1,
                                        half.map_or(-1, |half| unit_depth(u, half) as i32),
                                    ]
                                })
                                .collect(),
                        }
                    })
                    .collect();
                let final_units = last.map_or(0, |side| side.units.len() as u32);

                if !comparable {
                    for unit in &mut units {
                        unit.added = 0;
                        unit.sold = 0;
                    }
                }
                PlayerGame {
                    half,
                    fights_won: fights.iter().filter(|f| f.result == "Win").count() as u32,
                    final_units,
                    peak_units: fights.iter().map(|f| f.units).max().unwrap_or(0),
                    mean_level: last.map_or(0.0, |side| {
                        side.units.iter().map(|u| u.level + 1).sum::<u32>() as f32
                            / side.units.len().max(1) as f32
                    }),
                    max_level: last
                        .and_then(|side| side.units.iter().map(|u| u.level + 1).max())
                        .unwrap_or(0),
                    level_ups: levels.values().sum(),
                    units_added: comparable.then_some(added),
                    units_sold: comparable.then_some(sold),
                    techs: last.map_or(0, |side| {
                        side.enabled_techs
                            .iter()
                            .map(|t| t.techs.len() as u32)
                            .sum()
                    }),
                    cards: sides
                        .iter()
                        .map(|(_, side)| side.cards_chosen.len() as u32)
                        .sum(),
                    opening: fights
                        .iter()
                        .find(|f| f.fight == 1)
                        .and_then(|f| Some(f.deployment.as_ref()?.label.clone())),
                    units,
                    fights,
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BattleRecord;
    use std::path::PathBuf;

    fn unit(unit_id: u32, x: i32, y: i32) -> PlacedUnit {
        PlacedUnit {
            unit_id,
            name: String::new(),
            index: 0,
            level: 0,
            exp: 0,
            xp_to_next: None,
            x,
            y,
            rotated: false,
            footprint: super::super::footprint(unit_id),
            equipment: vec![],
            is_new: false,
            xp_gained: None,
        }
    }

    #[test]
    fn test_bands_follow_the_line_and_the_towers() {
        // A 2x2 Marksman (id 2), in the south half's own frame. Front edge 3 squares behind the
        // zone's front edge: aggressive. 4 squares: standard.
        assert_eq!(depth_of(0, -50, 20), Depth::Aggressive);
        assert_eq!(depth_of(0, -60, 20), Depth::Standard);
        // The towers' front faces are at -160: up to 5 squares in front is midboard, and so is
        // level with them back to their centers (-170). Behind that is backboard.
        assert_eq!(depth_of(0, -110, 20), Depth::Midboard);
        assert_eq!(depth_of(0, -100, 20), Depth::Standard);
        assert_eq!(depth_of(0, -170, 20), Depth::Midboard);
        assert_eq!(depth_of(0, -180, 20), Depth::Backboard);
        // Flank columns are past the line however deep they reach.
        assert_eq!(depth_of(330, 200, 20), Depth::Aggressive);
        // A 7x7 Abyss flush with the front edge is aggressive though its center is 4.5 squares
        // back.
        assert_eq!(depth_of(0, -45, 70), Depth::Aggressive);
    }

    #[test]
    fn test_classify_per_half_and_symmetry() {
        // Two Marksmen at the line, one each side: symmetric. On the north half the frame turns
        // around, so the same units mirrored classify the same.
        let south = [unit(2, -100, -20), unit(2, 100, -20)];
        let d = classify(&south, Half::South).unwrap();
        assert_eq!(d.label, "Sym aggressive");
        assert_eq!(d.lean, Some(0.0));
        let north = [unit(2, 100, 20), unit(2, -100, 20)];
        assert_eq!(classify(&north, Half::North).unwrap(), d);

        // Both on the player's right: asymmetric. North's right is the shared frame's -x.
        let lopsided = [unit(2, -100, 20), unit(2, -150, 20)];
        let d = classify(&lopsided, Half::North).unwrap();
        assert_eq!(d.label, "Asym aggressive");
        assert_eq!(d.lean, Some(1.0));

        // Most of the area behind the towers: backboard, with shares by area.
        let deep = [unit(2, 0, -20), unit(1, 0, -250), unit(1, 60, -250)];
        let d = classify(&deep, Half::South).unwrap();
        assert_eq!(d.label, "Backboard");
        assert_eq!(d.lean, None);
        assert!((d.bands[0] - 4.0 / 36.0).abs() < 1e-6, "{:?}", d.bands);
        assert!((d.bands[3] - 32.0 / 36.0).abs() < 1e-6, "{:?}", d.bands);

        // Spread evenly, front to back: no band leads with 40%, so standard.
        let spread = [
            unit(2, -100, -20),
            unit(2, -100, -80),
            unit(2, -100, -150),
            unit(2, -100, -250),
            unit(2, 100, -250),
        ];
        let d = classify(&spread, Half::South).unwrap();
        assert_eq!(d.bands, [0.2, 0.2, 0.2, 0.4]);
        assert_eq!(d.label, "Backboard", "40% is enough");
        let d = classify(&spread[..4], Half::South).unwrap();
        assert_eq!(d.label, "Standard");
        assert_eq!(d.lean, None);

        assert_eq!(classify(&[], Half::South), None);
    }

    /// The 2340 fixture (opponent's name removed): every fight's result is listed, the last one
    /// included, and the summaries follow from them.
    #[test]
    fn test_player_games_2340() {
        let path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/battle_record_2340.xml");
        let detail = BattleRecord::new_from_file(path, false)
            .unwrap()
            .match_detail();
        assert_eq!(detail.version, 2340);
        assert!(detail.deployments_known());
        let games = detail.player_games();
        assert_eq!(games.len(), 2);

        // The control: each lost fight is followed by a drop in that player's HP.
        for game in &games {
            for pair in game.fights.windows(2) {
                assert_eq!(
                    pair[0].result == "Lose",
                    pair[1].hp < pair[0].hp,
                    "fight {}",
                    pair[0].fight
                );
            }
        }
        let results =
            |g: &PlayerGame| -> String { g.fights.iter().map(|f| &f.result[..1]).collect() };
        assert_eq!(results(&games[0]), "LLLLWWWW");
        assert_eq!(results(&games[1]), "WWWWLLLL");
        assert_eq!(detail.players[0].won, Some(true), "won the deciding fight");
        assert_eq!(detail.players[1].won, Some(false));
        assert_eq!(games[0].fights_won, 4);

        for game in &games {
            assert_eq!(game.fights.len(), 8);
            assert!(game.half.is_some());
            assert!(game.fights.iter().all(|f| f.deployment.is_some()));
            assert_eq!(
                game.opening.as_ref(),
                game.fights[0].deployment.as_ref().map(|d| &d.label),
                "the match is classed by round 1"
            );
            assert_eq!(
                game.final_units as usize,
                game.fights.last().unwrap().board.len()
            );
            assert!(game.peak_units >= game.final_units);
            assert!(game.max_level >= 1 && game.mean_level >= 1.0);
            // Units added over the match, less those sold, make the final army.
            assert_eq!(
                game.units_added.unwrap() - game.units_sold.unwrap(),
                game.final_units
            );
            let per_type: u32 = game.units.iter().map(|u| u.added - u.sold).sum();
            assert_eq!(per_type, game.final_units);
            for f in &game.fights {
                let d = f.deployment.as_ref().unwrap();
                assert!((d.bands.iter().sum::<f32>() - 1.0).abs() < 1e-4);
            }
        }
        // The two armies stand on opposite halves.
        assert_ne!(games[0].half, games[1].half);
    }

    /// Before 2340 the last fight's result is the match result, as `detail` has it.
    #[test]
    fn test_player_games_2324() {
        let path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/battle_record_2324.xml");
        let detail = BattleRecord::new_from_file(path, false)
            .unwrap()
            .match_detail();
        let games = detail.player_games();
        for (game, player) in games.iter().zip(&detail.players) {
            let last = game.fights.last().unwrap();
            assert_eq!(last.result == "Win", player.won == Some(true));
            assert!(game.fights.iter().all(|f| f.deployment.is_some()));
            // This fixture has no record of round 1, so the match isn't classed, though later
            // rounds are.
            assert_eq!(game.fights[0].fight, 2);
            assert_eq!(game.opening, None);
        }
    }

    /// Older boards aren't classified, and their summaries still fill in.
    #[test]
    fn test_player_games_1x() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/battle_record.xml");
        let detail = BattleRecord::new_from_file(path, false)
            .unwrap()
            .match_detail();
        assert!(!detail.deployments_known());
        let games = detail.player_games();
        assert!(games
            .iter()
            .flat_map(|g| &g.fights)
            .all(|f| f.deployment.is_none()));
        assert!(games
            .iter()
            .all(|g| g.final_units > 0 && g.opening.is_none()));
    }

    /// Deployment labels and fight results over a folder of extracted replays:
    /// `MECHABELLUM_REPLAYS=<dir> cargo test -p mechabellum -- --ignored deployments_folder --nocapture`
    #[test]
    #[ignore]
    fn test_deployments_folder() {
        let dir = std::env::var("MECHABELLUM_REPLAYS").expect("MECHABELLUM_REPLAYS");
        let mut labels: BTreeMap<String, (u32, u32)> = BTreeMap::new();
        let (mut matches, mut fights) = (0, 0);
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|e| e != "xml") {
                continue;
            }
            let Ok(record) = BattleRecord::new_from_file(path, false) else {
                continue;
            };
            let detail = record.match_detail();
            if !detail.deployments_known() {
                continue;
            }
            matches += 1;
            for game in detail.player_games() {
                for f in &game.fights {
                    let Some(d) = &f.deployment else { continue };
                    fights += 1;
                    let entry = labels.entry(d.label.clone()).or_default();
                    entry.0 += 1;
                    entry.1 += u32::from(f.result == "Win");
                }
            }
        }
        println!("{matches} matches, {fights} deployments");
        for (label, (n, wins)) in labels {
            println!("{label:16} {n:5} deployments, won {wins} fights");
        }
    }
}
