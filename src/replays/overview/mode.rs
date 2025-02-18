use anyhow::Result;

use crate::BattleRecord;
use crate::BattleOverview;

use super::{GameMode, PlayerName, PlayerResult};

impl GameMode {
    pub fn get_battle_overview(self, battle: &BattleRecord) -> Result<BattleOverview> {
        let mut winners = Vec::new();
        let mut losers = Vec::new();

        let mut last_round: usize = 0;
        let mut need_last_round_set = true;

        for (i, player) in battle.player_records.player_record.iter().enumerate() {
            let name: PlayerName = player.name.clone().into();
            let rounds = &player.player_round_records.player_round_record;
            let start_hp: i32 = rounds[1].player_data.reactor_core.parse().unwrap_or(0);
            let last_round_hp: i32 = rounds[rounds.len() - 1].player_data.reactor_core.parse().unwrap_or(0);
            let last_round_snapshot = &battle.match_datas.match_snapshot_data[battle.match_datas.match_snapshot_data.len() - 1];
            let last_round_result = rounds[rounds.len() - 1].player_data.pre_round_fight_result.as_str();

            if need_last_round_set {
                last_round = last_round_snapshot.round.parse().unwrap_or(0);
                need_last_round_set = false;
            }

            match &self {
                GameMode::VS_1_1(_, _) => {
                    if let Some(last_fight_result) = last_round_snapshot.last_fight_result.as_ref() {
                        let score: usize = last_fight_result.reports.fight_report[i].score.parse().unwrap_or(0);
                        let result = PlayerResult { name, last_round_hp, score };
                        if last_round_result == "Win" {
                            winners.push(
                                result
                            );
                        } else {
                            losers.push(
                                result
                            );
                        }
                    }
                }
                _ => panic!("Haven't calculated this game mode yet! {self:?}"),
            }
        }

        Ok(BattleOverview {
            occurred_on: battle.create_time.to_owned(),
            mode: self,
            rounds: last_round,
            winners,
            losers,
        })
    }
}