use super::{Skills, Specialist, Unit, UnitInstance, UnitTechs};

#[derive(Debug, Eq, PartialEq)]
pub struct PlayerName(String);

impl PlayerName {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for PlayerName {
    fn from(name: String) -> Self {
        PlayerName(name)
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct PlayerSummary {
    /// Account ID, stable across name changes.
    pub id: String,
    pub name: PlayerName,
    pub last_round_hp: i32,
    pub score: usize,
    pub unlocked_units: Vec<Unit>,
    pub last_round_units: Vec<UnitInstance>,
    pub specialist_cards: Vec<Specialist>,
    pub skills: Vec<Skills>,
    /// Techs researched by the final round, per unit type.
    pub enabled_techs: Vec<UnitTechs>,
}
