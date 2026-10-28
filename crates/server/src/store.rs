//! SQLite storage: one row per match and per player, plus the compressed original replay so
//! everything derived can be rebuilt when the parser improves (`Store::reindex`).

use anyhow::{Context, Result};
use flate2::{read::GzDecoder, write::GzEncoder, Compression};
use mechabellum::{BattleRecord, MatchDetail, PlayerSummary};
use rayon::prelude::*;
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::{Mutex, MutexGuard, PoisonError};

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
    added     INTEGER NOT NULL,
    sold      INTEGER NOT NULL,
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
/// isn't settled either.
const SCHEMA_VERSION: i64 = 19;

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
      AND (?8 = 1 OR instr(m.battle_id, '__') = 0)",
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
/// rotation), HP, and results. Names, and anything only one client records, are left out.
fn fingerprint(record: &BattleRecord) -> String {
    use std::fmt::Write;
    let detail = record.match_detail();
    let mut facts = format!(
        "{}|{}|{}\n",
        detail.battle_id, detail.version, detail.map_id
    );
    for round in &detail.rounds {
        for (side, player) in round.sides.iter().zip(&detail.players) {
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
                round.round, player.id, side.hp, side.result
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
        let keys_bound = conn
            .prepare("SELECT 1 FROM pragma_table_info('upload_keys') WHERE name = 'player_id'")?
            .exists([])?;
        if !keys_bound {
            conn.execute_batch("ALTER TABLE upload_keys ADD COLUMN player_id TEXT")?;
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
            conn.execute_batch(
                "ALTER TABLE matches ADD COLUMN settled INTEGER NOT NULL DEFAULT 1",
            )?;
        }
        let store = Store {
            conn: Mutex::new(conn),
        };
        // A fresh database has nothing to rebuild.
        if store.match_count()? == 0 {
            store.mark_current()?;
        }
        Ok(store)
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

        let bound: Option<String> = self
            .conn()
            .query_row(
                "SELECT player_id FROM upload_keys WHERE name = ?1",
                [key_name],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        let Some(player) = bound.clone().or_else(|| claimed.map(str::to_owned)) else {
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
            .find(|p| p.id.trim() == player)
        else {
            return reject(
                403,
                format!("This key shares games for player {player}, who isn't in this match."),
                vec![],
            );
        };
        let own_name = own.name.clone();
        if bound.is_none() {
            self.conn().execute(
                "UPDATE upload_keys SET player_id = ?2 WHERE name = ?1 AND player_id IS NULL",
                params![key_name, player],
            )?;
        }

        let shared = mechabellum::hide_other_names(document, &player);
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
        if let IngestOutcome::Duplicate { battle_id } = &outcome {
            self.conn().execute(
                "UPDATE players SET name = ?3 WHERE battle_id = ?1 AND player_id = ?2 AND name = ?4",
                params![battle_id, player, own_name, mechabellum::HIDDEN_NAME],
            )?;
        }
        Ok(Ok(outcome))
    }

    /// The account a key uploads for, once bound.
    pub fn key_player(&self, key_name: &str) -> Result<Option<String>> {
        Ok(self
            .conn()
            .query_row(
                "SELECT player_id FROM upload_keys WHERE name = ?1",
                [key_name],
                |r| r.get(0),
            )
            .optional()?
            .flatten())
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

    /// Record ratings for players in a stored match. Returns how many were stored; ratings for
    /// players not in that match are ignored.
    pub fn add_ratings(&self, battle_id: &str, ratings: &[Rating]) -> Result<usize> {
        let mut conn = self.conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut stored = 0;
        for r in ratings.iter().filter(|r| r.plausible()) {
            let in_match = tx
                .prepare_cached("SELECT 1 FROM players WHERE battle_id = ?1 AND player_id = ?2")?
                .exists(params![battle_id, r.player_id])?;
            if in_match {
                stored += tx.execute(
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
        const COUNTED: &str = "settled = 1 AND instr(battle_id, '__') = 0
            AND EXISTS (SELECT 1 FROM players p WHERE p.battle_id = matches.battle_id)";
        let versions = conn
            .prepare(&format!(
                "SELECT version, SUM({COUNTED}) FROM matches GROUP BY version ORDER BY version DESC"
            ))?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        let modes = conn
            .prepare(&format!(
                "SELECT match_mode, game_mode, SUM({COUNTED}) FROM matches
                 GROUP BY match_mode, game_mode ORDER BY SUM({COUNTED}) DESC, COUNT(*) DESC"
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
    /// Replays are decompressed and parsed on every core without holding the database; only
    /// the final rewrite is one locked transaction, so readers wait briefly if at all.
    pub fn reindex(&self) -> Result<usize> {
        let blobs: Vec<(String, Vec<u8>)> = self
            .conn()
            .prepare("SELECT battle_id, xml_gz FROM replay_blobs")?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;

        let parse = |battle_id: &str, gz: &[u8]| -> Option<(BattleRecord, MatchDetail)> {
            let parsed = (|| -> Result<BattleRecord> {
                let mut xml = String::new();
                GzDecoder::new(gz).read_to_string(&mut xml)?;
                Ok(BattleRecord::from_bytes(
                    xml.as_bytes(),
                    Path::new(battle_id),
                )?)
            })();
            match parsed.and_then(|record| {
                let detail = contained(battle_id, || Ok(record.match_detail()))?;
                Ok((record, detail))
            }) {
                Ok(parsed) => Some(parsed),
                // Keep the rest of the database usable if one stored replay is unreadable.
                Err(e) => {
                    log::warn!("reindex {battle_id}: {e:#}");
                    None
                }
            }
        };
        let mut records: BTreeMap<String, (BattleRecord, MatchDetail)> = blobs
            .into_par_iter()
            .filter_map(|(battle_id, gz)| Some((battle_id.clone(), parse(&battle_id, &gz)?)))
            .collect();

        let mut conn = self.conn();
        let mut tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        // The replays stored now, with their uploaders. Uploads and the desktop app's replay
        // watcher keep storing while the others are parsed above (the app upgrades in the
        // background): ones stored meanwhile are parsed here, under the lock, and ones removed
        // meanwhile aren't brought back.
        let stored: BTreeMap<String, Option<String>> = tx
            .prepare(
                "SELECT b.battle_id, m.uploaded_by FROM replay_blobs b
                 LEFT JOIN matches m ON m.battle_id = b.battle_id",
            )?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        records.retain(|battle_id, _| stored.contains_key(battle_id));
        let late: Vec<(String, Vec<u8>)> = {
            let mut stmt = tx.prepare("SELECT xml_gz FROM replay_blobs WHERE battle_id = ?1")?;
            stored
                .keys()
                .filter(|battle_id| !records.contains_key(*battle_id))
                .filter_map(|battle_id| {
                    let gz: Vec<u8> = stmt.query_row([battle_id], |r| r.get(0)).ok()?;
                    Some((battle_id.clone(), gz))
                })
                .collect()
        };
        for (battle_id, gz) in late {
            if let Some(parsed) = parse(&battle_id, &gz) {
                records.insert(battle_id, parsed);
            }
        }

        tx.execute("DELETE FROM matches", [])?;
        let mut inserted = 0;
        for (battle_id, (record, detail)) in &records {
            let uploaded_by = stored.get(battle_id).cloned().flatten();
            // One record that won't insert is logged and skipped, not allowed to undo the rest:
            // every command upgrades first, so a failure here would stop the server starting.
            let savepoint = tx.savepoint()?;
            match contained(battle_id, || {
                insert_record(
                    &savepoint,
                    battle_id,
                    record,
                    detail,
                    uploaded_by.as_deref(),
                )
            }) {
                Ok(()) => {
                    savepoint.commit()?;
                    inserted += 1;
                }
                Err(e) => log::warn!("reindex {battle_id}: not stored: {e:#}"),
            }
        }
        tx.commit()?;
        Ok(inserted)
    }

    pub fn recent_matches(&self, limit: u32) -> Result<Vec<MatchRow>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
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
             FROM matches m ORDER BY m.occurred_on DESC, m.battle_id DESC LIMIT ?1",
        )?;
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
