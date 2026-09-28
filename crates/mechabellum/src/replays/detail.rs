//! Everything needed to redraw one match: each round's layout for both sides, with levels, XP,
//! equipment, and techs.
//!
//! A player's round record for round `r` holds the army exactly as it fought in round `r - 1`
//! (it matches that fight's report position for position), along with that fight's result and
//! the techs researched by then.

use super::battle_record::{
    ActiveTechnologiesUnitData, PlayerRecord, PlayerRoundRecord, UnitsNewUnitData,
};
use super::final_round::replay_deployment;
use super::game_data::{footprint, xp_to_next_level};
use super::names::{resolve, GameNames, NameKind};
use super::{BattleRecord, Item, Unit};

use serde::Serialize;

use std::collections::BTreeMap;
use std::str::FromStr;

#[derive(Debug, Serialize)]
pub struct MatchDetail {
    pub battle_id: String,
    pub version: u32,
    pub map_id: String,
    pub map_name: Option<String>,
    pub match_mode: String,
    pub game_mode: String,
    pub players: Vec<PlayerInfo>,
    /// Ascending by round.
    pub rounds: Vec<RoundLayout>,
}

#[derive(Debug, Serialize)]
pub struct PlayerInfo {
    pub id: String,
    pub name: String,
    pub team: String,
    /// Whether this player won the match, when the replay says.
    pub won: Option<bool>,
    /// Techs the player brought for each unit (their Unit Modification loadout).
    pub loadout: Vec<UnitTechs>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UnitTechs {
    pub unit_id: u32,
    pub unit_name: String,
    pub techs: Vec<TechRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TechRef {
    pub id: String,
    pub name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RoundLayout {
    pub round: u32,
    /// One per player, in `MatchDetail::players` order; `None` if that player has no record for
    /// this round.
    pub sides: Vec<Option<SideLayout>>,
}

/// A card offered between rounds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CardRef {
    pub id: String,
    pub name: Option<String>,
    /// "specialist", "unit", "skill", "equipment", or "other".
    pub kind: &'static str,
    /// For unit reinforcements and unit specialists: the unit it's about.
    pub unit_id: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct NamedRef {
    pub id: String,
    pub name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Structure {
    /// Strengthen level, when recorded.
    pub level: Option<i32>,
    /// Command Center: active skills. Research Center: blueprints.
    pub items: Vec<NamedRef>,
}

#[derive(Debug, Serialize)]
pub struct SideLayout {
    /// "Win", "Lose" or "Deuce" for this round's fight. For the last round, the match result.
    pub result: String,
    /// Reactor core HP going into the fight.
    pub hp: i32,
    pub units: Vec<PlacedUnit>,
    /// Techs researched by this round, per unit type.
    pub enabled_techs: Vec<UnitTechs>,
    pub specialists: Vec<String>,
    /// Cards offered before this round's fight (2.0+), and the ones picked. In modes where
    /// players choose from one shared set, every player gets the same offers.
    pub cards_offered: Vec<CardRef>,
    pub cards_chosen: Vec<CardRef>,
    pub command_center: Structure,
    pub research_center: Structure,
    /// Buildings standing for the fight (walls, cannons, barricades). For the last round,
    /// those standing as its deployment began: ones built during it aren't recorded.
    pub buildings: Vec<PlacedBuilding>,
    /// The units were rebuilt by replaying the round's actions (the last round, which has no
    /// snapshot after it) rather than read from a snapshot.
    pub rebuilt: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlacedBuilding {
    /// A `ConstructionData` ID.
    pub id: u32,
    pub name: Option<String>,
    /// Center, in replay units (10 per grid square).
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Serialize)]
pub struct PlacedUnit {
    /// Base unit ID (AsyncBattle copies folded into the original).
    pub unit_id: u32,
    pub name: String,
    pub index: u32,
    /// 0-based, as stored in replays.
    pub level: u32,
    /// Progress within the current level.
    pub exp: u32,
    /// XP needed to leave this level, when known.
    pub xp_to_next: Option<u32>,
    pub x: i32,
    pub y: i32,
    pub rotated: bool,
    /// Grid squares (width, depth) before rotation, when known.
    pub footprint: Option<(u8, u8)>,
    pub equipment: Vec<NamedRef>,
}

fn base_unit_id(raw: &str) -> Option<u32> {
    let id: u32 = raw.trim().parse().ok()?;
    Some(match id {
        5001..=5099 => id - 5000,
        5201 => 2001,
        5202 => 2002,
        5203 => 4001,
        _ => id,
    })
}

fn named(kind: NameKind, id: &str) -> NamedRef {
    NamedRef {
        id: id.to_owned(),
        name: resolve(kind, id).map(str::to_owned),
    }
}

/// The unit a card or specialist is named after, e.g. "Subsidized Mustang", "Rhino Assault",
/// or "Titan：Abyss". The longest unit name wins, so "Experimental Death Knell" beats "Death Knell".
pub fn unit_id_in_name(name: &str) -> Option<u32> {
    if let Some(id) = unit_id_by_name(name) {
        return Some(id);
    }
    let lower = name.to_lowercase();
    let units = GameNames::bundled().categories.get("MechData")?;
    units
        .iter()
        // Originals only (1-99, giants 2001-4999); skip AsyncBattle copies and enemy-only units.
        .filter(|(id, _)| (1..100).contains(*id) || (2001..5000).contains(*id))
        .filter(|(_, unit)| {
            let unit = unit.to_lowercase();
            // Match whole words: "Fang" must not match "Fanged".
            lower.match_indices(unit.as_str()).any(|(at, m)| {
                let before = lower[..at].chars().last();
                let after = lower[at + m.len()..].chars().next();
                !before.is_some_and(char::is_alphanumeric)
                    && !after.is_some_and(char::is_alphanumeric)
            })
        })
        .max_by_key(|(id, unit)| (unit.len(), std::cmp::Reverse(**id)))
        .map(|(id, _)| *id as u32)
}

/// Unit ID for a unit's display name (the lowest, so AsyncBattle copies map to the original).
/// Card names can carry a prefix, as in "Titan：Abyss".
fn unit_id_by_name(name: &str) -> Option<u32> {
    let units = GameNames::bundled().categories.get("MechData")?;
    let lookup = |wanted: &str| {
        units
            .iter()
            .filter(|(_, n)| n.as_str() == wanted)
            .map(|(id, _)| *id as u32)
            .min()
    };
    lookup(name).or_else(|| {
        let bare = name.rsplit([':', '：']).next()?.trim();
        (bare != name).then(|| lookup(bare)).flatten()
    })
}

pub(crate) fn card_ref(id: &str) -> CardRef {
    let names = GameNames::bundled();
    let numeric: Option<u64> = id.trim().parse().ok();
    let category = numeric.and_then(|n| names.category_of(NameKind::Card, n));
    let name = numeric
        .and_then(|n| names.resolve(NameKind::Card, n))
        .map(str::to_owned);
    let kind = match category {
        Some("OfficerData") => "specialist",
        Some("UnitReinforceData") => "unit",
        Some(c) if c.starts_with("CSD_") => "skill",
        Some(c) if c.ends_with("EquipmentData") => "equipment",
        _ => "other",
    };
    let unit_id = match kind {
        "unit" | "specialist" | "skill" => name.as_deref().and_then(unit_id_in_name),
        _ => None,
    };
    CardRef {
        id: id.to_owned(),
        name,
        kind,
        unit_id,
    }
}

fn tech_ref(id: &str) -> TechRef {
    TechRef {
        id: id.to_owned(),
        name: resolve(NameKind::Tech, id).map(str::to_owned),
    }
}

fn unit_name(raw_id: &str) -> String {
    Unit::from_replay_id(raw_id).display_name()
}

fn placed_unit(data: &UnitsNewUnitData) -> Option<PlacedUnit> {
    let unit_id = base_unit_id(&data.id)?;
    let level = data.level.trim().parse().unwrap_or(0);
    Some(PlacedUnit {
        unit_id,
        name: unit_name(&data.id),
        index: data.index.trim().parse().unwrap_or(0),
        level,
        exp: data.exp.trim().parse().unwrap_or(0),
        // Star Expedition copies level at their own rates, so look up the raw ID.
        xp_to_next: xp_to_next_level(data.id.trim().parse().unwrap_or(unit_id), level),
        x: data.position.x.trim().parse().unwrap_or(0),
        y: data.position.y.trim().parse().unwrap_or(0),
        rotated: data.is_rotate.trim().eq_ignore_ascii_case("true"),
        footprint: footprint(unit_id),
        equipment: data
            .equipments
            .ids(&data.equipment_id)
            .map(|id| NamedRef {
                id: id.to_owned(),
                name: Some(Item::from_str(id).map_or_else(|_| id.to_owned(), |i| i.display_name())),
            })
            .collect(),
    })
}

pub(crate) fn enabled_techs(units: &[ActiveTechnologiesUnitData]) -> Vec<UnitTechs> {
    units
        .iter()
        .filter_map(|u| {
            let techs: Vec<TechRef> = u
                .techs
                .iter()
                .flat_map(|t| &t.tech)
                .map(|t| tech_ref(&t.data))
                .collect();
            if techs.is_empty() {
                return None;
            }
            Some(UnitTechs {
                unit_id: base_unit_id(&u.id)?,
                unit_name: unit_name(&u.id),
                techs,
            })
        })
        .collect()
}

fn loadout(player: &PlayerRecord) -> Vec<UnitTechs> {
    player
        .data
        .unit_datas
        .unit_data
        .iter()
        .filter_map(|u| {
            Some(UnitTechs {
                unit_id: base_unit_id(&u.id)?,
                unit_name: unit_name(&u.id),
                techs: u
                    .techs
                    .iter()
                    .flat_map(|t| &t.tech)
                    .map(|t| tech_ref(&t.data))
                    .collect(),
            })
        })
        .collect()
}

impl BattleRecord {
    pub fn match_detail(&self) -> MatchDetail {
        let players: Vec<&PlayerRecord> = self.player_records.player_record.iter().collect();
        let mut rounds: BTreeMap<u32, Vec<Option<SideLayout>>> = BTreeMap::new();
        let version: u32 = self.version.trim().parse().unwrap_or(0);

        // Cards offered in round r's preparation, per player (the fight of round r).
        let offers: BTreeMap<u32, &Vec<super::battle_record::IntList>> = self
            .match_datas
            .match_snapshot_data
            .iter()
            .filter_map(|s| Some((s.round.trim().parse().ok()?, &s.reinforce_items.per_player)))
            .collect();
        // Cards picked during each player's round r preparation.
        let picks: Vec<BTreeMap<u32, Vec<String>>> = players
            .iter()
            .map(|p| {
                p.player_round_records
                    .player_round_record
                    .iter()
                    .filter_map(|r| {
                        let round: u32 = r.round.trim().parse().ok()?;
                        let ids = r
                            .action_records
                            .match_action_data
                            .iter()
                            .filter(|a| a.xsi_type.ends_with("ChooseReinforceItem"))
                            .filter_map(|a| a.id.clone())
                            // 0 marks a skipped pick.
                            .filter(|id| id.trim() != "0")
                            .collect();
                        Some((round, ids))
                    })
                    .collect()
            })
            .collect();

        // Record n holds the army as round n's deployment began. So the army that fought round
        // n is in record n + 1, and the last round's is rebuilt from its record and actions.
        for (i, player) in players.iter().enumerate() {
            let records: BTreeMap<u32, &PlayerRoundRecord> = player
                .player_round_records
                .player_round_record
                .iter()
                .filter_map(|r| Some((r.round.trim().parse().ok()?, r)))
                .collect();
            let Some(&last) = records.keys().next_back() else {
                continue;
            };
            let won = player
                .player_round_records
                .player_round_record
                .last()
                .map(|r| r.player_data.pre_round_fight_result == "Win");
            for fight in 1..=last {
                let Some(&entering) = records.get(&fight) else {
                    continue;
                };
                let (after, units, result, rebuilt) = match records.get(&(fight + 1)) {
                    Some(&next) => (
                        next,
                        next.player_data.units.new_unit_data.clone(),
                        next.player_data.pre_round_fight_result.clone(),
                        false,
                    ),
                    None => (
                        entering,
                        replay_deployment(
                            &entering.player_data,
                            &entering.action_records.match_action_data,
                        ),
                        match won {
                            Some(true) => "Win".into(),
                            Some(false) => "Lose".into(),
                            None => String::new(),
                        },
                        true,
                    ),
                };
                if units.is_empty() {
                    continue;
                }
                let data = &after.player_data;
                let levels: Vec<i32> = data
                    .tower_strengthen_levels
                    .int
                    .iter()
                    .filter_map(|l| l.trim().parse().ok())
                    .collect();
                let side = SideLayout {
                    cards_offered: offers
                        .get(&fight)
                        .and_then(|lists| match lists.len() {
                            // One shared list: every player picks from the same cards.
                            1 => lists.first(),
                            _ => lists.get(i),
                        })
                        .map(|list| list.int.iter().map(|id| card_ref(id)).collect())
                        .unwrap_or_default(),
                    cards_chosen: picks[i]
                        .get(&fight)
                        .map(|ids| ids.iter().map(|id| card_ref(id)).collect())
                        .unwrap_or_default(),
                    // Index 0 is taken to be the Command Center and 1 the Research Center.
                    command_center: Structure {
                        level: levels.first().copied(),
                        items: data
                            .energy_tower_skills
                            .int
                            .iter()
                            .map(|id| named(NameKind::TowerSkill, id))
                            .collect(),
                    },
                    research_center: Structure {
                        level: levels.get(1).copied(),
                        items: data
                            .bluepints
                            .int
                            .iter()
                            .map(|id| named(NameKind::Blueprint, id))
                            .collect(),
                    },
                    result,
                    // Going into the fight.
                    hp: entering
                        .player_data
                        .reactor_core
                        .trim()
                        .parse()
                        .unwrap_or(0),
                    units: units.iter().filter_map(placed_unit).collect(),
                    buildings: data
                        .construction_snapshot_datas
                        .construction_snapshot_data
                        .iter()
                        .filter_map(|b| {
                            Some(PlacedBuilding {
                                id: b.id.trim().parse().ok()?,
                                name: resolve(NameKind::Construction, b.id.trim())
                                    .map(str::to_owned),
                                x: b.position.x.trim().parse().ok()?,
                                y: b.position.y.trim().parse().ok()?,
                            })
                        })
                        .collect(),
                    enabled_techs: enabled_techs(&data.active_technologies.unit_data),
                    specialists: data
                        .officers
                        .int
                        .iter()
                        .map(|id| super::specialist::specialist_name(id, version))
                        .collect(),
                    rebuilt,
                };
                let slot = rounds
                    .entry(fight - 1)
                    .or_insert_with(|| (0..players.len()).map(|_| None).collect());
                slot[i] = Some(side);
            }
        }

        MatchDetail {
            battle_id: self.battle_info.battle_id.clone(),
            version: self.version.trim().parse().unwrap_or(0),
            map_id: self.battle_info.map_id.clone(),
            map_name: resolve(NameKind::Map, &self.battle_info.map_id).map(str::to_owned),
            match_mode: self.battle_info.match_mode.clone(),
            game_mode: self.battle_info.game_mode.clone(),
            players: players
                .iter()
                .map(|p| PlayerInfo {
                    id: p.id.clone(),
                    name: p.name.clone(),
                    team: p.data.team.clone(),
                    won: p
                        .player_round_records
                        .player_round_record
                        .last()
                        .map(|r| r.player_data.pre_round_fight_result == "Win"),
                    loadout: loadout(p),
                })
                .collect(),
            rounds: rounds
                .into_iter()
                .map(|(round, sides)| RoundLayout { round, sides })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_match_detail_2324() {
        let path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/battle_record_2324.xml");
        let detail = BattleRecord::new_from_file(path, false)
            .unwrap()
            .match_detail();
        assert_eq!(detail.version, 2324);
        assert_eq!(detail.players.len(), 2);
        assert!(detail.map_name.is_some());
        assert!(detail.players.iter().all(|p| !p.loadout.is_empty()));

        // This fixture has no record 1, so fights 2..=9 (round indices 1..=8), each with both
        // sides. Fight n shows record n + 1's army: fight 2 is record 3 (10 units for side 0).
        let rounds: Vec<u32> = detail.rounds.iter().map(|r| r.round).collect();
        assert_eq!(rounds, (1..=8).collect::<Vec<_>>());
        let first = detail.rounds[0].sides[0].as_ref().unwrap();
        assert_eq!(first.units.len(), 10);
        assert!(!first.rebuilt);
        // The last fight has no record after it; its armies are rebuilt from record 9's
        // snapshot (23 and 25 units) and that round's actions.
        let last = detail.rounds.last().unwrap();
        assert!(last.sides.iter().flatten().all(|s| s.rebuilt));
        assert!(last.sides[0].as_ref().unwrap().units.len() >= 23);
        assert!(last.sides[1].as_ref().unwrap().units.len() >= 25);
        let (a, b) = (
            last.sides[0].as_ref().unwrap(),
            last.sides[1].as_ref().unwrap(),
        );
        // Shared frame: the two armies deploy on opposite halves.
        let max_a = a.units.iter().map(|u| u.y).max().unwrap();
        let min_b = b.units.iter().map(|u| u.y).min().unwrap();
        assert!(max_a < 200 && min_b > 0, "{max_a} {min_b}");
        assert!(
            a.units.iter().all(|u| u.footprint.is_some()),
            "every 2.0 unit has a footprint"
        );
        assert!(a
            .enabled_techs
            .iter()
            .flat_map(|u| &u.techs)
            .all(|t| t.name.is_some()));
        assert_ne!(a.result, b.result, "one side wins each fight");

        // 2.0 replays record the cards offered each round and which were picked.
        let offered: usize = detail
            .rounds
            .iter()
            .flat_map(|r| r.sides.iter().flatten())
            .map(|s| s.cards_offered.len())
            .sum();
        assert!(offered > 0);
        let chosen: Vec<&CardRef> = detail
            .rounds
            .iter()
            .flat_map(|r| r.sides.iter().flatten())
            .flat_map(|s| &s.cards_chosen)
            .collect();
        assert!(!chosen.is_empty());
        assert!(chosen.iter().all(|c| c.name.is_some()), "{chosen:?}");
        assert!(chosen
            .iter()
            .any(|c| c.kind == "unit" && c.unit_id.is_some()));
        assert_eq!(unit_id_by_name("Titan：Abyss"), Some(29));
        assert_eq!(unit_id_in_name("Subsidized Mustang"), Some(7));
        assert_eq!(unit_id_in_name("Rhino Assault"), Some(5));
        assert_eq!(unit_id_in_name("Vanguard-Melting Point"), Some(4));
        assert_eq!(unit_id_in_name("Experimental Death Knell"), Some(4001));
        assert_eq!(unit_id_in_name("Supply Enhancement"), None);
    }
}
