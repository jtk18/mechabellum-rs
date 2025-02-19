mod mode;

use crate::BattleRecord;

use strum::{Display, EnumString};

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

#[derive(Debug, Eq, PartialEq)]
pub struct UnitInstance {
    pub unit_type: Unit,
    pub xp: usize,
}

#[derive(Debug, Eq, PartialEq, Display, EnumString)]
#[strum(ascii_case_insensitive)]
pub enum Unit {
    #[strum(serialize = "1", serialize = "Fortress")]
    Fortress,
    #[strum(serialize = "2", serialize = "Marksman")]
    Marksman,
    #[strum(
        serialize = "4",
        serialize = "MeltingPoint",
        serialize = "Melting Point",
        to_string = "Melting Point"
    )]
    MeltingPoint,
    #[strum(serialize = "6", serialize = "Wasp")]
    Wasp,
    #[strum(serialize = "7", serialize = "Mustang")]
    Mustang,
    #[strum(
        serialize = "8",
        serialize = "Steel Ball",
        serialize = "SteelBall",
        to_string = "Steel Ball"
    )]
    SteelBall,
    #[strum(serialize = "9", serialize = "Fang")]
    Fang,
    #[strum(serialize = "10", serialize = "Crawler")]
    Crawler,
    #[strum(serialize = "13", serialize = "Sledgehammer")]
    Sledgehammer,
    #[strum(serialize = "15", serialize = "Arclight")]
    Arclight,
    #[strum(serialize = "18", serialize = "Wraith")]
    Wraith,
    #[strum(serialize = "19", serialize = "Scorpion")]
    Scorpion,
    #[strum(serialize = "20", serialize = "Firebadger")]
    Firebadger,
    #[strum(serialize = "22", serialize = "Typhoon")]
    Typhoon,
    #[strum(serialize = "24", serialize = "Tarantula")]
    Tarantula,
    #[strum(serialize = "26", serialize = "Farseer")]
    Farseer,
    #[strum(default)]
    Unknown(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_parse_units() {
        let tests = &[
            ("1", Unit::Fortress),
            ("Fortress", Unit::Fortress),
            ("fortress", Unit::Fortress),
            ("2", Unit::Marksman),
            ("Marksman", Unit::Marksman),
            ("marksman", Unit::Marksman),
            ("4", Unit::MeltingPoint),
            ("Melting Point", Unit::MeltingPoint),
            ("meltingpoint", Unit::MeltingPoint),
            ("6", Unit::Wasp),
            ("Wasp", Unit::Wasp),
            ("wasp", Unit::Wasp),
            ("7", Unit::Mustang),
            ("Mustang", Unit::Mustang),
            ("mustang", Unit::Mustang),
            ("8", Unit::SteelBall),
            ("Steel Ball", Unit::SteelBall),
            ("steelball", Unit::SteelBall),
            ("9", Unit::Fang),
            ("Fang", Unit::Fang),
            ("fang", Unit::Fang),
            ("10", Unit::Crawler),
            ("Crawler", Unit::Crawler),
            ("crawler", Unit::Crawler),
            ("13", Unit::Sledgehammer),
            ("SledgeHammer", Unit::Sledgehammer),
            ("sledgehammer", Unit::Sledgehammer),
            ("15", Unit::Arclight),
            ("Arclight", Unit::Arclight),
            ("arclight", Unit::Arclight),
            ("18", Unit::Wraith),
            ("Wraith", Unit::Wraith),
            ("wraith", Unit::Wraith),
            ("19", Unit::Scorpion),
            ("Scorpion", Unit::Scorpion),
            ("scorpion", Unit::Scorpion),
            ("20", Unit::Firebadger),
            ("Firebadger", Unit::Firebadger),
            ("firebadger", Unit::Firebadger),
            ("22", Unit::Typhoon),
            ("Typhoon", Unit::Typhoon),
            ("typhoon", Unit::Typhoon),
            ("24", Unit::Tarantula),
            ("Tarantula", Unit::Tarantula),
            ("tarantula", Unit::Tarantula),
            ("26", Unit::Farseer),
            ("Farseer", Unit::Farseer),
            ("farseer", Unit::Farseer),
            (
                "sir_not_appearing_in_film",
                Unit::Unknown(String::from("sir_not_appearing_in_film")),
            ),
        ];

        for (test, expected) in tests {
            assert_eq!(&Unit::from_str(test).unwrap(), expected);
        }
    }

    #[test]
    fn test_to_string_units() {
        let tests = &[
            (Unit::Fortress, "Fortress"),
            (Unit::Marksman, "Marksman"),
            (Unit::MeltingPoint, "Melting Point"),
            (Unit::Wasp, "Wasp"),
            (Unit::Mustang, "Mustang"),
            (Unit::SteelBall, "Steel Ball"),
            (Unit::Fang, "Fang"),
            (Unit::Crawler, "Crawler"),
            (Unit::Sledgehammer, "Sledgehammer"),
            (Unit::Arclight, "Arclight"),
            (Unit::Wraith, "Wraith"),
            (Unit::Scorpion, "Scorpion"),
            (Unit::Firebadger, "Firebadger"),
            (Unit::Typhoon, "Typhoon"),
            (Unit::Tarantula, "Tarantula"),
            (Unit::Farseer, "Farseer"),
            (Unit::Unknown(String::from("A New Unit!")), "A New Unit!"),
        ];

        for (test, expected) in tests {
            assert_eq!(test.to_string().as_str(), *expected)
        }
    }
}
