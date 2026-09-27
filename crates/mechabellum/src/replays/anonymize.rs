//! Removing other players' names from a replay before it's shared. Account IDs, ratings, and
//! game data stay, so players remain pseudonymous rather than disappearing from the stats.

/// Name given to players whose names were removed.
pub const HIDDEN_NAME: &str = "Opponent";

/// The replay document with every player's `<name>` replaced by [`HIDDEN_NAME`], except the
/// player with account ID `keep`. Names only appear in each `<PlayerRecord>`'s `<name>` element.
pub fn hide_other_names(document: &str, keep: &str) -> String {
    let mut out = String::with_capacity(document.len());
    let mut rest = document;
    while let Some(start) = rest.find("<PlayerRecord>") {
        let end = rest[start..]
            .find("</PlayerRecord>")
            .map_or(rest.len(), |e| start + e);
        out.push_str(&rest[..start]);
        let record = &rest[start..end];
        let id = between(record, "<id>", "</id>").map(str::trim);
        if id == Some(keep) {
            out.push_str(record);
        } else {
            out.push_str(&replace_first_name(record));
        }
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

fn between<'a>(s: &'a str, open: &str, close: &str) -> Option<&'a str> {
    let a = s.find(open)? + open.len();
    let b = a + s[a..].find(close)?;
    Some(&s[a..b])
}

fn replace_first_name(record: &str) -> String {
    let (Some(a), Some(b)) = (record.find("<name>"), record.find("</name>")) else {
        return record.to_owned();
    };
    if b < a {
        return record.to_owned();
    }
    format!("{}<name>{HIDDEN_NAME}{}", &record[..a], &record[b..])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BattleRecord;
    use std::path::{Path, PathBuf};

    #[test]
    fn test_hides_everyone_but_the_uploader() {
        let path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/battle_record_2324.xml");
        let original = std::fs::read_to_string(path).unwrap();
        let record = BattleRecord::from_bytes(original.as_bytes(), Path::new("x")).unwrap();
        let players: Vec<(String, String)> = record
            .player_records
            .player_record
            .iter()
            .map(|p| (p.id.clone(), p.name.clone()))
            .collect();
        let (keep_id, keep_name) = &players[1];
        let (other_id, other_name) = &players[0];

        let shared = hide_other_names(&original, keep_id);
        assert!(!shared.contains(&format!("<name>{other_name}</name>")));
        let reparsed = BattleRecord::from_bytes(shared.as_bytes(), Path::new("x")).unwrap();
        let names: Vec<(&str, &str)> = reparsed
            .player_records
            .player_record
            .iter()
            .map(|p| (p.id.as_str(), p.name.as_str()))
            .collect();
        assert!(names.contains(&(keep_id.as_str(), keep_name.as_str())));
        assert!(names.contains(&(other_id.as_str(), HIDDEN_NAME)));
        // Nothing else changed: same length apart from the one name.
        assert_eq!(
            shared.len() as i64 - original.len() as i64,
            HIDDEN_NAME.len() as i64 - other_name.len() as i64
        );
        assert!(reparsed.validate().is_empty());
    }
}
