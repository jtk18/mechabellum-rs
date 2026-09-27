use strum::{Display, EnumString};

#[derive(Debug, Eq, PartialEq, Display, EnumString)]
#[strum(ascii_case_insensitive)]
pub enum Skills {
    #[strum(
        serialize = "300001",
        serialize = "MissileStrike",
        to_string = "Missile Strike"
    )]
    MissileStrike,
    #[strum(
        serialize = "200001",
        serialize = "ElectromagneticImpact",
        to_string = "Electromagnetic Impact"
    )]
    ElectromagneticImpact,
    #[strum(
        serialize = "800001",
        serialize = "ShieldAirdrop",
        to_string = "Shield Airdrop"
    )]
    ShieldAirdrop,
    #[strum(
        serialize = "1100001",
        serialize = "IntensiveTraining",
        to_string = "Intensive Training"
    )]
    IntensiveTraining,
    #[strum(
        serialize = "1000001",
        serialize = "1000002",
        serialize = "Redeployment",
        to_string = "Redeployment"
    )]
    Redeployment,
    #[strum(
        serialize = "1200001",
        serialize = "UndergroundThreat",
        to_string = "Underground Threat"
    )]
    UndergroundThreat,
    #[strum(
        serialize = "1200002",
        serialize = "1200006",
        serialize = "RhinoAssault",
        to_string = "Rhino Assault"
    )]
    RhinoAssault,
    #[strum(
        serialize = "1200003",
        serialize = "WaspSwarm",
        to_string = "Wasp Swarm"
    )]
    WaspSwarm,
    #[strum(
        serialize = "1200007",
        serialize = "SummonPhoenix",
        to_string = "Summon Phoenix"
    )]
    SummonPhoenix,
    #[strum(
        serialize = "1200011",
        serialize = "FirebadgerAssault",
        to_string = "Firebadger Assault"
    )]
    FirebadgerAssault,
    #[strum(
        serialize = "1500001",
        serialize = "1500002",
        serialize = "MobileBeacon",
        to_string = "Mobile Beacon"
    )]
    MobileBeacon,
    #[strum(
        serialize = "200003",
        serialize = "PhotonEmission",
        to_string = "Photon Emission"
    )]
    PhotonEmission,
    #[strum(
        serialize = "100002",
        serialize = "IncendiaryBomb",
        to_string = "Incendiary Bomb"
    )]
    IncendiaryBomb,
    #[strum(
        serialize = "300003",
        serialize = "OrbitalBombardment",
        to_string = "Orbital Bombardment"
    )]
    OrbitalBombardment,
    #[strum(
        serialize = "400002",
        serialize = "400003",
        serialize = "StickyOilBomb",
        to_string = "Sticky Oil Bomb"
    )]
    StickyOilBomb,
    #[strum(
        serialize = "600002",
        serialize = "SmokeBomb",
        to_string = "Smoke Bomb"
    )]
    SmokeBomb,
    #[strum(
        serialize = "900001",
        serialize = "FieldRecovery",
        to_string = "Field Recovery"
    )]
    FieldRecovery,
    #[strum(
        serialize = "900002",
        serialize = "900003",
        serialize = "InefficientRecovery",
        to_string = "Inefficient Recovery"
    )]
    InefficientRecovery,
    #[strum(
        serialize = "1200008",
        serialize = "SummonWraith",
        to_string = "Summon Wraith"
    )]
    SummonWraith,
    #[strum(
        serialize = "1200009",
        serialize = "ScorpionAssault",
        to_string = "Scorpion Assault"
    )]
    ScorpionAssault,
    #[strum(
        serialize = "1200010",
        serialize = "TyphoonAssault",
        to_string = "Typhoon Assault"
    )]
    TyphoonAssault,
    #[strum(
        serialize = "200002",
        serialize = "ElectromagneticBlast",
        to_string = "Electromagnetic Blast"
    )]
    ElectromagneticBlast,
    #[strum(serialize = "300004", serialize = "Nuke", to_string = "Nuke")]
    Nuke,
    #[strum(
        serialize = "300005",
        serialize = "LightningStorm",
        to_string = "Lightning Storm"
    )]
    LightningStorm,
    #[strum(serialize = "300006", serialize = "IonBlast", to_string = "Ion Blast")]
    IonBlast,
    #[strum(
        serialize = "300007",
        serialize = "OrbitalJavelin",
        to_string = "Orbital Javelin"
    )]
    OrbitalJavelin,
    #[strum(
        serialize = "500002",
        serialize = "AcidBlast",
        to_string = "Acid Blast"
    )]
    AcidBlast,
    #[strum(
        serialize = "1200004",
        serialize = "MobilizeBattleship",
        to_string = "Mobilize Battleship"
    )]
    MobilizeBattleship,
    #[strum(
        serialize = "1200005",
        serialize = "VulcansDescent",
        to_string = "Vulcan's Descent"
    )]
    VulcansDescent,
    #[strum(
        serialize = "1200012",
        serialize = "FortressDescent",
        to_string = "Fortress' Descent"
    )]
    FortressDescent,
    #[strum(
        serialize = "1200013",
        serialize = "MeltingPointsDescent",
        to_string = "Melting Point's Descent"
    )]
    MeltingPointsDescent,

    #[strum(default)]
    Unknown(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_parse_skills() {
        let tests = &[
            ("300001", Skills::MissileStrike),
            ("missilestrike", Skills::MissileStrike),
            ("Missile STRIKE", Skills::MissileStrike),
            ("200001", Skills::ElectromagneticImpact),
            ("Electromagneticimpact", Skills::ElectromagneticImpact),
            ("electromagnetic IMPACT", Skills::ElectromagneticImpact),
            ("800001", Skills::ShieldAirdrop),
            ("Shield Airdrop", Skills::ShieldAirdrop),
            ("shieldairdrop", Skills::ShieldAirdrop),
            ("1100001", Skills::IntensiveTraining),
            ("IntensiveTraining", Skills::IntensiveTraining),
            ("Intensive TRAINING", Skills::IntensiveTraining),
            ("1000001", Skills::Redeployment),
            ("Redeployment", Skills::Redeployment),
            ("1000002", Skills::Redeployment),
            ("1200001", Skills::UndergroundThreat),
            ("undergroundthreat", Skills::UndergroundThreat),
            ("underground threat", Skills::UndergroundThreat),
            ("1200002", Skills::RhinoAssault),
            ("Rhino Assault", Skills::RhinoAssault),
            ("rhino Assault", Skills::RhinoAssault),
            ("1200006", Skills::RhinoAssault),
            ("1200003", Skills::WaspSwarm),
            ("Waspswarm", Skills::WaspSwarm),
            ("wasp swarm", Skills::WaspSwarm),
            ("1200007", Skills::SummonPhoenix),
            ("Summonphoenix", Skills::SummonPhoenix),
            ("summon phoenix", Skills::SummonPhoenix),
            ("1200011", Skills::FirebadgerAssault),
            ("firebadgerassault", Skills::FirebadgerAssault),
            ("Firebadger assault", Skills::FirebadgerAssault),
            ("1500001", Skills::MobileBeacon),
            ("mobilebeacon", Skills::MobileBeacon),
            ("mobile Beacon", Skills::MobileBeacon),
            ("1500002", Skills::MobileBeacon),
            ("200003", Skills::PhotonEmission),
            ("photonemission", Skills::PhotonEmission),
            ("Photon Emission", Skills::PhotonEmission),
            ("100002", Skills::IncendiaryBomb),
            ("incendiarybomb", Skills::IncendiaryBomb),
            ("Incendiary Bomb", Skills::IncendiaryBomb),
            ("300003", Skills::OrbitalBombardment),
            ("OrbitalBombardment", Skills::OrbitalBombardment),
            ("orbital Bombardment", Skills::OrbitalBombardment),
            ("400002", Skills::StickyOilBomb),
            ("400003", Skills::StickyOilBomb),
            ("Stickyoilbomb", Skills::StickyOilBomb),
            ("Sticky oil BOMB", Skills::StickyOilBomb),
            ("600002", Skills::SmokeBomb),
            ("Smokebomb", Skills::SmokeBomb),
            ("Smoke BOMB", Skills::SmokeBomb),
            ("900001", Skills::FieldRecovery),
            ("fieldrecovery", Skills::FieldRecovery),
            ("field recovery", Skills::FieldRecovery),
            ("900002", Skills::InefficientRecovery),
            ("900003", Skills::InefficientRecovery),
            ("Inefficientrecovery", Skills::InefficientRecovery),
            ("Inefficient Recovery", Skills::InefficientRecovery),
            ("1200008", Skills::SummonWraith),
            ("summonwraith", Skills::SummonWraith),
            ("summon wRAITH", Skills::SummonWraith),
            ("1200009", Skills::ScorpionAssault),
            ("Scorpionassault", Skills::ScorpionAssault),
            ("Scorpion Assault", Skills::ScorpionAssault),
            ("1200010", Skills::TyphoonAssault),
            ("Typhoonassault", Skills::TyphoonAssault),
            ("Typhoonassault", Skills::TyphoonAssault),
            ("200002", Skills::ElectromagneticBlast),
            ("electromagneticblast", Skills::ElectromagneticBlast),
            ("Electromagnetic BLAST", Skills::ElectromagneticBlast),
            ("300004", Skills::Nuke),
            ("nuke", Skills::Nuke),
            ("300005", Skills::LightningStorm),
            ("lightningstorm", Skills::LightningStorm),
            ("Lightning storm", Skills::LightningStorm),
            ("300006", Skills::IonBlast),
            ("IonBlast", Skills::IonBlast),
            ("Ion Blast", Skills::IonBlast),
            ("300007", Skills::OrbitalJavelin),
            ("Orbitaljavelin", Skills::OrbitalJavelin),
            ("orbital Javelin", Skills::OrbitalJavelin),
            ("500002", Skills::AcidBlast),
            ("Acid Blast", Skills::AcidBlast),
            ("AcidBlast", Skills::AcidBlast),
            ("1200004", Skills::MobilizeBattleship),
            ("mobilizebattleship", Skills::MobilizeBattleship),
            ("Mobilize Battleship", Skills::MobilizeBattleship),
            ("1200005", Skills::VulcansDescent),
            ("Vulcansdescent", Skills::VulcansDescent),
            ("vulcan's descent", Skills::VulcansDescent),
            ("1200012", Skills::FortressDescent),
            ("Fortressdescent", Skills::FortressDescent),
            ("Fortress' Descent", Skills::FortressDescent),
            ("1200013", Skills::MeltingPointsDescent),
            ("Meltingpointsdescent", Skills::MeltingPointsDescent),
            ("Melting Point's Descent", Skills::MeltingPointsDescent),
        ];

        for (test, expected) in tests {
            assert_eq!(&Skills::from_str(test).unwrap(), expected);
        }
    }

    #[test]
    fn test_to_string_skills() {
        let tests = &[
            (Skills::AcidBlast, "Acid Blast"),
            (Skills::ElectromagneticBlast, "Electromagnetic Blast"),
            (Skills::ElectromagneticImpact, "Electromagnetic Impact"),
            (Skills::FieldRecovery, "Field Recovery"),
            (Skills::FirebadgerAssault, "Firebadger Assault"),
            (Skills::FortressDescent, "Fortress' Descent"),
            (Skills::IncendiaryBomb, "Incendiary Bomb"),
            (Skills::InefficientRecovery, "Inefficient Recovery"),
            (Skills::IntensiveTraining, "Intensive Training"),
            (Skills::IonBlast, "Ion Blast"),
            (Skills::LightningStorm, "Lightning Storm"),
            (Skills::MeltingPointsDescent, "Melting Point's Descent"),
            (Skills::MissileStrike, "Missile Strike"),
            (Skills::MobileBeacon, "Mobile Beacon"),
            (Skills::MobilizeBattleship, "Mobilize Battleship"),
            (Skills::Nuke, "Nuke"),
            (Skills::OrbitalBombardment, "Orbital Bombardment"),
            (Skills::OrbitalJavelin, "Orbital Javelin"),
            (Skills::PhotonEmission, "Photon Emission"),
            (Skills::Redeployment, "Redeployment"),
            (Skills::RhinoAssault, "Rhino Assault"),
            (Skills::ScorpionAssault, "Scorpion Assault"),
            (Skills::ShieldAirdrop, "Shield Airdrop"),
            (Skills::SmokeBomb, "Smoke Bomb"),
            (Skills::StickyOilBomb, "Sticky Oil Bomb"),
            (Skills::SummonPhoenix, "Summon Phoenix"),
            (Skills::SummonWraith, "Summon Wraith"),
            (Skills::TyphoonAssault, "Typhoon Assault"),
            (Skills::UndergroundThreat, "Underground Threat"),
            (Skills::VulcansDescent, "Vulcan's Descent"),
            (Skills::WaspSwarm, "Wasp Swarm"),
            (Skills::Unknown("NOT HERE".into()), "NOT HERE"),
        ];

        for (test, expected) in tests {
            assert_eq!(&test.to_string().as_str(), expected);
        }
    }
}

impl Skills {
    /// Display name, falling back to the game's name table for IDs this enum doesn't know.
    pub fn display_name(&self) -> String {
        match self {
            Skills::Unknown(id) => {
                super::names::resolve(super::names::NameKind::CommanderSkill, id)
                    .map(str::to_owned)
                    .unwrap_or_else(|| id.clone())
            }
            known => known.to_string(),
        }
    }
}
