use super::{BattleRecord, GameMode};

use super::{Item, MapPoint, PlayerName, PlayerSummary, Skills, Specialist, Unit, UnitInstance};

use anyhow::Result;

use std::str::FromStr;

#[derive(Debug)]
pub struct BattleOverview {
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
            let last_round_snapshot = &self.match_datas.match_snapshot_data
                [self.match_datas.match_snapshot_data.len() - 1];
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
                .filter_map(|unit_id| Unit::from_str(unit_id).ok())
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

            if let Some(last_fight_result) = last_round_snapshot.last_fight_result.as_ref() {
                let score: usize = last_fight_result.reports.fight_report[i]
                    .score
                    .parse()
                    .unwrap_or(0);

                let last_round_units: Vec<UnitInstance> = last_fight_result.reports.fight_report[i]
                    .unit_datas
                    .new_unit_data
                    .iter()
                    .filter_map(|data| {
                        Unit::from_str(data.id.as_str())
                            .and_then(|unit_type| {
                                Ok(UnitInstance {
                                    unit_type,
                                    xp: data.exp.parse().unwrap_or(0),
                                    position: MapPoint(
                                        data.position.x.parse().unwrap_or(0),
                                        data.position.y.parse().unwrap_or(0),
                                    ),
                                    equipment: if data.equipment_id.as_str().eq("0") {
                                        None
                                    } else {
                                        Item::from_str(&data.equipment_id).ok()
                                    },
                                })
                            })
                            .ok()
                    })
                    .collect();
                let summary = PlayerSummary {
                    name,
                    last_round_hp,
                    score,
                    last_round_units,
                    unlocked_units,
                    specialist_cards,
                    skills,
                };
                if last_round_result == "Win" {
                    winners.push(summary);
                } else {
                    losers.push(summary);
                }
            }
        }

        Ok(BattleOverview {
            occurred_on: self.create_time.to_owned(),
            mode: self.into(),
            rounds: last_round,
            winners,
            losers,
        })
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
}
