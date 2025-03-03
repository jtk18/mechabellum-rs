use super::{Skills, Specialist, Unit, UnitInstance};

#[derive(Debug, Eq, PartialEq)]
pub struct PlayerName(String);

impl From<String> for PlayerName {
    fn from(name: String) -> Self {
        PlayerName(name)
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct PlayerSummary {
    pub name: PlayerName,
    pub last_round_hp: i32,
    pub score: usize,
    pub unlocked_units: Vec<Unit>,
    pub last_round_units: Vec<UnitInstance>,
    pub specialist_cards: Vec<Specialist>,
    pub skills: Vec<Skills>,
}
