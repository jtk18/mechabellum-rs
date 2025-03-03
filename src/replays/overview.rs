use super::{BattleRecord, GameMode, PlayerResult};

use anyhow::Result;

#[derive(Debug)]
pub struct BattleOverview {
    pub occurred_on: String,
    pub mode: GameMode,
    pub rounds: usize,
    pub winners: Vec<PlayerResult>,
    pub losers: Vec<PlayerResult>,
}

impl BattleRecord {
    pub fn get_battle_overview(&self) -> Result<BattleOverview> {
        let mode = GameMode::try_from(self)?;
        mode.get_battle_overview(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::path::PathBuf;

    #[test]
    fn test_parse() {
        let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures");

        let record = BattleRecord::new_from_file(fixtures.join("battle_record.xml"), false)
            .expect("this shouldn't die");
        let overview = record.get_battle_overview().expect("Shouldn't die");
        //println!("{:#?}", overview);

        let record = BattleRecord::new_from_file(fixtures.join("battle_record2.xml"), false)
            .expect("this shouldn't die");

        let overview = record.get_battle_overview().expect("Shouldn't die");
        //println!("{:#?}", overview);

        let record = BattleRecord::new_from_file(fixtures.join("4player_brawl2.xml"), false)
            .expect("this shouldn't die");

        let overview = record.get_battle_overview().expect("Shouldn't die");
        println!("{:#?}", overview);
    }
}
