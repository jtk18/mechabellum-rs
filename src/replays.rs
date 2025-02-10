mod battle_record;
mod overview;
mod parse_error;

use chrono::prelude::{DateTime, SecondsFormat, Utc};
use serde_xml_rs::from_str;

pub use battle_record::BattleRecord;
pub use parse_error::ParseError;

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
        let creation_system_time: DateTime<Utc> = path.metadata().and_then(|md| md.created()).map_err(|err| { ParseError::UnableToRead(path.into(), err.to_string().into()) })?.into();
        let creation_time = creation_system_time.to_rfc3339_opts(SecondsFormat::Secs, true);
        let bytes = fs::read(path).map_err(|err| { ParseError::UnableToRead(path.into(), err.to_string().into()) })?;

        for chunk in bytes.utf8_chunks() {
            if let Some(start_position) = chunk.valid().find("<?xml") {
                let end = if let Some(end_position) = chunk.valid().find("</BattleRecord>") {
                    end_position + "</BattleRecord>".len()
                } else {
                    continue;
                };
                let embedded_document = &chunk.valid()[start_position..end];
                let mut record: BattleRecord =  from_str(embedded_document).map_err(|e| {
                    ParseError::UnableToParse(
                        path.into(),
                        embedded_document.to_owned().into(),
                        e.to_string().into(),
                    )
                })?;

                record.create_time = creation_time;

                return Ok(record);
            }
        }

        Err(ParseError::NoBattleRecord(path.into()))
    }
}


// tested in src/replays/overview.rs