//! ID -> display name lookup generated from the game's own localization table.
//!
//! The hand-written enums (`Unit`, `Specialist`, `Skills`, `Item`) cover content from older
//! replays, some of which has since been removed from the game. This table covers everything the
//! currently installed game knows about. `resolve` consults it first and callers fall back to the
//! legacy enums, then to the raw ID.

use serde::{Deserialize, Serialize};

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::OnceLock;

/// Bundled table, regenerated with `mechabellum-replay-parser gen-names`.
const BUNDLED: &str = include_str!("../../data/names.json");

/// What kind of thing an ID refers to. Numeric IDs collide across kinds (1001 is a Larva, a map,
/// a blueprint...), so every lookup has to say what it is looking for.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum NameKind {
    Unit,
    Officer,
    CommanderSkill,
    Equipment,
    Tech,
    Construction,
    Map,
    /// Command Center skills.
    TowerSkill,
    /// Research Center blueprints.
    Blueprint,
    /// A card offered between rounds: a specialist, unit reinforcement, skill, or item.
    Card,
}

impl NameKind {
    fn matches(self, category: &str) -> bool {
        match self {
            NameKind::Unit => category == "MechData",
            NameKind::Officer => category == "OfficerData" || category == "TDOfficerData",
            NameKind::CommanderSkill => category.starts_with("CSD_"),
            NameKind::Equipment => category.ends_with("EquipmentData"),
            NameKind::Tech => {
                category.contains("Tech")
                    || category == "BurrowData"
                    || category == "SearchTargetSpecificData"
            }
            NameKind::Construction => category == "ConstructionData",
            NameKind::Map => category == "MatchSetting",
            NameKind::TowerSkill => category == "EnergyTowerSkillData",
            NameKind::Blueprint => category == "BlueprintData",
            NameKind::Card => {
                category == "OfficerData"
                    || category == "UnitReinforceData"
                    || category.starts_with("CSD_")
                    || category.ends_with("EquipmentData")
            }
        }
    }

    /// Preferred category when several matching categories name the same ID.
    fn primary(self) -> &'static str {
        match self {
            NameKind::Unit => "MechData",
            NameKind::Officer => "OfficerData",
            NameKind::CommanderSkill => "",
            NameKind::Equipment => "EquipmentData",
            NameKind::Tech => "TechnologyData",
            NameKind::Construction => "ConstructionData",
            NameKind::Map => "MatchSetting",
            NameKind::TowerSkill => "EnergyTowerSkillData",
            NameKind::Blueprint => "BlueprintData",
            NameKind::Card => "OfficerData",
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct GameNames {
    /// Game version the table was extracted from.
    pub version: u32,
    /// Localization category (e.g. `MechData`) -> ID -> English name.
    pub categories: BTreeMap<String, BTreeMap<u64, String>>,
}

impl GameNames {
    /// The table bundled into this build.
    pub fn bundled() -> &'static GameNames {
        static NAMES: OnceLock<GameNames> = OnceLock::new();
        NAMES.get_or_init(|| serde_json::from_str(BUNDLED).expect("bundled names.json is valid"))
    }

    /// Every ID of this kind that has a name.
    pub fn ids(&self, kind: NameKind) -> std::collections::BTreeSet<u64> {
        self.categories
            .iter()
            .filter(|(category, _)| kind.matches(category))
            .flat_map(|(_, ids)| ids.keys().copied())
            .collect()
    }

    /// The localization category that names this ID, for kinds that span several.
    pub fn category_of(&self, kind: NameKind, id: u64) -> Option<&str> {
        if self
            .categories
            .get(kind.primary())
            .is_some_and(|ids| ids.contains_key(&id))
        {
            return Some(kind.primary());
        }
        self.categories
            .iter()
            .find(|(category, ids)| kind.matches(category) && ids.contains_key(&id))
            .map(|(category, _)| category.as_str())
    }

    pub fn resolve(&self, kind: NameKind, id: u64) -> Option<&str> {
        if let Some(name) = self
            .categories
            .get(kind.primary())
            .and_then(|ids| ids.get(&id))
        {
            return Some(name);
        }
        self.categories
            .iter()
            .filter(|(category, _)| kind.matches(category))
            .find_map(|(_, ids)| ids.get(&id))
            .map(String::as_str)
    }

    /// Extract the English `ConfigData/<Category>/name_<ID>` entries from the game's
    /// `Mechabellum_Data/resources.assets`.
    ///
    /// Each localization entry is laid out as
    /// `i32 key_len, key (4-aligned), i32 (unused), i32 lang_count, lang_count x (i32 len, utf8 (4-aligned))`
    /// with English first.
    pub fn extract_from_assets(bytes: &[u8], version: u32) -> GameNames {
        const PREFIX: &[u8] = b"ConfigData/";
        let align = |x: usize| (x + 3) & !3;
        let read_i32 = |at: usize| -> Option<i32> {
            bytes
                .get(at..at + 4)
                .map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        };

        let mut names = GameNames {
            version,
            ..Default::default()
        };
        let mut i = 4;
        while let Some(offset) = find(&bytes[i..], PREFIX) {
            let start = i + offset;
            i = start + 1;
            let Some(key_len) = read_i32(start - 4).filter(|l| (12..=200).contains(l)) else {
                continue;
            };
            let key_end = start + key_len as usize;
            let Some(key) = bytes
                .get(start..key_end)
                .and_then(|k| std::str::from_utf8(k).ok())
            else {
                continue;
            };
            let mut p = align(key_end) + 4;
            let Some(lang_count) = read_i32(p).filter(|n| (1..=20).contains(n)) else {
                continue;
            };
            p += 4;
            let Some(english) = read_i32(p)
                .filter(|l| (0..100_000).contains(l))
                .and_then(|l| bytes.get(p + 4..p + 4 + l as usize))
                .and_then(|s| std::str::from_utf8(s).ok())
            else {
                continue;
            };

            // ConfigData/<Category>/name_<ID>
            let mut parts = key.splitn(3, '/').skip(1);
            let (Some(category), Some(field)) = (parts.next(), parts.next()) else {
                continue;
            };
            let Some(id) = field
                .strip_prefix("name_")
                .and_then(|id| id.parse::<u64>().ok())
            else {
                continue;
            };
            if lang_count > 0 && !english.is_empty() {
                names
                    .categories
                    .entry(category.to_owned())
                    .or_default()
                    .insert(id, english.to_owned());
            }
            i = key_end;
        }
        names
    }

    /// Extract from an install directory, e.g.
    /// `C:\Program Files (x86)\Steam\steamapps\common\Mechabellum`.
    /// The version is read from `ProjectDatas/GameData.grdb` when present.
    pub fn extract_from_install(game_dir: &Path) -> std::io::Result<GameNames> {
        let assets = std::fs::read(game_dir.join("Mechabellum_Data").join("resources.assets"))?;
        let version = std::fs::read_to_string(game_dir.join("ProjectDatas").join("GameData.grdb"))
            .ok()
            .and_then(|s| {
                let start = s.find("<SaveVersion>")? + "<SaveVersion>".len();
                let end = start + s[start..].find('<')?;
                s[start..end].parse().ok()
            })
            .unwrap_or(0);
        Ok(Self::extract_from_assets(&assets, version))
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// Resolve a name from the bundled table.
pub fn resolve(kind: NameKind, id: &str) -> Option<&'static str> {
    GameNames::bundled().resolve(kind, id.trim().parse().ok()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bundled_lookup() {
        assert_eq!(resolve(NameKind::Unit, "1"), Some("Fortress"));
        assert_eq!(resolve(NameKind::Unit, "30"), Some("Void Eye"));
        assert_eq!(resolve(NameKind::Unit, "2002"), Some("Mountain"));
        // 1001 collides across categories; each kind gets its own answer.
        assert_eq!(resolve(NameKind::Unit, "1001"), Some("Larva"));
        assert_eq!(resolve(NameKind::Tech, "1001"), Some("Barrier"));
        assert_eq!(resolve(NameKind::Tech, "10510"), Some("Mechanical Rage"));
        assert_eq!(
            resolve(NameKind::Equipment, "13030010"),
            Some("Dominion Core")
        );
        assert_eq!(resolve(NameKind::Map, "1001"), Some("Railway Town"));
        assert_eq!(resolve(NameKind::Unit, "not a number"), None);
    }

    #[test]
    fn test_extract_record() {
        // One synthetic record: key, pad, unused i32, 2 languages.
        let mut b = vec![0u8; 4];
        let key = b"ConfigData/MechData/name_30";
        b.extend((key.len() as i32).to_le_bytes());
        b.extend(key);
        while b.len() % 4 != 0 {
            b.push(0);
        }
        b.extend(0i32.to_le_bytes());
        b.extend(2i32.to_le_bytes());
        for s in ["Void Eye", "Oeil du vide"] {
            b.extend((s.len() as i32).to_le_bytes());
            b.extend(s.as_bytes());
            while b.len() % 4 != 0 {
                b.push(0);
            }
        }
        let names = GameNames::extract_from_assets(&b, 1);
        assert_eq!(names.resolve(NameKind::Unit, 30), Some("Void Eye"));
    }
}

#[cfg(test)]
mod display_name_tests {
    use crate::{Item, Skills, Specialist};
    use std::str::FromStr;

    #[test]
    fn test_display_name_fallbacks() {
        assert_eq!(
            Specialist::from_str("20310").unwrap().display_name(),
            "Attack Enhancement"
        );
        assert_eq!(
            Skills::from_str("900010").unwrap().display_name(),
            "Unit Recycling"
        );
        assert_eq!(
            Item::from_str("13030022").unwrap().display_name(),
            "Secondary Fire Control System"
        );
    }
}
