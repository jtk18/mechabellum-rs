//! The army that fought the last round.
//!
//! Each round record holds the army as it stood when that round's deployment began, plus the
//! actions taken during it. The army that fought round n is therefore in record n + 1. The last
//! round has no record after it, so its army is rebuilt here by replaying that round's actions
//! onto its starting snapshot.
//!
//! Replaying every earlier round and comparing with the next record reproduces 99.2% of units
//! exactly across 473 replays (versions 1425-2324). The misses: units sold by a commander skill
//! whose ID the replay hides, cards that add units which are never moved afterwards, and older
//! replays' purchases that are never moved (those record no position).

use super::battle_record::{
    MatchActionData, PlayerData, PlayerDataUnitsNewUnitDataPosition, UnitEquipment,
    UnitsNewUnitData,
};
use super::game_data::xp_to_next_level;
use super::names::GameNames;

use std::collections::BTreeMap;

/// Replay one round's deployment actions onto the army it started with. Units whose position
/// is unknown are left out.
pub(crate) fn replay_deployment(
    start: &PlayerData,
    actions: &[MatchActionData],
) -> Vec<UnitsNewUnitData> {
    let mut deploy = Deployment {
        units: start
            .units
            .new_unit_data
            .iter()
            .filter_map(|u| Some((u.index.trim().parse().ok()?, u.clone())))
            .collect(),
        next_index: start.unit_index.trim().parse().unwrap_or(0),
        bought: Vec::new(),
    };
    let skills: BTreeMap<&str, &str> = start
        .commander_skills
        .commander_skill_data
        .iter()
        .map(|s| (s.index.trim(), s.id.trim()))
        .collect();
    let mut history: Vec<Deployment> = Vec::new();

    for action in actions {
        let kind = action.xsi_type.trim_start_matches("PAD_");
        match kind {
            "Undo" => {
                if let Some(previous) = history.pop() {
                    deploy = previous;
                }
                continue;
            }
            "FinishDeploy" => continue,
            _ => history.push(deploy.clone()),
        }
        match kind {
            "BuyUnit" => deploy.buy(action),
            "MoveUnit" => deploy.moves(action),
            "UpgradeUnit" => {
                if let Some(unit) = index(&action.uidx).and_then(|i| deploy.units.get_mut(&i)) {
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
                    unit.level = (level + 1).to_string();
                }
            }
            "UseEquipment" => {
                let unit = index(&action.unit_index).and_then(|i| deploy.units.get_mut(&i));
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
                    .or_else(|| {
                        skills
                            .get(action.skill_index.as_deref().unwrap_or("").trim())
                            .copied()
                    });
                if id.is_some_and(sells_unit) {
                    if let Some(i) = index(&action.unit_index) {
                        deploy.units.remove(&i);
                    }
                }
            }
            _ => {}
        }
    }

    deploy
        .units
        .into_values()
        .filter(|u| !u.position.x.trim().is_empty() && !u.position.y.trim().is_empty())
        .collect()
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

#[derive(Clone)]
struct Deployment {
    units: BTreeMap<i64, UnitsNewUnitData>,
    next_index: i64,
    /// Indices of units bought this round, in order.
    bought: Vec<i64>,
}

impl Deployment {
    fn buy(&mut self, action: &MatchActionData) {
        let Some(unit_id) = action.uid.as_deref().map(str::trim) else {
            return;
        };
        let position = action
            .buy_position
            .as_ref()
            .map(|p| (p.x.trim(), p.y.trim()))
            .unwrap_or(("", ""));
        self.units.insert(
            self.next_index,
            new_unit(unit_id, self.next_index, position, false),
        );
        self.bought.push(self.next_index);
        self.next_index += 1;
    }

    fn moves(&mut self, action: &MatchActionData) {
        let Some(moves) = &action.move_unit_datas else {
            return;
        };
        for m in &moves.move_unit_data {
            let Ok(i) = m.unit_index.trim().parse::<i64>() else {
                continue;
            };
            let unit_id = m.unit_id.trim();
            let position = (m.position.x.trim(), m.position.y.trim());
            let rotated = m.is_rotate.trim().eq_ignore_ascii_case("true");
            match self.units.get_mut(&i) {
                // A card's unit holds this index, not the purchase filed under it: move this
                // round's purchases from i up by one.
                Some(unit) if unit.id.trim() != unit_id && self.bought.contains(&i) => {
                    for b in self.bought.iter_mut().rev().filter(|b| **b >= i) {
                        if let Some(bought) = self.units.remove(b) {
                            let mut bought = bought;
                            bought.index = (*b + 1).to_string();
                            self.units.insert(*b + 1, bought);
                        }
                        *b += 1;
                    }
                    self.next_index += 1;
                    self.units
                        .insert(i, new_unit(unit_id, i, position, rotated));
                }
                Some(unit) => {
                    unit.position.x = position.0.to_owned();
                    unit.position.y = position.1.to_owned();
                    unit.is_rotate = rotated.to_string();
                }
                // A unit this round added some other way (a card): first seen when moved.
                None => {
                    self.units
                        .insert(i, new_unit(unit_id, i, position, rotated));
                    self.next_index = self.next_index.max(i + 1);
                }
            }
        }
    }
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

fn index(value: &Option<String>) -> Option<i64> {
    value.as_deref()?.trim().parse().ok()
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
