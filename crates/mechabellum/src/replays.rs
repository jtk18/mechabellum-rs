mod battle_record;
mod detail;
mod game_data;
mod items;
mod map;
mod mode;
mod names;
mod overview;
mod parse_error;
mod player;
mod skills;
mod specialist;
mod units;

use chrono::prelude::{DateTime, SecondsFormat, Utc};
use serde_xml_rs::from_str;

pub use battle_record::BattleRecord;
pub use detail::unit_id_in_name;
pub use detail::{
    CardRef, MatchDetail, NamedRef, PlacedUnit, PlayerInfo, RoundLayout, SideLayout, Structure,
    TechRef, UnitTechs,
};
pub use game_data::{footprint, xp_to_next_level, GRID_SQUARE};
pub use items::Item;
pub use map::MapPoint;
pub use mode::GameMode;
pub use names::{resolve as resolve_name, GameNames, NameKind};
pub use overview::BattleOverview;
pub use parse_error::ParseError;
pub use player::*;
pub use skills::Skills;
pub use specialist::{specialist_name, Specialist, LEGACY_NAMES_BEFORE};
pub use units::*;

use std::ffi::OsStr;
use std::fs;
use std::io::Write;
use std::path::Path;

impl BattleRecord {
    pub fn new_from_files<P: AsRef<Path>>(
        files: &[P],
        save_embedded_delete_original: bool,
    ) -> (Vec<Self>, Vec<ParseError>) {
        let mut v = Vec::new();
        let mut e = Vec::new();

        for file in files {
            let file_path = file.as_ref();
            match Self::new_from_file(file_path, save_embedded_delete_original) {
                Ok(record) => v.push(record),
                Err(err) => e.push(err),
            }
        }

        (v, e)
    }

    pub fn new_from_file<P: AsRef<Path>>(
        file: P,
        save_embedded_delete_original: bool,
    ) -> Result<Self, ParseError> {
        let path = file.as_ref();
        let creation_system_time: DateTime<Utc> = path
            .metadata()
            .and_then(|md| md.created())
            .map_err(|err| ParseError::UnableToRead(path.into(), err.to_string().into()))?
            .into();
        let creation_time = creation_system_time.to_rfc3339_opts(SecondsFormat::Secs, true);
        let bytes = fs::read(path)
            .map_err(|err| ParseError::UnableToRead(path.into(), err.to_string().into()))?;

        let embedded_document = Self::extract_document(&bytes)
            .ok_or_else(|| ParseError::NoBattleRecord(path.into()))?;
        let mut record = Self::parse_document(embedded_document, path)?;
        record.create_time = creation_time;

        if save_embedded_delete_original && path.extension() == Some(&OsStr::new("grbr")) {
            let mk_file = path.with_extension("xml");
            fs::File::create(&mk_file)
                .and_then(|mut f| write!(f, "{}\n", embedded_document))
                .and_then(|_| fs::remove_file(path))
                .map_err(|e| {
                    ParseError::UnableToRemoveOriginal(path.into(), e.to_string().into())
                })?;
        }

        Ok(record)
    }

    /// Parse a replay held in memory: a `.grbr` or an already extracted `.xml`. `source` only
    /// labels errors. `create_time` is left as recorded in the replay; see
    /// `BattleOverview::occurred_on` for a usable timestamp.
    pub fn from_bytes(bytes: &[u8], source: &Path) -> Result<Self, ParseError> {
        let document = Self::extract_document(bytes)
            .ok_or_else(|| ParseError::NoBattleRecord(source.into()))?;
        Self::parse_document(document, source)
    }

    /// Find the embedded `<?xml ...>...</BattleRecord>` document inside a replay file.
    pub fn extract_document(bytes: &[u8]) -> Option<&str> {
        bytes.utf8_chunks().find_map(|chunk| {
            let valid = chunk.valid();
            let start = valid.find("<?xml")?;
            let end = valid.find("</BattleRecord>")? + "</BattleRecord>".len();
            valid.get(start..end)
        })
    }

    fn parse_document(document: &str, source: &Path) -> Result<Self, ParseError> {
        from_str(document).map_err(|e| {
            ParseError::UnableToParse(
                source.into(),
                document.to_owned().into(),
                e.to_string().into(),
            )
        })
    }
}

// tested in src/replays/overview.rs
