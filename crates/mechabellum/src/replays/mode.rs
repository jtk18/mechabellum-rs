use crate::BattleRecord;

use super::PlayerName;

#[allow(non_camel_case_types)]
#[derive(Debug)]
pub enum GameMode {
    VS_2_2((PlayerName, PlayerName), (PlayerName, PlayerName)),
    VS_4_FFA(PlayerName, PlayerName, PlayerName, PlayerName),
    VS_1_1(PlayerName, PlayerName),
    VS_1_1_Competition(PlayerName, PlayerName),
    VS_1_1_Unknown(String, PlayerName, PlayerName),
    Solo(PlayerName),
    UnknownGameMode(String, Vec<PlayerName>),
}

impl From<&BattleRecord> for GameMode {
    fn from(record: &BattleRecord) -> Self {
        let names: Vec<String> = record
            .player_records
            .player_record
            .iter()
            .map(|v| v.name.to_owned())
            .collect();
        match record.battle_info.match_mode.as_str() {
            "VS_4_Scuffle" => GameMode::VS_4_FFA(
                names[0].clone().into(),
                names[1].clone().into(),
                names[2].clone().into(),
                names[3].clone().into(),
            ),
            "VS_1_1" => match record.battle_info.game_mode.as_str() {
                "Normal" => GameMode::VS_1_1(names[0].clone().into(), names[1].clone().into()),
                "Competition" => {
                    GameMode::VS_1_1_Competition(names[0].clone().into(), names[1].clone().into())
                }
                "Survive" => GameMode::Solo(names[0].clone().into()),
                _ => GameMode::VS_1_1_Unknown(
                    record.battle_info.game_mode.as_str().to_string(),
                    names[0].clone().into(),
                    names[1].clone().into(),
                ),
            },
            "VS_2_2" => GameMode::VS_2_2(
                (names[0].clone().into(), names[1].clone().into()),
                (names[2].clone().into(), names[3].clone().into()),
            ),
            _ => GameMode::UnknownGameMode(
                record.battle_info.match_mode.to_owned(),
                names.into_iter().map(Into::into).collect(),
            ),
        }
    }
}
