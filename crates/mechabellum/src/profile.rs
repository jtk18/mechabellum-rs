//! The player's own game data (`ProjectDatas/GameData.grdb`): which techs each unit has
//! unlocked in Unit Modification.

use crate::{GameNames, NameKind};

use serde::Serialize;

/// Largest GameData.grdb read, in bytes. Real files are around 100 KiB.
const MAX_FILE: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UnitLoadout {
    pub unit_id: u32,
    pub unit_name: Option<String>,
    pub techs: Vec<LoadoutTech>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LoadoutTech {
    pub id: u32,
    pub name: Option<String>,
}

/// Every `<UnitUnlockedTechData>` in a GameData.grdb document, in file order.
pub fn unlocked_techs(document: &str) -> Vec<UnitLoadout> {
    if document.len() > MAX_FILE {
        return Vec::new();
    }
    let names = GameNames::bundled();
    let mut out = Vec::new();
    let mut rest = document;
    while let Some(start) = rest.find("<UnitUnlockedTechData>") {
        rest = &rest[start + "<UnitUnlockedTechData>".len()..];
        let Some(end) = rest.find("</UnitUnlockedTechData>") else {
            break;
        };
        let block = &rest[..end];
        rest = &rest[end..];
        let Some(unit_id) = element(block, "id").and_then(|v| v.trim().parse::<u32>().ok()) else {
            continue;
        };
        let techs = values(block)
            .map(|id| LoadoutTech {
                id,
                name: names.resolve(NameKind::Tech, id as u64).map(str::to_owned),
            })
            .collect();
        out.push(UnitLoadout {
            unit_id,
            unit_name: names
                .resolve(NameKind::Unit, unit_id as u64)
                .map(str::to_owned),
            techs,
        });
    }
    out
}

/// Text of the first `<tag>...</tag>` in `block`.
fn element<'a>(block: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("<{tag}>");
    let start = block.find(&open)? + open.len();
    let end = block[start..].find(&format!("</{tag}>"))?;
    Some(&block[start..start + end])
}

/// Every `<Value>n</Value>` in `block`.
fn values(block: &str) -> impl Iterator<Item = u32> + '_ {
    block
        .split("<Value>")
        .skip(1)
        .filter_map(|v| v.split("</Value>").next()?.trim().parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unlocked_techs() {
        let doc = r#"<GameData><unlockedTechDatas>
            <UnitUnlockedTechData><id>10</id><unlockedTechnologies>
              <SerializeInt><Value>10510</Value></SerializeInt>
              <SerializeInt><Value>2610</Value></SerializeInt>
            </unlockedTechnologies></UnitUnlockedTechData>
            <UnitUnlockedTechData><id>9</id><unlockedTechnologies /></UnitUnlockedTechData>
            <UnitUnlockedTechData><id>x</id></UnitUnlockedTechData>
        </unlockedTechDatas></GameData>"#;
        let units = unlocked_techs(doc);
        assert_eq!(units.len(), 2, "the entry without a numeric ID is skipped");
        assert_eq!(units[0].unit_id, 10);
        assert_eq!(units[0].unit_name.as_deref(), Some("Crawler"));
        assert_eq!(
            units[0].techs.iter().map(|t| t.id).collect::<Vec<_>>(),
            vec![10510, 2610]
        );
        assert!(units[0].techs.iter().all(|t| t.name.is_some()));
        assert!(units[1].techs.is_empty());
    }
}
