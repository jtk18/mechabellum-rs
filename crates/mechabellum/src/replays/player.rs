use super::{CardRef, Skills, Specialist, StarterPack, Unit, UnitInstance, UnitTechs};

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
    /// Specialists held at the end, named for the replay's game version (see `specialist_name`).
    pub specialist_names: Vec<String>,
    pub skills: Vec<Skills>,
    /// Techs researched by the final round, per unit type.
    pub enabled_techs: Vec<UnitTechs>,
    /// Specialists chosen before the first round (the earliest round record with any).
    pub starting_specialists: Vec<String>,
    /// Every card the player picked between rounds, in order. Unlike `specialist_cards`, this
    /// leaves out specialists the game granted without a pick.
    pub picked_cards: Vec<CardRef>,
    /// The starter pack picked before round 1.
    pub starter_pack: Option<StarterPack>,
}
