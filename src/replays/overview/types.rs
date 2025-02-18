use crate::BattleRecord;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum GameModeParseError {
    #[error("Unknown game mode {0}")]
    UnknownGameMode(String),
    #[error("Unknown match mode {0}")]
    UnknownMatchMode(String),
    #[error("Expected {0} players in match, but found {1}: {2:?}")]
    PlayerMismatch(u8, u8, Vec<String>),
}

#[allow(non_camel_case_types)]
#[derive(Debug)]
pub enum GameMode {
    VS_2_2((PlayerName, PlayerName), (PlayerName, PlayerName)),
    VS_4_FFA(PlayerName, PlayerName, PlayerName, PlayerName),
    VS_1_1(PlayerName, PlayerName),
    VS_1_1_Competition(PlayerName, PlayerName),
    Solo(PlayerName),
}

impl TryFrom<&BattleRecord> for GameMode {
    type Error = GameModeParseError;
    fn try_from(record: &BattleRecord) -> Result<Self, Self::Error> {
        let names: Vec<String> = record
            .player_records
            .player_record
            .iter()
            .map(|v| v.name.to_owned())
            .collect();
        match record.battle_info.match_mode.as_str() {
            "VS_4_Scuffle" => {
                if names.len() != 4 {
                    return Err(GameModeParseError::PlayerMismatch(
                        4,
                        names.len() as u8,
                        names,
                    ));
                }

                Ok(GameMode::VS_4_FFA(
                    names[0].clone().into(),
                    names[1].clone().into(),
                    names[2].clone().into(),
                    names[3].clone().into(),
                ))
            }
            "VS_1_1" => {
                if names.len() != 2 {
                    return Err(GameModeParseError::PlayerMismatch(
                        2,
                        names.len() as u8,
                        names,
                    ));
                }

                match record.battle_info.game_mode.as_str() {
                    "Normal" => Ok(GameMode::VS_1_1(
                        names[0].clone().into(),
                        names[1].clone().into(),
                    )),
                    "Competition" => Ok(GameMode::VS_1_1_Competition(
                        names[0].clone().into(),
                        names[1].clone().into(),
                    )),
                    "Survive" => Ok(GameMode::Solo(names[0].clone().into())),
                    _ => Err(GameModeParseError::UnknownGameMode(
                        record.battle_info.game_mode.to_owned(),
                    )),
                }
            }
            "VS_2_2" => {
                if names.len() != 4 {
                    return Err(GameModeParseError::PlayerMismatch(
                        4,
                        names.len() as u8,
                        names,
                    ));
                }

                Ok(GameMode::VS_2_2(
                    (names[0].clone().into(), names[1].clone().into()),
                    (names[2].clone().into(), names[3].clone().into()),
                ))
            }
            _ => Err(GameModeParseError::UnknownMatchMode(
                record.battle_info.match_mode.to_owned(),
            )),
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct PlayerName(String);

impl From<String> for PlayerName {
    fn from(name: String) -> Self {
        PlayerName(name)
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct PlayerResult {
    pub name: PlayerName,
    pub last_round_hp: i32,
    pub score: usize,
}

#[derive(Debug)]
pub struct PlayerData {
    pub name: PlayerName,
    pub commander: Commander,
    pub starting_hp: i32,
    pub ending_hp: i32,
    pub unlocked_units: Vec<Unit>,
}

#[derive(Debug, Eq, PartialEq)]
pub struct Commander(String);

impl From<String> for Commander {
    fn from(name: String) -> Self {
        Commander(name)
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct Unit(String);

impl From<String> for Unit {
    fn from(name: String) -> Self {
        Unit(name)
    }
}
