use strum::{Display, EnumString};

use std::str::FromStr;

#[derive(Debug, Eq, PartialEq, Display, EnumString)]
#[strum(ascii_case_insensitive)]
pub enum Specialist {
    #[strum(
        serialize = "10007",
        serialize = "AdvancedShieldDevice",
        to_string = "Advanced Shield Device"
    )]
    AdvancedShieldDevice,
    #[strum(
        serialize = "10008",
        serialize = "AdvancedMissileDevice",
        to_string = "Advanced Missile Device"
    )]
    AdvancedMissileDevice,
    #[strum(
        serialize = "10009",
        serialize = "QuickTeleport",
        to_string = "Quick Teleport"
    )]
    QuickTeleport,
    #[strum(
        serialize = "10011",
        serialize = "MissileSpecialist",
        to_string = "Missile Specialist"
    )]
    MissileSpecialist,
    #[strum(
        serialize = "10013",
        serialize = "AmplifySpecialist",
        to_string = "Amplify Specialist"
    )]
    AmplifySpecialist,
    #[strum(
        serialize = "10014",
        serialize = "TrainingSpecialist",
        to_string = "Training Specialist"
    )]
    TrainingSpecialist,
    #[strum(
        serialize = "30104",
        serialize = "ImprovedFortress",
        to_string = "Improved Fortress"
    )]
    ImprovedFortress,
    #[strum(
        serialize = "30105",
        serialize = "ExtendedRangeFortress",
        to_string = "Extended Range Fortress"
    )]
    ExtendedRangeFortress,
    #[strum(
        serialize = "30202",
        serialize = "SmartMarksman",
        to_string = "Smart Marksman"
    )]
    SmartMarksman,
    #[strum(
        serialize = "30204",
        serialize = "EliteMarksman",
        to_string = "Elite Marksman"
    )]
    EliteMarksman,
    #[strum(
        serialize = "30302",
        serialize = "AssaultVulcan",
        to_string = "Assault Vulcan"
    )]
    AssaultVulcan,
    #[strum(
        serialize = "30401",
        serialize = "AssaultMeltingPoint",
        to_string = "Assault Melting Point"
    )]
    AssaultMeltingPoint,
    #[strum(
        serialize = "30402",
        serialize = "ImprovedMeltingPoint",
        to_string = "Improved Melting Point"
    )]
    ImprovedMeltingPoint,
    #[strum(
        serialize = "30403",
        serialize = "MassProducedMeltingPoint",
        to_string = "Mass-Produced Melting Point"
    )]
    MassProducedMeltingPoint,
    #[strum(
        serialize = "30501",
        serialize = "MassProducedRhino",
        to_string = "Mass-Produced Rhino"
    )]
    MassProducedRhino,
    #[strum(
        serialize = "30502",
        serialize = "BerserkRhino",
        to_string = "Berserk Rhino"
    )]
    BerserkRhino,
    #[strum(
        serialize = "30503",
        serialize = "EliteRhino",
        to_string = "Elite Rhino"
    )]
    EliteRhino,
    #[strum(
        serialize = "30601",
        serialize = "MassProducedWasp",
        to_string = "Mass-Produced Wasp"
    )]
    MassProducedWasp,
    #[strum(
        serialize = "30803",
        serialize = "ImprovedSteelBall",
        to_string = "Improved Steel Ball"
    )]
    ImprovedSteelBall,
    #[strum(
        serialize = "30804",
        serialize = "EliteSteelBall",
        to_string = "Elite Steel Ball"
    )]
    EliteSteelBall,
    #[strum(serialize = "30901", serialize = "EliteFang", to_string = "Elite Fang")]
    EliteFang,
    #[strum(
        serialize = "30902",
        serialize = "AssaultFang",
        to_string = "Assault Fang"
    )]
    AssaultFang,
    #[strum(
        serialize = "31101",
        serialize = "FortifiedOverlord",
        to_string = "Fortified Overlord"
    )]
    FortifiedOverlord,
    #[strum(
        serialize = "31102",
        serialize = "MassProducedOverlord",
        to_string = "Mass-Produced Overlord"
    )]
    MassProducedOverlord,
    #[strum(
        serialize = "31201",
        serialize = "AssaultStormcaller",
        to_string = "Assault Stormcaller"
    )]
    AssaultStormcaller,
    #[strum(
        serialize = "31205",
        serialize = "EliteStormcaller",
        to_string = "Elite Stormcaller"
    )]
    EliteStormcaller,
    #[strum(
        serialize = "31301",
        serialize = "MassProducedSledgehammer",
        to_string = "Mass-Produced Sledgehammer"
    )]
    MassProducedSledgehammer,
    #[strum(
        serialize = "31302",
        serialize = "ExtendedRangeSledgehammer",
        to_string = "Extended Range Sledgehammer"
    )]
    ExtendedRangeSledgehammer,
    #[strum(
        serialize = "31304",
        serialize = "ImprovedSledgehammer",
        to_string = "Improved Sledgehammer"
    )]
    ImprovedSledgehammer,
    #[strum(
        serialize = "31402",
        serialize = "FortifiedHacker",
        to_string = "Fortified Hacker"
    )]
    FortifiedHacker,
    #[strum(
        serialize = "31403",
        serialize = "EliteHacker",
        to_string = "Elite Hacker"
    )]
    EliteHacker,
    #[strum(
        serialize = "31502",
        serialize = "SmartArclight",
        to_string = "Smart Arclight"
    )]
    SmartArclight,
    #[strum(
        serialize = "31505",
        serialize = "EliteArclight",
        to_string = "Elite Arclight"
    )]
    EliteArclight,
    #[strum(
        serialize = "31601",
        serialize = "MassProducedPhoenix",
        to_string = "Mass-Produced Phoenix"
    )]
    MassProducedPhoenix,
    #[strum(
        serialize = "31602",
        serialize = "ExtendedRangePhoenix",
        to_string = "Extended Range Phoenix"
    )]
    ExtendedRangePhoenix,
    #[strum(
        serialize = "31603",
        serialize = "ImprovedPhoenix",
        to_string = "Improved Phoenix"
    )]
    ImprovedPhoenix,
    #[strum(
        serialize = "31604",
        serialize = "ElitePhoenix",
        to_string = "Elite Phoenix"
    )]
    ElitePhoenix,
    #[strum(
        serialize = "31701",
        serialize = "ExtendedRangeWarFactory",
        to_string = "Extended Range War Factory"
    )]
    ExtendedRangeWarFactory,
    #[strum(
        serialize = "31702",
        serialize = "ImprovedWarFactory",
        to_string = "Improved War Factory"
    )]
    ImprovedWarFactory,
    #[strum(
        serialize = "31801",
        serialize = "MassProducedWraith",
        to_string = "Mass-Produced Wraith"
    )]
    MassProducedWraith,
    #[strum(
        serialize = "31802",
        serialize = "ImprovedWraith",
        to_string = "Improved Wraith"
    )]
    ImprovedWraith,
    #[strum(
        serialize = "31902",
        serialize = "MassProducedScorpion",
        to_string = "Mass-Produced Scorpion"
    )]
    MassProducedScorpion,
    #[strum(
        serialize = "31903",
        serialize = "ImprovedScorpion",
        to_string = "Improved Scorpion"
    )]
    ImprovedScorpion,
    #[strum(
        serialize = "32301",
        serialize = "ImprovedSandworm",
        to_string = "Improved Sandworm"
    )]
    ImprovedSandworm,
    #[strum(
        serialize = "32302",
        serialize = "MassProducedSandworm",
        to_string = "Mass-Produced Sandworm"
    )]
    MassProducedSandworm,
    #[strum(
        serialize = "32401",
        serialize = "ImprovedTarantula",
        to_string = "Improved Tarantula"
    )]
    ImprovedTarantula,
    #[strum(
        serialize = "32402",
        serialize = "EliteTarantula",
        to_string = "Elite Tarantula"
    )]
    EliteTarantula,
    #[strum(
        serialize = "32501",
        serialize = "ExtendedRangePhantomRay",
        to_string = "Extended Range Phantom Ray"
    )]
    ExtendedRangePhantomRay,
    #[strum(
        serialize = "20029",
        serialize = "MarksmanSpecialist",
        to_string = "Marksman Specialist"
    )]
    MarksmanSpecialist,
    #[strum(
        serialize = "20032",
        serialize = "EliteSpecialist",
        to_string = "Elite Specialist"
    )]
    EliteSpecialist,
    #[strum(
        serialize = "20033",
        serialize = "RhinoSpecialist",
        to_string = "Rhino Specialist"
    )]
    RhinoSpecialist,
    #[strum(
        serialize = "20034",
        serialize = "CostControlSpecialist",
        to_string = "Cost Control Specialist"
    )]
    CostControlSpecialist,
    #[strum(
        serialize = "20035",
        serialize = "FortifiedSpecialist",
        to_string = "Fortified Specialist"
    )]
    FortifiedSpecialist,
    #[strum(
        serialize = "20036",
        serialize = "SabertoothSpecialist",
        to_string = "Sabertooth Specialist"
    )]
    SabertoothSpecialist,
    #[strum(
        serialize = "20038",
        serialize = "FireBadgerSpecialist",
        to_string = "Fire Badger Specialist"
    )]
    FireBadgerSpecialist,
    #[strum(
        serialize = "10010",
        serialize = "QuickSupplySpecialist",
        to_string = "Quick Supply Specialist"
    )]
    QuickSupplySpecialist,
    #[strum(
        serialize = "30101",
        serialize = "MassProducedFortress",
        to_string = "Mass-Produced Fortress"
    )]
    MassProducedFortress,
    #[strum(
        serialize = "30102",
        serialize = "AssaultFortress",
        to_string = "Assault Fortress"
    )]
    AssaultFortress,
    #[strum(
        serialize = "30201",
        serialize = "ExtendedRangeMarksman",
        to_string = "Extended Range Marksman"
    )]
    ExtendedRangeMarksman,
    #[strum(
        serialize = "30203",
        serialize = "SubsidizedMarksman",
        to_string = "Subsidized Marksman"
    )]
    SubsidizedMarksman,
    #[strum(
        serialize = "30301",
        serialize = "ExtendedRangeVulcan",
        to_string = "Extended Range Vulcan"
    )]
    ExtendedRangeVulcan,
    #[strum(
        serialize = "30602",
        serialize = "ImprovedWasp",
        to_string = "Improved Wasp"
    )]
    ImprovedWasp,
    #[strum(serialize = "30604", serialize = "EliteWasp", to_string = "Elite Wasp")]
    EliteWasp,
    #[strum(
        serialize = "30701",
        serialize = "SubsidizedMustang",
        to_string = "Subsidized Mustang"
    )]
    SubsidizedMustang,
    #[strum(
        serialize = "30703",
        serialize = "EliteMustang",
        to_string = "Elite Mustang"
    )]
    EliteMustang,
    #[strum(
        serialize = "30801",
        serialize = "SubsidizedSteelBall",
        to_string = "Subsidized Steel Ball"
    )]
    SubsidizedSteelBall,
    #[strum(
        serialize = "31002",
        serialize = "EliteCrawler",
        to_string = "Elite Crawler"
    )]
    EliteCrawler,
    #[strum(
        serialize = "31104",
        serialize = "ImprovedOverlord",
        to_string = "Improved Overlord"
    )]
    ImprovedOverlord,
    #[strum(
        serialize = "31202",
        serialize = "ExtendedRangeStormcaller",
        to_string = "Extended Range Stormcaller"
    )]
    ExtendedRangeStormcaller,
    #[strum(
        serialize = "31203",
        serialize = "SubsidizedStormcaller",
        to_string = "Subsidized Stormcaller"
    )]
    SubsidizedStormcaller,
    #[strum(
        serialize = "31305",
        serialize = "EliteSledgehammer",
        to_string = "Elite Sledgehammer"
    )]
    EliteSledgehammer,
    #[strum(
        serialize = "31501",
        serialize = "SubsidizedArclight",
        to_string = "Subsidized Arclight"
    )]
    SubsidizedArclight,
    #[strum(
        serialize = "31504",
        serialize = "ExtendedRangeArclight",
        to_string = "Extended Range Arclight"
    )]
    ExtendedRangeArclight,
    #[strum(
        serialize = "31901",
        serialize = "EfficientTechResearch",
        to_string = "Efficient Tech Research"
    )]
    EfficientTechResearch,
    #[strum(
        serialize = "20003",
        serialize = "GiantSpecialist",
        to_string = "Giant Specialist"
    )]
    GiantSpecialist,
    #[strum(
        serialize = "20005",
        serialize = "AerialSpecialist",
        to_string = "Aerial Specialist"
    )]
    AerialSpecialist,
    #[strum(
        serialize = "20021",
        serialize = "EfficientGiantManufacturing",
        to_string = "Efficient Giant Manufacturing"
    )]
    EfficientGiantManufacturing,
    #[strum(
        serialize = "20022",
        serialize = "AdvancedPowerSystem",
        to_string = "Advanced Power System"
    )]
    AdvancedPowerSystem,
    #[strum(
        serialize = "20004",
        serialize = "SpeedSpecialist",
        to_string = "Speed Specialist"
    )]
    SpeedSpecialist,
    #[strum(
        serialize = "20024",
        serialize = "FarseerSpecialist",
        to_string = "Farseer Specialist"
    )]
    FarseerSpecialist,
    #[strum(
        serialize = "20037",
        serialize = "TyphoonSpecialist",
        to_string = "Typhoon Specialist"
    )]
    TyphoonSpecialist,
    #[strum(
        serialize = "20039",
        serialize = "SupplyEnhancement",
        to_string = "Supply Enhancement"
    )]
    SupplyEnhancement,
    #[strum(
        serialize = "20007",
        serialize = "SupplySpecialist",
        to_string = "Supply Specialist"
    )]
    SupplySpecialist,
    #[strum(
        serialize = "10002",
        serialize = "QuickCooldown",
        to_string = "Quick Cooldown"
    )]
    QuickCooldown,
    #[strum(
        serialize = "10001",
        serialize = "AdditionalDeploymentSlot",
        to_string = "Additional Deployment Slot"
    )]
    AdditionalDeploymentSlot,
    #[strum(
        serialize = "10004",
        serialize = "FortifiedMustang",
        to_string = "Fortified Mustang"
    )]
    FortifiedMustang,
    #[strum(
        serialize = "30702",
        serialize = "SubsidizedCrawler",
        to_string = "Subsidized Crawler"
    )]
    SubsidizedCrawler,
    #[strum(
        serialize = "31001",
        serialize = "FortifiedArclight",
        to_string = "Fortified Arclight"
    )]
    FortifiedArclight,
    #[strum(
        serialize = "31503",
        serialize = "AdvancedDefensiveTactics",
        to_string = "Advanced Defensive Tactics"
    )]
    AdvancedDefensiveTactics,
    #[strum(
        serialize = "20001",
        serialize = "AdvancedOffensiveTactics",
        to_string = "Advanced Offensive Tactics"
    )]
    AdvancedOffensiveTactics,
    #[strum(
        serialize = "20002",
        serialize = "AdvancedTargetingSystem",
        to_string = "Advanced Targeting System"
    )]
    AdvancedTargetingSystem,
    #[strum(
        serialize = "20006",
        serialize = "EfficientLightManufacturing",
        to_string = "Efficient Light Manufacturing"
    )]
    EfficientLightManufacturing,
    #[strum(
        serialize = "20023",
        serialize = "SuperSupplyEnhancement",
        to_string = "Super Supply Enhancement"
    )]
    SuperSupplyEnhancement,
    #[strum(default)]
    Unknown(String),
}

/// The hand-written tables were built from version 1527 replays. The game has reused specialist
/// IDs since (10004 was Fortified Mustang and is now Additional Deployment Slot), so from 1545,
/// the first version with the newer replay format, the game's own name table comes first.
pub const LEGACY_NAMES_BEFORE: u32 = 1545;

/// Name of the specialist with this ID, as of the replay's game version.
pub fn specialist_name(id: &str, version: u32) -> String {
    let current = || super::names::resolve(super::names::NameKind::Officer, id).map(str::to_owned);
    let legacy = || match Specialist::from_str(id) {
        Ok(Specialist::Unknown(_)) | Err(_) => None,
        Ok(known) => Some(known.to_string()),
    };
    let name = if version >= LEGACY_NAMES_BEFORE {
        current().or_else(legacy)
    } else {
        legacy().or_else(current)
    };
    name.unwrap_or_else(|| id.to_owned())
}

impl Specialist {
    /// Display name, falling back to the game's name table for IDs this enum doesn't know.
    pub fn display_name(&self) -> String {
        match self {
            Specialist::Unknown(id) => super::names::resolve(super::names::NameKind::Officer, id)
                .map(str::to_owned)
                .unwrap_or_else(|| id.clone()),
            known => known.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_parse_specialists() {
        let tests = &[
            ("10007", Specialist::AdvancedShieldDevice),
            ("AdvancedShielddevice", Specialist::AdvancedShieldDevice),
            ("advanced shield DEVICE", Specialist::AdvancedShieldDevice),
            ("10008", Specialist::AdvancedMissileDevice),
            ("advancedmissiledevice", Specialist::AdvancedMissileDevice),
            ("Advanced MISSILE device", Specialist::AdvancedMissileDevice),
            ("10009", Specialist::QuickTeleport),
            ("Quickteleport", Specialist::QuickTeleport),
            ("quick TELEport", Specialist::QuickTeleport),
            ("10011", Specialist::MissileSpecialist),
            ("missilespecialist", Specialist::MissileSpecialist),
            ("missile specialist", Specialist::MissileSpecialist),
            ("10013", Specialist::AmplifySpecialist),
            ("amplifyspecialist", Specialist::AmplifySpecialist),
            ("Amplify Specialist", Specialist::AmplifySpecialist),
            ("10014", Specialist::TrainingSpecialist),
            ("TrainingSpecialist", Specialist::TrainingSpecialist),
            ("TrainING SPECialist", Specialist::TrainingSpecialist),
            ("30104", Specialist::ImprovedFortress),
            ("ImprovedFORTress", Specialist::ImprovedFortress),
            ("improved fortress", Specialist::ImprovedFortress),
            ("30105", Specialist::ExtendedRangeFortress),
            ("ExtendedrangeFortress", Specialist::ExtendedRangeFortress),
            ("extended range fortress", Specialist::ExtendedRangeFortress),
            ("30202", Specialist::SmartMarksman),
            ("smartmarksman", Specialist::SmartMarksman),
            ("smart marksman", Specialist::SmartMarksman),
            ("30204", Specialist::EliteMarksman),
            ("elitemarksman", Specialist::EliteMarksman),
            ("Elite MARKSman", Specialist::EliteMarksman),
            ("30302", Specialist::AssaultVulcan),
            ("assaultVULCAN", Specialist::AssaultVulcan),
            ("assault vulcan", Specialist::AssaultVulcan),
            ("30401", Specialist::AssaultMeltingPoint),
            ("assaultmeltINGpoint", Specialist::AssaultMeltingPoint),
            ("Assault MELTING point", Specialist::AssaultMeltingPoint),
            ("30402", Specialist::ImprovedMeltingPoint),
            ("improvedmeltingpoint", Specialist::ImprovedMeltingPoint),
            ("improved meltING point", Specialist::ImprovedMeltingPoint),
            ("30403", Specialist::MassProducedMeltingPoint),
            (
                "massproducedmeltingpoint",
                Specialist::MassProducedMeltingPoint,
            ),
            (
                "mass-produced melting point",
                Specialist::MassProducedMeltingPoint,
            ),
            ("30501", Specialist::MassProducedRhino),
            ("massproducedrhino", Specialist::MassProducedRhino),
            ("mass-produced rhino", Specialist::MassProducedRhino),
            ("30502", Specialist::BerserkRhino),
            ("berserkrhino", Specialist::BerserkRhino),
            ("BERSERK rhino", Specialist::BerserkRhino),
            ("30503", Specialist::EliteRhino),
            ("EliteRHINO", Specialist::EliteRhino),
            ("elite rhino", Specialist::EliteRhino),
            ("30601", Specialist::MassProducedWasp),
            ("Massproducedwasp", Specialist::MassProducedWasp),
            ("Mass-PRODUCED WAsp", Specialist::MassProducedWasp),
            ("30803", Specialist::ImprovedSteelBall),
            ("improvedsteelball", Specialist::ImprovedSteelBall),
            ("improved STEEL ball", Specialist::ImprovedSteelBall),
            ("30804", Specialist::EliteSteelBall),
            ("elitesteelball", Specialist::EliteSteelBall),
            ("Elite Steel BALL", Specialist::EliteSteelBall),
            ("30901", Specialist::EliteFang),
            ("elitefang", Specialist::EliteFang),
            ("ELITE fang", Specialist::EliteFang),
            ("30902", Specialist::AssaultFang),
            ("assaultFANG", Specialist::AssaultFang),
            ("Assault Fang", Specialist::AssaultFang),
            ("31101", Specialist::FortifiedOverlord),
            ("Fortified overlord", Specialist::FortifiedOverlord),
            ("FORTIFIEDoverlord", Specialist::FortifiedOverlord),
            ("31102", Specialist::MassProducedOverlord),
            ("massproducedOVERLORD", Specialist::MassProducedOverlord),
            ("mass-PRODuced overlord", Specialist::MassProducedOverlord),
            ("31201", Specialist::AssaultStormcaller),
            ("assaultSTORMcaller", Specialist::AssaultStormcaller),
            ("ASSAult STORMCALLER", Specialist::AssaultStormcaller),
            ("31205", Specialist::EliteStormcaller),
            ("elitestormcaller", Specialist::EliteStormcaller),
            ("elite stormcaller", Specialist::EliteStormcaller),
            ("31301", Specialist::MassProducedSledgehammer),
            (
                "massproducedSLEDGEhammer",
                Specialist::MassProducedSledgehammer,
            ),
            (
                "MASS-produced Sledgehammer",
                Specialist::MassProducedSledgehammer,
            ),
            ("31302", Specialist::ExtendedRangeSledgehammer),
            (
                "ExtendedRangeSLEDGEHAMMER",
                Specialist::ExtendedRangeSledgehammer,
            ),
            (
                "Extended Range Sledgehammer",
                Specialist::ExtendedRangeSledgehammer,
            ),
            ("31304", Specialist::ImprovedSledgehammer),
            ("improvedsledgehammer", Specialist::ImprovedSledgehammer),
            ("IMPROVED Sledgehammer", Specialist::ImprovedSledgehammer),
            ("31402", Specialist::FortifiedHacker),
            ("fortifiedhacker", Specialist::FortifiedHacker),
            ("fortified Hacker", Specialist::FortifiedHacker),
            ("31403", Specialist::EliteHacker),
            ("EliteHacker", Specialist::EliteHacker),
            ("elite hacker", Specialist::EliteHacker),
            ("31502", Specialist::SmartArclight),
            ("smartarclight", Specialist::SmartArclight),
            ("Smart Arclight", Specialist::SmartArclight),
            ("31505", Specialist::EliteArclight),
            ("EliteArclight", Specialist::EliteArclight),
            ("elite Arclight", Specialist::EliteArclight),
            ("31601", Specialist::MassProducedPhoenix),
            ("massproducedphoenix", Specialist::MassProducedPhoenix),
            ("Mass-Produced PHOENix", Specialist::MassProducedPhoenix),
            ("31602", Specialist::ExtendedRangePhoenix),
            ("ExtendedrangePhoenix", Specialist::ExtendedRangePhoenix),
            ("Extended Range Phoenix", Specialist::ExtendedRangePhoenix),
            ("31603", Specialist::ImprovedPhoenix),
            ("ImprovedPhoenix", Specialist::ImprovedPhoenix),
            ("Improved Phoenix", Specialist::ImprovedPhoenix),
            ("31604", Specialist::ElitePhoenix),
            ("ElitePhoenix", Specialist::ElitePhoenix),
            ("Elite PHOENix", Specialist::ElitePhoenix),
            ("31701", Specialist::ExtendedRangeWarFactory),
            (
                "ExtendedRanGEWARFactory",
                Specialist::ExtendedRangeWarFactory,
            ),
            (
                "Extended Range WAR FACtory",
                Specialist::ExtendedRangeWarFactory,
            ),
            ("31702", Specialist::ImprovedWarFactory),
            ("IMprovedWarfactory", Specialist::ImprovedWarFactory),
            ("IMproved War factory", Specialist::ImprovedWarFactory),
            ("31801", Specialist::MassProducedWraith),
            ("massproducedwraith", Specialist::MassProducedWraith),
            ("Mass-Produced Wraith", Specialist::MassProducedWraith),
            ("31802", Specialist::ImprovedWraith),
            ("IMprovedWraith", Specialist::ImprovedWraith),
            ("Improved WRAITH", Specialist::ImprovedWraith),
            ("31902", Specialist::MassProducedScorpion),
            ("Massproducedscorpion", Specialist::MassProducedScorpion),
            ("Mass-produced Scorpion", Specialist::MassProducedScorpion),
            ("31903", Specialist::ImprovedScorpion),
            ("improvedscorpion", Specialist::ImprovedScorpion),
            ("IMproved scorpion", Specialist::ImprovedScorpion),
            ("32301", Specialist::ImprovedSandworm),
            ("ImprovedSandworm", Specialist::ImprovedSandworm),
            ("IMproved Sandworm", Specialist::ImprovedSandworm),
            ("32302", Specialist::MassProducedSandworm),
            ("Massproducedsandworm", Specialist::MassProducedSandworm),
            ("Mass-produced sandworm", Specialist::MassProducedSandworm),
            ("32401", Specialist::ImprovedTarantula),
            ("Improvedtarantula", Specialist::ImprovedTarantula),
            ("Improved Tarantula", Specialist::ImprovedTarantula),
            ("32402", Specialist::EliteTarantula),
            ("EliteTarantula", Specialist::EliteTarantula),
            ("Elite Tarantula", Specialist::EliteTarantula),
            ("32501", Specialist::ExtendedRangePhantomRay),
            (
                "Extendedrangephantomray",
                Specialist::ExtendedRangePhantomRay,
            ),
            (
                "Extended Range PHANTOM RAY",
                Specialist::ExtendedRangePhantomRay,
            ),
            ("20029", Specialist::MarksmanSpecialist),
            ("MarksmanSpecialist", Specialist::MarksmanSpecialist),
            ("marksman specialist", Specialist::MarksmanSpecialist),
            ("20032", Specialist::EliteSpecialist),
            ("elitespecialist", Specialist::EliteSpecialist),
            ("Elite Specialist", Specialist::EliteSpecialist),
            ("20033", Specialist::RhinoSpecialist),
            ("Rhinospecialist", Specialist::RhinoSpecialist),
            ("Rhino Specialist", Specialist::RhinoSpecialist),
            ("20034", Specialist::CostControlSpecialist),
            ("CostControlSpecialist", Specialist::CostControlSpecialist),
            ("Cost Control Specialist", Specialist::CostControlSpecialist),
            ("20035", Specialist::FortifiedSpecialist),
            ("Fortifiedspecialist", Specialist::FortifiedSpecialist),
            ("fortified SPECIALIST", Specialist::FortifiedSpecialist),
            ("20036", Specialist::SabertoothSpecialist),
            ("Sabertoothspecialist", Specialist::SabertoothSpecialist),
            ("Sabertooth Specialist", Specialist::SabertoothSpecialist),
            ("20038", Specialist::FireBadgerSpecialist),
            ("Firebadgerspecialist", Specialist::FireBadgerSpecialist),
            ("Fire badger specialist", Specialist::FireBadgerSpecialist),
            ("10010", Specialist::QuickSupplySpecialist),
            ("Quicksupplyspecialist", Specialist::QuickSupplySpecialist),
            ("Quick Supply specialist", Specialist::QuickSupplySpecialist),
            ("30101", Specialist::MassProducedFortress),
            ("Massproducedfortress", Specialist::MassProducedFortress),
            ("Mass-produced Fortress", Specialist::MassProducedFortress),
            ("30102", Specialist::AssaultFortress),
            ("Assaultfortress", Specialist::AssaultFortress),
            ("Assault Fortress", Specialist::AssaultFortress),
            ("30201", Specialist::ExtendedRangeMarksman),
            ("Extendedrangemarksman", Specialist::ExtendedRangeMarksman),
            ("Extended Range Marksman", Specialist::ExtendedRangeMarksman),
            ("30203", Specialist::SubsidizedMarksman),
            ("Subsidizedmarksman", Specialist::SubsidizedMarksman),
            ("Subsidized Marksman", Specialist::SubsidizedMarksman),
            ("30301", Specialist::ExtendedRangeVulcan),
            ("Extendedrangevulcan", Specialist::ExtendedRangeVulcan),
            ("extended range vulcan", Specialist::ExtendedRangeVulcan),
            ("30602", Specialist::ImprovedWasp),
            ("improvedwasp", Specialist::ImprovedWasp),
            ("Improved WASP", Specialist::ImprovedWasp),
            ("30604", Specialist::EliteWasp),
            ("EliteWASP", Specialist::EliteWasp),
            ("Elite Wasp", Specialist::EliteWasp),
            ("30701", Specialist::SubsidizedMustang),
            ("Subsidizedmustang", Specialist::SubsidizedMustang),
            ("Subsidized mustang", Specialist::SubsidizedMustang),
            ("30703", Specialist::EliteMustang),
            ("Elitemustang", Specialist::EliteMustang),
            ("Elite Mustang", Specialist::EliteMustang),
            ("30801", Specialist::SubsidizedSteelBall),
            ("SubsidizedSteelball", Specialist::SubsidizedSteelBall),
            ("Subsidized Steel Ball", Specialist::SubsidizedSteelBall),
            ("31002", Specialist::EliteCrawler),
            ("Elitecrawler", Specialist::EliteCrawler),
            ("Elite Crawler", Specialist::EliteCrawler),
            ("31104", Specialist::ImprovedOverlord),
            ("improvedoverlord", Specialist::ImprovedOverlord),
            ("improved overlord", Specialist::ImprovedOverlord),
            ("31202", Specialist::ExtendedRangeStormcaller),
            (
                "Extendedrangestormcaller",
                Specialist::ExtendedRangeStormcaller,
            ),
            (
                "Extended range stormcaller",
                Specialist::ExtendedRangeStormcaller,
            ),
            ("31203", Specialist::SubsidizedStormcaller),
            ("Subsidizedstormcaller", Specialist::SubsidizedStormcaller),
            ("SUBsidized STORMCALLER", Specialist::SubsidizedStormcaller),
            ("31305", Specialist::EliteSledgehammer),
            ("Elitesledgehammer", Specialist::EliteSledgehammer),
            ("Elite sledgehammer", Specialist::EliteSledgehammer),
            ("31501", Specialist::SubsidizedArclight),
            ("Subsidizedarclight", Specialist::SubsidizedArclight),
            ("Subsidized Arclight", Specialist::SubsidizedArclight),
            ("31504", Specialist::ExtendedRangeArclight),
            ("extendedrangearclight", Specialist::ExtendedRangeArclight),
            ("extended range arclight", Specialist::ExtendedRangeArclight),
            ("31901", Specialist::EfficientTechResearch),
            ("EfficientTechresearch", Specialist::EfficientTechResearch),
            ("Efficient Tech Research", Specialist::EfficientTechResearch),
            ("20003", Specialist::GiantSpecialist),
            ("Giantspecialist", Specialist::GiantSpecialist),
            ("Giant Specialist", Specialist::GiantSpecialist),
            ("20005", Specialist::AerialSpecialist),
            ("aerialspecialist", Specialist::AerialSpecialist),
            ("aerial specialist", Specialist::AerialSpecialist),
            ("20021", Specialist::EfficientGiantManufacturing),
            (
                "Efficientgiantmanufacturing",
                Specialist::EfficientGiantManufacturing,
            ),
            (
                "efficient giant manuFACTuring",
                Specialist::EfficientGiantManufacturing,
            ),
            ("20022", Specialist::AdvancedPowerSystem),
            ("Advancedpowersystem", Specialist::AdvancedPowerSystem),
            ("Advanced Power SysTEM", Specialist::AdvancedPowerSystem),
            ("20004", Specialist::SpeedSpecialist),
            ("speedspecialist", Specialist::SpeedSpecialist),
            ("speed specialist", Specialist::SpeedSpecialist),
            ("20024", Specialist::FarseerSpecialist),
            ("farseerspecialist", Specialist::FarseerSpecialist),
            ("farseer specialist", Specialist::FarseerSpecialist),
            ("20037", Specialist::TyphoonSpecialist),
            ("Typhoonspecialist", Specialist::TyphoonSpecialist),
            ("Typhoon specialist", Specialist::TyphoonSpecialist),
            ("20039", Specialist::SupplyEnhancement),
            ("supplyenhancement", Specialist::SupplyEnhancement),
            ("Supply enhancement", Specialist::SupplyEnhancement),
            ("20007", Specialist::SupplySpecialist),
            ("supplyspecialist", Specialist::SupplySpecialist),
            ("supply specialist", Specialist::SupplySpecialist),
            ("10002", Specialist::QuickCooldown),
            ("quickcooldown", Specialist::QuickCooldown),
            ("quick cooldown", Specialist::QuickCooldown),
            ("10001", Specialist::AdditionalDeploymentSlot),
            (
                "AdditionaldeploymentSLOT",
                Specialist::AdditionalDeploymentSlot,
            ),
            (
                "Additional deployment sLot",
                Specialist::AdditionalDeploymentSlot,
            ),
            ("10004", Specialist::FortifiedMustang),
            ("fortifiedmustang", Specialist::FortifiedMustang),
            ("fortified mustang", Specialist::FortifiedMustang),
            ("30702", Specialist::SubsidizedCrawler),
            ("subsidizedcrawler", Specialist::SubsidizedCrawler),
            ("subsidized crawLer", Specialist::SubsidizedCrawler),
            ("31001", Specialist::FortifiedArclight),
            ("fortifiedarclight", Specialist::FortifiedArclight),
            ("Fortified Arclight", Specialist::FortifiedArclight),
            ("31503", Specialist::AdvancedDefensiveTactics),
            (
                "Advanceddefensivetactics",
                Specialist::AdvancedDefensiveTactics,
            ),
            (
                "Advanced Defensive tacTICS",
                Specialist::AdvancedDefensiveTactics,
            ),
            ("20001", Specialist::AdvancedOffensiveTactics),
            (
                "Advancedoffensivetactics",
                Specialist::AdvancedOffensiveTactics,
            ),
            (
                "Advanced offensive tactics",
                Specialist::AdvancedOffensiveTactics,
            ),
            ("20002", Specialist::AdvancedTargetingSystem),
            (
                "Advancedtargetingsystem",
                Specialist::AdvancedTargetingSystem,
            ),
            (
                "Advanced TARgeting SYStem",
                Specialist::AdvancedTargetingSystem,
            ),
            ("20006", Specialist::EfficientLightManufacturing),
            (
                "efficientlightmanuFACTuring",
                Specialist::EfficientLightManufacturing,
            ),
            (
                "efficient light Manufacturing",
                Specialist::EfficientLightManufacturing,
            ),
            ("20023", Specialist::SuperSupplyEnhancement),
            ("supersupplyenhancement", Specialist::SuperSupplyEnhancement),
            (
                "super supply enhancement",
                Specialist::SuperSupplyEnhancement,
            ),
        ];

        for (test, expected) in tests {
            assert_eq!(&Specialist::from_str(test).unwrap(), expected);
        }
    }

    #[test]
    fn test_to_string_specialists() {
        let tests = &[
            (
                Specialist::AdditionalDeploymentSlot,
                "Additional Deployment Slot",
            ),
            (
                Specialist::AdvancedDefensiveTactics,
                "Advanced Defensive Tactics",
            ),
            (Specialist::AdvancedMissileDevice, "Advanced Missile Device"),
            (
                Specialist::AdvancedOffensiveTactics,
                "Advanced Offensive Tactics",
            ),
            (Specialist::AdvancedPowerSystem, "Advanced Power System"),
            (Specialist::AdvancedShieldDevice, "Advanced Shield Device"),
            (
                Specialist::AdvancedTargetingSystem,
                "Advanced Targeting System",
            ),
            (Specialist::AerialSpecialist, "Aerial Specialist"),
            (Specialist::AmplifySpecialist, "Amplify Specialist"),
            (Specialist::AssaultFang, "Assault Fang"),
            (Specialist::AssaultFortress, "Assault Fortress"),
            (Specialist::AssaultMeltingPoint, "Assault Melting Point"),
            (Specialist::AssaultStormcaller, "Assault Stormcaller"),
            (Specialist::AssaultVulcan, "Assault Vulcan"),
            (Specialist::BerserkRhino, "Berserk Rhino"),
            (Specialist::CostControlSpecialist, "Cost Control Specialist"),
            (
                Specialist::EfficientGiantManufacturing,
                "Efficient Giant Manufacturing",
            ),
            (
                Specialist::EfficientLightManufacturing,
                "Efficient Light Manufacturing",
            ),
            (Specialist::EfficientTechResearch, "Efficient Tech Research"),
            (Specialist::EliteArclight, "Elite Arclight"),
            (Specialist::EliteCrawler, "Elite Crawler"),
            (Specialist::EliteFang, "Elite Fang"),
            (Specialist::EliteHacker, "Elite Hacker"),
            (Specialist::EliteMarksman, "Elite Marksman"),
            (Specialist::EliteMustang, "Elite Mustang"),
            (Specialist::ElitePhoenix, "Elite Phoenix"),
            (Specialist::EliteRhino, "Elite Rhino"),
            (Specialist::EliteSledgehammer, "Elite Sledgehammer"),
            (Specialist::EliteSpecialist, "Elite Specialist"),
            (Specialist::EliteSteelBall, "Elite Steel Ball"),
            (Specialist::EliteStormcaller, "Elite Stormcaller"),
            (Specialist::EliteTarantula, "Elite Tarantula"),
            (Specialist::EliteWasp, "Elite Wasp"),
            (Specialist::ExtendedRangeArclight, "Extended Range Arclight"),
            (Specialist::ExtendedRangeFortress, "Extended Range Fortress"),
            (Specialist::ExtendedRangeMarksman, "Extended Range Marksman"),
            (
                Specialist::ExtendedRangePhantomRay,
                "Extended Range Phantom Ray",
            ),
            (Specialist::ExtendedRangePhoenix, "Extended Range Phoenix"),
            (
                Specialist::ExtendedRangeSledgehammer,
                "Extended Range Sledgehammer",
            ),
            (
                Specialist::ExtendedRangeStormcaller,
                "Extended Range Stormcaller",
            ),
            (Specialist::ExtendedRangeVulcan, "Extended Range Vulcan"),
            (
                Specialist::ExtendedRangeWarFactory,
                "Extended Range War Factory",
            ),
            (Specialist::FarseerSpecialist, "Farseer Specialist"),
            (Specialist::FireBadgerSpecialist, "Fire Badger Specialist"),
            (Specialist::FortifiedArclight, "Fortified Arclight"),
            (Specialist::FortifiedHacker, "Fortified Hacker"),
            (Specialist::FortifiedMustang, "Fortified Mustang"),
            (Specialist::FortifiedOverlord, "Fortified Overlord"),
            (Specialist::FortifiedSpecialist, "Fortified Specialist"),
            (Specialist::GiantSpecialist, "Giant Specialist"),
            (Specialist::ImprovedFortress, "Improved Fortress"),
            (Specialist::ImprovedMeltingPoint, "Improved Melting Point"),
            (Specialist::ImprovedOverlord, "Improved Overlord"),
            (Specialist::ImprovedPhoenix, "Improved Phoenix"),
            (Specialist::ImprovedSandworm, "Improved Sandworm"),
            (Specialist::ImprovedScorpion, "Improved Scorpion"),
            (Specialist::ImprovedSledgehammer, "Improved Sledgehammer"),
            (Specialist::ImprovedSteelBall, "Improved Steel Ball"),
            (Specialist::ImprovedTarantula, "Improved Tarantula"),
            (Specialist::ImprovedWarFactory, "Improved War Factory"),
            (Specialist::ImprovedWasp, "Improved Wasp"),
            (Specialist::ImprovedWraith, "Improved Wraith"),
            (Specialist::MarksmanSpecialist, "Marksman Specialist"),
            (Specialist::MassProducedFortress, "Mass-Produced Fortress"),
            (
                Specialist::MassProducedMeltingPoint,
                "Mass-Produced Melting Point",
            ),
            (Specialist::MassProducedOverlord, "Mass-Produced Overlord"),
            (Specialist::MassProducedPhoenix, "Mass-Produced Phoenix"),
            (Specialist::MassProducedRhino, "Mass-Produced Rhino"),
            (Specialist::MassProducedSandworm, "Mass-Produced Sandworm"),
            (Specialist::MassProducedScorpion, "Mass-Produced Scorpion"),
            (
                Specialist::MassProducedSledgehammer,
                "Mass-Produced Sledgehammer",
            ),
            (Specialist::MassProducedWasp, "Mass-Produced Wasp"),
            (Specialist::MassProducedWraith, "Mass-Produced Wraith"),
            (Specialist::MissileSpecialist, "Missile Specialist"),
            (Specialist::QuickCooldown, "Quick Cooldown"),
            (Specialist::QuickSupplySpecialist, "Quick Supply Specialist"),
            (Specialist::QuickTeleport, "Quick Teleport"),
            (Specialist::RhinoSpecialist, "Rhino Specialist"),
            (Specialist::SabertoothSpecialist, "Sabertooth Specialist"),
            (Specialist::SmartArclight, "Smart Arclight"),
            (Specialist::SmartMarksman, "Smart Marksman"),
            (Specialist::SpeedSpecialist, "Speed Specialist"),
            (Specialist::SubsidizedArclight, "Subsidized Arclight"),
            (Specialist::SubsidizedCrawler, "Subsidized Crawler"),
            (Specialist::SubsidizedMarksman, "Subsidized Marksman"),
            (Specialist::SubsidizedMustang, "Subsidized Mustang"),
            (Specialist::SubsidizedSteelBall, "Subsidized Steel Ball"),
            (Specialist::SubsidizedStormcaller, "Subsidized Stormcaller"),
            (
                Specialist::SuperSupplyEnhancement,
                "Super Supply Enhancement",
            ),
            (Specialist::SupplyEnhancement, "Supply Enhancement"),
            (Specialist::TrainingSpecialist, "Training Specialist"),
            (Specialist::TyphoonSpecialist, "Typhoon Specialist"),
            (Specialist::Unknown("NOT KNOWN".into()), "NOT KNOWN"),
        ];

        for (test, expected) in tests {
            assert_eq!(&test.to_string().as_str(), expected);
        }
    }
}
