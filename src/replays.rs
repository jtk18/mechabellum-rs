mod battle_record;

use anyhow::{bail, Result};
use serde_xml_rs::from_str;

pub use battle_record::BattleRecord;

use std::fs;
use std::path::Path;

impl BattleRecord {
    pub fn new_from_files<P: AsRef<Path>>(files: &[P]) -> Result<Vec<BattleRecord>> {
        let mut v = Vec::new();

        for file in files {
            let file_path = file.as_ref();
            match Self::new_from_file(file_path) {
                Ok(record) => v.push(record),
                Err(e) => println!(
                    "File {:?} was not a battle record or we had issues parsing it: {e}",
                    file_path
                ),
            }
        }

        Ok(v)
    }

    pub fn new_from_file<P: AsRef<Path>>(file: P) -> Result<BattleRecord> {
        let path = file.as_ref();
        let bytes = fs::read(path)?;

        for chunk in bytes.utf8_chunks() {
            if let Some(start_position) = chunk.valid().find("<?xml") {
                let end = if let Some(end_position) = chunk.valid().find("</BattleRecord>") {
                    end_position + "</BattleRecord>".len()
                } else {
                    continue;
                };
                let record: BattleRecord = from_str(&chunk.valid()[start_position..end])?;
                return Ok(record);
            }
        }

        bail!("Unable to find BattleRecord in {}", path.display());
    }
}
