//! A whole match from one player's side, for match cards and whole-game stats: the army's size
//! and levels, the units used and sold, and how each fight's army was deployed.
//!
//! Everything here is derived from `MatchDetail`, so it follows the same round model: a fight's
//! army is the one that fought it, and the last fight's is rebuilt from its actions.

use super::detail::{MatchDetail, PlacedUnit, SideLayout};
use super::game_data::GRID_SQUARE;
use crate::board::{Half, TOWER_Y, ZONE_FRONT, ZONE_HALF_WIDTH};

use serde::Serialize;

use std::collections::BTreeMap;

/// Earliest build whose deployment geometry this module knows: the 2.0 board with its Command
/// and Research Centers, measured on versions 2259 and later. Midboard and backboard are defined
/// by the towers, so older replays aren't classified. (1.x 1v1 replays stay within the same
/// coordinates; only 2v2 and brawls reach past ±1,000.)
pub const DEPLOYMENT_GEOMETRY_FROM: u32 = 2259;

/// A unit is aggressive when its front edge is within this of the zone's front edge (3 squares
/// before 2026-10-27).
const AGGRESSIVE_REACH: i32 = 5 * GRID_SQUARE;
/// An army is aggressive or backboard when that band holds the largest share of it and at least
/// this much; otherwise it's midboard. Without a floor, an army spread evenly changed class from
/// fight to fight on a percentage point.
const DISTINCT_SHARE: f32 = 0.4;
/// A deployment is asymmetric when its units lean at least this far to one side: (right - left)
/// / total, by footprint area. 0.5 is three quarters on one side.
const ASYM_LEAN: f32 = 0.5;

/// How far forward a unit, or most of an army, stands. Front to back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Depth {
    /// At the line: front edge within 5 squares of the zone's front edge, or in a flank column.
    Aggressive,
    /// From the aggressive band back to the towers' centers. Also the class of an army that
    /// neither other band leads with `DISTINCT_SHARE` of it.
    Midboard,
    /// Behind the towers' centers.
    Backboard,
}

impl Depth {
    /// Whether an army of this class is called sym or asym: one grouped on one side is a different
    /// deployment from one spread across both. Backboard isn't split yet.
    fn has_sides(self) -> bool {
        matches!(self, Depth::Aggressive | Depth::Midboard)
    }

    const ALL: [Depth; 3] = [Depth::Aggressive, Depth::Midboard, Depth::Backboard];

    /// The class's name, by whether the army is grouped on one side. A symmetric midboard army is
    /// "Standard" and a one-sided one "Asym mid aggro".
    fn label(self, asym: Option<bool>) -> &'static str {
        match (self, asym) {
            (Depth::Aggressive, Some(true)) => "Asym aggressive",
            (Depth::Aggressive, _) => "Sym aggressive",
            (Depth::Midboard, Some(true)) => "Asym mid aggro",
            (Depth::Midboard, _) => "Standard",
            (Depth::Backboard, _) => "Backboard",
        }
    }
}

/// How two 1v1 deployments meet when both are grouped on one side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Meeting {
    /// On the same lane, so the two armies meet head on.
    #[serde(rename = "Asym headbutt")]
    Headbutt,
    /// On opposite lanes, so each army faces the other's empty side.
    #[serde(rename = "Asym crossfire")]
    Crossfire,
}

impl Meeting {
    /// As serialized.
    pub fn label(self) -> &'static str {
        match self {
            Meeting::Headbutt => "Asym headbutt",
            Meeting::Crossfire => "Asym crossfire",
        }
    }
}

/// How one player's army was deployed for one fight.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Deployment {
    /// "Sym aggressive", "Asym aggressive", "Standard", "Asym mid aggro" or "Backboard".
    pub label: String,
    /// The band holding the largest share of the army (ties go to the one further forward), when
    /// that's aggressive or backboard with at least 40% of it; otherwise midboard.
    pub depth: Depth,
    /// How the army leans, -1 all on the player's own left, 1 all on their right, by footprint
    /// area: an aggressive army's units in that band, a midboard one's whole army (midboard is
    /// also the class of an army no band leads, so its own band can be a sliver). `None` for
    /// backboard.
    pub lean: Option<f32>,
    /// Share of the army in each band, front to back, by footprint area.
    pub bands: [f32; 3],
    /// Units in a flank column (they open along the edges of the opponent's half from round 3).
    pub flank: u32,
}

impl Deployment {
    /// Grouped on one side.
    pub fn is_asym(&self) -> bool {
        self.lean.is_some_and(|lean| lean.abs() >= ASYM_LEAN)
    }

    /// The side of the board a one-sided army stands on, in the shared frame (1 east, -1 west).
    fn lane(&self, half: Half) -> Option<i32> {
        let side = self.lean.filter(|_| self.is_asym())?.signum() as i32;
        Some(match half {
            Half::South => side,
            Half::North => -side,
        })
    }
}

/// How two players' deployments for the same fight meet, when both are grouped on one side.
pub fn meeting(a: (&Deployment, Half), b: (&Deployment, Half)) -> Option<Meeting> {
    let (a, b) = (a.0.lane(a.1)?, b.0.lane(b.1)?);
    Some(if a == b {
        Meeting::Headbutt
    } else {
        Meeting::Crossfire
    })
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
    /// The army's worth in supply: each unit's own sell value, as the game records it (discounts
    /// and free units included; upgrades and techs not). `None` if a unit's isn't known.
    pub value: Option<u32>,
    /// `None` before the 2.0 board, outside 1v1, or for an empty army.
    pub deployment: Option<Deployment>,
    /// In a 1v1, how the two players' deployments met, when both were grouped on one side.
    pub meeting: Option<Meeting>,
    /// The half this fight's army stood on, when its deployment is classified.
    #[serde(skip)]
    half: Option<Half>,
    /// The army as it fought, in the board's shared frame, for drawing it: `[unit_id, x, y,
    /// width, depth, level, band]`, with positions in replay units (10 per square), the
    /// footprint in squares after rotation, the level 1-based, and the unit's `Depth` band as
    /// 0-2 front to back (-1 when the deployment isn't classified). Units with no known
    /// footprint are 2x2.
    pub board: Vec<[i32; 7]>,
}

/// One unit type over a match.
#[derive(Debug, Clone, Serialize)]
pub struct UnitUse {
    pub unit_id: u32,
    pub name: String,
    /// Copies added over the match: the starter pack's, purchases, and cards'. `None` when
    /// `PlayerGame::units_added` is.
    pub added: Option<u32>,
    /// Copies sold. `None` when `PlayerGame::units_sold` is.
    pub sold: Option<u32>,
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
    /// The last fight's army's worth in supply (`FightSummary::value`).
    pub final_value: Option<u32>,
    /// The most any fight's army was worth. `None` if a fight's worth isn't known.
    pub peak_value: Option<u32>,
    /// Mean level of the last fight's army, 1-based.
    pub mean_level: f32,
    /// Highest level in the last fight's army, 1-based. (`UnitUse::max_level` covers the match.)
    pub max_level: u32,
    /// Unit upgrades bought over the match, units later sold included. Levels units arrive with,
    /// from cards or techs, aren't upgrades. `None` as for `units_added`: a replay that starts
    /// mid-match would count only part of the match.
    pub level_ups: Option<u32>,
    /// Units added over the match (starter pack, purchases, cards). `None` unless the fights run
    /// from the first without a gap, and in AsyncBattle, whose opponents' rounds come from
    /// different games and don't follow on from each other.
    pub units_added: Option<u32>,
    /// Units sold over the match: in one fight's army and gone from the next. `None` in
    /// AsyncBattle, as for `units_added`.
    pub units_sold: Option<u32>,
    /// Techs researched by the last fight.
    pub techs: u32,
    /// Cards picked between rounds. `None` as for `level_ups`.
    pub cards: Option<u32>,
    /// Round 1's deployment label: how the match is classed for this player, by round 1's
    /// positioning, not the last round's. `None` when round 1 isn't recorded or isn't classified;
    /// a later round never stands in for it.
    pub opening: Option<String>,
    /// How round 1's deployments met (`FightSummary::meeting`).
    pub opening_meeting: Option<Meeting>,
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
        .filter(|u| u.placed && !in_flank_column(u.x))
        .map(|u| u.y)
        .collect();
    if ys.is_empty() {
        ys = units.iter().filter(|u| u.placed).map(|u| u.y).collect();
    }
    ys.sort_unstable();
    let y = *ys.get(ys.len() / 2)?;
    Some(if y < 0 { Half::South } else { Half::North })
}

/// The band a unit stands in, from its position in its own side's frame.
fn depth_of(x: i32, y: i32, depth: i32) -> Depth {
    let front_edge = y + depth / 2;
    if in_flank_column(x) || front_edge >= ZONE_FRONT - AGGRESSIVE_REACH {
        Depth::Aggressive
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
    let mut bands = [0f32; 3];
    // Area on the player's own left and right, per band.
    let (mut left, mut right) = ([0f32; 3], [0f32; 3]);
    let mut flank = 0;
    // A unit with no recorded position has no band.
    for unit in units.iter().filter(|u| u.placed) {
        let (x, _) = own_frame(half, unit.x, unit.y);
        let (w, d) = extent(unit, true);
        let area = (w * d) as f32;
        let depth = unit_depth(unit, half);
        let band = depth as usize;
        bands[band] += area;
        flank += u32::from(in_flank_column(x));
        match x.signum() {
            -1 => left[band] += area,
            1 => right[band] += area,
            _ => {
                left[band] += area / 2.0;
                right[band] += area / 2.0;
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
        Depth::Midboard
    };
    // The lean of the units that give the army its class: the aggressive band's, or for midboard,
    // which is also the class of an army no band leads, the whole army's.
    let (l, r) = match depth {
        Depth::Midboard => (left.iter().sum(), right.iter().sum()),
        _ => (left[depth as usize], right[depth as usize]),
    };
    let lean = (depth.has_sides() && l + r > 0.0).then(|| (r - l) / (r + l));
    let label = depth
        .label(lean.map(|lean| lean.abs() >= ASYM_LEAN))
        .to_owned();
    Some(Deployment {
        label,
        depth,
        lean,
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
        let mut games = self.player_games_apart();
        // How the two deployments met, fight by fight, in a 1v1.
        if let [a, b] = games.as_mut_slice() {
            for fa in &mut a.fights {
                let Some(fb) = b.fights.iter_mut().find(|f| f.fight == fa.fight) else {
                    continue;
                };
                let met = match (&fa.deployment, fa.half, &fb.deployment, fb.half) {
                    // Two armies on one half can only come from a crafted replay.
                    (Some(da), Some(ha), Some(db), Some(hb)) if ha != hb => {
                        meeting((da, ha), (db, hb))
                    }
                    _ => None,
                };
                fa.meeting = met;
                fb.meeting = met;
            }
            for game in [a, b] {
                game.opening_meeting = game
                    .fights
                    .iter()
                    .find(|f| f.fight == 1)
                    .and_then(|f| f.meeting);
            }
        }
        games
    }

    fn player_games_apart(&self) -> Vec<PlayerGame> {
        let classify_deployments = self.deployments_known();
        // Flank columns turn units on end on the 2.0 board in every mode, classified or not.
        let board_2 = self.version >= DEPLOYMENT_GEOMETRY_FROM;
        let async_battle = self.game_mode.trim() == "AsyncBattle";
        (0..self.players.len())
            .map(|i| {
                let sides: Vec<(u32, &SideLayout)> = self
                    .rounds
                    .iter()
                    .filter_map(|r| Some((r.round + 1, r.sides.get(i)?.as_ref()?)))
                    .filter(|(_, side)| !side.units.is_empty())
                    .collect();
                let half = sides.first().and_then(|(_, side)| half_of(&side.units));
                // Units added and sold are diffs between consecutive armies, which only add up
                // when the fights run from the first without a gap: a missing round 1 would leave
                // the starting army uncounted. AsyncBattle opponents' rounds come from different
                // games, so their armies don't follow on from each other at all.
                // No fights at all is nothing to count, not a count of zero.
                let comparable = !async_battle
                    && !sides.is_empty()
                    && sides
                        .iter()
                        .enumerate()
                        .all(|(n, (fight, _))| *fight == n as u32 + 1);

                let mut uses: BTreeMap<u32, UnitUse> = BTreeMap::new();
                let mut sold = 0;
                let mut added = 0;
                for (n, (fight, side)) in sides.iter().enumerate() {
                    let mut counts: BTreeMap<u32, u32> = BTreeMap::new();
                    for unit in &side.units {
                        *counts.entry(unit.unit_id).or_default() += 1;
                        let entry = uses.entry(unit.unit_id).or_insert_with(|| UnitUse {
                            unit_id: unit.unit_id,
                            name: unit.name.clone(),
                            added: Some(0),
                            sold: Some(0),
                            peak: 0,
                            last: 0,
                            max_level: 0,
                            first_fight: *fight,
                        });
                        entry.max_level = entry.max_level.max(unit.level.saturating_add(1));
                        if unit.is_new {
                            *entry.added.get_or_insert(0) += 1;
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
                                        *entry.sold.get_or_insert(0) += 1;
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
                        // Each fight's own half: AsyncBattle rounds can come from either side.
                        let half = half_of(&side.units).filter(|_| classify_deployments);
                        FightSummary {
                            fight: *fight,
                            result: side.result.clone(),
                            hp: side.hp,
                            units: side.units.len() as u32,
                            // Saturating: an upload can claim any SellSupply.
                            value: side
                                .units
                                .iter()
                                .try_fold(0u32, |sum, u| Some(sum.saturating_add(u.value?))),
                            deployment: half.and_then(|half| classify(&side.units, half)),
                            meeting: None,
                            half,
                            board: side
                                .units
                                .iter()
                                .filter(|u| u.placed)
                                .map(|u| {
                                    let (w, d) = extent(u, board_2);
                                    [
                                        u.unit_id as i32,
                                        u.x,
                                        u.y,
                                        w / GRID_SQUARE,
                                        d / GRID_SQUARE,
                                        (u.level as i32).saturating_add(1),
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
                        unit.added = None;
                        unit.sold = None;
                    }
                }
                PlayerGame {
                    half,
                    fights_won: fights.iter().filter(|f| f.result == "Win").count() as u32,
                    final_units,
                    peak_units: fights.iter().map(|f| f.units).max().unwrap_or(0),
                    final_value: fights.last().and_then(|f| f.value),
                    peak_value: fights
                        .iter()
                        .map(|f| f.value)
                        .collect::<Option<Vec<u32>>>()
                        .and_then(|values| values.into_iter().max()),
                    mean_level: last.map_or(0.0, |side| {
                        side.units
                            .iter()
                            .map(|u| u64::from(u.level.saturating_add(1)))
                            .sum::<u64>() as f32
                            / side.units.len().max(1) as f32
                    }),
                    max_level: last
                        .and_then(|side| side.units.iter().map(|u| u.level.saturating_add(1)).max())
                        .unwrap_or(0),
                    level_ups: comparable
                        .then(|| sides.iter().map(|(_, side)| side.upgrades_bought).sum()),
                    units_added: comparable.then_some(added),
                    units_sold: comparable.then_some(sold),
                    techs: last.map_or(0, |side| {
                        side.enabled_techs
                            .iter()
                            .map(|t| t.techs.len() as u32)
                            .sum()
                    }),
                    cards: comparable.then(|| {
                        sides
                            .iter()
                            .map(|(_, side)| side.cards_chosen.len() as u32)
                            .sum()
                    }),
                    opening: fights
                        .iter()
                        .find(|f| f.fight == 1)
                        .and_then(|f| Some(f.deployment.as_ref()?.label.clone())),
                    opening_meeting: None,
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
            placed: true,
            rotated: false,
            footprint: super::super::footprint(unit_id),
            equipment: vec![],
            is_new: false,
            xp_gained: None,
            value: Some(100),
        }
    }

    #[test]
    fn test_bands_follow_the_line_and_the_towers() {
        // A 2x2 Marksman (id 2), in the south half's own frame. Front edge 5 squares behind the
        // zone's front edge (-10): aggressive. 6 squares: midboard, which runs back to the
        // towers' centers (-170). Behind that is backboard.
        assert_eq!(depth_of(0, -70, 20), Depth::Aggressive);
        assert_eq!(depth_of(0, -80, 20), Depth::Midboard);
        assert_eq!(depth_of(0, -110, 20), Depth::Midboard);
        assert_eq!(depth_of(0, -170, 20), Depth::Midboard);
        assert_eq!(depth_of(0, -180, 20), Depth::Backboard);
        // Flank columns are past the line however deep they reach.
        assert_eq!(depth_of(330, 200, 20), Depth::Aggressive);
        // A 7x7 Abyss is aggressive with its front edge 5 squares back, though its center is 8.5
        // squares back.
        assert_eq!(depth_of(0, -95, 70), Depth::Aggressive);
        assert_eq!(depth_of(0, -105, 70), Depth::Midboard);
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
        assert!((d.bands[2] - 32.0 / 36.0).abs() < 1e-6, "{:?}", d.bands);

        // A real asym midboard: the opponent's round 1 in
        // 20261027--67342143, exactly as recorded (shared frame, south half). Ids: 9 Fang 5x2
        // (the first one turned), 21 Sabertooth 3x3, 2 Marksman 2x2.
        let mut turned = unit(9, 0, -205);
        turned.rotated = true;
        let one_side = [
            turned,
            unit(9, -175, -170),
            unit(9, -145, -80),
            unit(21, -75, -125),
            unit(21, -215, -125),
            unit(2, -240, -100),
            unit(2, -50, -100),
        ];
        let d = classify(&one_side, Half::South).unwrap();
        assert_eq!(d.depth, Depth::Midboard, "{:?}", d.bands);
        assert_eq!(d.label, "Asym mid aggro");
        // Everything but the turned Fang, which straddles the middle, is on its left.
        assert!((d.lean.unwrap() + 46.0 / 56.0).abs() < 1e-6, "{:?}", d.lean);
        let expected = [0.0, 46.0 / 56.0, 10.0 / 56.0];
        assert!(
            d.bands
                .iter()
                .zip(expected)
                .all(|(a, b)| (a - b).abs() < 1e-6),
            "{:?}",
            d.bands
        );
        // The same army spread across both sides is standard: a symmetric midboard.
        let mut split = one_side;
        split[4].x = 215;
        split[6].x = 50;
        assert_eq!(classify(&split, Half::South).unwrap().label, "Standard");
        let standard = [unit(2, -100, -80), unit(2, 0, -80), unit(2, 100, -80)];
        let d = classify(&standard, Half::South).unwrap();
        assert_eq!((d.label.as_str(), d.lean), ("Standard", Some(0.0)));
        let lopsided = [unit(2, 100, -80), unit(2, 150, -80), unit(2, 200, -80)];
        let d = classify(&lopsided, Half::South).unwrap();
        assert_eq!((d.label.as_str(), d.lean), ("Asym mid aggro", Some(1.0)));

        // Half the ground behind the towers: backboard. 40% is enough.
        let deepish = [
            unit(2, -100, -20),
            unit(2, 100, -80),
            unit(2, 100, -250),
            unit(2, -100, -250),
        ];
        let d = classify(&deepish, Half::South).unwrap();
        assert_eq!(d.bands, [0.25, 0.25, 0.5]);
        assert_eq!(d.label, "Backboard");
        // Spread evenly, front to back: a third each, so no band leads with 40% and the army is
        // midboard, sided by the whole army.
        let spread = [unit(2, -100, -20), unit(2, 100, -80), unit(2, 0, -250)];
        let d = classify(&spread, Half::South).unwrap();
        assert_eq!(d.label, "Standard");
        assert_eq!(d.lean, Some(0.0));
        // The same spread all on one side: its midboard ground alone is on one side whatever
        // happens, so the whole army decides.
        let one_side: Vec<PlacedUnit> = spread
            .iter()
            .map(|u| unit(u.unit_id, -u.x.abs(), u.y))
            .collect();
        assert_eq!(
            classify(&one_side, Half::South).unwrap().label,
            "Asym mid aggro"
        );

        assert_eq!(classify(&[], Half::South), None);
    }

    /// A real asym headbutt: round 1 of 20261027--67347842, exactly as recorded (shared frame).
    /// Neither army is standard.
    #[test]
    fn test_asym_mid_aggro_and_headbutt() {
        let turned = |id, x, y| {
            let mut u = unit(id, x, y);
            u.rotated = true;
            u
        };
        // South: three 4x2s (id 30) and two 3x3 Tarantulas (24) in a row in midboard, two
        // Crawlers (10) turned behind them, all on the player's right.
        let south = [
            unit(30, 230, -90),
            unit(30, 50, -90),
            unit(30, 140, -90),
            unit(24, 95, -85),
            unit(24, 185, -85),
            turned(10, 50, -125),
            turned(10, 230, -125),
        ];
        let s = classify(&south, Half::South).unwrap();
        assert_eq!((s.label.as_str(), s.lean), ("Asym mid aggro", Some(1.0)));
        assert_eq!(s.bands, [0.0, 1.0, 0.0]);
        // North: the Tarantulas' front edges are 3 squares from the zone's, aggressive, but the
        // other 53% of the ground is midboard. All of it is on the player's left.
        let north = [
            unit(31, 140, 80),
            unit(31, 60, 80),
            unit(31, 220, 80),
            unit(24, 185, 55),
            unit(24, 95, 55),
            unit(2, 180, 130),
            unit(2, 90, 130),
        ];
        let n = classify(&north, Half::North).unwrap();
        assert_eq!((n.label.as_str(), n.lean), ("Asym mid aggro", Some(-1.0)));
        // One's right is the other's left: both on the shared frame's east lane.
        assert_eq!(
            meeting((&s, Half::South), (&n, Half::North)),
            Some(Meeting::Headbutt)
        );
        // North on the west lane instead: they pass each other.
        let west: Vec<PlacedUnit> = north.iter().map(|u| unit(u.unit_id, -u.x, u.y)).collect();
        let w = classify(&west, Half::North).unwrap();
        assert_eq!(
            meeting((&s, Half::South), (&w, Half::North)),
            Some(Meeting::Crossfire)
        );
        // A symmetric army meets nobody head on.
        let spread = classify(&[unit(2, -100, -80), unit(2, 100, -80)], Half::South).unwrap();
        assert_eq!(meeting((&spread, Half::South), (&n, Half::North)), None);
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
            let per_type: u32 = game
                .units
                .iter()
                .map(|u| u.added.unwrap() - u.sold.unwrap())
                .sum();
            assert_eq!(per_type, game.final_units);
            for f in &game.fights {
                let d = f.deployment.as_ref().unwrap();
                assert!((d.bands.iter().sum::<f32>() - 1.0).abs() < 1e-4);
            }
        }
        // The two armies stand on opposite halves.
        assert_ne!(games[0].half, games[1].half);

        // Both sides see the same meeting each fight, and round 1's classes the match.
        for (a, b) in games[0].fights.iter().zip(&games[1].fights) {
            assert_eq!(a.meeting, b.meeting, "fight {}", a.fight);
            let both_asym = a.deployment.as_ref().unwrap().is_asym()
                && b.deployment.as_ref().unwrap().is_asym();
            assert_eq!(a.meeting.is_some(), both_asym, "fight {}", a.fight);
        }
        for game in &games {
            assert_eq!(game.opening_meeting, game.fights[0].meeting);
            // Every fight is priced: the last fight's purchases from the match's own records or
            // the list prices.
            assert!(game.fights.iter().all(|f| f.value.is_some()));
            assert_eq!(game.final_value, game.fights.last().unwrap().value);
            assert!(game.peak_value >= game.final_value);
        }
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
            // rounds are, and units added and sold can't be counted.
            assert_eq!(game.fights[0].fight, 2);
            assert_eq!(game.opening, None);
            assert_eq!((game.units_added, game.units_sold), (None, None));
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

    fn fixture(name: &str) -> BattleRecord {
        BattleRecord::new_from_file(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("fixtures")
                .join(name),
            false,
        )
        .unwrap()
    }

    /// Every tag `<name>` in `xml` from byte `from` on, up to `limit` of them, with its content
    /// replaced by `value`.
    fn set_tags(xml: &str, from: usize, limit: usize, name: &str, value: &str) -> String {
        let (open, close) = (format!("<{name}>"), format!("</{name}>"));
        let mut out = String::from(&xml[..from]);
        let mut rest = &xml[from..];
        for _ in 0..limit {
            let Some(start) = rest.find(&open) else {
                break;
            };
            let after = start + open.len();
            let Some(end) = rest[after..].find(&close) else {
                break;
            };
            out.push_str(&rest[..after]);
            out.push_str(value);
            rest = &rest[after + end..];
        }
        out.push_str(rest);
        out
    }

    /// A crafted replay claiming a position at the edges of i32, absurd levels and sell values
    /// is summarized without overflowing anything (a debug build panicked on each), and the
    /// implausible values don't count: units fall back to list prices.
    #[test]
    fn test_crafted_extremes_stay_in_range() {
        let path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/battle_record_2340.xml");
        let mut xml = std::fs::read_to_string(&path).unwrap();
        // One unit of the south army that fought fight 1 (round 2's record), far off the board:
        // the rest keep the army on its half, so the geometry runs in the south frame.
        let fought = xml.find("<round>2</round>").unwrap();
        xml = set_tags(&xml, fought, 1, "x", "-2147483648");
        xml = set_tags(&xml, fought, 1, "y", "2147483647");
        xml = set_tags(&xml, 0, usize::MAX, "Level", "4000000000");
        xml = set_tags(&xml, 0, usize::MAX, "SellSupply", "4294967295");
        // Unit indices at the edge of i64 (a summon makes the rebuild count them).
        xml = set_tags(&xml, 0, usize::MAX, "unitIndex", "9223372036854775807");
        let record = BattleRecord::from_bytes(xml.as_bytes(), &path).unwrap();
        let games = record.match_detail().player_games();
        assert_eq!(games.len(), 2);
        for game in &games {
            for fight in &game.fights {
                let value = fight.value.expect("list prices cover every unit here");
                assert!(value <= fight.units * 800, "fight {}: {value}", fight.fight);
            }
            assert!(game.mean_level.is_finite() && game.max_level <= 1_001);
        }
    }

    /// A replay claiming a mode with more players than it holds (a 1v1 labelled 2v2) is read
    /// without indexing past its players.
    #[test]
    fn test_a_mode_claiming_more_players_than_recorded() {
        let path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/battle_record_2340.xml");
        let xml = std::fs::read_to_string(&path).unwrap();
        for mode in ["VS_2_2", "VS_4_Scuffle"] {
            let crafted = xml.replace(
                "<MatchMode>VS_1_1</MatchMode>",
                &format!("<MatchMode>{mode}</MatchMode>"),
            );
            assert_ne!(crafted, xml, "the fixture names its mode");
            let record = BattleRecord::from_bytes(crafted.as_bytes(), &path).unwrap();
            assert!(record.get_battle_overview().is_ok(), "{mode}");
            assert_eq!(record.match_detail().player_games().len(), 2, "{mode}");
        }
    }

    /// A concession during the last round's deployment: that round is never fought, the player
    /// who conceded loses, and their final army is the one that fought the round before.
    #[test]
    fn test_concession_during_deployment() {
        let mut record = fixture("battle_record_2340.xml");
        let last = record.player_records.player_record[0]
            .player_round_records
            .player_round_record
            .last_mut()
            .unwrap();
        let give_up = super::super::battle_record::MatchActionData {
            xsi_type: "PAD_GiveUp".into(),
            ..Default::default()
        };
        last.action_records.match_action_data.insert(0, give_up);

        assert!(!record.last_round_fought());
        assert_eq!(record.match_results(), [Some(false), Some(true)]);
        let detail = record.match_detail();
        assert_eq!(detail.rounds.len(), 7, "fight 8 never happened");
        assert_eq!(detail.players[0].won, Some(false));
        let games = detail.player_games();
        assert!(games.iter().all(|g| g.fights.len() == 7));
        let overview = record.get_battle_overview().unwrap();
        assert_eq!(overview.losers[0].id, detail.players[0].id);
        // The final army is the one that fought fight 7: record 8's snapshot.
        assert_eq!(
            overview.losers[0].last_round_units.len() as u32,
            games[0].final_units
        );
    }

    /// From 2340, a fight the list doesn't cover is unknown. It never falls back to
    /// `preRoundFightResult`, which reads "Win" for both players.
    #[test]
    fn test_short_or_strange_result_lists_stay_unknown() {
        let mut record = fixture("battle_record_2340.xml");
        for player in &mut record.player_records.player_record {
            for round in &mut player.player_round_records.player_round_record {
                round.player_data.round_fight_result.results.pop();
            }
        }
        assert_eq!(record.match_results(), [None, None]);
        let detail = record.match_detail();
        let last = detail.rounds.last().unwrap();
        assert!(last.sides.iter().flatten().all(|s| s.result.is_empty()));
        for round in &detail.rounds {
            let wins = round
                .sides
                .iter()
                .flatten()
                .filter(|s| s.result == "Win")
                .count();
            assert!(wins <= 1, "fight {}: two winners", round.round + 1);
        }

        let mut strange = fixture("battle_record_2340.xml");
        for round in &mut strange.player_records.player_record[0]
            .player_round_records
            .player_round_record
        {
            round.player_data.round_fight_result.results[0] = "Banana".into();
        }
        let detail = strange.match_detail();
        assert_eq!(detail.rounds[0].sides[0].as_ref().unwrap().result, "");
    }

    /// Before 2340 the match is judged by the result of the fight before the last round. A last
    /// record of round 1 has none: its `preRoundFightResult` reads "Win" for both players.
    #[test]
    fn test_no_fight_to_judge_by() {
        let mut record = fixture("battle_record.xml");
        for player in &mut record.player_records.player_record {
            player
                .player_round_records
                .player_round_record
                .retain(|r| r.round.trim().parse::<u32>().is_ok_and(|n| n <= 1));
        }
        assert!(record.match_results().iter().all(Option::is_none));
    }

    /// Competition replays that keep only round 0 and one late round write "Win" for every
    /// player (four of the 1.x replays on hand: two winners each). Without the round before the
    /// last, the result is unknown.
    #[test]
    fn test_a_gap_before_the_last_round_leaves_the_result_unknown() {
        let mut record = fixture("battle_record.xml");
        let full = record.match_results();
        assert_eq!(
            full.iter().filter(|r| **r == Some(true)).count(),
            1,
            "{full:?}"
        );
        assert!(record.results_settled());
        for player in &mut record.player_records.player_record {
            let records = &mut player.player_round_records.player_round_record;
            let last = records
                .iter()
                .filter_map(|r| r.round.trim().parse::<u32>().ok())
                .max()
                .unwrap();
            records.retain(|r| {
                r.round
                    .trim()
                    .parse::<u32>()
                    .is_ok_and(|n| n == 0 || n == last)
            });
        }
        assert!(record.match_results().iter().all(Option::is_none));
        assert!(!record.results_settled());
    }

    /// The control for the last round's techs: what a round researched, added to what it began
    /// with, is what the next record holds, in every round that has a next record.
    #[test]
    fn test_techs_after_a_round_match_the_next_record() {
        use super::super::detail::{enabled_techs, techs_after_round};
        let count = |techs: &[super::super::UnitTechs]| -> usize {
            techs.iter().map(|t| t.techs.len()).sum()
        };
        let mut checked = 0;
        for name in ["battle_record_2340.xml", "battle_record_2324.xml"] {
            let record = fixture(name);
            for player in &record.player_records.player_record {
                let rounds = &player.player_round_records.player_round_record;
                for pair in rounds.windows(2) {
                    let next = enabled_techs(&pair[1].player_data.active_technologies.unit_data);
                    assert_eq!(
                        count(&techs_after_round(&pair[0])),
                        count(&next),
                        "{name} round {}",
                        pair[0].round.trim()
                    );
                    checked += 1;
                }
            }
        }
        assert!(checked > 30, "{checked}");
        // So the last fight's techs include the last round's research.
        let detail = fixture("battle_record_2340.xml").match_detail();
        let games = detail.player_games();
        let last = detail.rounds.last().unwrap().sides[0].as_ref().unwrap();
        assert_eq!(games[0].techs as usize, count(&last.enabled_techs));
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
