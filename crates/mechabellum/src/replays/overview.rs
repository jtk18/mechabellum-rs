use super::{BattleRecord, GameMode};

use super::{Item, MapPoint, PlayerName, PlayerSummary, Skills, Specialist, Unit, UnitInstance};

use anyhow::Result;

use std::str::FromStr;

#[derive(Debug)]
pub struct BattleOverview {
    /// Unique per match, e.g. `20260924--67113734`.
    pub battle_id: String,
    /// Game version (build number) the replay was recorded with, e.g. 2324.
    pub version: u32,
    pub map_id: String,
    /// RFC 3339. Uses the recorded start time when present (newer versions), else the date in the
    /// battle ID, else the file creation time.
    pub occurred_on: String,
    pub mode: GameMode,
    pub rounds: usize,
    pub winners: Vec<PlayerSummary>,
    pub losers: Vec<PlayerSummary>,
}

impl BattleRecord {
    pub fn get_battle_overview(&self) -> Result<BattleOverview> {
        let mut winners = Vec::new();
        let mut losers = Vec::new();

        let mut last_round: usize = 0;
        let mut need_last_round_set = true;

        for (i, player) in self.player_records.player_record.iter().enumerate() {
            let name: PlayerName = player.name.clone().into();
            let rounds = &player.player_round_records.player_round_record;
            if rounds.len() == 0 {
                // bot likely, skip for now
                continue;
            }
            let last_round_hp: i32 = rounds[rounds.len() - 1]
                .player_data
                .reactor_core
                .parse()
                .unwrap_or(0);
            let Some(last_round_snapshot) = self.match_datas.match_snapshot_data.last() else {
                continue;
            };
            let last_round_result = rounds[rounds.len() - 1]
                .player_data
                .pre_round_fight_result
                .as_str();

            let unlocked_units: Vec<Unit> = rounds[rounds.len() - 1]
                .player_data
                .shop
                .unlocked_units
                .int
                .iter()
                .map(|unit_id| Unit::from_replay_id(unit_id))
                .collect();

            let specialist_cards: Vec<Specialist> = rounds[rounds.len() - 1]
                .player_data
                .officers
                .int
                .iter()
                .filter_map(|specialist_id| Specialist::from_str(specialist_id).ok())
                .collect();

            let skills: Vec<Skills> = rounds[rounds.len() - 1]
                .player_data
                .commander_skills
                .commander_skill_data
                .iter()
                .filter_map(|skill_data| Skills::from_str(&skill_data.id).ok())
                .collect();

            if need_last_round_set {
                last_round = last_round_snapshot.round.parse().unwrap_or(0);
                need_last_round_set = false;
            }

            if let Some(report) = last_round_snapshot
                .last_fight_result
                .as_ref()
                .and_then(|result| result.reports.fight_report.get(i))
            {
                let score: usize = report.score.parse().unwrap_or(0);

                let last_round_units: Vec<UnitInstance> = report
                    .unit_datas
                    .new_unit_data
                    .iter()
                    .map(|data| UnitInstance {
                        unit_type: Unit::from_replay_id(&data.id),
                        xp: data.exp.parse().unwrap_or(0),
                        position: MapPoint(
                            data.position.x.parse().unwrap_or(0),
                            data.position.y.parse().unwrap_or(0),
                        ),
                        equipment: data
                            .equipments
                            .ids(&data.equipment_id)
                            .filter_map(|id| Item::from_str(id).ok())
                            .collect(),
                    })
                    .collect();
                let summary = PlayerSummary {
                    id: player.id.clone(),
                    name,
                    last_round_hp,
                    score,
                    last_round_units,
                    unlocked_units,
                    starting_specialists: rounds
                        .iter()
                        .map(|r| &r.player_data.officers.int)
                        .find(|ids| !ids.is_empty())
                        .map(|ids| {
                            ids.iter()
                                .map(|id| {
                                    super::specialist::specialist_name(
                                        id,
                                        self.version.trim().parse().unwrap_or(0),
                                    )
                                })
                                .collect()
                        })
                        .unwrap_or_default(),
                    specialist_names: rounds[rounds.len() - 1]
                        .player_data
                        .officers
                        .int
                        .iter()
                        .map(|id| {
                            super::specialist::specialist_name(
                                id,
                                self.version.trim().parse().unwrap_or(0),
                            )
                        })
                        .collect(),
                    specialist_cards,
                    skills,
                    picked_cards: rounds
                        .iter()
                        .flat_map(|r| &r.action_records.match_action_data)
                        .filter(|a| a.xsi_type.ends_with("ChooseReinforceItem"))
                        .filter_map(|a| a.id.as_deref())
                        .filter(|id| id.trim() != "0")
                        .map(super::detail::card_ref)
                        .collect(),
                    enabled_techs: super::detail::enabled_techs(
                        &rounds[rounds.len() - 1]
                            .player_data
                            .active_technologies
                            .unit_data,
                    ),
                };
                if last_round_result == "Win" {
                    winners.push(summary);
                } else {
                    losers.push(summary);
                }
            }
        }

        Ok(BattleOverview {
            battle_id: self.battle_info.battle_id.clone(),
            version: self.version.trim().parse().unwrap_or(0),
            map_id: self.battle_info.map_id.clone(),
            occurred_on: self.occurred_on(),
            mode: self.into(),
            rounds: last_round,
            winners,
            losers,
        })
    }
}

impl BattleRecord {
    fn occurred_on(&self) -> String {
        use chrono::{DateTime, NaiveDate, SecondsFormat};

        if let Some(start) = self
            .battle_info
            .start_time
            .trim()
            .parse::<i64>()
            .ok()
            .filter(|t| *t > 0)
            .and_then(|t| DateTime::from_timestamp(t, 0))
        {
            return start.to_rfc3339_opts(SecondsFormat::Secs, true);
        }
        // BattleID looks like 20260924--67113734
        if let Some(date) = self
            .battle_info
            .battle_id
            .get(..8)
            .and_then(|d| NaiveDate::parse_from_str(d, "%Y%m%d").ok())
        {
            return date.format("%Y-%m-%dT00:00:00Z").to_string();
        }
        // Offline games are named by local time instead: 25-01-27__20-36-14-492
        if let Some(when) = self
            .battle_info
            .battle_id
            .get(..18)
            .and_then(|d| chrono::NaiveDateTime::parse_from_str(d, "%y-%m-%d__%H-%M-%S").ok())
        {
            return when.format("%Y-%m-%dT%H:%M:%SZ").to_string();
        }
        self.create_time.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::path::PathBuf;

    #[test]
    fn test_parse() {
        let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures");

        let record = BattleRecord::new_from_file(fixtures.join("battle_record.xml"), false)
            .expect("this shouldn't die");
        let _overview = record.get_battle_overview().expect("Shouldn't die");
        //println!("{:#?}", _overview);

        let record = BattleRecord::new_from_file(fixtures.join("battle_record2.xml"), false)
            .expect("this shouldn't die");

        let _overview = record.get_battle_overview().expect("Shouldn't die");
        //println!("{:#?}", _overview);

        let record = BattleRecord::new_from_file(fixtures.join("4player_brawl2.xml"), false)
            .expect("this shouldn't die");

        let overview = record.get_battle_overview().expect("Shouldn't die");
        println!("{:#?}", overview);
    }

    /// Replays from both ends of the supported range: an original 1425 `.grbr` and a 2324 record,
    /// which dropped fields older versions required and added multi-item equipment.
    #[test]
    fn test_parse_across_versions() {
        let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures");

        for file in [
            "1425_20241123--67298547_[小疯子]VS[kaltorak18].grbr",
            "battle_record_2324.xml",
        ] {
            let record = BattleRecord::new_from_file(fixtures.join(file), false)
                .unwrap_or_else(|e| panic!("{file}: {e}"));
            let overview = record.get_battle_overview().expect("overview");
            let players: Vec<_> = overview.winners.iter().chain(&overview.losers).collect();
            assert_eq!(players.len(), 2, "{file}");
            for player in players {
                assert!(!player.last_round_units.is_empty(), "{file}");
                for unit in &player.last_round_units {
                    assert!(
                        !matches!(unit.unit_type, Unit::Unknown(_)),
                        "{file}: unresolved unit {:?}",
                        unit.unit_type
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod date_tests {
    use super::*;

    fn with_id(id: &str) -> BattleRecord {
        let mut record = BattleRecord::default();
        record.battle_info.battle_id = id.into();
        record
    }

    #[test]
    fn test_occurred_on_from_battle_ids() {
        assert_eq!(
            with_id("20260924--67113734").occurred_on(),
            "2026-09-24T00:00:00Z"
        );
        assert_eq!(
            with_id("25-01-27__20-36-14-492").occurred_on(),
            "2025-01-27T20:36:14Z",
            "offline games"
        );
    }
}
