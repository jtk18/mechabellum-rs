//! SQLite storage: one row per match and per player, plus the compressed original replay so
//! everything derived can be rebuilt when the parser improves (`Store::reindex`).

use anyhow::{Context, Result};
use flate2::{read::GzDecoder, write::GzEncoder, Compression};
use mechabellum::{BattleRecord, PlayerSummary};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::{Mutex, MutexGuard, PoisonError};

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS matches (
    battle_id   TEXT PRIMARY KEY,
    version     INTEGER NOT NULL,
    match_mode  TEXT NOT NULL,
    game_mode   TEXT NOT NULL,
    map_id      TEXT NOT NULL,
    rounds      INTEGER NOT NULL,
    occurred_on TEXT NOT NULL,
    ingested_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);
CREATE TABLE IF NOT EXISTS players (
    battle_id TEXT NOT NULL REFERENCES matches(battle_id) ON DELETE CASCADE,
    player_id TEXT NOT NULL,
    name      TEXT NOT NULL,
    won       INTEGER NOT NULL,
    final_hp  INTEGER NOT NULL,
    score     INTEGER NOT NULL,
    PRIMARY KEY (battle_id, player_id)
);
CREATE TABLE IF NOT EXISTS player_units (
    battle_id TEXT NOT NULL,
    player_id TEXT NOT NULL,
    unit      TEXT NOT NULL,
    count     INTEGER NOT NULL,
    PRIMARY KEY (battle_id, player_id, unit),
    FOREIGN KEY (battle_id, player_id) REFERENCES players(battle_id, player_id) ON DELETE CASCADE
);
CREATE TABLE IF NOT EXISTS player_specialists (
    battle_id  TEXT NOT NULL,
    player_id  TEXT NOT NULL,
    specialist TEXT NOT NULL,
    PRIMARY KEY (battle_id, player_id, specialist),
    FOREIGN KEY (battle_id, player_id) REFERENCES players(battle_id, player_id) ON DELETE CASCADE
);
CREATE TABLE IF NOT EXISTS replay_blobs (
    battle_id TEXT PRIMARY KEY,
    xml_gz    BLOB NOT NULL
);
CREATE INDEX IF NOT EXISTS matches_version ON matches(version);
CREATE TABLE IF NOT EXISTS upload_keys (
    name       TEXT PRIMARY KEY,
    -- SHA-256 of the key; the key itself is shown once at creation and never stored
    key_hash   BLOB NOT NULL UNIQUE,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    revoked_at TEXT
);
-- Files already read from a watched replay folder, so restarts skip them without parsing.
CREATE TABLE IF NOT EXISTS ingested_files (
    path           TEXT PRIMARY KEY,
    size           INTEGER NOT NULL,
    modified       INTEGER NOT NULL,
    battle_id      TEXT,
    error          TEXT,
    parser_version TEXT NOT NULL
);
";

/// Failed files are retried when the parser changes.
const PARSER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Prefix on generated upload keys, so they're recognizable in configs and logs.
const KEY_PREFIX: &str = "mbk_";

/// All access goes through one connection, so writes from this process are serialized. Every
/// write runs in an IMMEDIATE transaction, which takes SQLite's write lock up front, so
/// writes are also serialized against other processes on the same file (e.g. `import` while
/// the server runs).
pub struct Store {
    conn: Mutex<Connection>,
}

#[derive(Debug, Serialize)]
pub struct UploadKey {
    pub name: String,
    pub created_at: String,
    pub revoked_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum IngestOutcome {
    Added { battle_id: String },
    Duplicate { battle_id: String },
}

#[derive(Debug, Default, Deserialize)]
pub struct StatsFilter {
    pub min_version: Option<u32>,
    pub max_version: Option<u32>,
    /// e.g. `VS_1_1`, `VS_2_2`, `VS_4_Scuffle`
    pub match_mode: Option<String>,
    /// e.g. `Normal`, `Competition`, `AsyncBattle`
    pub game_mode: Option<String>,
    /// Only count this player's side (account ID).
    pub player_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Stats {
    pub matches: u64,
    pub player_results: u64,
    /// Units in the final-round army, per player result.
    pub units: Vec<UsageStat>,
    pub specialists: Vec<UsageStat>,
}

#[derive(Debug, Serialize)]
pub struct UsageStat {
    pub name: String,
    /// Player results that fielded it.
    pub picks: u64,
    pub wins: u64,
    pub pick_rate: f64,
    pub win_rate: f64,
}

/// Pick and win counts per game version, for spotting balance changes between patches.
#[derive(Debug, Serialize)]
pub struct VersionTrends {
    /// Ascending. Every `cells` list below is aligned with this.
    pub versions: Vec<VersionTotals>,
    pub units: Vec<Trend>,
    pub specialists: Vec<Trend>,
}

#[derive(Debug, Serialize)]
pub struct VersionTotals {
    pub version: u32,
    pub matches: u64,
    pub player_results: u64,
}

#[derive(Debug, Serialize)]
pub struct Trend {
    pub name: String,
    pub picks: u64,
    /// One per version; `None` where it was never fielded.
    pub cells: Vec<Option<TrendCell>>,
}

#[derive(Debug, Serialize)]
pub struct TrendCell {
    pub picks: u64,
    pub wins: u64,
}

/// What the stored matches can be filtered by, with match counts.
#[derive(Debug, Serialize)]
pub struct Facets {
    /// Newest first.
    pub versions: Vec<(u32, u64)>,
    /// (match_mode, game_mode, matches), most common first.
    pub modes: Vec<(String, String, u64)>,
    /// The account with the most matches: on a desktop install, the owner.
    pub main_player: Option<(String, String)>,
}

#[derive(Debug, Serialize)]
pub struct MatchRow {
    pub battle_id: String,
    pub version: u32,
    pub match_mode: String,
    pub game_mode: String,
    pub rounds: u32,
    pub occurred_on: String,
    pub winners: Vec<String>,
    pub losers: Vec<String>,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        Self::init(Connection::open(path).with_context(|| format!("opening {}", path.display()))?)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        // Wait for another process's write lock instead of failing with SQLITE_BUSY.
        conn.busy_timeout(std::time::Duration::from_secs(10))?;
        // WAL + synchronous=NORMAL: commits skip an fsync each; a power cut can lose the last few
        // commits but never corrupts the database.
        conn.execute_batch(
            "PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL; PRAGMA foreign_keys = ON;",
        )?;
        conn.execute_batch(SCHEMA)?;
        let has_uploader = conn
            .prepare("SELECT 1 FROM pragma_table_info('matches') WHERE name = 'uploaded_by'")?
            .exists([])?;
        if !has_uploader {
            conn.execute_batch("ALTER TABLE matches ADD COLUMN uploaded_by TEXT")?;
        }
        Ok(Store {
            conn: Mutex::new(conn),
        })
    }

    /// A panic mid-transaction poisons the mutex. The transaction was rolled back when it
    /// unwound, so the connection is still consistent and can keep serving.
    pub(crate) fn conn(&self) -> MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Parse and store one replay (`.grbr` bytes or extracted XML) as a single transaction.
    /// `uploaded_by` records which upload key sent it.
    pub fn ingest(
        &self,
        bytes: &[u8],
        source: &Path,
        uploaded_by: Option<&str>,
    ) -> Result<IngestOutcome> {
        // Parse and compress before taking the lock so concurrent uploads only serialize on
        // the actual writes.
        let document = BattleRecord::extract_document(bytes)
            .with_context(|| format!("{}: no battle record found", source.display()))?;
        let record = BattleRecord::from_bytes(document.as_bytes(), source)?;
        let battle_id = match record.battle_info.battle_id.trim() {
            "" | "0" => format!("sha256:{:x}", Sha256::digest(document.as_bytes())),
            id => id.to_owned(),
        };
        let mut gz = GzEncoder::new(Vec::new(), Compression::default());
        gz.write_all(document.as_bytes())?;
        let xml_gz = gz.finish()?;

        let mut conn = self.conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let exists = tx
            .prepare_cached("SELECT 1 FROM matches WHERE battle_id = ?1")?
            .exists([&battle_id])?;
        if exists {
            return Ok(IngestOutcome::Duplicate { battle_id });
        }
        insert_record(&tx, &battle_id, &record, uploaded_by)?;
        tx.execute(
            "INSERT INTO replay_blobs (battle_id, xml_gz) VALUES (?1, ?2)",
            params![battle_id, xml_gz],
        )?;
        tx.commit()?;
        Ok(IngestOutcome::Added { battle_id })
    }

    /// Create an upload key. Returns the key, which is only available now; the database keeps
    /// its hash.
    pub fn create_key(&self, name: &str) -> Result<String> {
        let mut secret = [0u8; 32];
        getrandom::getrandom(&mut secret).map_err(|e| anyhow::anyhow!("rng: {e}"))?;
        let key = format!("{KEY_PREFIX}{}", hex(&secret));
        self.conn()
            .execute(
                "INSERT INTO upload_keys (name, key_hash) VALUES (?1, ?2)",
                params![name, Sha256::digest(key.as_bytes()).as_slice()],
            )
            .with_context(|| format!("creating key {name:?} (names must be unique)"))?;
        Ok(key)
    }

    /// Name of the active key matching `presented`, if any. Lookup is by SHA-256, so response
    /// time doesn't depend on how much of the key matched.
    pub fn verify_key(&self, presented: &str) -> Result<Option<String>> {
        if !presented.starts_with(KEY_PREFIX) {
            return Ok(None);
        }
        let hash = Sha256::digest(presented.as_bytes());
        Ok(self
            .conn()
            .query_row(
                "SELECT name FROM upload_keys WHERE key_hash = ?1 AND revoked_at IS NULL",
                [hash.as_slice()],
                |r| r.get(0),
            )
            .optional()?)
    }

    /// Returns false if no active key has that name.
    pub fn revoke_key(&self, name: &str) -> Result<bool> {
        let changed = self.conn().execute(
            "UPDATE upload_keys SET revoked_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
             WHERE name = ?1 AND revoked_at IS NULL",
            [name],
        )?;
        Ok(changed > 0)
    }

    pub fn list_keys(&self) -> Result<Vec<UploadKey>> {
        let conn = self.conn();
        let mut stmt =
            conn.prepare("SELECT name, created_at, revoked_at FROM upload_keys ORDER BY name")?;
        let keys = stmt
            .query_map([], |r| {
                Ok(UploadKey {
                    name: r.get(0)?,
                    created_at: r.get(1)?,
                    revoked_at: r.get(2)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(keys)
    }

    /// Whether `path` was already handled with this size and modification time. Failures count
    /// as handled until the parser version changes.
    pub fn file_seen(&self, path: &str, size: u64, modified: i64) -> Result<bool> {
        Ok(self
            .conn()
            .prepare_cached(
                "SELECT 1 FROM ingested_files
                 WHERE path = ?1 AND size = ?2 AND modified = ?3
                   AND (error IS NULL OR parser_version = ?4)",
            )?
            .exists(params![path, size as i64, modified, PARSER_VERSION])?)
    }

    pub fn mark_file(
        &self,
        path: &str,
        size: u64,
        modified: i64,
        result: std::result::Result<&str, &str>,
    ) -> Result<()> {
        let (battle_id, error) = match result {
            Ok(id) => (Some(id), None),
            Err(e) => (None, Some(e)),
        };
        self.conn().execute(
            "INSERT OR REPLACE INTO ingested_files
                 (path, size, modified, battle_id, error, parser_version)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                path,
                size as i64,
                modified,
                battle_id,
                error,
                PARSER_VERSION
            ],
        )?;
        Ok(())
    }

    /// Like `stats`, but broken down by game version. The filter's version bounds still apply.
    pub fn version_trends(&self, filter: &StatsFilter) -> Result<VersionTrends> {
        let conn = self.conn();
        let scope = "WITH scope AS (
                SELECT p.battle_id, p.player_id, p.won, m.version FROM players p
                JOIN matches m ON m.battle_id = p.battle_id
                WHERE (?1 IS NULL OR m.version >= ?1)
                  AND (?2 IS NULL OR m.version <= ?2)
                  AND (?3 IS NULL OR m.match_mode = ?3)
                  AND (?4 IS NULL OR m.game_mode = ?4)
                  AND (?5 IS NULL OR p.player_id = ?5))";
        let args = params![
            filter.min_version,
            filter.max_version,
            filter.match_mode,
            filter.game_mode,
            filter.player_id
        ];

        let versions: Vec<VersionTotals> = conn
            .prepare(&format!(
                "{scope} SELECT version, COUNT(DISTINCT battle_id), COUNT(*) FROM scope
                 GROUP BY version ORDER BY version"
            ))?
            .query_map(args, |r| {
                Ok(VersionTotals {
                    version: r.get(0)?,
                    matches: r.get(1)?,
                    player_results: r.get(2)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        let column: BTreeMap<u32, usize> = versions
            .iter()
            .enumerate()
            .map(|(i, v)| (v.version, i))
            .collect();

        let trends = |table: &str, name: &str| -> Result<Vec<Trend>> {
            let mut by_name: BTreeMap<String, Trend> = BTreeMap::new();
            let sql = format!(
                "{scope} SELECT t.{name}, s.version, COUNT(*), SUM(s.won) FROM {table} t
                 JOIN scope s ON s.battle_id = t.battle_id AND s.player_id = t.player_id
                 GROUP BY t.{name}, s.version"
            );
            let mut stmt = conn.prepare(&sql)?;
            let mut rows = stmt.query(args)?;
            while let Some(row) = rows.next()? {
                let (label, version, picks, wins): (String, u32, u64, u64) =
                    (row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?);
                let trend = by_name.entry(label.clone()).or_insert_with(|| Trend {
                    name: label,
                    picks: 0,
                    cells: (0..versions.len()).map(|_| None).collect(),
                });
                trend.picks += picks;
                if let Some(&i) = column.get(&version) {
                    trend.cells[i] = Some(TrendCell { picks, wins });
                }
            }
            let mut out: Vec<Trend> = by_name.into_values().collect();
            out.sort_by(|a, b| b.picks.cmp(&a.picks).then_with(|| a.name.cmp(&b.name)));
            Ok(out)
        };
        let units = trends("player_units", "unit")?;
        let specialists = trends("player_specialists", "specialist")?;
        Ok(VersionTrends {
            versions,
            units,
            specialists,
        })
    }

    pub fn facets(&self) -> Result<Facets> {
        let conn = self.conn();
        let versions = conn
            .prepare(
                "SELECT version, COUNT(*) FROM matches GROUP BY version ORDER BY version DESC",
            )?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        let modes = conn
            .prepare(
                "SELECT match_mode, game_mode, COUNT(*) FROM matches
                 GROUP BY match_mode, game_mode ORDER BY COUNT(*) DESC",
            )?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<rusqlite::Result<_>>()?;
        let main_player = conn
            .query_row(
                // Name from their most recent match: accounts can be renamed.
                "SELECT p.player_id,
                        (SELECT q.name FROM players q JOIN matches m ON m.battle_id = q.battle_id
                         WHERE q.player_id = p.player_id
                         ORDER BY m.occurred_on DESC, m.version DESC LIMIT 1)
                 FROM players p GROUP BY p.player_id ORDER BY COUNT(*) DESC LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        Ok(Facets {
            versions,
            modes,
            main_player,
        })
    }

    pub fn match_count(&self) -> Result<u64> {
        Ok(self
            .conn()
            .query_row("SELECT COUNT(*) FROM matches", [], |r| r.get(0))?)
    }

    pub fn active_key_count(&self) -> Result<u64> {
        Ok(self.conn().query_row(
            "SELECT COUNT(*) FROM upload_keys WHERE revoked_at IS NULL",
            [],
            |r| r.get(0),
        )?)
    }

    /// Rebuild every derived row from the stored replays. Run after upgrading the parser.
    pub fn reindex(&self) -> Result<usize> {
        let mut conn = self.conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let uploaders: BTreeMap<String, Option<String>> = tx
            .prepare("SELECT battle_id, uploaded_by FROM matches")?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        let blobs: Vec<(String, Vec<u8>)> = tx
            .prepare("SELECT battle_id, xml_gz FROM replay_blobs")?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        tx.execute("DELETE FROM matches", [])?;
        for (battle_id, gz) in &blobs {
            let mut xml = String::new();
            GzDecoder::new(gz.as_slice()).read_to_string(&mut xml)?;
            let record = BattleRecord::from_bytes(xml.as_bytes(), Path::new(battle_id))?;
            let uploaded_by = uploaders.get(battle_id).cloned().flatten();
            insert_record(&tx, battle_id, &record, uploaded_by.as_deref())?;
        }
        tx.commit()?;
        Ok(blobs.len())
    }

    pub fn recent_matches(&self, limit: u32) -> Result<Vec<MatchRow>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT m.battle_id, m.version, m.match_mode, m.game_mode, m.rounds, m.occurred_on,
                    COALESCE((SELECT group_concat(name, char(31)) FROM players p
                              WHERE p.battle_id = m.battle_id AND p.won = 1), ''),
                    COALESCE((SELECT group_concat(name, char(31)) FROM players p
                              WHERE p.battle_id = m.battle_id AND p.won = 0), '')
             FROM matches m ORDER BY m.occurred_on DESC LIMIT ?1",
        )?;
        let split = |s: String| -> Vec<String> {
            s.split('\u{1f}')
                .filter(|n| !n.is_empty())
                .map(str::to_owned)
                .collect()
        };
        let rows = stmt
            .query_map([limit], |r| {
                Ok(MatchRow {
                    battle_id: r.get(0)?,
                    version: r.get(1)?,
                    match_mode: r.get(2)?,
                    game_mode: r.get(3)?,
                    rounds: r.get(4)?,
                    occurred_on: r.get(5)?,
                    winners: split(r.get(6)?),
                    losers: split(r.get(7)?),
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    pub fn stats(&self, filter: &StatsFilter) -> Result<Stats> {
        let conn = self.conn();
        // Filtered player results. Every stat below is computed over this set.
        let scope = "WITH scope AS (
                SELECT p.battle_id, p.player_id, p.won FROM players p
                JOIN matches m ON m.battle_id = p.battle_id
                WHERE (?1 IS NULL OR m.version >= ?1)
                  AND (?2 IS NULL OR m.version <= ?2)
                  AND (?3 IS NULL OR m.match_mode = ?3)
                  AND (?4 IS NULL OR m.game_mode = ?4)
                  AND (?5 IS NULL OR p.player_id = ?5))";
        let args = params![
            filter.min_version,
            filter.max_version,
            filter.match_mode,
            filter.game_mode,
            filter.player_id
        ];

        let (matches, player_results): (u64, u64) = conn.query_row(
            &format!("{scope} SELECT COUNT(DISTINCT battle_id), COUNT(*) FROM scope"),
            args,
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;

        let usage = |table: &str, column: &str| -> Result<Vec<UsageStat>> {
            let sql = format!(
                "{scope} SELECT t.{column}, COUNT(*), SUM(s.won) FROM {table} t
                 JOIN scope s ON s.battle_id = t.battle_id AND s.player_id = t.player_id
                 GROUP BY t.{column} ORDER BY COUNT(*) DESC, t.{column}"
            );
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt
                .query_map(args, |r| {
                    let picks: u64 = r.get(1)?;
                    let wins: u64 = r.get(2)?;
                    Ok(UsageStat {
                        name: r.get(0)?,
                        picks,
                        wins,
                        pick_rate: ratio(picks, player_results),
                        win_rate: ratio(wins, picks),
                    })
                })?
                .collect::<rusqlite::Result<_>>()?;
            Ok(rows)
        };

        Ok(Stats {
            matches,
            player_results,
            units: usage("player_units", "unit")?,
            specialists: usage("player_specialists", "specialist")?,
        })
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn ratio(n: u64, d: u64) -> f64 {
    if d == 0 {
        0.0
    } else {
        n as f64 / d as f64
    }
}

fn insert_record(
    tx: &rusqlite::Transaction,
    battle_id: &str,
    record: &BattleRecord,
    uploaded_by: Option<&str>,
) -> Result<()> {
    let overview = record.get_battle_overview()?;
    tx.execute(
        "INSERT INTO matches
             (battle_id, version, match_mode, game_mode, map_id, rounds, occurred_on, uploaded_by)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            battle_id,
            overview.version,
            record.battle_info.match_mode,
            record.battle_info.game_mode,
            overview.map_id,
            overview.rounds as i64,
            overview.occurred_on,
            uploaded_by,
        ],
    )?;
    let sides = overview
        .winners
        .iter()
        .map(|p| (p, true))
        .chain(overview.losers.iter().map(|p| (p, false)));
    for (player, won) in sides {
        insert_player(tx, battle_id, player, won)?;
    }
    Ok(())
}

fn insert_player(
    tx: &rusqlite::Transaction,
    battle_id: &str,
    player: &PlayerSummary,
    won: bool,
) -> Result<()> {
    // Bots and some special modes have no account ID; fall back to the name.
    let player_id = match player.id.trim() {
        "" | "0" => player.name.as_str(),
        id => id,
    };
    tx.execute(
        "INSERT OR IGNORE INTO players (battle_id, player_id, name, won, final_hp, score)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            battle_id,
            player_id,
            player.name.as_str(),
            won,
            player.last_round_hp,
            player.score as i64
        ],
    )?;

    let mut units: BTreeMap<String, i64> = BTreeMap::new();
    for unit in &player.last_round_units {
        *units.entry(unit.unit_type.display_name()).or_default() += 1;
    }
    for (unit, count) in units {
        tx.execute(
            "INSERT INTO player_units (battle_id, player_id, unit, count) VALUES (?1, ?2, ?3, ?4)",
            params![battle_id, player_id, unit, count],
        )?;
    }
    for specialist in &player.specialist_cards {
        tx.execute(
            "INSERT OR IGNORE INTO player_specialists (battle_id, player_id, specialist)
             VALUES (?1, ?2, ?3)",
            params![battle_id, player_id, specialist.display_name()],
        )?;
    }
    Ok(())
}
