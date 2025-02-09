mod battle_record;
mod parse_error;

use serde_xml_rs::from_str;

pub use battle_record::BattleRecord;
pub use parse_error::{ParseError, ParsedDocument, ParsedErrorMessage, ParsedReplay};

use std::fs;
use std::path::Path;

impl BattleRecord {
    pub fn new_from_files<P: AsRef<Path>>(files: &[P]) -> (Vec<BattleRecord>, Vec<ParseError>) {
        let mut v = Vec::new();
        let mut e = Vec::new();

        for file in files {
            let file_path = file.as_ref();
            match Self::new_from_file(file_path) {
                Ok(record) => v.push(record),
                Err(err) => e.push(err),
            }
        }

        (v, e)
    }

    pub fn new_from_file<P: AsRef<Path>>(file: P) -> Result<BattleRecord, ParseError> {
        let path = file.as_ref();
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(e) => return Err(ParseError::UnableToRead(path.into(), e.to_string().into())),
        };

        for chunk in bytes.utf8_chunks() {
            if let Some(start_position) = chunk.valid().find("<?xml") {
                let end = if let Some(end_position) = chunk.valid().find("</BattleRecord>") {
                    end_position + "</BattleRecord>".len()
                } else {
                    continue;
                };
                let embedded_document = &chunk.valid()[start_position..end];
                return match from_str(embedded_document) {
                    Ok(record) => Ok(record),
                    Err(e) => Err(ParseError::UnableToParse(
                        path.into(),
                        embedded_document.to_owned().into(),
                        e.to_string().into(),
                    )),
                };
            }
        }

        Err(ParseError::NoBattleRecord(path.into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::path::PathBuf;

    #[test]
    fn test_parse() {
        let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures");
        let _ = BattleRecord::new_from_file(fixtures.join("battle_record.xml"))
            .expect("this shouldn't die");

        let _ = BattleRecord::new_from_file(fixtures.join("battle_record2.xml"))
            .expect("this shouldn't die");
    }
}
