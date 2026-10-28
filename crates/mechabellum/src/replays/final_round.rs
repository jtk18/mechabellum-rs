//! The army that fought the last round.
//!
//! Each round record holds the army as it stood when that round's deployment began, plus the
//! actions taken during it. The army that fought round n is therefore in record n + 1. The last
//! round has no record after it, so its army is rebuilt here by replaying that round's actions
//! onto its starting snapshot.
//!
//! Replaying every earlier round and comparing with the next record reproduces every round of
//! the 13 2.0 replays on hand (184 of 184, 2,801 units) and 99.7% of units across the 473 1.x
//! replays (5,944 of 6,156 rounds exact). 1.x records no buy positions, so a purchase is placed
//! by its move; the misses are mostly cards that add units which are never moved afterwards.

use super::battle_record::{
    MatchActionData, PlayerData, PlayerDataUnitsNewUnitDataPosition, UnitEquipment,
    UnitsNewUnitData,
};
use super::game_data::xp_to_next_level;
use super::names::GameNames;

use std::collections::BTreeMap;

/// Replay one round's deployment actions onto the army it started with. A unit the replay gives
/// no position (older replays record none for a purchase that's never moved) is kept with an
/// empty one: it's in the army and its value, but can't be drawn or put in a band.
///
/// Purchases take the next free unit indices in order, but unit cards and summoning skills take
/// indices too, and the replay doesn't say how many. Every `MoveUnit` names an index and the unit
/// type there, so the count each card or summon took is chosen to agree with the most of those
/// facts (see `align_claims`). Skills gained during the round (skill cards, and blueprints named
/// after a skill) add skill slots, so a hidden skill ID released through a new slot resolves too.
pub(crate) fn replay_deployment(
    start: &PlayerData,
    actions: &[MatchActionData],
) -> Vec<UnitsNewUnitData> {
    let actions = standing_actions(actions);
    let mut units: BTreeMap<i64, UnitsNewUnitData> = start
        .units
        .new_unit_data
        .iter()
        .filter_map(|u| Some((unit_index(&u.index)?, u.clone())))
        .collect();
    let first_index = unit_index(&start.unit_index).unwrap_or(0).max(0);
    let claimed = align_claims(&actions, &units, first_index);
    let mut slots: BTreeMap<u32, String> = start
        .commander_skills
        .commander_skill_data
        .iter()
        .filter_map(|s| Some((s.index.trim().parse().ok()?, s.id.trim().to_owned())))
        .collect();

    let mut next_index = first_index;
    // Indices of this round's purchases, in order.
    let mut bought: Vec<i64> = Vec::new();
    for (position, action) in actions.iter().enumerate() {
        next_index += claimed.get(&position).copied().unwrap_or(0);
        match action.xsi_type.trim_start_matches("PAD_") {
            "ChooseReinforceItem" | "ActiveBlueprint" => {
                if let Some(skill) = skill_gained(action) {
                    let slot = slots.keys().next_back().map_or(0, |last| last + 1);
                    slots.insert(slot, skill.to_string());
                }
            }
            "BuyUnit" => {
                let Some(unit_id) = action.uid.as_deref().map(str::trim) else {
                    continue;
                };
                let position = action
                    .buy_position
                    .as_ref()
                    .map(|p| (p.x.trim(), p.y.trim()))
                    .unwrap_or(("", ""));
                // Past any index a card's unit, first seen when moved, already holds.
                while units.contains_key(&next_index) {
                    next_index += 1;
                }
                units.insert(next_index, new_unit(unit_id, next_index, position, false));
                bought.push(next_index);
                next_index += 1;
            }
            "MoveUnit" => {
                let Some(moves) = &action.move_unit_datas else {
                    continue;
                };
                for m in &moves.move_unit_data {
                    let Some(i) = unit_index(&m.unit_index) else {
                        continue;
                    };
                    let position = (m.position.x.trim(), m.position.y.trim());
                    let rotated = m.is_rotate.trim().eq_ignore_ascii_case("true");
                    match units.get_mut(&i) {
                        Some(unit) => {
                            unit.position.x = position.0.to_owned();
                            unit.position.y = position.1.to_owned();
                            unit.is_rotate = rotated.to_string();
                        }
                        None => {
                            // A purchase of this type not yet placed (older replays record no
                            // buy position) took another index than the one it's moved under:
                            // this move places it.
                            let unplaced = bought.iter().copied().find(|b| {
                                units.get(b).is_some_and(|u| {
                                    u.id.trim() == m.unit_id.trim()
                                        && u.position.x.trim().is_empty()
                                })
                            });
                            let mut unit = match unplaced.and_then(|b| units.remove(&b)) {
                                Some(unit) => {
                                    bought.retain(|b| Some(*b) != unplaced);
                                    unit
                                }
                                // A unit a card or skill added this round: first seen when moved.
                                None => new_unit(m.unit_id.trim(), i, position, rotated),
                            };
                            unit.index = i.to_string();
                            unit.position.x = position.0.to_owned();
                            unit.position.y = position.1.to_owned();
                            unit.is_rotate = rotated.to_string();
                            units.insert(i, unit);
                            next_index = next_index.max(i + 1);
                        }
                    }
                }
            }
            "UpgradeUnit" => {
                if let Some(unit) = index(&action.uidx).and_then(|i| units.get_mut(&i)) {
                    let level: u32 = unit.level.trim().parse().unwrap_or(0);
                    // Upgrading spends the XP bar: what's left over carries into the new level.
                    let exp: u32 = unit.exp.trim().parse().unwrap_or(0);
                    let cap = unit
                        .id
                        .trim()
                        .parse()
                        .ok()
                        .and_then(|id| xp_to_next_level(id, level));
                    unit.exp = exp.saturating_sub(cap.unwrap_or(exp)).to_string();
                    unit.level = level.saturating_add(1).to_string();
                }
            }
            "UseEquipment" => {
                let unit = index(&action.unit_index).and_then(|i| units.get_mut(&i));
                if let (Some(unit), Some(id)) = (unit, &action.equipment_id) {
                    unit.equipments.equipment.push(UnitEquipment {
                        data: id.trim().to_owned(),
                    });
                }
            }
            "ReleaseCommanderSkill" => {
                // Opponents' skill IDs can be hidden (0); the skill slot still says which.
                let id = action
                    .id
                    .as_deref()
                    .map(str::trim)
                    .filter(|id| !id.is_empty() && *id != "0")
                    .map(str::to_owned)
                    .or_else(|| {
                        let slot = action.skill_index.as_deref()?.trim().parse().ok()?;
                        slots.get(&slot).cloned()
                    });
                if id.as_deref().is_some_and(sells_unit) {
                    if let Some(i) = index(&action.unit_index) {
                        units.remove(&i);
                    }
                }
            }
            _ => {}
        }
    }

    units.into_values().collect()
}

/// Most unit indices one unit card can take (cards give one to four units), and one summoning
/// skill.
const MAX_CARD_UNITS: i64 = 4;
const MAX_SUMMONED: i64 = 2;
/// Cards and summons whose counts are searched; any beyond these take none. Real rounds have one
/// or two, and the search grows as 5^n.
const MAX_CLAIMS_SEARCHED: usize = 5;
/// The most actions and moves a round can have for the search to run.
const MAX_SEARCHED_ROUND: usize = 1_000;

/// How many unit indices each unit card or summoning skill in the round took, by its position in
/// `actions`: the counts that agree with the most `MoveUnit` facts (an index holding the named
/// unit type when it's moved), each disagreement costing two agreements, ties going to fewer.
fn align_claims(
    actions: &[&MatchActionData],
    start: &BTreeMap<i64, UnitsNewUnitData>,
    first_index: i64,
) -> BTreeMap<usize, i64> {
    let names = GameNames::bundled();
    let unit_cards = names.categories.get("UnitReinforceData");
    let claims: Vec<(usize, i64)> = actions
        .iter()
        .enumerate()
        .filter_map(
            |(position, action)| match action.xsi_type.trim_start_matches("PAD_") {
                "ChooseReinforceItem" => {
                    let id: u64 = action.id.as_deref()?.trim().parse().ok()?;
                    unit_cards
                        .is_some_and(|cards| cards.contains_key(&id))
                        .then_some((position, MAX_CARD_UNITS))
                }
                // A skill released on no unit can summon some.
                "ReleaseCommanderSkill" => index(&action.unit_index)
                    .is_none_or(|i| i < 0)
                    .then_some((position, MAX_SUMMONED)),
                _ => None,
            },
        )
        .take(MAX_CLAIMS_SEARCHED)
        .collect();
    if claims.is_empty() {
        return BTreeMap::new();
    }
    // (position, index, unit type) for every unit moved.
    let facts: Vec<(usize, i64, &str)> = actions
        .iter()
        .enumerate()
        .filter_map(|(position, action)| Some((position, action.move_unit_datas.as_ref()?)))
        .flat_map(|(position, moves)| {
            moves
                .move_unit_data
                .iter()
                .filter_map(move |m| Some((position, unit_index(&m.unit_index)?, m.unit_id.trim())))
        })
        .collect();
    // Real rounds have at most 44 actions and 90 moves (8,199 rounds on hand). The search below
    // tries up to 5^5 combinations, each over every action and move, so a crafted round past
    // this gets none and its purchases simply skip taken indices.
    if actions.len() + facts.len() > MAX_SEARCHED_ROUND {
        return BTreeMap::new();
    }

    let mut counts = vec![0i64; claims.len()];
    let mut best: Option<((i64, i64), Vec<i64>)> = None;
    loop {
        // Who holds each index under these counts: a purchase (its unit type and position) or a
        // card's or summon's claim (its position).
        let mut owners: BTreeMap<i64, (usize, Option<&str>)> = BTreeMap::new();
        let mut next_index = first_index;
        for (position, action) in actions.iter().enumerate() {
            if let Some(n) = claims.iter().position(|(p, _)| *p == position) {
                for _ in 0..counts[n] {
                    owners.insert(next_index, (position, None));
                    next_index += 1;
                }
            }
            if action.xsi_type.ends_with("BuyUnit") {
                owners.insert(next_index, (position, action.uid.as_deref().map(str::trim)));
                next_index += 1;
            }
        }
        let score: i64 = facts
            .iter()
            .map(|&(position, i, unit_id)| {
                let agrees = match (start.get(&i), owners.get(&i)) {
                    (Some(unit), _) if unit.id.trim() == unit_id => true,
                    (_, Some(&(owner, bought))) => {
                        owner < position && bought.is_none_or(|bought| bought == unit_id)
                    }
                    _ => false,
                };
                if agrees {
                    1
                } else {
                    -2
                }
            })
            .sum();
        let key = (score, -counts.iter().sum::<i64>());
        if best.as_ref().is_none_or(|(b, _)| key > *b) {
            best = Some((key, counts.clone()));
        }
        // The next combination of counts, odometer style.
        let mut digit = 0;
        loop {
            if digit == counts.len() {
                let (_, counts) = best.unwrap_or_default();
                return claims.iter().map(|(p, _)| *p).zip(counts).collect();
            }
            if counts[digit] < claims[digit].1 {
                counts[digit] += 1;
                break;
            }
            counts[digit] = 0;
            digit += 1;
        }
    }
}

/// The skill a card pick or blueprint adds to the player's skill slots, if it adds one: a skill
/// card, or a blueprint named after a skill.
fn skill_gained(action: &MatchActionData) -> Option<u64> {
    let names = GameNames::bundled();
    let id: u64 = action.id.as_deref()?.trim().parse().ok()?;
    let skills = || {
        names
            .categories
            .iter()
            .filter(|(category, _)| category.starts_with("CSD_"))
            .flat_map(|(_, skills)| skills.iter())
    };
    if action.xsi_type.ends_with("ChooseReinforceItem") {
        return skills().any(|(skill, _)| *skill == id).then_some(id);
    }
    let blueprint = names.categories.get("BlueprintData")?.get(&id)?;
    skills()
        .filter(|(_, name)| *name == blueprint)
        .map(|(skill, _)| *skill)
        .min()
}

/// The round's actions still standing after undos. Each `PAD_Undo` takes back the latest action
/// still standing, of any kind, as `replay_deployment` does.
pub(crate) fn standing_actions(actions: &[MatchActionData]) -> Vec<&MatchActionData> {
    let mut standing = Vec::new();
    for action in actions {
        match action.xsi_type.trim_start_matches("PAD_") {
            "Undo" => {
                standing.pop();
            }
            "FinishDeploy" => {}
            _ => standing.push(action),
        }
    }
    standing
}

/// Techs researched during a round, as (raw unit ID, tech ID). Checked against the next record
/// in every round that has one: 170 of 170 rounds over the 13 2.0 replays on hand (2026-10-27).
pub(crate) fn techs_bought(actions: &[MatchActionData]) -> Vec<(&str, &str)> {
    standing_actions(actions)
        .into_iter()
        .filter(|a| a.xsi_type.ends_with("UpgradeTechnology"))
        .filter_map(|a| Some((a.uid.as_deref()?.trim(), a.tech_id.as_deref()?.trim())))
        .collect()
}

/// Unit upgrades bought during a round. Levels units arrive with (from cards or techs) aren't
/// upgrades and aren't counted.
pub(crate) fn upgrades_bought(actions: &[MatchActionData]) -> u32 {
    standing_actions(actions)
        .into_iter()
        .filter(|a| a.xsi_type.ends_with("UpgradeUnit"))
        .count() as u32
}

fn new_unit(id: &str, index: i64, (x, y): (&str, &str), rotated: bool) -> UnitsNewUnitData {
    UnitsNewUnitData {
        id: id.to_owned(),
        index: index.to_string(),
        level: "0".into(),
        exp: "0".into(),
        position: PlayerDataUnitsNewUnitDataPosition {
            text: None,
            x: x.to_owned(),
            y: y.to_owned(),
        },
        equipment_id: "0".into(),
        is_rotate: rotated.to_string(),
        ..Default::default()
    }
}

/// Unit indices count up from 0 over a match, so real ones stay in the hundreds. One far outside
/// this is crafted and ignored, which keeps the index arithmetic in range. Negative ones are
/// kept: a skill released on no unit names -1.
const INDEX_LIMIT: i64 = 1_000_000;

fn unit_index(value: &str) -> Option<i64> {
    value
        .trim()
        .parse()
        .ok()
        .filter(|i| (-INDEX_LIMIT..=INDEX_LIMIT).contains(i))
}

fn index(value: &Option<String>) -> Option<i64> {
    unit_index(value.as_deref()?)
}

/// Commander skills that sell (recycle) the unit they target.
fn sells_unit(id: &str) -> bool {
    let Ok(id) = id.parse::<u64>() else {
        return false;
    };
    GameNames::bundled()
        .categories
        .get("CSD_SellUnit")
        .is_some_and(|skills| skills.contains_key(&id))
}

#[cfg(test)]
mod tests {
    use super::super::battle_record::BattleRecord;
    use super::*;
    use std::path::PathBuf;

    /// Positions and types by index, the parts the board draws.
    fn layout(units: &[UnitsNewUnitData]) -> BTreeMap<String, (String, String, String, bool)> {
        units
            .iter()
            .map(|u| {
                (
                    u.index.trim().to_owned(),
                    (
                        u.id.trim().to_owned(),
                        u.position.x.trim().to_owned(),
                        u.position.y.trim().to_owned(),
                        u.is_rotate.trim().eq_ignore_ascii_case("true"),
                    ),
                )
            })
            .collect()
    }

    /// The control: replaying round n's actions onto round n's snapshot must give round n + 1's
    /// snapshot. Every earlier round has that answer on record.
    #[test]
    fn test_replaying_a_round_reproduces_the_next_snapshot() {
        let path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/battle_record_2324.xml");
        let record = BattleRecord::new_from_file(path, false).unwrap();
        let (mut checked, mut exact) = (0, 0);
        for player in &record.player_records.player_record {
            let rounds = &player.player_round_records.player_round_record;
            for pair in rounds.windows(2).skip(1) {
                let got = replay_deployment(
                    &pair[0].player_data,
                    &pair[0].action_records.match_action_data,
                );
                checked += 1;
                if layout(&got) == layout(&pair[1].player_data.units.new_unit_data) {
                    exact += 1;
                }
            }
        }
        assert!(checked > 0);
        assert_eq!(exact, checked, "{exact} of {checked} rounds reproduced");
    }

    /// The same control over a folder of replays, reporting the share of units reproduced:
    /// `MECHABELLUM_REPLAYS=<dir> cargo test -p mechabellum -- --ignored replay_folder --nocapture`
    #[test]
    #[ignore]
    fn test_replay_folder() {
        let dir = std::env::var("MECHABELLUM_REPLAYS").expect("MECHABELLUM_REPLAYS");
        let (mut units, mut wrong, mut rounds, mut exact) = (0usize, 0usize, 0, 0);
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|e| e != "xml") {
                continue;
            }
            let Ok(record) = BattleRecord::new_from_file(path, false) else {
                continue;
            };
            for player in &record.player_records.player_record {
                let records = &player.player_round_records.player_round_record;
                for pair in records.windows(2).skip(1) {
                    let got = layout(&replay_deployment(
                        &pair[0].player_data,
                        &pair[0].action_records.match_action_data,
                    ));
                    let want = layout(&pair[1].player_data.units.new_unit_data);
                    rounds += 1;
                    exact += usize::from(got == want);
                    units += want.len();
                    wrong += want
                        .keys()
                        .chain(got.keys().filter(|k| !want.contains_key(*k)))
                        .filter(|k| got.get(*k) != want.get(*k))
                        .count();
                }
            }
        }
        println!(
            "rounds {exact}/{rounds} exact; units wrong {wrong}/{units} ({:.1}%)",
            100.0 * wrong as f64 / units.max(1) as f64
        );
    }
}
