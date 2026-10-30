//! SQLite storage: one row per match and per player, plus the compressed original replay so
//! everything derived can be rebuilt when the parser improves (`Store::reindex`).

use anyhow::{Context, Result};
use flate2::{read::GzDecoder, write::GzEncoder, Compression};
use mechabellum::{BattleRecord, MatchDetail, PlayerSummary};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};

use crate::pseudonym::Pseudonyms;

const SCHEMA: &str = "
-- Upstream: each player's copy of a shared match, reduced to a fingerprint of what the game
-- recorded (armies, HP, results). Copies from two players that agree corroborate the match.
CREATE TABLE IF NOT EXISTS match_copies (
    battle_id   TEXT NOT NULL,
    uploaded_by TEXT NOT NULL,
    player_id   TEXT NOT NULL,
    fingerprint TEXT NOT NULL,
    at          TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    PRIMARY KEY (battle_id, uploaded_by)
);
CREATE TABLE IF NOT EXISTS matches (
    battle_id   TEXT PRIMARY KEY,
    version     INTEGER NOT NULL,
    match_mode  TEXT NOT NULL,
    game_mode   TEXT NOT NULL,
    map_id      TEXT NOT NULL,
    rounds      INTEGER NOT NULL,
    occurred_on TEXT NOT NULL,
    ingested_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    -- 0 when the replay doesn't say who won (mechabellum's results_settled): kept for the match
    -- list, left out of every stat.
    settled     INTEGER NOT NULL DEFAULT 1
);
CREATE TABLE IF NOT EXISTS players (
    battle_id TEXT NOT NULL REFERENCES matches(battle_id) ON DELETE CASCADE,
    player_id TEXT NOT NULL,
    name      TEXT NOT NULL,
    won       INTEGER NOT NULL,
    final_hp  INTEGER NOT NULL,
    score     INTEGER NOT NULL,
    -- JSON mechabellum::StarterPack, when the replay records the pick.
    starter_pack TEXT,
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
-- Techs researched by the final round, for unit types the player fielded then.
CREATE TABLE IF NOT EXISTS player_techs (
    battle_id TEXT NOT NULL,
    player_id TEXT NOT NULL,
    unit      TEXT NOT NULL,
    tech      TEXT NOT NULL,
    -- The game's ID for the tech, for its icon. Names repeat across units; IDs don't.
    tech_id   TEXT,
    PRIMARY KEY (battle_id, player_id, unit, tech),
    FOREIGN KEY (battle_id, player_id) REFERENCES players(battle_id, player_id) ON DELETE CASCADE
);
-- Cards picked between rounds (specialists, unit reinforcements, skills, items).
CREATE TABLE IF NOT EXISTS player_cards (
    battle_id TEXT NOT NULL,
    player_id TEXT NOT NULL,
    card      TEXT NOT NULL,
    kind      TEXT NOT NULL,
    picks     INTEGER NOT NULL,
    PRIMARY KEY (battle_id, player_id, card),
    FOREIGN KEY (battle_id, player_id) REFERENCES players(battle_id, player_id) ON DELETE CASCADE
);
-- The whole match from each player's side, for match cards and whole-game stats.
CREATE TABLE IF NOT EXISTS player_games (
    battle_id   TEXT NOT NULL,
    player_id   TEXT NOT NULL,
    fights_won  INTEGER NOT NULL,
    final_units INTEGER NOT NULL,
    peak_units  INTEGER NOT NULL,
    -- Supply the last fight's army, and the costliest one, were worth; NULL when unpriced.
    final_value INTEGER,
    peak_value  INTEGER,
    mean_level  REAL NOT NULL,
    max_level   INTEGER NOT NULL,
    -- NULL unless the fights run from the first without a gap, and in AsyncBattle, whose
    -- opponents' rounds don't follow on from each other.
    level_ups   INTEGER,
    units_added INTEGER,
    units_sold  INTEGER,
    techs       INTEGER NOT NULL,
    cards       INTEGER,
    -- Round 1's deployment label, which classes the match for this player.
    opening     TEXT,
    -- How round 1's deployments met when both were one-sided (an asym headbutt).
    meeting     TEXT,
    -- JSON mechabellum::PlayerGame, which a match card draws.
    card        TEXT NOT NULL,
    PRIMARY KEY (battle_id, player_id),
    FOREIGN KEY (battle_id, player_id) REFERENCES players(battle_id, player_id) ON DELETE CASCADE
);
-- Every fight from each side, with how the army was deployed (NULL off the 2.0 board or
-- outside 1v1; see mechabellum::classify).
CREATE TABLE IF NOT EXISTS player_fights (
    battle_id  TEXT NOT NULL,
    player_id  TEXT NOT NULL,
    fight      INTEGER NOT NULL,
    result     TEXT NOT NULL,
    hp         INTEGER NOT NULL,
    units      INTEGER NOT NULL,
    deployment TEXT,
    PRIMARY KEY (battle_id, player_id, fight),
    FOREIGN KEY (battle_id, player_id) REFERENCES players(battle_id, player_id) ON DELETE CASCADE
);
-- Every unit type a player fielded during the match, not only in the final army.
CREATE TABLE IF NOT EXISTS player_unit_use (
    battle_id TEXT NOT NULL,
    player_id TEXT NOT NULL,
    unit      TEXT NOT NULL,
    -- NULL when the match's own units added and sold are (see player_games).
    added     INTEGER,
    sold      INTEGER,
    peak      INTEGER NOT NULL,
    last      INTEGER NOT NULL,
    max_level INTEGER NOT NULL,
    PRIMARY KEY (battle_id, player_id, unit),
    FOREIGN KEY (battle_id, player_id) REFERENCES players(battle_id, player_id) ON DELETE CASCADE
);
-- Ratings at match start, captured from the game's log (replays don't record them).
CREATE TABLE IF NOT EXISTS player_ratings (
    battle_id   TEXT NOT NULL,
    player_id   TEXT NOT NULL,
    elo         INTEGER,
    rank_point  INTEGER,
    level       INTEGER,
    elo_games   INTEGER,
    PRIMARY KEY (battle_id, player_id)
);
-- Local app: which matches were offered to which community server, and any refusal.
CREATE TABLE IF NOT EXISTS synced_matches (
    server    TEXT NOT NULL,
    battle_id TEXT NOT NULL,
    rejected  TEXT,
    at        TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    PRIMARY KEY (server, battle_id)
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
-- Local app: the account logged in to the game on this machine, from its Player.log.
CREATE TABLE IF NOT EXISTS local_player (
    only      INTEGER PRIMARY KEY CHECK (only = 0),
    player_id TEXT NOT NULL
);
-- Community members who signed in with Steam. The Steam ID itself isn't kept: `steam` is its
-- pseudonym. Each sign-in adds an upload key with `account` set; uploads are recorded under the
-- account's name, so a member's devices count as one uploader.
CREATE TABLE IF NOT EXISTS accounts (
    name         TEXT PRIMARY KEY,
    steam        TEXT NOT NULL UNIQUE,
    -- The Mechabellum account it shares games for, bound on its first upload. Not unique: Steam
    -- can't say which Mechabellum account is whose, so an exclusive claim would only let the
    -- first claimant lock the real player out, and tell anyone who tries that the account shares.
    player_id    TEXT,
    created_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    signed_in_at TEXT,
    disabled_at  TEXT
);
-- Settings that belong with the data: whether stored replays are pseudonymized, and a check
-- value for the key that did it.
CREATE TABLE IF NOT EXISTS meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
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

/// Bumped when derived tables change; opening an older database rebuilds them from the stored
/// replays. 2: player_techs. 3: player_cards. 4: specialist names by game version.
/// 5: starting specialists count as picks. 6: dates of offline games. 7: tech IDs.
/// 8: the last round's army is rebuilt from its actions, not its starting snapshot.
/// 9: starter packs. 10: whole-match summaries and deployments per fight (`player_games`,
/// `player_fights`, `player_unit_use`); version 2340's fight results, which 9 misread as wins
/// for both players. 11: a match is classed by round 1's deployment (`opening`), not the one used
/// in the most fights. 12: midboard armies are sym or asym too. 13: the review's fixes: version
/// 2340 results never fall back to "Win", a concession during deployment ends the match a fight
/// early, techs include the last round's research, level-ups count upgrades bought, units
/// added and sold need fights from the first; the derived tables are recreated. 14: aggressive
/// reaches 5 squares, a one-sided standard-depth army is "Asym mid aggro", round 1's meeting
/// (asym headbutt), and army values in supply. 15: base prices for the big units (Mountain, War
/// Factory and Abyss 800, Overlord 500, giants 400). 16: three bands; standard is a symmetric
/// midboard army, Asym mid aggro a one-sided one. 17: asym armies on opposite lanes are an asym
/// crossfire. 18: implausible positions, levels and sell values are bounded; level-ups and cards
/// need fights from the first; a pre-2340 result needs the round before the last; matches that
/// don't say who won (`matches.settled`) leave the stats. 19: a match with no fight recorded
/// isn't settled either. 20: per-unit added and sold are NULL when the match's are; the rebuild
/// counts a unit card as at least one unit and only summoning skills as summons.
const SCHEMA_VERSION: i64 = 20;

/// Failed files are retried when the parser changes.
const PARSER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Active upload keys a signed-in account keeps, one per device; a new one past this revokes the
/// oldest.
const ACCOUNT_KEYS: i64 = 5;

/// Prefix on generated upload keys, so they're recognizable in configs and logs.
const KEY_PREFIX: &str = "mbk_";

/// All access goes through one connection, so writes from this process are serialized. Every
/// write runs in an IMMEDIATE transaction, which takes SQLite's write lock up front, so
/// writes are also serialized against other processes on the same file (e.g. `import` while
/// the server runs).
pub struct Store {
    conn: Mutex<Connection>,
    /// The database file; `None` in memory. A rebuild stages its rows in a file beside it.
    path: Option<PathBuf>,
    /// One rebuild at a time: each one replaces every derived row.
    rebuilding: Mutex<()>,
    /// Set on a community server: every replay is stored with pseudonymous player IDs and no
    /// names (see `pseudonym.rs`).
    pseudonyms: OnceLock<Pseudonyms>,
}

/// A community member who signed in with Steam.
#[derive(Debug, Serialize)]
pub struct Account {
    pub name: String,
    pub created_at: String,
    pub signed_in_at: Option<String>,
    pub disabled_at: Option<String>,
    /// Whether it shares games for a Mechabellum account yet.
    pub bound: bool,
    pub active_keys: u64,
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

/// Why a shared upload was turned away. `status` is the HTTP status to answer with.
#[derive(Debug, Serialize)]
pub struct Rejection {
    pub status: u16,
    pub reason: String,
    pub problems: Vec<mechabellum::Problem>,
}

/// A player's rating when a match started.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rating {
    pub player_id: String,
    pub elo: Option<i64>,
    pub rank_point: Option<i64>,
    pub level: Option<i64>,
    /// Ranked games played, when known.
    pub elo_games: Option<i64>,
}

/// Largest value a rating field may hold. Real MMR, rank points and game counts are far below
/// it; an upload key once stored i64::MAX and i64::MIN, and the MMR difference in `stats`
/// overflowed into a REAL that failed every stats query (red team, 2026-10-27).
const MAX_RATING: i64 = 1_000_000;

impl Rating {
    fn plausible(&self) -> bool {
        [self.elo, self.rank_point, self.level, self.elo_games]
            .into_iter()
            .flatten()
            .all(|v| (0..=MAX_RATING).contains(&v))
    }
}

/// Largest replay document accepted, uncompressed. Real ones are 0.3-2 MB of XML; a crafted one
/// with thousands of units bloats every card and match view built from it.
pub const MAX_DOCUMENT_BYTES: usize = 16 * 1024 * 1024;

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
    /// Only count the other sides of this player's matches (their opponents).
    pub opponents_of: Option<String>,
    /// How `version_trends` groups columns. Other queries ignore it.
    #[serde(default)]
    pub period: Period,
    /// Only count matches that two players shared and whose copies agree.
    #[serde(default)]
    pub corroborated_only: bool,
    /// Count offline games against bots too (practice and Testing Grounds, whose battle IDs
    /// are timestamps like `25-01-27__20-36-14-492`). Left out by default.
    #[serde(default)]
    pub include_offline: bool,
}

/// Matches with copies from at least two players that agree, and none that disagree.
const CORROBORATED: &str = "
    SELECT battle_id FROM match_copies GROUP BY battle_id
    HAVING COUNT(DISTINCT fingerprint) = 1 AND COUNT(DISTINCT player_id) >= 2";

/// Columns of a trend table.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Period {
    /// One column per game build.
    #[default]
    Build,
    /// One column per calendar month the games were played in.
    Month,
}

impl Period {
    /// SQL expression over `scope` columns naming a result's column, and one to order columns.
    fn sql(self) -> (&'static str, &'static str) {
        match self {
            Period::Build => ("CAST(version AS TEXT)", "MIN(version)"),
            Period::Month => ("substr(occurred_on, 1, 7)", "MIN(occurred_on)"),
        }
    }
}

/// SQL true for a match `m` the account logged in here played. A replay of a match only watched
/// is saved too; those are left out. With no logged-in account, or one with no stored match yet
/// (a fresh account), every match counts. Account IDs survive a renamed Steam account.
macro_rules! played_here {
    () => {
        "(NOT EXISTS (SELECT 1 FROM local_player l JOIN players q ON q.player_id = l.player_id)
          OR EXISTS (SELECT 1 FROM players own JOIN local_player l ON own.player_id = l.player_id
                     WHERE own.battle_id = m.battle_id))"
    };
}

/// `scope(battle_id, player_id, won, version, occurred_on)` over the player results `StatsFilter`
/// allows, with `$settled` added to the conditions.
macro_rules! scope {
    ($settled:literal) => {
        concat!(
            "WITH scope AS (
    SELECT p.battle_id, p.player_id, p.won, m.version, m.occurred_on FROM players p
    JOIN matches m ON m.battle_id = p.battle_id
    WHERE (?1 IS NULL OR m.version >= ?1)
      AND (?2 IS NULL OR m.version <= ?2)
      AND (?3 IS NULL OR m.match_mode = ?3)
      AND (?4 IS NULL OR m.game_mode = ?4)
      AND (?5 IS NULL OR p.player_id = ?5)
      AND (?6 IS NULL OR (p.player_id != ?6 AND EXISTS (
          SELECT 1 FROM players me WHERE me.battle_id = p.battle_id AND me.player_id = ?6
          -- Teams aren't stored; a 2v2 teammate shares the result, an opponent doesn't.
          AND (m.match_mode != 'VS_2_2' OR me.won IS NOT p.won))))
      AND (?7 = 0 OR m.battle_id IN (
          SELECT battle_id FROM match_copies GROUP BY battle_id
          HAVING COUNT(DISTINCT fingerprint) = 1 AND COUNT(DISTINCT player_id) >= 2))
      AND (?8 = 1 OR instr(m.battle_id, '__') = 0)
      AND ",
            played_here!(),
            $settled,
            ")"
        )
    };
}

/// The player results a stats query counts. Parameters come from `StatsFilter::params`. A match
/// whose replay doesn't say who won can't count toward any rate.
pub(crate) const SCOPE: &str = scope!(" AND m.settled = 1");
/// The same without the settled condition, for listing matches: one with no known winner is
/// still shown, as "result unknown".
pub(crate) const MATCH_SCOPE: &str = scope!("");

impl StatsFilter {
    pub(crate) fn params(&self) -> [&dyn rusqlite::ToSql; 8] {
        [
            &self.min_version,
            &self.max_version,
            &self.match_mode,
            &self.game_mode,
            &self.player_id,
            &self.opponents_of,
            &self.corroborated_only,
            &self.include_offline,
        ]
    }
}

#[derive(Debug, Serialize)]
pub struct Stats {
    pub matches: u64,
    pub player_results: u64,
    /// Units in the final-round army, per player result.
    pub units: Vec<UsageStat>,
    /// Specialists held at the end, including ones the game granted without a pick.
    pub specialists: Vec<UsageStat>,
    /// Specialists the player chose: their starting specialists and picks from offered cards.
    pub picked_specialists: Vec<UsageStat>,
    /// Specialists held at the end that were never picked: granted by the game, for example by
    /// the Research Center.
    pub granted_specialists: Vec<UsageStat>,
    /// Win rate by how the opponent's MMR compared, in 1v1 games where both ratings are known.
    pub vs_rating: Vec<RatingBand>,
}

/// Results against opponents whose MMR was within a range of the player's own.
#[derive(Debug, Serialize)]
pub struct RatingBand {
    pub name: String,
    /// Opponent's MMR minus the player's, inclusive lower bound; `None` is unbounded.
    pub from: Option<i64>,
    /// Exclusive upper bound; `None` is unbounded.
    pub to: Option<i64>,
    pub games: u64,
    pub wins: u64,
    pub win_rate: f64,
}

/// Edges between rating bands (opponent minus player).
const RATING_EDGES: [i64; 4] = [-150, -50, 50, 150];

fn rating_bands(differences: &[(i64, bool)]) -> Vec<RatingBand> {
    let names = [
        "Opponent 150+ lower",
        "50 to 150 lower",
        "Within 50",
        "50 to 150 higher",
        "Opponent 150+ higher",
    ];
    names
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let from = i.checked_sub(1).map(|j| RATING_EDGES[j]);
            let to = RATING_EDGES.get(i).copied();
            let inside = |d: i64| from.is_none_or(|f| d >= f) && to.is_none_or(|t| d < t);
            let (games, wins) = differences
                .iter()
                .filter(|(d, _)| inside(*d))
                .fold((0u64, 0u64), |(g, w), (_, won)| (g + 1, w + *won as u64));
            RatingBand {
                name: (*name).to_owned(),
                from,
                to,
                games,
                wins,
                win_rate: if games == 0 {
                    0.0
                } else {
                    wins as f64 / games as f64
                },
            }
        })
        .collect()
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
    /// The column's build number or month (`2026-09`).
    pub label: String,
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

/// How each tech affects a unit's results: win rate with the tech researched vs. without.
#[derive(Debug, Serialize)]
pub struct UnitTechStats {
    pub unit: String,
    /// Player results that fielded the unit in the final round.
    pub picks: u64,
    pub wins: u64,
    /// Most researched first.
    pub techs: Vec<TechStat>,
}

#[derive(Debug, Serialize)]
pub struct TechStat {
    pub tech: String,
    /// The game's ID for this tech on this unit, when known (for its icon).
    pub tech_id: Option<String>,
    /// Results that fielded the unit with this tech researched.
    pub picks: u64,
    pub wins: u64,
    /// Share of the unit's results that had this tech.
    pub pick_rate: f64,
    pub win_rate: f64,
    /// Win rate of the unit's results without this tech. `None` if every result had it.
    pub win_rate_without: Option<f64>,
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
pub struct RobustStats {
    pub units: Vec<crate::aggregate::RobustStat>,
    pub specialists: Vec<crate::aggregate::RobustStat>,
}

/// A shared match whose copies disagree.
#[derive(Debug, Serialize)]
pub struct Dispute {
    pub battle_id: String,
    pub copies: Vec<MatchCopy>,
}

#[derive(Debug, Serialize)]
pub struct MatchCopy {
    pub uploaded_by: String,
    pub player_id: String,
    pub fingerprint: String,
    pub at: String,
}

/// Derived tables, parents first: what `insert_record` writes and a rebuild replaces.
const DERIVED_TABLES: [&str; 9] = [
    "matches",
    "players",
    "player_units",
    "player_specialists",
    "player_techs",
    "player_cards",
    "player_games",
    "player_fights",
    "player_unit_use",
];

/// Stored replays read per query during a rebuild.
const REBUILD_PAGE: usize = 16;
/// Matches written to a rebuild's scratch database per transaction.
const REBUILD_BATCH: usize = 128;
/// Most parser threads a rebuild uses. Each holds one parsed replay (about 1 MB) at a time.
const REBUILD_WORKERS: usize = 8;
/// Unlocked rounds for replays stored while a rebuild stages. Each is shorter than the last;
/// the bound is for uploads that never stop.
const REBUILD_ROUNDS: usize = 3;

/// A replay as stored, on its way to the parser threads.
struct StoredReplay {
    battle_id: String,
    xml_gz: Vec<u8>,
    uploaded_by: Option<String>,
}

/// A replay parsed for a rebuild; `parsed` is `None` when it couldn't be read.
struct StagedReplay {
    battle_id: String,
    uploaded_by: Option<String>,
    parsed: Option<(BattleRecord, MatchDetail)>,
}

/// A rebuild's scratch database file, deleted when dropped.
struct Scratch {
    path: PathBuf,
}

impl Scratch {
    /// Beside the database file, so it's on the same disk; in the temp folder for an in-memory
    /// database. The name is unique to this process and rebuild.
    fn beside(db: Option<&Path>) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let unique = format!(
            "rebuild-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        let path = match db {
            Some(db) => {
                let mut name = db.as_os_str().to_owned();
                name.push(format!(".{unique}"));
                PathBuf::from(name)
            }
            None => std::env::temp_dir().join(format!("mechabellum-{unique}.db")),
        };
        let scratch = Scratch { path };
        scratch.remove();
        scratch
    }

    fn remove(&self) {
        for suffix in ["", "-journal", "-wal", "-shm"] {
            let mut name = self.path.as_os_str().to_owned();
            name.push(suffix);
            let _ = std::fs::remove_file(PathBuf::from(name));
        }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        self.remove();
    }
}

/// Decompress and parse one stored replay. An unreadable one is logged and skipped, so the rest
/// of the database stays usable.
fn parse_stored(battle_id: &str, gz: &[u8]) -> Option<(BattleRecord, MatchDetail)> {
    let parsed = contained(battle_id, || {
        let mut xml = String::new();
        GzDecoder::new(gz).read_to_string(&mut xml)?;
        let record = BattleRecord::from_bytes(xml.as_bytes(), Path::new(battle_id))?;
        let detail = record.match_detail();
        Ok((record, detail))
    });
    match parsed {
        Ok(parsed) => Some(parsed),
        Err(e) => {
            log::warn!("reindex {battle_id}: {e:#}");
            None
        }
    }
}

/// Write parsed replays into a rebuild's scratch database as they arrive. One record that won't
/// insert is logged and skipped, not allowed to undo the rest: every command upgrades first, so
/// a failure here would stop the server starting. Unreadable ones go in `rebuild_failed`, so
/// they aren't taken for replays stored meanwhile.
fn write_staged(out: &mut Connection, staged: mpsc::Receiver<StagedReplay>) -> Result<usize> {
    let mut written = 0;
    let mut staged = staged.into_iter().peekable();
    while staged.peek().is_some() {
        let mut tx = out.transaction()?;
        for replay in staged.by_ref().take(REBUILD_BATCH) {
            written += 1;
            let battle_id = replay.battle_id.as_str();
            let stored = match &replay.parsed {
                Some((record, detail)) => {
                    let savepoint = tx.savepoint()?;
                    match contained(battle_id, || {
                        insert_record(
                            &savepoint,
                            battle_id,
                            record,
                            detail,
                            replay.uploaded_by.as_deref(),
                        )
                    }) {
                        Ok(()) => {
                            savepoint.commit()?;
                            true
                        }
                        Err(e) => {
                            log::warn!("reindex {battle_id}: not stored: {e:#}");
                            false
                        }
                    }
                }
                None => false,
            };
            if !stored {
                tx.execute(
                    "INSERT OR IGNORE INTO rebuild_failed (battle_id) VALUES (?1)",
                    [battle_id],
                )?;
            }
        }
        tx.commit()?;
    }
    Ok(written)
}

/// Creates the tables and adds the columns later versions brought, on the database and on a
/// rebuild's scratch copy alike.
fn migrate(conn: &Connection) -> Result<()> {
    conn.execute_batch(SCHEMA)?;
    let has_uploader = conn
        .prepare("SELECT 1 FROM pragma_table_info('matches') WHERE name = 'uploaded_by'")?
        .exists([])?;
    if !has_uploader {
        conn.execute_batch("ALTER TABLE matches ADD COLUMN uploaded_by TEXT")?;
    }
    let keys_bound = conn
        .prepare("SELECT 1 FROM pragma_table_info('upload_keys') WHERE name = 'player_id'")?
        .exists([])?;
    if !keys_bound {
        conn.execute_batch("ALTER TABLE upload_keys ADD COLUMN player_id TEXT")?;
    }
    let keys_have_accounts = conn
        .prepare("SELECT 1 FROM pragma_table_info('upload_keys') WHERE name = 'account'")?
        .exists([])?;
    if !keys_have_accounts {
        conn.execute_batch("ALTER TABLE upload_keys ADD COLUMN account TEXT")?;
    }
    let ratings_self_reported = conn
        .prepare("SELECT 1 FROM pragma_table_info('player_ratings') WHERE name = 'self_reported'")?
        .exists([])?;
    if !ratings_self_reported {
        conn.execute_batch(
            "ALTER TABLE player_ratings ADD COLUMN self_reported INTEGER NOT NULL DEFAULT 0",
        )?;
    }
    let techs_have_ids = conn
        .prepare("SELECT 1 FROM pragma_table_info('player_techs') WHERE name = 'tech_id'")?
        .exists([])?;
    if !techs_have_ids {
        conn.execute_batch("ALTER TABLE player_techs ADD COLUMN tech_id TEXT")?;
    }
    let players_have_packs = conn
        .prepare("SELECT 1 FROM pragma_table_info('players') WHERE name = 'starter_pack'")?
        .exists([])?;
    if !players_have_packs {
        conn.execute_batch("ALTER TABLE players ADD COLUMN starter_pack TEXT")?;
    }
    let matches_settled = conn
        .prepare("SELECT 1 FROM pragma_table_info('matches') WHERE name = 'settled'")?
        .exists([])?;
    if !matches_settled {
        conn.execute_batch("ALTER TABLE matches ADD COLUMN settled INTEGER NOT NULL DEFAULT 1")?;
    }
    Ok(())
}

/// Runs parser work on one replay with a panic turned into an error. A crafted replay that trips
/// an unchecked assumption must cost that replay alone: never a whole rebuild (every command
/// upgrades first, so a panic there would stop the server starting) or a request.
fn contained<T>(battle_id: &str, work: impl FnOnce() -> Result<T>) -> Result<T> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(work)).unwrap_or_else(|panic| {
        let message = panic
            .downcast_ref::<&str>()
            .map(|s| (*s).to_owned())
            .or_else(|| panic.downcast_ref::<String>().cloned())
            .unwrap_or_default();
        Err(anyhow::anyhow!(
            "{battle_id}: the parser panicked: {message}"
        ))
    })
}

/// What every copy of a match must agree on: each round's armies (unit, position, level, XP,
/// rotation), HP, and results, by each player's place in the replay. Names and account IDs,
/// which a server with pseudonyms rewrites, and anything only one client records, are left out.
fn fingerprint(record: &BattleRecord) -> String {
    use std::fmt::Write;
    let detail = record.match_detail();
    let mut facts = format!(
        "{}|{}|{}\n",
        detail.battle_id, detail.version, detail.map_id
    );
    for round in &detail.rounds {
        for (seat, side) in round.sides.iter().enumerate() {
            let Some(side) = side else { continue };
            let mut units: Vec<_> = side
                .units
                .iter()
                .map(|u| (u.unit_id, u.x, u.y, u.level, u.exp, u.rotated))
                .collect();
            units.sort_unstable();
            let _ = writeln!(
                facts,
                "{}|{}|{}|{}|{units:?}",
                round.round, seat, side.hp, side.result
            );
        }
    }
    hex(&Sha256::digest(facts.as_bytes()))
}

/// A match a sharing server wouldn't take.
#[derive(Debug, Serialize)]
pub struct SyncRejection {
    pub battle_id: String,
    pub reason: String,
    pub occurred_on: String,
    pub version: u32,
}

#[derive(Debug, Serialize)]
pub struct MatchRow {
    pub battle_id: String,
    pub version: u32,
    pub match_mode: String,
    pub game_mode: String,
    pub rounds: u32,
    pub occurred_on: String,
    /// Map name, or its ID when the name is unknown.
    pub map: String,
    pub winners: Vec<String>,
    pub losers: Vec<String>,
    /// Each winner's MMR at match start, in the same order, when the game log had it.
    pub winner_elos: Vec<Option<i64>>,
    pub loser_elos: Vec<Option<i64>>,
    /// Each winner's starter pack, in the same order, when the replay records one.
    pub winner_packs: Vec<Option<mechabellum::StarterPack>>,
    pub loser_packs: Vec<Option<mechabellum::StarterPack>>,
    /// Each winner's round 1 deployment (see `mechabellum::classify`), which classes the match
    /// for them, when classified.
    pub winner_deployments: Vec<Option<String>>,
    pub loser_deployments: Vec<Option<String>>,
    /// How round 1's deployments met in a 1v1, when both were one-sided ("Asym headbutt").
    pub meeting: Option<String>,
    /// False when the replay doesn't say who won: everyone is listed as a loser then, and the
    /// match counts toward no stat.
    pub settled: bool,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path).with_context(|| format!("opening {}", path.display()))?;
        Self::init(conn, Some(path.to_owned()))
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?, None)
    }

    fn init(conn: Connection, path: Option<PathBuf>) -> Result<Self> {
        // Wait for another process's write lock instead of failing with SQLITE_BUSY.
        conn.busy_timeout(std::time::Duration::from_secs(10))?;
        // WAL + synchronous=NORMAL: commits skip an fsync each; a power cut can lose the last few
        // commits but never corrupts the database.
        conn.execute_batch(
            "PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL; PRAGMA foreign_keys = ON;",
        )?;
        migrate(&conn)?;
        let store = Store {
            conn: Mutex::new(conn),
            path,
            rebuilding: Mutex::new(()),
            pseudonyms: OnceLock::new(),
        };
        // A fresh database has nothing to rebuild.
        if store.match_count()? == 0 {
            store.mark_current()?;
        }
        Ok(store)
    }

    /// The check value of the pseudonym key this database's replays were rewritten with, if any.
    /// A server must use the same key from then on.
    pub fn pseudonym_check(&self) -> Result<Option<String>> {
        self.meta("pseudonym_key")
    }

    /// Store every replay from now on with pseudonymous player IDs and no names, and rewrite
    /// the ones already stored (once; it's recorded in `meta`). Refuses a key other than the one
    /// the database was rewritten with.
    pub fn use_pseudonyms(&self, pseudonyms: Pseudonyms) -> Result<()> {
        let check = pseudonyms.check_value();
        match self.pseudonym_check()? {
            Some(stored) if stored != check => anyhow::bail!(
                "this database's players were pseudonymized with a different key; start the \
                 server with that key (its check value is {stored})"
            ),
            Some(_) => {}
            // Recorded before rewriting, so an interrupted rewrite resumes with the same key.
            None => self.set_meta("pseudonym_key", &check)?,
        }
        if self.pseudonyms.set(pseudonyms).is_err() {
            anyhow::bail!("pseudonyms are already on");
        }
        if self.meta("pseudonymized")?.as_deref() != Some("1") {
            self.pseudonymize_stored()?;
            self.set_meta("pseudonymized", "1")?;
        }
        Ok(())
    }

    /// Rewrite every stored replay and every table that holds account IDs, then rebuild the
    /// derived rows from the rewritten replays. Pseudonyms map to themselves, so running it again
    /// after an interruption finishes the job.
    fn pseudonymize_stored(&self) -> Result<()> {
        let pseudonyms = self.pseudonyms.get().context("pseudonyms aren't on")?;
        let started = std::time::Instant::now();
        let mut after = String::new();
        let mut rewritten = 0;
        loop {
            let page = self.stored_page(&after)?;
            let Some(last) = page.last() else { break };
            after = last.battle_id.clone();
            let mut changed = Vec::new();
            for replay in page {
                let mut xml = String::new();
                if let Err(e) = GzDecoder::new(&replay.xml_gz[..]).read_to_string(&mut xml) {
                    log::warn!("pseudonymize {}: {e}", replay.battle_id);
                    continue;
                }
                let new = pseudonyms.document(&xml);
                if new != xml {
                    let mut gz = GzEncoder::new(Vec::new(), Compression::default());
                    gz.write_all(new.as_bytes())?;
                    changed.push((replay.battle_id, gz.finish()?));
                }
            }
            let mut conn = self.conn();
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            for (battle_id, xml_gz) in &changed {
                tx.execute(
                    "UPDATE replay_blobs SET xml_gz = ?2 WHERE battle_id = ?1",
                    params![battle_id, xml_gz],
                )?;
            }
            tx.commit()?;
            rewritten += changed.len();
        }

        let mut conn = self.conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        for (table, key) in [
            ("player_ratings", "battle_id"),
            ("match_copies", "uploaded_by"),
            ("upload_keys", "name"),
            ("local_player", "only"),
        ] {
            let rows: Vec<(rusqlite::types::Value, String)> = tx
                .prepare(&format!(
                    "SELECT {key}, player_id FROM {table} WHERE player_id IS NOT NULL"
                ))?
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<rusqlite::Result<_>>()?;
            for (row, player) in rows {
                let new = pseudonyms.id(&player);
                if new != player {
                    tx.execute(
                        &format!(
                            "UPDATE OR REPLACE {table} SET player_id = ?3
                             WHERE {key} = ?1 AND player_id = ?2"
                        ),
                        params![row, player, new],
                    )?;
                }
            }
        }
        tx.commit()?;
        drop(conn);
        let stored = self.reindex()?;

        // A copy's fingerprint can be recomputed from the replay when the stored replay is
        // that uploader's own copy; it no longer depends on account IDs either way.
        let copies: Vec<(String, String)> = self
            .conn()
            .prepare(
                "SELECT c.battle_id, c.uploaded_by FROM match_copies c
                 JOIN matches m ON m.battle_id = c.battle_id AND m.uploaded_by = c.uploaded_by",
            )?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        for (battle_id, uploaded_by) in copies {
            let Some(xml) = self.replay_document(&battle_id)? else {
                continue;
            };
            let Ok(record) = BattleRecord::from_bytes(xml.as_bytes(), Path::new(&battle_id)) else {
                continue;
            };
            let Ok(print) = contained(&battle_id, || Ok(fingerprint(&record))) else {
                continue;
            };
            self.conn().execute(
                "UPDATE match_copies SET fingerprint = ?3 WHERE battle_id = ?1 AND uploaded_by = ?2",
                params![battle_id, uploaded_by, print],
            )?;
        }
        log::info!(
            "pseudonymized {rewritten} stored replays and rebuilt {stored} matches in {:.1?}",
            started.elapsed()
        );
        Ok(())
    }

    fn meta(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn()
            .query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0))
            .optional()?)
    }

    fn set_meta(&self, key: &str, value: &str) -> Result<()> {
        self.conn().execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES (?1, ?2)",
            params![key, value],
        )?;
        Ok(())
    }

    /// Whether stored stats were derived by an older version of this code and should be rebuilt
    /// with `upgrade`. Until then, queries still work but may miss newer data.
    pub fn needs_upgrade(&self) -> Result<bool> {
        let version: i64 = self
            .conn()
            .query_row("PRAGMA user_version", [], |r| r.get(0))?;
        Ok(version < SCHEMA_VERSION)
    }

    /// Rebuild derived stats if they are out of date. Returns whether anything was rebuilt.
    pub fn upgrade(&self) -> Result<bool> {
        if !self.needs_upgrade()? {
            return Ok(false);
        }
        log::info!("rebuilding stats for schema version {SCHEMA_VERSION}");
        let started = std::time::Instant::now();
        // Derived tables that changed shape since schema 10 are rebuilt from scratch: CREATE TABLE
        // IF NOT EXISTS never alters a table that's already there.
        self.conn().execute_batch(&format!(
            "DROP TABLE IF EXISTS player_games; DROP TABLE IF EXISTS player_fights;
             DROP TABLE IF EXISTS player_unit_use; {SCHEMA}"
        ))?;
        let count = self.reindex()?;
        self.mark_current()?;
        log::info!("rebuilt {count} matches in {:.1?}", started.elapsed());
        Ok(true)
    }

    fn mark_current(&self) -> Result<()> {
        self.conn()
            .execute_batch(&format!("PRAGMA user_version = {SCHEMA_VERSION}"))?;
        Ok(())
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
        anyhow::ensure!(
            document.len() <= MAX_DOCUMENT_BYTES,
            "{}: replay of {} bytes is larger than {MAX_DOCUMENT_BYTES}",
            source.display(),
            document.len()
        );
        let pseudonymized;
        let document = match self.pseudonyms.get() {
            Some(pseudonyms) => {
                pseudonymized = pseudonyms.document(document);
                pseudonymized.as_str()
            }
            None => document,
        };
        let record = BattleRecord::from_bytes(document.as_bytes(), source)?;
        let detail = contained(&source.display().to_string(), || Ok(record.match_detail()))?;
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
        contained(&battle_id, || {
            insert_record(&tx, &battle_id, &record, &detail, uploaded_by)
        })?;
        // A blob with no match row is left from a failed rebuild; this copy replaces it.
        tx.execute(
            "INSERT OR REPLACE INTO replay_blobs (battle_id, xml_gz) VALUES (?1, ?2)",
            params![battle_id, xml_gz],
        )?;
        tx.commit()?;
        Ok(IngestOutcome::Added { battle_id })
    }

    /// Store a replay shared by a community member, after the checks in docs/upload-trust.md:
    /// - it must pass validation, and offline games aren't accepted
    /// - each key uploads for one Mechabellum account, bound on first upload from `claimed`,
    ///   and that account must have played in the match
    /// - every other player's name is removed before storing
    ///
    /// When the match is already stored (the opponent shared it first), the uploader's own name
    /// is filled in.
    pub fn ingest_shared(
        &self,
        bytes: &[u8],
        key_name: &str,
        claimed: Option<&str>,
    ) -> Result<std::result::Result<IngestOutcome, Rejection>> {
        let reject = |status: u16, reason: String, problems: Vec<mechabellum::Problem>| {
            Ok(Err(Rejection {
                status,
                reason,
                problems,
            }))
        };
        let Some(document) = BattleRecord::extract_document(bytes) else {
            return reject(400, "Not a Mechabellum replay.".into(), vec![]);
        };
        let record = match BattleRecord::from_bytes(document.as_bytes(), Path::new("<upload>")) {
            Ok(record) => record,
            Err(e) => return reject(422, format!("The replay couldn't be read: {e}"), vec![]),
        };
        let problems = record.validate();
        if problems.iter().any(|p| p.code == "offline") {
            return reject(
                422,
                "Offline games against bots aren't shared.".into(),
                problems,
            );
        }
        if !problems.is_empty() {
            return reject(422, "The replay doesn't add up.".into(), problems);
        }

        let pseudonyms = self.pseudonyms.get();
        let as_stored = |id: &str| pseudonyms.map_or_else(|| id.trim().to_owned(), |p| p.id(id));
        let bound = self.bound_player(key_name)?;
        let Some(player) = bound.clone().or_else(|| claimed.map(as_stored)) else {
            return reject(
                400,
                "Say which player you are with the X-Mechabellum-Player header.".into(),
                vec![],
            );
        };
        let Some(own) = record
            .player_records
            .player_record
            .iter()
            .find(|p| as_stored(&p.id) == player)
        else {
            // A pseudonym isn't echoed: an unbound key could otherwise learn any account's.
            let reason = match pseudonyms {
                Some(_) => "This key shares games for a player who isn't in this match.".into(),
                None => {
                    format!("This key shares games for player {player}, who isn't in this match.")
                }
            };
            return reject(403, reason, vec![]);
        };
        let own_name = own.name.clone();
        if bound.is_none() {
            self.bind_player(key_name, &player)?;
        }

        // A server with pseudonyms rewrites every name and ID as it stores the replay.
        let shared = match pseudonyms {
            Some(_) => document.to_owned(),
            None => mechabellum::hide_other_names(document, &player),
        };
        let outcome = self.ingest(shared.as_bytes(), Path::new("<upload>"), Some(key_name))?;
        let battle_id = match &outcome {
            IngestOutcome::Added { battle_id } | IngestOutcome::Duplicate { battle_id } => {
                battle_id
            }
        };
        self.conn().execute(
            "INSERT OR REPLACE INTO match_copies (battle_id, uploaded_by, player_id, fingerprint)
             VALUES (?1, ?2, ?3, ?4)",
            params![battle_id, key_name, player, fingerprint(&record)],
        )?;
        if let (IngestOutcome::Duplicate { battle_id }, None) = (&outcome, pseudonyms) {
            self.conn().execute(
                "UPDATE players SET name = ?3 WHERE battle_id = ?1 AND player_id = ?2 AND name = ?4",
                params![battle_id, player, own_name, mechabellum::HIDDEN_NAME],
            )?;
        }
        Ok(Ok(outcome))
    }

    /// The account an uploader (a hand-made key's name, or a signed-in member's account) shares
    /// games for, once bound.
    pub fn key_player(&self, uploader: &str) -> Result<Option<String>> {
        self.bound_player(uploader)
    }

    fn bound_player(&self, uploader: &str) -> Result<Option<String>> {
        Ok(self
            .conn()
            .query_row(
                "SELECT player_id FROM accounts WHERE name = ?1
                 UNION ALL
                 SELECT player_id FROM upload_keys WHERE name = ?1 AND account IS NULL
                 LIMIT 1",
                [uploader],
                |r| r.get(0),
            )
            .optional()?
            .flatten())
    }

    /// Bind an unbound uploader to the account it shares games for.
    fn bind_player(&self, uploader: &str, player: &str) -> Result<()> {
        let conn = self.conn();
        let bound = conn.execute(
            "UPDATE accounts SET player_id = ?2 WHERE name = ?1 AND player_id IS NULL",
            params![uploader, player],
        )?;
        if bound == 0 {
            conn.execute(
                "UPDATE upload_keys SET player_id = ?2
                 WHERE name = ?1 AND account IS NULL AND player_id IS NULL",
                params![uploader, player],
            )?;
        }
        Ok(())
    }

    /// Whether `player` took part in a stored match.
    pub fn ratings_allowed(&self, battle_id: &str, player: &str) -> Result<bool> {
        Ok(self
            .conn()
            .prepare_cached("SELECT 1 FROM players WHERE battle_id = ?1 AND player_id = ?2")?
            .exists(params![battle_id, player])?)
    }

    /// Which of these battle IDs aren't stored yet.
    pub fn missing(&self, battle_ids: &[String]) -> Result<Vec<String>> {
        let conn = self.conn();
        let mut stmt = conn.prepare_cached("SELECT 1 FROM matches WHERE battle_id = ?1")?;
        let mut missing = Vec::new();
        for id in battle_ids {
            if !stmt.exists([id])? {
                missing.push(id.clone());
            }
        }
        Ok(missing)
    }

    /// The battle IDs among `battle_ids` that `uploader` hasn't shared a copy of. Asked per
    /// uploader, so a member's client sends its own copy of a match the opponent already shared
    /// (corroborating it, and carrying its ratings), and the answer says nothing about what
    /// anyone else shared.
    pub fn missing_for(&self, uploader: &str, battle_ids: &[String]) -> Result<Vec<String>> {
        let conn = self.conn();
        let mut stmt = conn.prepare_cached(
            "SELECT 1 FROM match_copies c JOIN matches m ON m.battle_id = c.battle_id
             WHERE c.battle_id = ?1 AND c.uploaded_by = ?2",
        )?;
        let mut missing = Vec::new();
        for id in battle_ids {
            if !stmt.exists(params![id, uploader])? {
                missing.push(id.clone());
            }
        }
        Ok(missing)
    }

    /// Record ratings a member's client captured for a match that member shared. Ratings for
    /// any other match are ignored, so the answer can't say who played in a match. A stored
    /// rating is replaced only by a first-hand report (from the uploader bound to that player)
    /// of one that wasn't first-hand: an opponent's report never overwrites anything, and the
    /// first first-hand report is never replaced (binding is only a claim, and the real player's
    /// client reports as each match ends). Returns how many were stored.
    pub fn add_shared_ratings(
        &self,
        battle_id: &str,
        ratings: &[Rating],
        uploader: &str,
    ) -> Result<usize> {
        let own = self.bound_player(uploader)?;
        let shared: bool = self.conn().query_row(
            "SELECT EXISTS (SELECT 1 FROM match_copies WHERE battle_id = ?1 AND uploaded_by = ?2)",
            params![battle_id, uploader],
            |r| r.get(0),
        )?;
        if !shared {
            return Ok(0);
        }
        self.store_ratings(battle_id, ratings, own.as_deref())
    }

    /// Record ratings for players in a stored match. Returns how many were stored; ratings for
    /// players not in that match are ignored.
    pub fn add_ratings(&self, battle_id: &str, ratings: &[Rating]) -> Result<usize> {
        self.store_ratings(battle_id, ratings, None)
    }

    /// `own`: the reporter's own player, whose rating it reports first-hand.
    fn store_ratings(
        &self,
        battle_id: &str,
        ratings: &[Rating],
        own: Option<&str>,
    ) -> Result<usize> {
        let mut conn = self.conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut stored = 0;
        for r in ratings.iter().filter(|r| r.plausible()) {
            let player_id = match self.pseudonyms.get() {
                Some(pseudonyms) => pseudonyms.id(&r.player_id),
                None => r.player_id.clone(),
            };
            let in_match = tx
                .prepare_cached("SELECT 1 FROM players WHERE battle_id = ?1 AND player_id = ?2")?
                .exists(params![battle_id, player_id])?;
            if in_match {
                let first_hand = own == Some(player_id.as_str());
                stored += tx.execute(
                    "INSERT INTO player_ratings
                         (battle_id, player_id, elo, rank_point, level, elo_games, self_reported)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                     ON CONFLICT (battle_id, player_id) DO UPDATE SET
                         elo = excluded.elo, rank_point = excluded.rank_point,
                         level = excluded.level, elo_games = excluded.elo_games,
                         self_reported = excluded.self_reported
                     WHERE player_ratings.self_reported = 0 AND excluded.self_reported = 1",
                    params![
                        battle_id,
                        player_id,
                        r.elo,
                        r.rank_point,
                        r.level,
                        r.elo_games,
                        first_hand
                    ],
                )?;
            }
        }
        tx.commit()?;
        Ok(stored)
    }

    /// Record ratings without checking the match: the game logs ratings when a match starts, before
    /// its replay exists. For the local app only; shared uploads go through `add_ratings`.
    /// Record the account logged in to the game on this machine (from its Player.log).
    pub fn set_local_player(&self, player_id: &str) -> Result<()> {
        self.conn().execute(
            "INSERT OR REPLACE INTO local_player (only, player_id) VALUES (0, ?1)",
            params![player_id],
        )?;
        Ok(())
    }

    pub fn put_ratings(&self, battle_id: &str, ratings: &[Rating]) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        for r in ratings.iter().filter(|r| r.plausible()) {
            tx.execute(
                "INSERT OR REPLACE INTO player_ratings
                     (battle_id, player_id, elo, rank_point, level, elo_games)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    battle_id,
                    r.player_id,
                    r.elo,
                    r.rank_point,
                    r.level,
                    r.elo_games
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Ratings for a stored match, one per player that has one.
    pub fn ratings(&self, battle_id: &str) -> Result<Vec<Rating>> {
        let conn = self.conn();
        let mut stmt = conn.prepare_cached(
            "SELECT player_id, elo, rank_point, level, elo_games FROM player_ratings
             WHERE battle_id = ?1 ORDER BY player_id",
        )?;
        let rows = stmt
            .query_map([battle_id], |r| {
                Ok(Rating {
                    player_id: r.get(0)?,
                    elo: r.get(1)?,
                    rank_point: r.get(2)?,
                    level: r.get(3)?,
                    elo_games: r.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    /// Matches already offered to `server`: battle ID -> why it was refused, if it was.
    pub fn synced(
        &self,
        server: &str,
    ) -> Result<std::collections::HashMap<String, Option<String>>> {
        let conn = self.conn();
        let mut stmt =
            conn.prepare("SELECT battle_id, rejected FROM synced_matches WHERE server = ?1")?;
        let rows = stmt
            .query_map([server], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    pub fn mark_synced(&self, server: &str, battle_id: &str, rejected: Option<&str>) -> Result<()> {
        self.conn().execute(
            "INSERT OR REPLACE INTO synced_matches (server, battle_id, rejected) VALUES (?1, ?2, ?3)",
            params![server, battle_id, rejected],
        )?;
        Ok(())
    }

    /// Matches `server` refused, newest first, with the reason it gave.
    pub fn sync_rejections(&self, server: &str) -> Result<Vec<SyncRejection>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT s.battle_id, s.rejected, m.occurred_on, m.version FROM synced_matches s
             JOIN matches m ON m.battle_id = s.battle_id
             WHERE s.server = ?1 AND s.rejected IS NOT NULL
             ORDER BY m.occurred_on DESC",
        )?;
        let rows = stmt
            .query_map([server], |r| {
                Ok(SyncRejection {
                    battle_id: r.get(0)?,
                    reason: r.get(1)?,
                    occurred_on: r.get(2)?,
                    version: r.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    /// Forget which matches `server` refused, so they're offered again (after an update that
    /// fixes why they were refused, for example).
    pub fn clear_sync_rejections(&self, server: &str) -> Result<usize> {
        Ok(self.conn().execute(
            "DELETE FROM synced_matches WHERE server = ?1 AND rejected IS NOT NULL",
            [server],
        )?)
    }

    /// Battle IDs of the matches `player_id` played in, newest first.
    pub fn battle_ids_for(&self, player_id: &str) -> Result<Vec<String>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT m.battle_id FROM matches m
             WHERE EXISTS (SELECT 1 FROM players p
                           WHERE p.battle_id = m.battle_id AND p.player_id = ?1)
             ORDER BY m.occurred_on DESC",
        )?;
        let ids = stmt
            .query_map([player_id], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        Ok(ids)
    }

    /// Every stored battle ID, newest first.
    pub fn battle_ids(&self) -> Result<Vec<String>> {
        let conn = self.conn();
        let mut stmt = conn.prepare("SELECT battle_id FROM matches ORDER BY occurred_on DESC")?;
        let ids = stmt
            .query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        Ok(ids)
    }

    /// A stored replay's XML.
    pub fn replay_document(&self, battle_id: &str) -> Result<Option<String>> {
        let blob: Option<Vec<u8>> = self
            .conn()
            .query_row(
                "SELECT xml_gz FROM replay_blobs WHERE battle_id = ?1",
                [battle_id],
                |r| r.get(0),
            )
            .optional()?;
        let Some(gz) = blob else {
            return Ok(None);
        };
        let mut xml = String::new();
        GzDecoder::new(gz.as_slice()).read_to_string(&mut xml)?;
        Ok(Some(xml))
    }

    /// Remove everything an uploader shared, e.g. after finding them poisoning the data.
    /// Returns how many matches were removed.
    pub fn purge_uploader(&self, key_name: &str) -> Result<usize> {
        let mut conn = self.conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute(
            "DELETE FROM replay_blobs WHERE battle_id IN
                 (SELECT battle_id FROM matches WHERE uploaded_by = ?1)",
            [key_name],
        )?;
        tx.execute(
            "DELETE FROM player_ratings WHERE battle_id IN
                 (SELECT battle_id FROM matches WHERE uploaded_by = ?1)",
            [key_name],
        )?;
        let removed = tx.execute("DELETE FROM matches WHERE uploaded_by = ?1", [key_name])?;
        // Its copies no longer vouch for anything.
        tx.execute(
            "DELETE FROM match_copies WHERE uploaded_by = ?1",
            [key_name],
        )?;
        tx.commit()?;
        Ok(removed)
    }

    /// Matches whose players' copies disagree: someone edited theirs, or the game recorded
    /// something differently than expected. Each with its copies.
    pub fn disputes(&self) -> Result<Vec<Dispute>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT battle_id, uploaded_by, player_id, fingerprint, at FROM match_copies
             WHERE battle_id IN (SELECT battle_id FROM match_copies GROUP BY battle_id
                                 HAVING COUNT(DISTINCT fingerprint) > 1)
             ORDER BY battle_id, at",
        )?;
        let mut disputes: Vec<Dispute> = Vec::new();
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                MatchCopy {
                    uploaded_by: r.get(1)?,
                    player_id: r.get(2)?,
                    fingerprint: r.get(3)?,
                    at: r.get(4)?,
                },
            ))
        })?;
        for row in rows {
            let (battle_id, copy) = row?;
            match disputes.last_mut() {
                Some(d) if d.battle_id == battle_id => d.copies.push(copy),
                _ => disputes.push(Dispute {
                    battle_id,
                    copies: vec![copy],
                }),
            }
        }
        Ok(disputes)
    }

    /// Games and wins per (unit or specialist, uploader) in the filtered results. Matches that
    /// came from the local watcher or the admin token count as one uploader, "".
    fn uploader_counts(
        &self,
        filter: &StatsFilter,
        table: &'static str,
        column: &'static str,
    ) -> Result<BTreeMap<String, Vec<crate::aggregate::UploaderCount>>> {
        let conn = self.conn();
        let scope = SCOPE;
        let mut stmt = conn.prepare(&format!(
            "{scope} SELECT t.{column}, COALESCE(m.uploaded_by, ''), COUNT(*), SUM(s.won)
             FROM {table} t
             JOIN scope s ON s.battle_id = t.battle_id AND s.player_id = t.player_id
             JOIN matches m ON m.battle_id = s.battle_id
             GROUP BY 1, 2"
        ))?;
        let mut rows = stmt.query(filter.params())?;
        let mut out: BTreeMap<String, Vec<crate::aggregate::UploaderCount>> = BTreeMap::new();
        while let Some(row) = rows.next()? {
            out.entry(row.get(0)?)
                .or_default()
                .push(crate::aggregate::UploaderCount {
                    uploader: row.get(1)?,
                    games: row.get(2)?,
                    wins: row.get(3)?,
                });
        }
        Ok(out)
    }

    /// Unit and specialist win rates that no single uploader can move much.
    pub fn robust_stats(&self, filter: &StatsFilter) -> Result<RobustStats> {
        use crate::aggregate::robust_stats;
        Ok(RobustStats {
            units: robust_stats(&self.uploader_counts(filter, "player_units", "unit")?),
            specialists: robust_stats(&self.uploader_counts(
                filter,
                "player_specialists",
                "specialist",
            )?),
        })
    }

    /// Uploaders whose results are implausibly far from everyone else's, for review.
    pub fn outliers(&self, filter: &StatsFilter) -> Result<Vec<crate::aggregate::Outlier>> {
        use crate::aggregate::outliers;
        let mut all = outliers(&self.uploader_counts(filter, "player_units", "unit")?);
        all.extend(outliers(&self.uploader_counts(
            filter,
            "player_specialists",
            "specialist",
        )?));
        all.sort_by(|a, b| b.z.abs().total_cmp(&a.z.abs()));
        Ok(all)
    }

    /// How many shared matches are corroborated (two players' copies agree) and disputed.
    pub fn corroboration(&self) -> Result<(u64, u64)> {
        let conn = self.conn();
        let corroborated =
            conn.query_row(&format!("SELECT COUNT(*) FROM ({CORROBORATED})"), [], |r| {
                r.get(0)
            })?;
        let disputed = conn.query_row(
            "SELECT COUNT(*) FROM (SELECT battle_id FROM match_copies GROUP BY battle_id
                                   HAVING COUNT(DISTINCT fingerprint) > 1)",
            [],
            |r| r.get(0),
        )?;
        Ok((corroborated, disputed))
    }

    pub fn has_pseudonyms(&self) -> bool {
        self.pseudonyms.get().is_some()
    }

    /// The account for a Steam ID, created on its first sign-in; `Err` with a reason when it may
    /// not share. Only the Steam ID's pseudonym is stored.
    pub fn sign_in(&self, steam_id: &str) -> Result<std::result::Result<String, String>> {
        let pseudonyms = self.pseudonyms.get().context("pseudonyms aren't on")?;
        let steam = pseudonyms.steam(steam_id);
        let name = format!("steam-{}", &steam[1..]);
        let conn = self.conn();
        conn.execute(
            "INSERT OR IGNORE INTO accounts (name, steam) VALUES (?1, ?2)",
            params![name, steam],
        )?;
        let disabled: Option<String> = conn.query_row(
            "SELECT disabled_at FROM accounts WHERE name = ?1 AND steam = ?2",
            params![name, steam],
            |r| r.get(0),
        )?;
        if disabled.is_some() {
            return Ok(Err("This account can't share games.".into()));
        }
        conn.execute(
            "UPDATE accounts SET signed_in_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
             WHERE name = ?1",
            [&name],
        )?;
        Ok(Ok(name))
    }

    /// A new upload key for one of a signed-in account's devices. Returns the key, which is only
    /// available now. Past `ACCOUNT_KEYS` active keys, the oldest is revoked.
    pub fn create_account_key(&self, account: &str) -> Result<String> {
        let mut secret = [0u8; 32];
        getrandom::getrandom(&mut secret).map_err(|e| anyhow::anyhow!("rng: {e}"))?;
        let key = format!("{KEY_PREFIX}{}", hex(&secret));
        let mut conn = self.conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let known: bool = tx.query_row(
            "SELECT EXISTS (SELECT 1 FROM accounts WHERE name = ?1 AND disabled_at IS NULL)",
            [account],
            |r| r.get(0),
        )?;
        anyhow::ensure!(known, "no active account {account:?}");
        tx.execute(
            "UPDATE upload_keys SET revoked_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
             WHERE rowid IN (SELECT rowid FROM upload_keys
                             WHERE account = ?1 AND revoked_at IS NULL
                             ORDER BY rowid DESC LIMIT -1 OFFSET ?2)",
            params![account, ACCOUNT_KEYS - 1],
        )?;
        let made: i64 = tx.query_row(
            "SELECT COUNT(*) FROM upload_keys WHERE account = ?1",
            [account],
            |r| r.get(0),
        )?;
        tx.execute(
            "INSERT INTO upload_keys (name, key_hash, account) VALUES (?1, ?2, ?3)",
            params![
                format!("{account}-{}", made + 1),
                Sha256::digest(key.as_bytes()).as_slice(),
                account
            ],
        )?;
        tx.commit()?;
        Ok(key)
    }

    pub fn list_accounts(&self) -> Result<Vec<Account>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT a.name, a.created_at, a.signed_in_at, a.disabled_at, a.player_id IS NOT NULL,
                    (SELECT COUNT(*) FROM upload_keys k
                     WHERE k.account = a.name AND k.revoked_at IS NULL)
             FROM accounts a ORDER BY a.created_at, a.name",
        )?;
        let accounts = stmt
            .query_map([], |r| {
                Ok(Account {
                    name: r.get(0)?,
                    created_at: r.get(1)?,
                    signed_in_at: r.get(2)?,
                    disabled_at: r.get(3)?,
                    bound: r.get(4)?,
                    active_keys: r.get(5)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(accounts)
    }

    /// Stop or allow an account's uploads and sign-ins. A disabled account's keys stop working
    /// at once, and the Mechabellum account it shared for may be claimed by another uploader.
    /// Returns false if there's no such account.
    pub fn set_account_disabled(&self, name: &str, disabled: bool) -> Result<bool> {
        let changed = self.conn().execute(
            "UPDATE accounts SET disabled_at =
                 CASE WHEN ?2 THEN strftime('%Y-%m-%dT%H:%M:%SZ', 'now') END
             WHERE name = ?1",
            params![name, disabled],
        )?;
        Ok(changed > 0)
    }

    /// Create an upload key. Returns the key, which is only available now; the database keeps
    /// its hash.
    pub fn create_key(&self, name: &str) -> Result<String> {
        // Signed-in accounts upload as `steam-...`; a hand-made key with such a name would share
        // their uploader bucket.
        anyhow::ensure!(
            !name.starts_with("steam-"),
            "key names starting with steam- are kept for signed-in accounts"
        );
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

    /// The uploader an active key belongs to, if any: its signed-in member's account, or the
    /// key's own name for a hand-made key. Lookup is by SHA-256, so response time doesn't depend
    /// on how much of the key matched.
    pub fn verify_key(&self, presented: &str) -> Result<Option<String>> {
        if !presented.starts_with(KEY_PREFIX) {
            return Ok(None);
        }
        let hash = Sha256::digest(presented.as_bytes());
        Ok(self
            .conn()
            .query_row(
                "SELECT COALESCE(k.account, k.name) FROM upload_keys k
                 LEFT JOIN accounts a ON a.name = k.account
                 WHERE k.key_hash = ?1 AND k.revoked_at IS NULL AND a.disabled_at IS NULL",
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

    pub fn tech_stats(&self, filter: &StatsFilter) -> Result<Vec<UnitTechStats>> {
        let conn = self.conn();
        let scope = SCOPE;
        let args = filter.params();
        let mut units: BTreeMap<String, UnitTechStats> = conn
            .prepare(&format!(
                "{scope} SELECT t.unit, COUNT(*), SUM(s.won) FROM player_units t
                 JOIN scope s ON s.battle_id = t.battle_id AND s.player_id = t.player_id
                 GROUP BY t.unit"
            ))?
            .query_map(args, |r| {
                Ok(UnitTechStats {
                    unit: r.get(0)?,
                    picks: r.get(1)?,
                    wins: r.get(2)?,
                    techs: Vec::new(),
                })
            })?
            .map(|u| u.map(|u| (u.unit.clone(), u)))
            .collect::<rusqlite::Result<_>>()?;

        let mut stmt = conn.prepare(&format!(
            "{scope} SELECT t.unit, t.tech, COUNT(*), SUM(s.won), MAX(t.tech_id) FROM player_techs t
             JOIN scope s ON s.battle_id = t.battle_id AND s.player_id = t.player_id
             GROUP BY t.unit, t.tech"
        ))?;
        let mut rows = stmt.query(args)?;
        while let Some(row) = rows.next()? {
            let (unit, tech, picks, wins, tech_id): (String, String, u64, u64, Option<String>) = (
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
            );
            let Some(u) = units.get_mut(&unit) else {
                continue;
            };
            let (without_picks, without_wins) = (u.picks - picks, u.wins - wins);
            u.techs.push(TechStat {
                tech,
                tech_id,
                picks,
                wins,
                pick_rate: ratio(picks, u.picks),
                win_rate: ratio(wins, picks),
                win_rate_without: (without_picks > 0).then(|| ratio(without_wins, without_picks)),
            });
        }
        let mut out: Vec<UnitTechStats> = units.into_values().collect();
        for u in &mut out {
            u.techs
                .sort_by(|a, b| b.picks.cmp(&a.picks).then_with(|| a.tech.cmp(&b.tech)));
        }
        out.sort_by(|a, b| b.picks.cmp(&a.picks).then_with(|| a.unit.cmp(&b.unit)));
        Ok(out)
    }

    /// One stored match, redrawn round by round from its replay.
    pub fn match_detail(&self, battle_id: &str) -> Result<Option<mechabellum::MatchDetail>> {
        let blob: Option<Vec<u8>> = self
            .conn()
            .query_row(
                "SELECT xml_gz FROM replay_blobs WHERE battle_id = ?1",
                [battle_id],
                |r| r.get(0),
            )
            .optional()?;
        let Some(gz) = blob else {
            return Ok(None);
        };
        let mut xml = String::new();
        GzDecoder::new(gz.as_slice()).read_to_string(&mut xml)?;
        let record = BattleRecord::from_bytes(xml.as_bytes(), Path::new(battle_id))?;
        contained(battle_id, || Ok(record.match_detail())).map(Some)
    }

    /// Like `stats`, but broken down by game version. The filter's version bounds still apply.
    pub fn version_trends(&self, filter: &StatsFilter) -> Result<VersionTrends> {
        let conn = self.conn();
        let scope = SCOPE;
        let args = filter.params();

        let (key, order) = filter.period.sql();

        let versions: Vec<VersionTotals> = conn
            .prepare(&format!(
                "{scope} SELECT {key}, COUNT(DISTINCT battle_id), COUNT(*) FROM scope
                 GROUP BY 1 ORDER BY {order}"
            ))?
            .query_map(args, |r| {
                Ok(VersionTotals {
                    label: r.get(0)?,
                    matches: r.get(1)?,
                    player_results: r.get(2)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        let column: BTreeMap<&str, usize> = versions
            .iter()
            .enumerate()
            .map(|(i, v)| (v.label.as_str(), i))
            .collect();

        let trends = |table: &'static str, name: &'static str| -> Result<Vec<Trend>> {
            let mut by_name: BTreeMap<String, Trend> = BTreeMap::new();
            let sql = format!(
                "{scope} SELECT t.{name}, {key}, COUNT(*), SUM(s.won) FROM {table} t
                 JOIN scope s ON s.battle_id = t.battle_id AND s.player_id = t.player_id
                 GROUP BY 1, 2"
            );
            let mut stmt = conn.prepare(&sql)?;
            let mut rows = stmt.query(args)?;
            while let Some(row) = rows.next()? {
                let (label, period, picks, wins): (String, String, u64, u64) =
                    (row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?);
                let trend = by_name.entry(label.clone()).or_insert_with(|| Trend {
                    name: label,
                    picks: 0,
                    cells: (0..versions.len()).map(|_| None).collect(),
                });
                trend.picks += picks;
                if let Some(&i) = column.get(period.as_str()) {
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
        // Counts are of the matches the stats count by default (SCOPE): with players, settled,
        // not offline. Every version and mode is still listed, so offline-only ones can be picked.
        const COUNTED: &str = concat!(
            "m.settled = 1 AND instr(m.battle_id, '__') = 0
             AND EXISTS (SELECT 1 FROM players p WHERE p.battle_id = m.battle_id) AND ",
            played_here!()
        );
        let versions = conn
            .prepare(&format!(
                "SELECT m.version, SUM({COUNTED}) FROM matches m GROUP BY m.version
                 ORDER BY m.version DESC"
            ))?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        let modes = conn
            .prepare(&format!(
                "SELECT m.match_mode, m.game_mode, SUM({COUNTED}) FROM matches m
                 GROUP BY m.match_mode, m.game_mode ORDER BY SUM({COUNTED}) DESC, COUNT(*) DESC"
            ))?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<rusqlite::Result<_>>()?;
        // "Me": the account the game says is logged in here, once it has a stored match.
        // Without that, whoever appears in the most matches.
        let logged_in = conn
            .query_row(
                "SELECT l.player_id,
                        (SELECT q.name FROM players q JOIN matches m ON m.battle_id = q.battle_id
                         WHERE q.player_id = l.player_id
                         ORDER BY m.occurred_on DESC, m.version DESC LIMIT 1)
                 FROM local_player l",
                [],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?)),
            )
            .optional()?
            .and_then(|(id, name)| Some((id, name?)));
        let most_matches = conn
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
            main_player: logged_in.or(most_matches),
        })
    }

    pub fn match_count(&self) -> Result<u64> {
        Ok(self
            .conn()
            .query_row("SELECT COUNT(*) FROM matches", [], |r| r.get(0))?)
    }

    /// Matches with no result to count: the game ended before the first fight (someone left
    /// in round 1), so the replay has no fight report.
    pub fn unfinished_count(&self) -> Result<u64> {
        Ok(self.conn().query_row(
            "SELECT COUNT(*) FROM matches m
             WHERE NOT EXISTS (SELECT 1 FROM players p WHERE p.battle_id = m.battle_id)",
            [],
            |r| r.get(0),
        )?)
    }

    pub fn active_key_count(&self) -> Result<u64> {
        Ok(self.conn().query_row(
            "SELECT COUNT(*) FROM upload_keys WHERE revoked_at IS NULL",
            [],
            |r| r.get(0),
        )?)
    }

    /// Rebuild every derived row from the stored replays. Run after upgrading the parser.
    ///
    /// Memory stays flat however many replays are stored: they're read a page at a time, parsed
    /// on worker threads that hand results on through bounded channels, and written to a scratch
    /// database beside this one. The parsers run in the caller's rayon pool (the desktop app
    /// installs a low-priority one), and replays stored meanwhile get further unlocked rounds.
    /// Only the swap at the end holds the lock, and it copies rows rather than parsing, so readers
    /// keep seeing the old rows until one short transaction replaces them all.
    pub fn reindex(&self) -> Result<usize> {
        let _one_at_a_time = self
            .rebuilding
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let scratch = Scratch::beside(self.path.as_deref());
        let started = std::time::Instant::now();
        let parsed = self.stage(&scratch.path)?;
        log::info!(
            "reindex: parsed {parsed} replays in {:.1?}",
            started.elapsed()
        );
        let started = std::time::Instant::now();
        let (stored, late) = self.swap_in(&scratch.path)?;
        log::info!(
            "reindex: wrote {stored} matches in {:.1?} ({late} parsed under the lock)",
            started.elapsed()
        );
        Ok(stored)
    }

    /// Parse every stored replay into a fresh scratch database at `scratch`, then the ones stored
    /// while that ran (uploads, the desktop app's backfill). Returns how many were parsed.
    fn stage(&self, scratch: &Path) -> Result<usize> {
        let mut out =
            Connection::open(scratch).with_context(|| format!("creating {}", scratch.display()))?;
        // The scratch copy is thrown away if anything goes wrong, so it needn't survive a crash.
        out.execute_batch("PRAGMA journal_mode = OFF; PRAGMA synchronous = OFF;")?;
        migrate(&out)?;
        out.execute_batch("CREATE TABLE rebuild_failed (battle_id TEXT PRIMARY KEY);")?;

        let mut after = String::new();
        let mut parsed = self.stage_pages(&mut out, || {
            let page = self.stored_page(&after)?;
            let Some(last) = page.last() else {
                return Ok(None);
            };
            after = last.battle_id.clone();
            Ok(Some(page))
        })?;
        for _ in 0..REBUILD_ROUNDS {
            let late = self.unstaged(&out)?;
            if late.is_empty() {
                break;
            }
            let mut late = late.into_iter();
            parsed += self.stage_pages(&mut out, || {
                let ids: Vec<String> = late.by_ref().take(REBUILD_PAGE).collect();
                if ids.is_empty() {
                    return Ok(None);
                }
                self.stored_by_id(&ids).map(Some)
            })?;
        }
        Ok(parsed)
    }

    /// Parse the pages `next_page` yields (`None` when there are no more) and write them to `out`.
    /// A reader thread feeds the parsers, the parsers feed a writer thread, and both channels are
    /// bounded, so only a few replays are in memory at once. Returns how many were written.
    fn stage_pages(
        &self,
        out: &mut Connection,
        mut next_page: impl FnMut() -> Result<Option<Vec<StoredReplay>>> + Send,
    ) -> Result<usize> {
        let workers = rayon::current_num_threads().clamp(1, REBUILD_WORKERS);
        let stop = AtomicBool::new(false);
        let stop = &stop;
        let (stored_tx, stored_rx) = mpsc::sync_channel::<StoredReplay>(workers * 2);
        let (parsed_tx, parsed_rx) = mpsc::sync_channel::<StagedReplay>(workers * 2);
        let stored_rx = Mutex::new(stored_rx);

        std::thread::scope(|scope| {
            let reader = scope.spawn(move || -> Result<()> {
                while !stop.load(Ordering::Relaxed) {
                    let Some(page) = next_page()? else { break };
                    for replay in page {
                        if stored_tx.send(replay).is_err() {
                            return Ok(());
                        }
                    }
                }
                Ok(())
            });
            let writer = scope.spawn(move || {
                let written = write_staged(out, parsed_rx);
                if written.is_err() {
                    stop.store(true, Ordering::Relaxed);
                }
                written
            });
            // The parsers run in the caller's pool. A thread inside a pool runs spawned work
            // while it waits, so even a one-thread pool gets through it: the reader and the
            // writer have threads of their own.
            rayon::scope(|pool| {
                for _ in 0..workers {
                    let (stored_rx, parsed_tx) = (&stored_rx, parsed_tx.clone());
                    pool.spawn(move |_| loop {
                        let next = stored_rx
                            .lock()
                            .unwrap_or_else(PoisonError::into_inner)
                            .recv();
                        let Ok(replay) = next else { break };
                        if stop.load(Ordering::Relaxed) {
                            break;
                        }
                        let parsed = parse_stored(&replay.battle_id, &replay.xml_gz);
                        let staged = StagedReplay {
                            battle_id: replay.battle_id,
                            uploaded_by: replay.uploaded_by,
                            parsed,
                        };
                        if parsed_tx.send(staged).is_err() {
                            break;
                        }
                    });
                }
            });
            // The parsers' clones are gone; dropping this one lets the writer finish.
            drop(parsed_tx);
            let read = reader
                .join()
                .unwrap_or_else(|_| Err(anyhow::anyhow!("the replay reader panicked")));
            let written = writer
                .join()
                .unwrap_or_else(|_| Err(anyhow::anyhow!("the rebuild writer panicked")));
            written.and_then(|n| read.map(|()| n))
        })
    }

    /// Up to `REBUILD_PAGE` stored replays whose battle IDs sort after `after`, with uploaders.
    fn stored_page(&self, after: &str) -> Result<Vec<StoredReplay>> {
        let conn = self.conn();
        let mut stmt = conn.prepare_cached(
            "SELECT b.battle_id, b.xml_gz, m.uploaded_by FROM replay_blobs b
             LEFT JOIN matches m ON m.battle_id = b.battle_id
             WHERE b.battle_id > ?1 ORDER BY b.battle_id LIMIT ?2",
        )?;
        let page = stmt
            .query_map(params![after, REBUILD_PAGE as i64], |r| {
                Ok(StoredReplay {
                    battle_id: r.get(0)?,
                    xml_gz: r.get(1)?,
                    uploaded_by: r.get(2)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(page)
    }

    /// The stored replays among `ids`, with uploaders; ones removed meanwhile are skipped.
    fn stored_by_id(&self, ids: &[String]) -> Result<Vec<StoredReplay>> {
        let conn = self.conn();
        let mut stmt = conn.prepare_cached(
            "SELECT b.xml_gz, m.uploaded_by FROM replay_blobs b
             LEFT JOIN matches m ON m.battle_id = b.battle_id WHERE b.battle_id = ?1",
        )?;
        let mut found = Vec::with_capacity(ids.len());
        for battle_id in ids {
            let row = stmt
                .query_row([battle_id], |r| Ok((r.get(0)?, r.get(1)?)))
                .optional()?;
            if let Some((xml_gz, uploaded_by)) = row {
                found.push(StoredReplay {
                    battle_id: battle_id.clone(),
                    xml_gz,
                    uploaded_by,
                });
            }
        }
        Ok(found)
    }

    /// Stored replays the scratch database `out` has neither staged nor given up on.
    fn unstaged(&self, out: &Connection) -> Result<Vec<String>> {
        let staged: BTreeSet<String> = out
            .prepare("SELECT battle_id FROM matches UNION SELECT battle_id FROM rebuild_failed")?
            .query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        let stored: Vec<String> = self
            .conn()
            .prepare("SELECT battle_id FROM replay_blobs")?
            .query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        Ok(stored
            .into_iter()
            .filter(|battle_id| !staged.contains(battle_id))
            .collect())
    }

    /// Replace every derived row with the staged ones, in one transaction. Replays stored since
    /// the last staging round are parsed here, under the lock, and ones removed meanwhile aren't
    /// brought back. Returns the matches stored and how many were parsed here.
    fn swap_in(&self, scratch: &Path) -> Result<(usize, usize)> {
        let scratch = scratch
            .to_str()
            .context("the scratch database's path isn't UTF-8")?;
        let mut conn = self.conn();
        conn.execute("ATTACH DATABASE ?1 AS rebuild", [scratch])?;
        let swapped = (|| -> Result<(usize, usize)> {
            let mut tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            // Read before the old rows go: their uploaders are kept.
            let late: Vec<(String, Option<String>)> = tx
                .prepare(
                    "SELECT b.battle_id, m.uploaded_by FROM main.replay_blobs b
                     LEFT JOIN main.matches m ON m.battle_id = b.battle_id
                     WHERE b.battle_id NOT IN (SELECT battle_id FROM rebuild.matches)
                       AND b.battle_id NOT IN (SELECT battle_id FROM rebuild.rebuild_failed)",
                )?
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<rusqlite::Result<_>>()?;
            tx.execute("DELETE FROM main.matches", [])?;
            for table in DERIVED_TABLES {
                // By name: columns added with ALTER TABLE sit in a different order in an older
                // database than in the scratch copy created fresh.
                let columns: Vec<String> = tx
                    .prepare("SELECT name FROM pragma_table_info(?1, 'main')")?
                    .query_map([table], |r| r.get(0))?
                    .collect::<rusqlite::Result<_>>()?;
                let columns = columns
                    .iter()
                    .map(|c| format!("\"{}\"", c.replace('"', "\"\"")))
                    .collect::<Vec<_>>()
                    .join(", ");
                tx.execute(
                    &format!(
                        "INSERT INTO main.{table} ({columns}) SELECT {columns} FROM rebuild.{table}
                         WHERE battle_id IN (SELECT battle_id FROM main.replay_blobs)"
                    ),
                    [],
                )?;
            }
            let late_count = late.len();
            for (battle_id, uploaded_by) in late {
                let gz: Option<Vec<u8>> = tx
                    .query_row(
                        "SELECT xml_gz FROM main.replay_blobs WHERE battle_id = ?1",
                        [&battle_id],
                        |r| r.get(0),
                    )
                    .optional()?;
                let Some((record, detail)) = gz.and_then(|gz| parse_stored(&battle_id, &gz)) else {
                    continue;
                };
                let savepoint = tx.savepoint()?;
                match contained(&battle_id, || {
                    insert_record(
                        &savepoint,
                        &battle_id,
                        &record,
                        &detail,
                        uploaded_by.as_deref(),
                    )
                }) {
                    Ok(()) => savepoint.commit()?,
                    Err(e) => log::warn!("reindex {battle_id}: not stored: {e:#}"),
                }
            }
            let stored: i64 =
                tx.query_row("SELECT COUNT(*) FROM main.matches", [], |r| r.get(0))?;
            tx.commit()?;
            Ok((stored as usize, late_count))
        })();
        conn.execute("DETACH DATABASE rebuild", [])?;
        swapped
    }

    pub fn recent_matches(&self, limit: u32) -> Result<Vec<MatchRow>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(concat!(
            "SELECT m.battle_id, m.version, m.match_mode, m.game_mode, m.rounds, m.occurred_on, m.map_id,
                    COALESCE((SELECT json_group_array(json_object(
                                  'name', p.name, 'elo', r.elo, 'deployment', g.opening,
                                  'pack', json(p.starter_pack)))
                              FROM players p LEFT JOIN player_ratings r
                                ON r.battle_id = p.battle_id AND r.player_id = p.player_id
                              LEFT JOIN player_games g
                                ON g.battle_id = p.battle_id AND g.player_id = p.player_id
                              WHERE p.battle_id = m.battle_id AND p.won = 1), '[]'),
                    COALESCE((SELECT json_group_array(json_object(
                                  'name', p.name, 'elo', r.elo, 'deployment', g.opening,
                                  'pack', json(p.starter_pack)))
                              FROM players p LEFT JOIN player_ratings r
                                ON r.battle_id = p.battle_id AND r.player_id = p.player_id
                              LEFT JOIN player_games g
                                ON g.battle_id = p.battle_id AND g.player_id = p.player_id
                              WHERE p.battle_id = m.battle_id AND p.won = 0), '[]'),
                    (SELECT g.meeting FROM player_games g
                     WHERE g.battle_id = m.battle_id AND g.meeting IS NOT NULL LIMIT 1),
                    m.settled
             FROM matches m WHERE ",
            // Matches only watched here are left out, as from the stats.
            played_here!(),
            " ORDER BY m.occurred_on DESC, m.battle_id DESC LIMIT ?1"
        ))?;
        // Each side as a JSON array of players. JSON, not separator characters: names come
        // from uploaded replays, and a name holding a separator once forged a second player
        // with any MMR it liked (review, 2026-10-27).
        #[derive(Deserialize)]
        struct Entry {
            name: String,
            elo: Option<i64>,
            deployment: Option<String>,
            pack: Option<mechabellum::StarterPack>,
        }
        type Side = (
            Vec<String>,
            Vec<Option<i64>>,
            Vec<Option<String>>,
            Vec<Option<mechabellum::StarterPack>>,
        );
        let split = |s: String| -> Side {
            let entries: Vec<Entry> = serde_json::from_str(&s).unwrap_or_default();
            let mut side: Side = Default::default();
            for e in entries {
                side.0.push(e.name);
                side.1.push(e.elo);
                side.2.push(e.deployment);
                side.3.push(e.pack);
            }
            side
        };
        let rows = stmt
            .query_map([limit], |r| {
                let (winners, winner_elos, winner_deployments, winner_packs) = split(r.get(7)?);
                let (losers, loser_elos, loser_deployments, loser_packs) = split(r.get(8)?);
                Ok(MatchRow {
                    battle_id: r.get(0)?,
                    version: r.get(1)?,
                    match_mode: r.get(2)?,
                    game_mode: r.get(3)?,
                    rounds: r.get(4)?,
                    occurred_on: r.get(5)?,
                    map: {
                        let id: String = r.get(6)?;
                        mechabellum::resolve_name(mechabellum::NameKind::Map, &id)
                            .map_or(id, str::to_owned)
                    },
                    winners,
                    losers,
                    winner_elos,
                    loser_elos,
                    winner_packs,
                    loser_packs,
                    winner_deployments,
                    loser_deployments,
                    meeting: r.get(9)?,
                    settled: r.get(10)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    pub fn stats(&self, filter: &StatsFilter) -> Result<Stats> {
        let conn = self.conn();
        // Filtered player results. Every stat below is computed over this set.
        let scope = SCOPE;
        let args = filter.params();

        let (matches, player_results): (u64, u64) = conn.query_row(
            &format!("{scope} SELECT COUNT(DISTINCT battle_id), COUNT(*) FROM scope"),
            args,
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;

        let usage = |table: &'static str, column: &'static str| -> Result<Vec<UsageStat>> {
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

        // Each counted result's MMR against its opponent's, for 1v1 games with both known.
        let differences: Vec<(i64, bool)> = conn
            .prepare(&format!(
                "{scope} SELECT r2.elo - r1.elo, s.won FROM scope s
                 JOIN matches m ON m.battle_id = s.battle_id AND m.match_mode = 'VS_1_1'
                 JOIN player_ratings r1 ON r1.battle_id = s.battle_id AND r1.player_id = s.player_id
                 JOIN players o ON o.battle_id = s.battle_id AND o.player_id != s.player_id
                 JOIN player_ratings r2 ON r2.battle_id = o.battle_id AND r2.player_id = o.player_id
                 WHERE r1.elo IS NOT NULL AND r2.elo IS NOT NULL"
            ))?
            .query_map(args, |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;

        Ok(Stats {
            matches,
            player_results,
            vs_rating: rating_bands(&differences),
            units: usage("player_units", "unit")?,
            specialists: usage("player_specialists", "specialist")?,
            picked_specialists: usage(
                "(SELECT battle_id, player_id, card FROM player_cards WHERE kind = 'specialist')",
                "card",
            )?,
            // Names can differ in case and punctuation between the held list and the cards.
            granted_specialists: usage(
                "(SELECT s.battle_id, s.player_id, s.specialist FROM player_specialists s
                  WHERE NOT EXISTS (
                      SELECT 1 FROM player_cards c
                      WHERE c.battle_id = s.battle_id AND c.player_id = s.player_id
                        AND replace(replace(lower(c.card), ' ', ''), '-', '')
                          = replace(replace(lower(s.specialist), ' ', ''), '-', '')))",
                "specialist",
            )?,
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
    tx: &Connection,
    battle_id: &str,
    record: &BattleRecord,
    detail: &MatchDetail,
    uploaded_by: Option<&str>,
) -> Result<()> {
    let overview = record.get_battle_overview()?;
    tx.execute(
        "INSERT INTO matches
             (battle_id, version, match_mode, game_mode, map_id, rounds, occurred_on, uploaded_by,
              settled)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            battle_id,
            overview.version,
            record.battle_info.match_mode,
            record.battle_info.game_mode,
            overview.map_id,
            overview.rounds as i64,
            overview.occurred_on,
            uploaded_by,
            // A match with no fight recorded (an offline game ended at once) has no result to
            // count either.
            record.results_settled() && !detail.rounds.is_empty(),
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
    insert_games(tx, battle_id, detail)
}

/// The match from each stored player's side (`mechabellum::PlayerGame`). Players the overview
/// left out (no fight report) have no `players` row, so they're skipped too.
fn insert_games(tx: &Connection, battle_id: &str, detail: &MatchDetail) -> Result<()> {
    let stored: std::collections::BTreeSet<String> = tx
        .prepare_cached("SELECT player_id FROM players WHERE battle_id = ?1")?
        .query_map([battle_id], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    for (player, game) in detail.players.iter().zip(detail.player_games()) {
        // As in insert_player: no account ID means the name stands in.
        let player_id = match player.id.trim() {
            "" | "0" => player.name.as_str(),
            id => id,
        };
        if !stored.contains(player_id) {
            continue;
        }
        tx.execute(
            "INSERT OR IGNORE INTO player_games
                 (battle_id, player_id, fights_won, final_units, peak_units, final_value,
                  peak_value, mean_level, max_level, level_ups, units_added, units_sold, techs,
                  cards, opening, meeting, card)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
            params![
                battle_id,
                player_id,
                game.fights_won,
                game.final_units,
                game.peak_units,
                game.final_value,
                game.peak_value,
                game.mean_level,
                game.max_level,
                game.level_ups,
                game.units_added,
                game.units_sold,
                game.techs,
                game.cards,
                game.opening,
                game.opening_meeting.map(mechabellum::Meeting::label),
                serde_json::to_string(&game)?,
            ],
        )?;
        for fight in &game.fights {
            tx.execute(
                "INSERT OR IGNORE INTO player_fights
                     (battle_id, player_id, fight, result, hp, units, deployment)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    battle_id,
                    player_id,
                    fight.fight,
                    fight.result,
                    fight.hp,
                    fight.units,
                    fight.deployment.as_ref().map(|d| &d.label),
                ],
            )?;
        }
        for unit in &game.units {
            tx.execute(
                "INSERT OR IGNORE INTO player_unit_use
                     (battle_id, player_id, unit, added, sold, peak, last, max_level)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    battle_id,
                    player_id,
                    unit.name,
                    unit.added,
                    unit.sold,
                    unit.peak,
                    unit.last,
                    unit.max_level,
                ],
            )?;
        }
    }
    Ok(())
}

fn insert_player(
    tx: &Connection,
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
        "INSERT OR IGNORE INTO players
             (battle_id, player_id, name, won, final_hp, score, starter_pack)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            battle_id,
            player_id,
            player.name.as_str(),
            won,
            player.last_round_hp,
            player.score as i64,
            player
                .starter_pack
                .as_ref()
                .map(serde_json::to_string)
                .transpose()?,
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
    let fielded: std::collections::BTreeSet<String> = player
        .last_round_units
        .iter()
        .map(|u| u.unit_type.display_name())
        .collect();
    for unit in &player.enabled_techs {
        if !fielded.contains(&unit.unit_name) {
            continue;
        }
        for tech in &unit.techs {
            let name = tech.name.as_deref().unwrap_or(&tech.id);
            tx.execute(
                "INSERT OR IGNORE INTO player_techs (battle_id, player_id, unit, tech, tech_id)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![battle_id, player_id, unit.unit_name, name, tech.id],
            )?;
        }
    }
    let mut picked: BTreeMap<(String, &str), i64> = BTreeMap::new();
    // Starting specialists are chosen too, just before round 1 instead of from offered cards.
    for name in &player.starting_specialists {
        *picked.entry((name.clone(), "specialist")).or_default() += 1;
    }
    for card in &player.picked_cards {
        let name = card.name.clone().unwrap_or_else(|| card.id.clone());
        *picked.entry((name, card.kind)).or_default() += 1;
    }
    for ((card, kind), picks) in picked {
        tx.execute(
            "INSERT OR IGNORE INTO player_cards (battle_id, player_id, card, kind, picks)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![battle_id, player_id, card, kind, picks],
        )?;
    }
    for specialist in &player.specialist_names {
        tx.execute(
            "INSERT OR IGNORE INTO player_specialists (battle_id, player_id, specialist)
             VALUES (?1, ?2, ?3)",
            params![battle_id, player_id, specialist],
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixtures() -> Vec<(String, Vec<u8>)> {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../mechabellum/fixtures");
        let mut found: Vec<(String, Vec<u8>)> = std::fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.extension().is_some_and(|e| e == "xml" || e == "grbr"))
            .map(|path| {
                let name = path.file_name().unwrap().to_string_lossy().into_owned();
                (name, std::fs::read(path).unwrap())
            })
            .collect();
        found.sort();
        found
    }

    /// Every fixture that stores, ingested the ordinary way.
    fn ingest_fixtures(store: &Store) -> usize {
        fixtures()
            .iter()
            .filter(|(name, bytes)| store.ingest(bytes, Path::new(name), Some("key")).is_ok())
            .count()
    }

    /// Every derived row, columns in name order, rows sorted; `ingested_at` is the time of the
    /// insert and is left out.
    fn derived_rows(store: &Store) -> Vec<String> {
        let conn = store.conn();
        let mut rows = Vec::new();
        for table in DERIVED_TABLES {
            let mut columns: Vec<String> = conn
                .prepare("SELECT name FROM pragma_table_info(?1, 'main')")
                .unwrap()
                .query_map([table], |r| r.get(0))
                .unwrap()
                .map(Result::unwrap)
                .filter(|c: &String| c != "ingested_at")
                .collect();
            columns.sort();
            let mut stmt = conn
                .prepare(&format!("SELECT {} FROM {table}", columns.join(", ")))
                .unwrap();
            let mut found: Vec<String> = stmt
                .query_map([], |r| {
                    let values = (0..columns.len())
                        .map(|i| {
                            r.get::<_, rusqlite::types::Value>(i)
                                .map(|v| format!("{v:?}"))
                        })
                        .collect::<rusqlite::Result<Vec<_>>>()?;
                    Ok(format!("{table}|{}", values.join("|")))
                })
                .unwrap()
                .map(Result::unwrap)
                .collect();
            found.sort();
            rows.extend(found);
        }
        rows
    }

    fn count(store: &Store, sql: &str) -> i64 {
        store.conn().query_row(sql, [], |r| r.get(0)).unwrap()
    }

    /// A rebuild writes exactly what ingesting the replays wrote. The comparison can fail: rows
    /// deleted from one table show up as a difference until the rebuild restores them.
    #[test]
    fn test_rebuild_reproduces_ingest() {
        let store = Store::open_in_memory().unwrap();
        let stored = ingest_fixtures(&store);
        assert!(stored >= 7, "{stored} fixtures stored");
        let ingested = derived_rows(&store);
        assert!(ingested.len() > 1000, "{} rows", ingested.len());

        store
            .conn()
            .execute_batch("DELETE FROM player_fights")
            .unwrap();
        assert_ne!(derived_rows(&store), ingested);

        assert_eq!(store.reindex().unwrap(), stored);
        assert_eq!(derived_rows(&store), ingested);
    }

    /// The desktop app rebuilds inside a small low-priority pool; a one-thread pool, where the
    /// caller is the pool's only thread, must still finish.
    #[test]
    fn test_rebuild_in_a_one_thread_pool() {
        let store = Store::open_in_memory().unwrap();
        let stored = ingest_fixtures(&store);
        let ingested = derived_rows(&store);
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(1)
            .build()
            .unwrap();
        assert_eq!(pool.install(|| store.reindex()).unwrap(), stored);
        assert_eq!(derived_rows(&store), ingested);
    }

    /// On disk the scratch database sits beside the real one and is gone afterwards.
    #[test]
    fn test_rebuild_on_disk_leaves_no_scratch() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("stats.db")).unwrap();
        let stored = ingest_fixtures(&store);
        let ingested = derived_rows(&store);
        assert_eq!(store.reindex().unwrap(), stored);
        assert_eq!(derived_rows(&store), ingested);
        let left: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| name.contains("rebuild"))
            .collect();
        assert!(left.is_empty(), "{left:?}");
    }

    /// Replays stored after staging are parsed at the swap, with their uploaders; ones removed
    /// meanwhile stay removed; an unreadable stored replay is skipped, not retried.
    #[test]
    fn test_rebuild_keeps_changes_made_while_staging() {
        let all = fixtures();
        let (late_name, late_bytes) = all
            .iter()
            .find(|(name, _)| name == "battle_record_2340.xml")
            .unwrap();
        let reference = Store::open_in_memory().unwrap();
        reference
            .ingest(late_bytes, Path::new(late_name), Some("late"))
            .unwrap();

        let store = Store::open_in_memory().unwrap();
        for (name, bytes) in all.iter().filter(|(name, _)| name != late_name) {
            let _ = store.ingest(bytes, Path::new(name), Some("key"));
        }
        let mut gz = GzEncoder::new(Vec::new(), Compression::default());
        gz.write_all(b"<BattleRecord>not a replay").unwrap();
        store
            .conn()
            .execute(
                "INSERT INTO replay_blobs (battle_id, xml_gz) VALUES ('junk', ?1)",
                [gz.finish().unwrap()],
            )
            .unwrap();
        let before = count(&store, "SELECT COUNT(*) FROM matches");
        let removed: String = store
            .conn()
            .query_row("SELECT battle_id FROM matches LIMIT 1", [], |r| r.get(0))
            .unwrap();

        let scratch = Scratch::beside(None);
        store.stage(&scratch.path).unwrap();
        store
            .ingest(late_bytes, Path::new(late_name), Some("late"))
            .unwrap();
        store
            .conn()
            .execute_batch(&format!(
                "DELETE FROM matches WHERE battle_id = '{removed}';
                 DELETE FROM replay_blobs WHERE battle_id = '{removed}';"
            ))
            .unwrap();
        let (stored, late) = store.swap_in(&scratch.path).unwrap();
        assert_eq!((stored as i64, late), (before, 1));

        let late_id: String = reference
            .conn()
            .query_row("SELECT battle_id FROM matches", [], |r| r.get(0))
            .unwrap();
        let uploader: Option<String> = store
            .conn()
            .query_row(
                "SELECT uploaded_by FROM matches WHERE battle_id = ?1",
                [&late_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(uploader.as_deref(), Some("late"));
        let gone = count(
            &store,
            &format!("SELECT COUNT(*) FROM players WHERE battle_id = '{removed}'"),
        );
        assert_eq!(gone, 0);
        let late_rows: Vec<String> = derived_rows(&store)
            .into_iter()
            .filter(|row| row.contains(&late_id))
            .collect();
        assert_eq!(late_rows, derived_rows(&reference));
    }

    #[test]
    fn test_a_parser_panic_is_one_replay_error() {
        let err = contained("b1", || -> Result<()> {
            panic!("attempt to add with overflow")
        })
        .unwrap_err();
        assert!(err.to_string().contains("b1"), "{err}");
        assert!(err.to_string().contains("overflow"), "{err}");
        assert_eq!(contained("b2", || Ok(7)).unwrap(), 7);
        assert!(contained("b3", || -> Result<()> { anyhow::bail!("plain error") }).is_err());
    }
}
