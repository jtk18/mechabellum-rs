mod mode;

use crate::BattleRecord;

#[allow(non_camel_case_types)]
#[derive(Debug)]
pub enum GameMode {
    VS_2_2((PlayerName, PlayerName), (PlayerName, PlayerName)),
    VS_4_FFA(PlayerName, PlayerName, PlayerName, PlayerName),
    VS_1_1(PlayerName, PlayerName),
    VS_1_1_Competition(PlayerName, PlayerName),
    Solo(PlayerName),
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
