use serde::Deserialize;

/// Created with https://github.com/Thomblin/xml_schema_generator

#[derive(Debug, Deserialize)]
pub struct BattleRecord {
    #[serde(rename = "$text")]
    pub text: Option<String>,
    #[serde(rename = "playerRecords")]
    pub player_records: PlayerRecords,
    #[serde(rename = "matchDatas")]
    pub match_datas: MatchDatas,
    #[serde(rename = "reinforceItems")]
    pub reinforce_items: ReinforceItems,
    #[serde(rename = "Version")]
    pub version: String,
    #[serde(rename = "Seat")]
    pub seat: String,
    #[serde(rename = "BattleInfo")]
    pub battle_info: BattleInfo,
    #[serde(rename = "CreateTime")]
    pub create_time: String,
}

#[derive(Debug, Deserialize)]
pub struct PlayerRecords {
    #[serde(rename = "$text")]
    pub text: Option<String>,
    #[serde(rename = "PlayerRecord", default)]
    pub player_record: Vec<PlayerRecord>,
}

#[derive(Debug, Deserialize)]
pub struct PlayerRecord {
    #[serde(rename = "$text")]
    pub text: Option<String>,
    pub id: String,
    pub name: String,
    pub seed: String,
    pub ad: String,
    pub data: Data,
    #[serde(rename = "playerRoundRecords")]
    pub player_round_records: PlayerRoundRecords,
}

#[derive(Debug, Deserialize)]
pub struct Data {
    #[serde(rename = "$text")]
    pub text: Option<String>,
    #[serde(rename = "reactorCore")]
    pub reactor_core: String,
    #[serde(rename = "MaxReactorCore")]
    pub max_reactor_core: String,
    #[serde(rename = "maxRoundSupply")]
    pub max_round_supply: String,
    #[serde(rename = "firstRoundSupply")]
    pub first_round_supply: String,
    #[serde(rename = "roundSupplyIncreaseValue")]
    pub round_supply_increase_value: String,
    pub team: String,
    #[serde(rename = "isLeader")]
    pub is_leader: String,
    #[serde(rename = "type")]
    pub data_type: String,
    #[serde(rename = "unitDatas")]
    pub unit_datas: DataUnitDatas,
    // pub style: Style,
}

#[derive(Debug, Deserialize)]
pub struct DataUnitDatas {
    #[serde(rename = "$text")]
    pub text: Option<String>,
    #[serde(rename = "unitData", default)]
    pub unit_data: Vec<UnitDatasUnitData>,
}

#[derive(Debug, Deserialize)]
pub struct UnitDatasUnitData {
    #[serde(rename = "$text")]
    pub text: Option<String>,
    pub id: String,
    #[serde(default)]
    pub techs: Vec<UnitDatasUnitDataTechs>,
    #[serde(rename = "unlockedTechs")]
    pub unlocked_techs: UnitDatasUnitDataUnlockedTechs,
}

#[derive(Debug, Deserialize)]
pub struct UnitDatasUnitDataTechs {
    #[serde(rename = "$text")]
    pub text: Option<String>,
    #[serde(default)]
    pub tech: Vec<UnitDatasUnitDataTechsTech>,
}

#[derive(Debug, Deserialize)]
pub struct UnitDatasUnitDataTechsTech {
    pub data: String,
}

#[derive(Debug, Deserialize)]
pub struct UnitDatasUnitDataUnlockedTechs {}

#[derive(Debug, Deserialize)]
pub struct Style {
    #[serde(rename = "$text")]
    pub text: Option<String>,
    #[serde(rename = "unitStyles")]
    pub unit_styles: UnitStyles,
}

#[derive(Debug, Deserialize)]
pub struct UnitStyles {
    #[serde(rename = "$text")]
    pub text: Option<String>,
    #[serde(rename = "unitStyle")]
    pub unit_style: Option<UnitStyle>,
}

#[derive(Debug, Deserialize)]
pub struct UnitStyle {
    #[serde(rename = "$text")]
    pub text: Option<String>,
    #[serde(rename = "unitID")]
    pub unit_id: String,
    #[serde(rename = "allLevelSkinID")]
    pub all_level_skin_id: String,
    #[serde(rename = "skinStyles")]
    pub skin_styles: SkinStyles,
}

#[derive(Debug, Deserialize)]
pub struct SkinStyles {
    #[serde(rename = "$text")]
    pub text: Option<String>,
    #[serde(rename = "skinStyle")]
    pub skin_style: SkinStyle,
}

#[derive(Debug, Deserialize)]
pub struct SkinStyle {
    pub data: String,
}

#[derive(Debug, Deserialize)]
pub struct PlayerRoundRecords {
    #[serde(rename = "$text")]
    pub text: Option<String>,
    #[serde(rename = "PlayerRoundRecord", default)]
    pub player_round_record: Vec<PlayerRoundRecord>,
}

#[derive(Debug, Deserialize)]
pub struct PlayerRoundRecord {
    #[serde(rename = "$text")]
    pub text: Option<String>,
    pub round: String,
    #[serde(rename = "playerData")]
    pub player_data: PlayerData,
    #[serde(rename = "actionRecords")]
    pub action_records: ActionRecords,
}

#[derive(Debug, Deserialize)]
pub struct PlayerData {
    #[serde(rename = "$text")]
    pub text: Option<String>,
    #[serde(rename = "reactorCore")]
    pub reactor_core: String,
    pub supply: String,
    #[serde(rename = "preRoundFightResult")]
    pub pre_round_fight_result: String,
    pub units: Units,
    #[serde(rename = "unitIndex")]
    pub unit_index: String,
    pub officers: Officers,
    #[serde(rename = "commanderSkills")]
    pub commander_skills: PlayerDataCommanderSkills,
    #[serde(rename = "activeTechnologies")]
    pub active_technologies: ActiveTechnologies,
    pub equipments: Equipments,
    pub shop: Shop,
    pub contraptions: PlayerDataContraptions,
    #[serde(rename = "contraptionIndex")]
    pub contraption_index: String,
    pub bluepints: Bluepints,
    #[serde(rename = "energyTowerSkills")]
    pub energy_tower_skills: EnergyTowerSkills,
    #[serde(rename = "towerStrengthenLevels")]
    pub tower_strengthen_levels: TowerStrengthenLevels,
    #[serde(rename = "IsSpecialSupply")]
    pub is_special_supply: String,
}

#[derive(Debug, Deserialize)]
pub struct Units {
    #[serde(rename = "$text")]
    pub text: Option<String>,
    #[serde(rename = "NewUnitData", default)]
    pub new_unit_data: Vec<UnitsNewUnitData>,
}

#[derive(Debug, Deserialize)]
pub struct UnitsNewUnitData {
    #[serde(rename = "$text")]
    pub text: Option<String>,
    pub id: String,
    #[serde(rename = "Index")]
    pub index: String,
    #[serde(rename = "RoundCount")]
    pub round_count: String,
    #[serde(rename = "Durability")]
    pub durability: String,
    #[serde(rename = "Exp")]
    pub exp: String,
    #[serde(rename = "Level")]
    pub level: String,
    #[serde(rename = "Position")]
    pub position: PlayerDataUnitsNewUnitDataPosition,
    #[serde(rename = "EquipmentID")]
    pub equipment_id: String,
    #[serde(rename = "IsRotate")]
    pub is_rotate: String,
    #[serde(rename = "SellSupply")]
    pub sell_supply: String,
}

#[derive(Debug, Deserialize)]
pub struct PlayerDataUnitsNewUnitDataPosition {
    #[serde(rename = "$text")]
    pub text: Option<String>,
    pub x: String,
    pub y: String,
}

#[derive(Debug, Deserialize)]
pub struct Officers {
    #[serde(rename = "$text")]
    pub text: Option<String>,
    #[serde(default)]
    pub int: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct PlayerDataCommanderSkills {
    #[serde(rename = "$text")]
    pub text: Option<String>,
    #[serde(rename = "CommanderSkillData", default)]
    pub commander_skill_data: Vec<CommanderSkillData>,
}

#[derive(Debug, Deserialize)]
pub struct CommanderSkillData {
    #[serde(rename = "$text")]
    pub text: Option<String>,
    pub index: String,
    pub id: String,
    #[serde(rename = "isActive")]
    pub is_active: String,
    #[serde(rename = "coolingRound")]
    pub cooling_round: String,
    #[serde(rename = "rangeItems")]
    pub range_items: RangeItems,
}

#[derive(Debug, Deserialize)]
pub struct RangeItems {}

#[derive(Debug, Deserialize)]
pub struct ActiveTechnologies {
    #[serde(rename = "$text")]
    pub text: Option<String>,
    #[serde(rename = "UnitData", default)]
    pub unit_data: Vec<ActiveTechnologiesUnitData>,
}

#[derive(Debug, Deserialize)]
pub struct ActiveTechnologiesUnitData {
    #[serde(rename = "$text")]
    pub text: Option<String>,
    pub id: String,
    #[serde(default)]
    pub techs: Vec<ActiveTechnologiesUnitDataTechs>,
    #[serde(rename = "unlockedTechs")]
    pub unlocked_techs: ActiveTechnologiesUnitDataUnlockedTechs,
}

#[derive(Debug, Deserialize)]
pub struct ActiveTechnologiesUnitDataTechs {
    #[serde(rename = "$text")]
    pub text: Option<String>,
    #[serde(default)]
    pub tech: Vec<ActiveTechnologiesUnitDataTechsTech>,
}

#[derive(Debug, Deserialize)]
pub struct ActiveTechnologiesUnitDataTechsTech {
    pub data: String,
}

#[derive(Debug, Deserialize)]
pub struct ActiveTechnologiesUnitDataUnlockedTechs {}

#[derive(Debug, Deserialize)]
pub struct Equipments {}

#[derive(Debug, Deserialize)]
pub struct Shop {
    #[serde(rename = "$text")]
    pub text: Option<String>,
    #[serde(rename = "unlockedUnits")]
    pub unlocked_units: UnlockedUnits,
    #[serde(rename = "lockedUnits")]
    pub locked_units: LockedUnits,
    #[serde(rename = "BuyCount")]
    pub buy_count: String,
    #[serde(rename = "MaxUnlockCount")]
    pub max_unlock_count: String,
    #[serde(rename = "UnlockCount")]
    pub unlock_count: String,
}

#[derive(Debug, Deserialize)]
pub struct UnlockedUnits {
    #[serde(rename = "$text")]
    pub text: Option<String>,
    #[serde(default)]
    pub int: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct LockedUnits {
    #[serde(rename = "$text")]
    pub text: Option<String>,
    #[serde(default)]
    pub int: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct PlayerDataContraptions {
    #[serde(rename = "$text")]
    pub text: Option<String>,
    #[serde(rename = "ContraptionData", default)]
    pub contraption_data: Vec<PlayerDataContraptionsContraptionData>,
}

#[derive(Debug, Deserialize)]
pub struct PlayerDataContraptionsContraptionData {
    #[serde(rename = "$text")]
    pub text: Option<String>,
    pub index: String,
    pub id: String,
    pub position: PlayerDataContraptionsContraptionDataPosition,
}

#[derive(Debug, Deserialize)]
pub struct PlayerDataContraptionsContraptionDataPosition {
    #[serde(rename = "$text")]
    pub text: Option<String>,
    pub x: String,
    pub y: String,
}

#[derive(Debug, Deserialize)]
pub struct Bluepints {
    #[serde(rename = "$text")]
    pub text: Option<String>,
    #[serde(default)]
    pub int: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct EnergyTowerSkills {
    #[serde(rename = "$text")]
    pub text: Option<String>,
    pub int: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TowerStrengthenLevels {
    #[serde(rename = "$text")]
    pub text: Option<String>,
    #[serde(default)]
    pub int: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct ActionRecords {
    #[serde(rename = "$text")]
    pub text: Option<String>,
    #[serde(rename = "MatchActionData", default)]
    pub match_action_data: Vec<MatchActionData>,
}

#[derive(Debug, Deserialize)]
pub struct MatchActionData {
    #[serde(rename = "type")]
    pub xsi_type: String,
    #[serde(rename = "$text")]
    pub text: Option<String>,
    #[serde(rename = "ExtraPosition")]
    pub extra_position: Option<ExtraPosition>,
    #[serde(rename = "Position")]
    pub position: Option<PlayerRoundRecordActionRecordsMatchActionDataPosition>,
    #[serde(rename = "ContraptionID")]
    pub contraption_id: Option<String>,
    #[serde(rename = "UnitIndex")]
    pub unit_index: Option<String>,
    #[serde(rename = "Positions")]
    pub positions: Option<Positions>,
    #[serde(rename = "SkillIndex")]
    pub skill_index: Option<String>,
    #[serde(rename = "Index")]
    pub index: Option<String>,
    #[serde(rename = "ID")]
    pub id: Option<String>,
    #[serde(rename = "TechID")]
    pub tech_id: Option<String>,
    #[serde(rename = "SkillID")]
    pub skill_id: Option<String>,
    #[serde(rename = "UIDX")]
    pub uidx: Option<String>,
    #[serde(rename = "UID")]
    pub uid: Option<String>,
    #[serde(rename = "moveUnitDatas")]
    pub move_unit_datas: Option<MoveUnitDatas>,
    #[serde(rename = "Time")]
    pub time: String,
    #[serde(rename = "LocalTime")]
    pub local_time: String,
}

#[derive(Debug, Deserialize)]
pub struct ExtraPosition {
    #[serde(rename = "$text")]
    pub text: Option<String>,
    pub x: String,
    pub y: String,
}

#[derive(Debug, Deserialize)]
pub struct PlayerRoundRecordActionRecordsMatchActionDataPosition {
    #[serde(rename = "$text")]
    pub text: Option<String>,
    pub x: String,
    pub y: String,
}

#[derive(Debug, Deserialize)]
pub struct Positions {
    #[serde(rename = "$text")]
    pub text: Option<String>,
    #[serde(rename = "MapVector", default)]
    pub map_vector: Vec<MapVector>,
}

#[derive(Debug, Deserialize)]
pub struct MapVector {
    #[serde(rename = "$text")]
    pub text: Option<String>,
    pub x: String,
    pub y: String,
}

#[derive(Debug, Deserialize)]
pub struct MoveUnitDatas {
    #[serde(rename = "$text")]
    pub text: Option<String>,
    #[serde(rename = "MoveUnitData", default)]
    pub move_unit_data: Vec<MoveUnitData>,
}

#[derive(Debug, Deserialize)]
pub struct MoveUnitData {
    #[serde(rename = "$text")]
    pub text: Option<String>,
    #[serde(rename = "unitID")]
    pub unit_id: String,
    #[serde(rename = "unitIndex")]
    pub unit_index: String,
    pub position: MatchActionDataMoveUnitDatasMoveUnitDataPosition,
    #[serde(rename = "isRotate")]
    pub is_rotate: String,
}

#[derive(Debug, Deserialize)]
pub struct MatchActionDataMoveUnitDatasMoveUnitDataPosition {
    #[serde(rename = "$text")]
    pub text: Option<String>,
    pub x: String,
    pub y: String,
}

#[derive(Debug, Deserialize)]
pub struct MatchDatas {
    #[serde(rename = "$text")]
    pub text: Option<String>,
    #[serde(rename = "MatchSnapshotData", default)]
    pub match_snapshot_data: Vec<MatchSnapshotData>,
}

#[derive(Debug, Deserialize)]
pub struct MatchSnapshotData {
    #[serde(rename = "$text")]
    pub text: Option<String>,
    pub round: String,
    #[serde(rename = "randomStateData")]
    pub random_state_data: RandomStateData,
    #[serde(rename = "reinforcePool")]
    pub reinforce_pool: ReinforcePool,
    #[serde(rename = "unitReinforceRounds")]
    pub unit_reinforce_rounds: UnitReinforceRounds,
    #[serde(rename = "lastFightResult")]
    pub last_fight_result: Option<LastFightResult>,
}

#[derive(Debug, Deserialize)]
pub struct RandomStateData {
    #[serde(rename = "$text")]
    pub text: Option<String>,
    #[serde(rename = "randomStates")]
    pub random_states: RandomStates,
}

#[derive(Debug, Deserialize)]
pub struct RandomStates {
    #[serde(rename = "$text")]
    pub text: Option<String>,
    #[serde(rename = "unsignedLong", default)]
    pub unsigned_long: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct ReinforcePool {
    #[serde(rename = "$text")]
    pub text: Option<String>,
    #[serde(default)]
    pub int: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct UnitReinforceRounds {
    #[serde(rename = "$text")]
    pub text: Option<String>,
    #[serde(default)]
    pub int: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct LastFightResult {
    #[serde(rename = "$text")]
    pub text: Option<String>,
    #[serde(rename = "Reports")]
    pub reports: Reports,
}

#[derive(Debug, Deserialize)]
pub struct Reports {
    #[serde(rename = "$text")]
    pub text: Option<String>,
    #[serde(rename = "FightReport", default)]
    pub fight_report: Vec<FightReport>,
}

#[derive(Debug, Deserialize)]
pub struct FightReport {
    #[serde(rename = "$text")]
    pub text: Option<String>,
    #[serde(rename = "Contraptions")]
    pub contraptions: FightReportContraptions,
    #[serde(rename = "unitDatas")]
    pub unit_datas: FightReportUnitDatas,
    #[serde(rename = "CommanderSkills")]
    pub commander_skills: FightReportCommanderSkills,
    #[serde(rename = "Score")]
    pub score: String,
    #[serde(rename = "DeadScore")]
    pub dead_score: String,
    #[serde(rename = "DestroyedCrystalCount")]
    pub destroyed_crystal_count: String,
    #[serde(rename = "AliveMechCount")]
    pub alive_mech_count: String,
    #[serde(rename = "FightRecord")]
    pub fight_record: FightRecord,
}

#[derive(Debug, Deserialize)]
pub struct FightReportContraptions {
    #[serde(rename = "$text")]
    pub text: Option<String>,
    #[serde(rename = "ContraptionData", default)]
    pub contraption_data: Vec<FightReportContraptionsContraptionData>,
}

#[derive(Debug, Deserialize)]
pub struct FightReportContraptionsContraptionData {
    #[serde(rename = "$text")]
    pub text: Option<String>,
    pub index: String,
    pub id: String,
    pub position: FightReportContraptionsContraptionDataPosition,
}

#[derive(Debug, Deserialize)]
pub struct FightReportContraptionsContraptionDataPosition {
    #[serde(rename = "$text")]
    pub text: Option<String>,
    pub x: String,
    pub y: String,
}

#[derive(Debug, Deserialize)]
pub struct FightReportUnitDatas {
    #[serde(rename = "$text")]
    pub text: Option<String>,
    #[serde(rename = "NewUnitData", default)]
    pub new_unit_data: Vec<UnitDatasNewUnitData>,
}

#[derive(Debug, Deserialize)]
pub struct UnitDatasNewUnitData {
    #[serde(rename = "$text")]
    pub text: Option<String>,
    pub id: String,
    #[serde(rename = "Index")]
    pub index: String,
    #[serde(rename = "RoundCount")]
    pub round_count: String,
    #[serde(rename = "Durability")]
    pub durability: String,
    #[serde(rename = "Exp")]
    pub exp: String,
    #[serde(rename = "Level")]
    pub level: String,
    #[serde(rename = "Position")]
    pub position: FightReportUnitDatasNewUnitDataPosition,
    #[serde(rename = "EquipmentID")]
    pub equipment_id: String,
    #[serde(rename = "IsRotate")]
    pub is_rotate: String,
    #[serde(rename = "SellSupply")]
    pub sell_supply: String,
}

#[derive(Debug, Deserialize)]
pub struct FightReportUnitDatasNewUnitDataPosition {
    #[serde(rename = "$text")]
    pub text: Option<String>,
    pub x: String,
    pub y: String,
}

#[derive(Debug, Deserialize)]
pub struct FightReportCommanderSkills {}

#[derive(Debug, Deserialize)]
pub struct FightRecord {}

#[derive(Debug, Deserialize)]
pub struct ReinforceItems {}

#[derive(Debug, Deserialize)]
pub struct BattleInfo {
    #[serde(rename = "$text")]
    pub text: Option<String>,
    #[serde(rename = "StartTime")]
    pub start_time: String,
    #[serde(rename = "SystemSeed")]
    pub system_seed: String,
    #[serde(rename = "BattleID")]
    pub battle_id: String,
    #[serde(rename = "PrepareTime")]
    pub prepare_time: String,
    #[serde(rename = "DeployTime")]
    pub deploy_time: String,
    #[serde(rename = "FightTime")]
    pub fight_time: String,
    #[serde(rename = "MapID")]
    pub map_id: String,
    #[serde(rename = "MaxRound")]
    pub max_round: String,
    #[serde(rename = "BlueprintIncreaseSupply")]
    pub blueprint_increase_supply: String,
    #[serde(rename = "EnableAdvanceTeam")]
    pub enable_advance_team: String,
    #[serde(rename = "EnableReinforcement")]
    pub enable_reinforcement: String,
    #[serde(rename = "GameMode")]
    pub game_mode: String,
    #[serde(rename = "MatchMode")]
    pub match_mode: String,
    #[serde(rename = "ScoreMode")]
    pub score_mode: String,
    #[serde(rename = "HostID")]
    pub host_id: String,
    #[serde(rename = "CloseUnitReinforce")]
    pub close_unit_reinforce: String,
    #[serde(rename = "SurviveModeDifficulty")]
    pub survive_mode_difficulty: String,
}
