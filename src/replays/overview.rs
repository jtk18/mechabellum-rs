mod types;

pub use types::*;

use super::BattleRecord;

use anyhow::Result;

#[derive(Debug)]
pub struct BattleOverview {
    pub occurred_on: String,
    pub mode: GameMode,
    pub winners: Vec<PlayerData>,
    pub losers: Vec<PlayerData>,
}

impl BattleOverview {
    pub fn new_from_record(record: &BattleRecord) -> Result<Self> {
        Ok(BattleOverview {
            occurred_on: record.create_time.to_owned(),
            mode: GameMode::try_from(record)?,
            winners: Vec::new(),
            losers: Vec::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::path::PathBuf;

    #[test]
    fn test_parse() {
        let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures");

        let record = BattleRecord::new_from_file(fixtures.join("battle_record.xml"))
            .expect("this shouldn't die");
        let overview = BattleOverview::new_from_record(&record).expect("Shouldn't die");
        println!("{:?}", overview);

        let record = BattleRecord::new_from_file(fixtures.join("battle_record2.xml"))
            .expect("this shouldn't die");

        let overview = BattleOverview::new_from_record(&record).expect("Shouldn't die");
        println!("{:?}", overview);
    }
}
