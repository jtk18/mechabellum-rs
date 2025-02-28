use super::{Unit, UnitInstance};

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
    pub unlocked_units: Vec<Unit>,
    pub last_round_units: Vec<UnitInstance>,
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
