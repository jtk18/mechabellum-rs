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
                let embedded_document = &chunk.valid()[start_position..end];
                //println!("{embedded_document}");
                let record: BattleRecord = from_str(embedded_document)?;
                return Ok(record);
            }
        }

        bail!("Unable to find BattleRecord in {}", path.display());
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
