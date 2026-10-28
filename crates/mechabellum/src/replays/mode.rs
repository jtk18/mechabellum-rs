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
        // A replay can claim a mode with more players than it holds; a missing one is unnamed.
        let name = |i: usize| names.get(i).cloned().unwrap_or_default();
        match record.battle_info.match_mode.as_str() {
            "VS_4_Scuffle" => GameMode::VS_4_FFA(
                name(0).into(),
                name(1).into(),
                name(2).into(),
                name(3).into(),
            ),
            "VS_1_1" => match record.battle_info.game_mode.as_str() {
                "Normal" => GameMode::VS_1_1(name(0).into(), name(1).into()),
                "Competition" => GameMode::VS_1_1_Competition(name(0).into(), name(1).into()),
                "Survive" => GameMode::Solo(name(0).into()),
                _ => GameMode::VS_1_1_Unknown(
                    record.battle_info.game_mode.as_str().to_string(),
                    name(0).into(),
                    name(1).into(),
                ),
            },
            "VS_2_2" => GameMode::VS_2_2(
                (name(0).into(), name(1).into()),
                (name(2).into(), name(3).into()),
            ),
            _ => GameMode::UnknownGameMode(
                record.battle_info.match_mode.to_owned(),
                names.into_iter().map(Into::into).collect(),
            ),
        }
    }
}
