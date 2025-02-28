use strum::{Display, EnumString};

#[derive(Debug, Eq, PartialEq, Display, EnumString)]
#[strum(ascii_case_insensitive)]
pub enum Item {
    #[strum(
        serialize = "13030001",
        serialize = "LaserSights",
        to_string = "Laser Sights"
    )]
    LaserSights,
    #[strum(
        serialize = "13030002",
        serialize = "HeavyArmor",
        to_string = "Heavy Armor"
    )]
    HeavyArmor,
    #[strum(
        serialize = "13030003",
        serialize = "ImprovedFirepowerControlSystem",
        to_string = "Improved Firepower Control System"
    )]
    ImprovedFirepowerControlSystem,
    #[strum(
        serialize = "13030007",
        serialize = "AmplifyingCore",
        to_string = "Amplifying Core"
    )]
    AmplifyingCore,
    #[strum(
        serialize = "13030009",
        serialize = "SmallAmplifyingCore",
        to_string = "Small Amplifying Core"
    )]
    SmallAmplifyingCore,
    #[strum(
        serialize = "1305001",
        serialize = "ChargedAmmo",
        to_string = "Charged Ammo"
    )]
    ChargedAmmo,
    #[strum(
        serialize = "1309001",
        serialize = "AbsorptionModule",
        to_string = "Absorption Module"
    )]
    AbsorptionModule,
    #[strum(serialize = "1307001", serialize = "Barrier")]
    Barrier,
    #[strum(
        serialize = "13020001",
        serialize = "NanoRepairKit",
        to_string = "Nano Repair Kit"
    )]
    NanoRepairKit,
    #[strum(
        serialize = "13010001",
        serialize = "PortableShield",
        to_string = "Portable Shield"
    )]
    PortableShield,
    #[strum(
        serialize = "13030004",
        serialize = "EnhancementModule",
        to_string = "Enhancement Module"
    )]
    EnhancementModule,
    #[strum(
        serialize = "13030006",
        serialize = "SuperHeavyArmor",
        to_string = "Super Heavy Armor"
    )]
    SuperHeavyArmor,
    #[strum(
        serialize = "13030008",
        serialize = "AdvancedFirepowerControlSystem",
        to_string = "Advanced Firepower Control System"
    )]
    AdvancedFirepowerControlSystem,
    #[strum(
        serialize = "1305002",
        serialize = "ParasiticAmmo",
        to_string = "Parasitic Ammo"
    )]
    ParasiticAmmo,
    #[strum(
        serialize = "1306001",
        serialize = "TankProductionLine",
        to_string = "Tank Production Line"
    )]
    TankProductionLine,
    #[strum(
        serialize = "1306002",
        serialize = "MustangProductionLine",
        to_string = "Mustang Production Line"
    )]
    MustangProductionLine,
    #[strum(
        serialize = "1306003",
        serialize = "SteelBallProductionLine",
        to_string = "Steel Ball Production Line"
    )]
    SteelBallProductionLine,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_parse_items() {
        let tests = &[
            ("13030001", Item::LaserSights),
            ("LaserSights", Item::LaserSights),
            ("Laser Sights", Item::LaserSights),
            ("13030002", Item::HeavyArmor),
            ("HeavyArmor", Item::HeavyArmor),
            ("Heavy Armor", Item::HeavyArmor),
            ("13030003", Item::ImprovedFirepowerControlSystem),
            (
                "ImprovedFirepowerControlSystem",
                Item::ImprovedFirepowerControlSystem,
            ),
            (
                "Improved Firepower Control System",
                Item::ImprovedFirepowerControlSystem,
            ),
            ("13030007", Item::AmplifyingCore),
            ("AmplifyingCore", Item::AmplifyingCore),
            ("Amplifying Core", Item::AmplifyingCore),
            ("13030009", Item::SmallAmplifyingCore),
            ("SmallAmplifyingCore", Item::SmallAmplifyingCore),
            ("Small Amplifying Core", Item::SmallAmplifyingCore),
            ("1305001", Item::ChargedAmmo),
            ("ChargedAmmo", Item::ChargedAmmo),
            ("Charged Ammo", Item::ChargedAmmo),
            ("1309001", Item::AbsorptionModule),
            ("AbsorptionModule", Item::AbsorptionModule),
            ("Absorption Module", Item::AbsorptionModule),
            ("1307001", Item::Barrier),
            ("Barrier", Item::Barrier),
            ("13020001", Item::NanoRepairKit),
            ("NanoRepairKit", Item::NanoRepairKit),
            ("Nano repair Kit", Item::NanoRepairKit),
            ("13010001", Item::PortableShield),
            ("PortableShield", Item::PortableShield),
            ("Portable Shield", Item::PortableShield),
            ("13030004", Item::EnhancementModule),
            ("EnhancementModule", Item::EnhancementModule),
            ("Enhancement module", Item::EnhancementModule),
            ("13030006", Item::SuperHeavyArmor),
            ("SuperheavyArmor", Item::SuperHeavyArmor),
            ("Super heavy armor", Item::SuperHeavyArmor),
            ("13030008", Item::AdvancedFirepowerControlSystem),
            (
                "advancedfirepowercontrolsystem",
                Item::AdvancedFirepowerControlSystem,
            ),
            (
                "Advanced firepower Control system",
                Item::AdvancedFirepowerControlSystem,
            ),
            ("1305002", Item::ParasiticAmmo),
            ("PARASITICAMMO", Item::ParasiticAmmo),
            ("PARAsiTIC aMMo", Item::ParasiticAmmo),
            ("1306001", Item::TankProductionLine),
            ("Tankproductionline", Item::TankProductionLine),
            ("tank production LINE", Item::TankProductionLine),
            ("1306002", Item::MustangProductionLine),
            ("mustangproductionline", Item::MustangProductionLine),
            ("MUSTANG production LINE", Item::MustangProductionLine),
            ("1306003", Item::SteelBallProductionLine),
            ("SteelballPRODUCTIONline", Item::SteelBallProductionLine),
            ("Steel ball production line", Item::SteelBallProductionLine),
        ];

        for (test, expected) in tests {
            assert_eq!(&Item::from_str(test).unwrap(), expected);
        }
    }

    #[test]
    fn test_to_string_items() {
        let tests = &[
            (Item::AbsorptionModule, "Absorption Module"),
            (
                Item::AdvancedFirepowerControlSystem,
                "Advanced Firepower Control System",
            ),
            (Item::AmplifyingCore, "Amplifying Core"),
            (Item::Barrier, "Barrier"),
            (Item::ChargedAmmo, "Charged Ammo"),
            (Item::EnhancementModule, "Enhancement Module"),
            (Item::HeavyArmor, "Heavy Armor"),
            (
                Item::ImprovedFirepowerControlSystem,
                "Improved Firepower Control System",
            ),
            (Item::LaserSights, "Laser Sights"),
            (Item::MustangProductionLine, "Mustang Production Line"),
            (Item::NanoRepairKit, "Nano Repair Kit"),
            (Item::ParasiticAmmo, "Parasitic Ammo"),
            (Item::PortableShield, "Portable Shield"),
            (Item::SmallAmplifyingCore, "Small Amplifying Core"),
            (Item::SteelBallProductionLine, "Steel Ball Production Line"),
            (Item::SuperHeavyArmor, "Super Heavy Armor"),
            (Item::TankProductionLine, "Tank Production Line"),
        ];

        for (test, expected) in tests {
            assert_eq!(&test.to_string().as_str(), expected);
        }
    }
}
