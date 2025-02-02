use serde::Deserialize;

#[derive(Debug, PartialEq, Default, Deserialize)]
#[serde(default)]
pub struct PlayerRecord {
    pub id: u64,
    pub name: String,
    pub seed: u32,
    pub ad: u32,
    pub data: Data,
    #[serde(rename = "playerRoundRecords")]
    pub round_records: Vec<PlayerRoundRecord>,
}

#[derive(Debug, PartialEq, Default, Deserialize)]
#[serde(default)]
pub struct Data {
    #[serde(rename="reactorCore")]
    pub reactor_core: u32,
}

#[derive(Debug, PartialEq, Default, Deserialize)]
#[serde(default)]
pub struct PlayerRoundRecord {
    pub round: u16,
}