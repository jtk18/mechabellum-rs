//! Replay ingestion and stats API. Runs standalone (`mechabellum-server serve`) or embedded in the
//! desktop app via [`router`].

pub mod aggregate;
mod games;
mod store;

pub use games::{
    ArmyMeans, CardPage, CardPlayer, DeploymentStat, GameStats, MatchCard, Matchup, UnitUseStat,
};
pub use store::{
    Dispute, Facets, IngestOutcome, MatchCopy, MatchRow, Period, Rating, RatingBand, Rejection,
    RobustStats, Stats, StatsFilter, Store, SyncRejection, TechStat, Trend, TrendCell,
    UnitTechStats, UploadKey, UsageStat, VersionTotals, VersionTrends,
};

/// Header naming the Mechabellum account (user ID) a shared upload is for.
pub const PLAYER_HEADER: &str = "x-mechabellum-player";
/// Name recorded for uploads made with the admin token.
const ADMIN: &str = "admin";

use axum::{
    body::Bytes,
    extract::{DefaultBodyLimit, Path as UrlPath, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use std::path::Path;
use std::sync::Arc;

/// Largest accepted upload. Replays are usually 0.3-2 MB.
const MAX_REPLAY_BYTES: usize = 32 * 1024 * 1024;

/// Who may upload. Reads are always open.
#[derive(Debug, Clone)]
pub enum UploadAuth {
    /// Anyone. For the local instance inside the desktop app.
    Open,
    /// `Authorization: Bearer <key>` with an active key from the database (recorded as the
    /// uploader), or the optional admin token (recorded as `admin`).
    Required { admin_token: Option<String> },
}

#[derive(Clone)]
struct AppState {
    store: Arc<Store>,
    auth: Arc<UploadAuth>,
}

pub fn router(store: Arc<Store>, auth: UploadAuth) -> Router {
    let state = AppState {
        store,
        auth: Arc::new(auth),
    };
    Router::new()
        .route("/", get(website))
        .route("/stats", get(stats_page))
        .route("/matches", get(matches_page))
        .route("/health", get(|| async { "ok" }))
        .route("/api/facets", get(facets))
        .route("/api/replays", get(list_replays).post(upload_replay))
        .route("/api/stats", get(stats))
        .route("/api/stats/versions", get(version_trends))
        .route("/api/stats/techs", get(tech_stats))
        .route("/api/stats/robust", get(robust_stats))
        .route("/api/stats/games", get(game_stats))
        .route("/api/cards", get(cards))
        .route("/api/replays/{battle_id}", get(match_detail))
        .route("/api/replays/missing", axum::routing::post(missing_replays))
        .route(
            "/api/replays/{battle_id}/ratings",
            axum::routing::post(add_ratings),
        )
        .layer(DefaultBodyLimit::max(MAX_REPLAY_BYTES))
        .with_state(state)
}

struct ApiError(StatusCode, String);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(serde_json::json!({ "error": self.1 }))).into_response()
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(e: anyhow::Error) -> Self {
        ApiError(StatusCode::INTERNAL_SERVER_ERROR, format!("{e:#}"))
    }
}

/// Run blocking store work (SQLite, XML parsing) off the async runtime.
async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> anyhow::Result<T> + Send + 'static,
) -> Result<T, ApiError> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .map_err(Into::into)
}

/// Constant-time equality for secrets. Both sides are hashed first so lengths match.
fn secrets_equal(a: &str, b: &str) -> bool {
    let (a, b) = (Sha256::digest(a.as_bytes()), Sha256::digest(b.as_bytes()));
    a.iter()
        .zip(b.iter())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

/// Returns the uploader's name, or `None` when uploads are open.
async fn authenticate(state: &AppState, headers: &HeaderMap) -> Result<Option<String>, ApiError> {
    let UploadAuth::Required { admin_token } = state.auth.as_ref() else {
        return Ok(None);
    };
    let unauthorized = || {
        ApiError(
            StatusCode::UNAUTHORIZED,
            "missing or invalid upload key".into(),
        )
    };
    let presented = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(unauthorized)?
        .to_owned();
    if admin_token
        .as_deref()
        .is_some_and(|admin| secrets_equal(&presented, admin))
    {
        return Ok(Some(ADMIN.into()));
    }
    let store = state.store.clone();
    blocking(move || store.verify_key(&presented))
        .await?
        .map(Some)
        .ok_or_else(unauthorized)
}

/// Bodies may be gzip-compressed (the desktop app sends them that way).
fn decompress(body: Bytes) -> Result<Bytes, ApiError> {
    if !body.starts_with(&[0x1f, 0x8b]) {
        return Ok(body);
    }
    let mut out = Vec::new();
    std::io::Read::read_to_end(
        &mut std::io::Read::take(
            flate2::read::GzDecoder::new(&body[..]),
            MAX_REPLAY_BYTES as u64 * 8,
        ),
        &mut out,
    )
    .map_err(|e| ApiError(StatusCode::BAD_REQUEST, format!("bad gzip body: {e}")))?;
    Ok(out.into())
}

async fn upload_replay(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let uploader = authenticate(&state, &headers).await?;
    let body = decompress(body)?;

    // Community uploads (a member's key) go through the checks in docs/upload-trust.md.
    if let Some(key) = uploader.clone().filter(|u| u != ADMIN) {
        let claimed = headers
            .get(PLAYER_HEADER)
            .and_then(|v| v.to_str().ok())
            .map(|v| v.trim().to_owned());
        let store = state.store.clone();
        let result = blocking(move || store.ingest_shared(&body, &key, claimed.as_deref())).await?;
        return Ok(match result {
            Ok(outcome) => {
                let status = match outcome {
                    IngestOutcome::Added { .. } => StatusCode::CREATED,
                    IngestOutcome::Duplicate { .. } => StatusCode::OK,
                };
                (status, Json(outcome)).into_response()
            }
            Err(rejection) => (
                StatusCode::from_u16(rejection.status).unwrap_or(StatusCode::BAD_REQUEST),
                Json(rejection),
            )
                .into_response(),
        });
    }

    if mechabellum::BattleRecord::extract_document(&body).is_none() {
        return Err(ApiError(
            StatusCode::BAD_REQUEST,
            "body is not a Mechabellum replay (.grbr or .xml)".into(),
        ));
    }
    let store = state.store.clone();
    let outcome = tokio::task::spawn_blocking(move || {
        store.ingest(&body, Path::new("<upload>"), uploader.as_deref())
    })
    .await
    .map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map_err(|e| match e.downcast_ref::<mechabellum::ParseError>() {
        // The upload was a replay but not one we can read: the client's problem, not ours.
        Some(parse) => ApiError(StatusCode::UNPROCESSABLE_ENTITY, parse.to_string()),
        None => e.into(),
    })?;
    let status = match outcome {
        IngestOutcome::Added { .. } => StatusCode::CREATED,
        IngestOutcome::Duplicate { .. } => StatusCode::OK,
    };
    Ok((status, Json(outcome)).into_response())
}

#[derive(Deserialize)]
struct MissingRequest {
    battle_ids: Vec<String>,
}

/// Which of these matches the server doesn't have, so clients only send those.
async fn missing_replays(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<MissingRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    authenticate(&state, &headers).await?;
    if request.battle_ids.len() > 5000 {
        return Err(ApiError(
            StatusCode::PAYLOAD_TOO_LARGE,
            "ask about at most 5000 matches at a time".into(),
        ));
    }
    let store = state.store.clone();
    let missing = blocking(move || store.missing(&request.battle_ids)).await?;
    Ok(Json(serde_json::json!({ "missing": missing })))
}

/// Ratings for a match's players, captured from the game's log. A community key may only add
/// ratings to matches its own player took part in.
async fn add_ratings(
    State(state): State<AppState>,
    headers: HeaderMap,
    UrlPath(battle_id): UrlPath<String>,
    Json(ratings): Json<Vec<Rating>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let uploader = authenticate(&state, &headers).await?;
    let store = state.store.clone();
    let stored = blocking(move || {
        if let Some(key) = uploader.filter(|u| u != ADMIN) {
            let Some(player) = store.key_player(&key)? else {
                return Ok(None);
            };
            if !store.missing(std::slice::from_ref(&battle_id))?.is_empty()
                || !store.ratings_allowed(&battle_id, &player)?
            {
                return Ok(None);
            }
        }
        store.add_ratings(&battle_id, &ratings).map(Some)
    })
    .await?
    .ok_or_else(|| {
        ApiError(
            StatusCode::FORBIDDEN,
            "ratings can only be added to your own stored matches".into(),
        )
    })?;
    Ok(Json(serde_json::json!({ "stored": stored })))
}

#[derive(Deserialize)]
struct ListQuery {
    limit: Option<u32>,
}

async fn list_replays(
    State(state): State<AppState>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Vec<MatchRow>>, ApiError> {
    let limit = query.limit.unwrap_or(50).min(500);
    let store = state.store.clone();
    Ok(Json(blocking(move || store.recent_matches(limit)).await?))
}

/// The public landing page (crates/server/web/index.html), built into the binary.
async fn website() -> axum::response::Html<&'static str> {
    axum::response::Html(include_str!("../web/index.html"))
}

/// The stats page (crates/server/web/stats.html), not linked from the landing page
/// while the stats are being worked on.
async fn stats_page() -> axum::response::Html<&'static str> {
    axum::response::Html(include_str!("../web/stats.html"))
}

/// Match cards (crates/server/web/matches.html), unlinked from the landing page like the stats.
async fn matches_page() -> axum::response::Html<&'static str> {
    axum::response::Html(include_str!("../web/matches.html"))
}

/// Patches and modes that have matches, for filters. Unlike `Store::facets`, it doesn't name any
/// player.
#[derive(serde::Serialize)]
struct PublicFacets {
    versions: Vec<(u32, u64)>,
    modes: Vec<(String, String, u64)>,
    corroborated: u64,
    disputed: u64,
}

async fn facets(State(state): State<AppState>) -> Result<Json<PublicFacets>, ApiError> {
    let store = state.store.clone();
    Ok(Json(
        blocking(move || {
            let f = store.facets()?;
            let (corroborated, disputed) = store.corroboration()?;
            Ok(PublicFacets {
                versions: f.versions,
                modes: f.modes,
                corroborated,
                disputed,
            })
        })
        .await?,
    ))
}

async fn stats(
    State(state): State<AppState>,
    Query(filter): Query<StatsFilter>,
) -> Result<Json<Stats>, ApiError> {
    let store = state.store.clone();
    Ok(Json(blocking(move || store.stats(&filter)).await?))
}

#[derive(Deserialize)]
struct CardQuery {
    limit: Option<u32>,
    offset: Option<u32>,
}

/// Match cards, newest first. Takes the stats filters, plus `limit` and `offset` for paging.
async fn cards(
    State(state): State<AppState>,
    Query(filter): Query<StatsFilter>,
    Query(page): Query<CardQuery>,
) -> Result<Json<CardPage>, ApiError> {
    let limit = page.limit.unwrap_or(24).min(100);
    let offset = page.offset.unwrap_or(0);
    let store = state.store.clone();
    Ok(Json(
        blocking(move || store.cards(&filter, limit, offset)).await?,
    ))
}

async fn game_stats(
    State(state): State<AppState>,
    Query(filter): Query<StatsFilter>,
) -> Result<Json<GameStats>, ApiError> {
    let store = state.store.clone();
    Ok(Json(blocking(move || store.game_stats(&filter)).await?))
}

async fn tech_stats(
    State(state): State<AppState>,
    Query(filter): Query<StatsFilter>,
) -> Result<Json<Vec<UnitTechStats>>, ApiError> {
    let store = state.store.clone();
    Ok(Json(blocking(move || store.tech_stats(&filter)).await?))
}

async fn match_detail(
    State(state): State<AppState>,
    UrlPath(battle_id): UrlPath<String>,
) -> Result<Json<mechabellum::MatchDetail>, ApiError> {
    let store = state.store.clone();
    blocking(move || store.match_detail(&battle_id))
        .await?
        .map(Json)
        .ok_or_else(|| ApiError(StatusCode::NOT_FOUND, "no such match".into()))
}

/// Win rates with each uploader's weight capped and a median of per-uploader rates.
async fn robust_stats(
    State(state): State<AppState>,
    Query(filter): Query<StatsFilter>,
) -> Result<Json<RobustStats>, ApiError> {
    let store = state.store.clone();
    Ok(Json(blocking(move || store.robust_stats(&filter)).await?))
}

async fn version_trends(
    State(state): State<AppState>,
    Query(filter): Query<StatsFilter>,
) -> Result<Json<VersionTrends>, ApiError> {
    let store = state.store.clone();
    Ok(Json(blocking(move || store.version_trends(&filter)).await?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    use std::path::PathBuf;

    const OLD: &str = "1425_20241123--67298547_[小疯子]VS[kaltorak18].grbr";
    const NEW: &str = "battle_record_2324.xml";

    fn fixture(name: &str) -> Vec<u8> {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../mechabellum/fixtures");
        std::fs::read(dir.join(name)).unwrap()
    }

    fn count(store: &Store, table: &str) -> u64 {
        store
            .conn()
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            .unwrap()
    }

    /// A match both players shared, with copies that agree, is corroborated; a copy that
    /// disagrees disputes it and takes it out of the corroborated view.
    #[test]
    fn test_corroboration() {
        let store = Store::open_in_memory().unwrap();
        store.create_key("mine").unwrap();
        store.create_key("theirs").unwrap();
        let corroborated_only = StatsFilter {
            corroborated_only: true,
            ..Default::default()
        };
        let battle = "20260924--67113734";

        store
            .ingest_shared(&fixture(NEW), "mine", Some(JTK18))
            .unwrap()
            .unwrap();
        assert_eq!(
            store.corroboration().unwrap(),
            (0, 0),
            "one copy proves nothing"
        );
        assert_eq!(store.stats(&corroborated_only).unwrap().matches, 0);

        // The same copy again from the same player still doesn't count twice.
        store
            .ingest_shared(&fixture(NEW), "mine", Some(JTK18))
            .unwrap()
            .unwrap();
        assert_eq!(store.corroboration().unwrap(), (0, 0));

        store
            .ingest_shared(&fixture(NEW), "theirs", Some(BOLSANO))
            .unwrap()
            .unwrap();
        assert_eq!(store.corroboration().unwrap(), (1, 0));
        assert_eq!(store.stats(&corroborated_only).unwrap().matches, 1);
        assert!(store.disputes().unwrap().is_empty());

        // A third copy that disagrees (as if edited) disputes the match.
        store
            .conn()
            .execute(
                "INSERT INTO match_copies (battle_id, uploaded_by, player_id, fingerprint)
                 VALUES (?1, 'someone', 'x', 'different')",
                [battle],
            )
            .unwrap();
        assert_eq!(store.corroboration().unwrap(), (0, 1));
        assert_eq!(store.stats(&corroborated_only).unwrap().matches, 0);
        let disputes = store.disputes().unwrap();
        assert_eq!(disputes.len(), 1);
        assert_eq!(disputes[0].battle_id, battle);
        assert_eq!(disputes[0].copies.len(), 3);

        // Purging the odd key restores it.
        store.purge_uploader("someone").unwrap();
        assert_eq!(store.corroboration().unwrap(), (1, 0));
    }

    #[test]
    fn test_win_rate_by_opponent_rating() {
        let store = Store::open_in_memory().unwrap();
        store.ingest(&fixture(NEW), Path::new("a"), None).unwrap();
        let battle = "20260924--67113734";
        let rating = |player: &str, elo: i64| Rating {
            player_id: player.into(),
            elo: Some(elo),
            rank_point: None,
            level: None,
            elo_games: None,
        };
        let mine = |store: &Store| {
            store
                .stats(&StatsFilter {
                    player_id: Some(JTK18.into()),
                    ..Default::default()
                })
                .unwrap()
                .vs_rating
        };
        assert!(mine(&store).iter().all(|b| b.games == 0), "no ratings yet");

        // The opponent was rated 100 higher.
        store
            .put_ratings(battle, &[rating(JTK18, 1400), rating(BOLSANO, 1500)])
            .unwrap();
        let bands = mine(&store);
        let band = bands.iter().find(|b| b.games > 0).expect("one rated game");
        assert_eq!((band.from, band.to), (Some(50), Some(150)));
        assert_eq!(band.games, 1);
        let won = store
            .recent_matches(1)
            .unwrap()
            .remove(0)
            .winners
            .contains(&"jtk18".to_owned());
        assert_eq!(band.wins, won as u64);
    }

    /// Text from replays and requests only ever reaches SQLite as bound parameters, so SQL in a
    /// player name, battle ID, or filter is stored or matched literally, never run.
    #[test]
    fn test_sql_in_replays_is_just_text() {
        let store = Store::open_in_memory().unwrap();
        let evil_name = "Robert'); DROP TABLE matches; --";
        let evil_id = "20260924--1' OR '1'='1";
        let document = String::from_utf8(fixture(NEW))
            .unwrap()
            .replacen(
                "<name>Bolsano</name>",
                &format!("<name>{evil_name}</name>"),
                1,
            )
            .replace("20260924--67113734", evil_id);
        let outcome = store
            .ingest(document.as_bytes(), Path::new("evil.xml"), None)
            .unwrap();
        assert!(matches!(outcome, IngestOutcome::Added { ref battle_id } if battle_id == evil_id));

        // Every table is still there, and the text was stored as written.
        for table in ["matches", "players", "player_units", "replay_blobs"] {
            assert!(count(&store, table) > 0, "{table}");
        }
        assert_eq!(name_of(&store, BOLSANO), evil_name);
        let detail = store
            .match_detail(evil_id)
            .unwrap()
            .expect("found by its literal ID");
        assert!(detail.players.iter().any(|p| p.name == evil_name));

        // Hostile filters match nothing instead of everything.
        let filtered = store
            .stats(&StatsFilter {
                player_id: Some("x' OR '1'='1".into()),
                match_mode: Some("'; DELETE FROM players; --".into()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(filtered.matches, 0);
        assert!(store.match_detail("' OR '1'='1").unwrap().is_none());
        assert_eq!(store.stats(&StatsFilter::default()).unwrap().matches, 1);
    }

    /// Starter packs reach the match list, one per player, for old and new replays alike.
    #[test]
    fn test_match_list_has_starter_packs() {
        let store = Store::open_in_memory().unwrap();
        store
            .ingest(&fixture("battle_record.xml"), Path::new("a"), None)
            .unwrap();
        store.ingest(&fixture(OLD), Path::new("b"), None).unwrap();
        let rows = store.recent_matches(10).unwrap();
        let packs: Vec<_> = rows
            .iter()
            .flat_map(|m| m.winner_packs.iter().chain(&m.loser_packs))
            .flatten()
            .collect();
        assert_eq!(packs.len(), 4, "both players of both matches");
        assert!(packs
            .iter()
            .any(|p| p.id == "9874" && p.units[0].count == 3));
        for m in &rows {
            assert_eq!(m.winner_packs.len(), m.winners.len());
            assert_eq!(m.loser_packs.len(), m.losers.len());
        }
    }

    #[test]
    fn test_match_cards_and_game_stats() {
        let store = Store::open_in_memory().unwrap();
        for (name, path) in [
            ("battle_record_2340.xml", "a"),
            (NEW, "b"),
            ("battle_record.xml", "c"),
        ] {
            store.ingest(&fixture(name), Path::new(path), None).unwrap();
        }
        assert_eq!(count(&store, "player_games"), 6);

        let page = store.cards(&StatsFilter::default(), 2, 0).unwrap();
        assert_eq!(page.total, 3);
        assert_eq!(page.cards.len(), 2, "limit");
        let newest = &page.cards[0];
        assert_eq!(newest.version, 2340, "newest first");
        // 2340 lists the deciding fight, so one player won and one lost.
        let won: Vec<bool> = newest.players.iter().map(|p| p.won).collect();
        assert_eq!(won, [true, false], "winners first");
        let game = newest.players[0].game.as_ref().unwrap();
        assert_eq!(game["fights"].as_array().unwrap().len(), 8);
        assert!(game["opening"].is_string(), "classed by round 1");
        // Cards name players by name only, never by account ID.
        let json = serde_json::to_string(&page).unwrap();
        assert!(!json.contains(JTK18), "{json}");
        let rest = store.cards(&StatsFilter::default(), 2, 2).unwrap();
        assert_eq!(rest.cards.len(), 1, "offset");
        let only_2340 = StatsFilter {
            min_version: Some(2340),
            ..Default::default()
        };
        assert_eq!(store.cards(&only_2340, 10, 0).unwrap().total, 1);

        let stats = store.game_stats(&StatsFilter::default()).unwrap();
        assert_eq!(stats.matches, 3);
        assert!(stats.lengths.contains(&(8, 1)), "{:?}", stats.lengths);
        // Matches are classed by round 1: only 2340's, since the 2324 fixture has no record of
        // round 1 and a later round doesn't stand in. The control: one winner per 1v1 match and
        // per fight, so wins across labels are half the results.
        let matches: u64 = stats.deployments.iter().map(|d| d.matches).sum();
        let wins: u64 = stats.deployments.iter().map(|d| d.wins).sum();
        let round_one: u64 = stats.deployments.iter().map(|d| d.round_one_wins).sum();
        assert_eq!((matches, wins, round_one), (2, 1, 1));
        // Each match counts once from each side: A against B mirrors B against A.
        assert!(!stats.matchups.is_empty());
        for m in &stats.matchups {
            let other = stats
                .matchups
                .iter()
                .find(|o| o.deployment == m.against && o.against == m.deployment)
                .unwrap();
            assert_eq!(other.matches, m.matches);
            assert_eq!(other.wins + m.wins, m.matches);
        }
        let (w, l) = (stats.winners.unwrap(), stats.losers.unwrap());
        assert_eq!((w.results, l.results), (3, 3));
        // The match list carries each player's round 1 deployment where it's classed.
        let rows = store.recent_matches(10).unwrap();
        let newest = rows.iter().find(|m| m.version == 2340).unwrap();
        assert!(newest
            .winner_deployments
            .iter()
            .chain(&newest.loser_deployments)
            .all(Option::is_some));
        for version in [2324, 1503] {
            let row = rows.iter().find(|m| m.version == version).unwrap();
            assert!(
                row.winner_deployments.iter().all(Option::is_none),
                "{version}: no round 1 record, or not the 2.0 board"
            );
        }
        assert!(stats.units.iter().all(|u| u.kept <= u.games && u.games > 0));
    }

    /// A database derived before schema 10 has no match summaries; upgrading rebuilds them.
    #[test]
    fn test_upgrade_adds_match_summaries() {
        let store = Store::open_in_memory().unwrap();
        store
            .ingest(&fixture("battle_record_2340.xml"), Path::new("a"), None)
            .unwrap();
        store
            .conn()
            .execute_batch(
                "DELETE FROM player_games; DELETE FROM player_fights; PRAGMA user_version = 9;",
            )
            .unwrap();
        assert!(store.needs_upgrade().unwrap());
        assert!(store.upgrade().unwrap());
        assert_eq!(count(&store, "player_games"), 2);
        assert_eq!(count(&store, "player_fights"), 16);
    }

    #[test]
    fn test_ingest_dedupes_and_counts() {
        let store = Store::open_in_memory().unwrap();
        let replay = fixture(NEW);
        let first = store.ingest(&replay, Path::new("a"), None).unwrap();
        assert!(matches!(first, IngestOutcome::Added { .. }));
        let second = store.ingest(&replay, Path::new("b"), None).unwrap();
        assert!(matches!(second, IngestOutcome::Duplicate { .. }));
        store
            .ingest(&fixture(OLD), Path::new("c"), Some("alice"))
            .unwrap();

        let stats = store.stats(&StatsFilter::default()).unwrap();
        assert_eq!(stats.matches, 2);
        assert_eq!(stats.player_results, 4);
        assert!(!stats.units.is_empty());
        let wins: u64 = store
            .recent_matches(10)
            .unwrap()
            .iter()
            .map(|m| m.winners.len() as u64)
            .sum();
        assert_eq!(wins, 2);

        let newer = store
            .stats(&StatsFilter {
                min_version: Some(2000),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(newer.matches, 1);

        // Views: "me" and "my opponents" split my matches between the two sides.
        let me = store
            .facets()
            .unwrap()
            .main_player
            .expect("someone played")
            .0;
        let mine = store
            .stats(&StatsFilter {
                player_id: Some(me.clone()),
                ..Default::default()
            })
            .unwrap();
        let theirs = store
            .stats(&StatsFilter {
                opponents_of: Some(me.clone()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(mine.matches, theirs.matches);
        assert_eq!(
            theirs.player_results, theirs.matches,
            "one opponent per 1v1 match"
        );
        assert!(theirs.player_results + mine.player_results <= stats.player_results);

        let trends = store.version_trends(&StatsFilter::default()).unwrap();
        let versions: Vec<&str> = trends.versions.iter().map(|v| v.label.as_str()).collect();
        assert_eq!(versions, vec!["1425", "2324"]);
        let months = store
            .version_trends(&StatsFilter {
                period: Period::Month,
                ..Default::default()
            })
            .unwrap();
        let labels: Vec<&str> = months.versions.iter().map(|v| v.label.as_str()).collect();
        assert_eq!(labels, vec!["2024-11", "2026-09"]);
        for trend in trends.units.iter().chain(&trends.specialists) {
            assert_eq!(trend.cells.len(), 2);
            let total: u64 = trend.cells.iter().flatten().map(|c| c.picks).sum();
            assert_eq!(total, trend.picks, "{}", trend.name);
        }
        // Per-version cells add up to the overall stats.
        let overall = &stats.units[0];
        let trend = trends
            .units
            .iter()
            .find(|t| t.name == overall.name)
            .unwrap();
        assert_eq!(trend.picks, overall.picks);

        let techs = store.tech_stats(&StatsFilter::default()).unwrap();
        let with_techs = techs
            .iter()
            .find(|u| !u.techs.is_empty())
            .expect("some unit has techs");
        for tech in &with_techs.techs {
            assert!(tech.picks <= with_techs.picks);
            if let Some(without) = tech.win_rate_without {
                assert!((0.0..=1.0).contains(&without));
            }
        }

        let battle_id = store.recent_matches(1).unwrap()[0].battle_id.clone();
        let detail = store.match_detail(&battle_id).unwrap().expect("stored");
        assert!(!detail.rounds.is_empty());
        assert!(store.match_detail("nope").unwrap().is_none());

        assert_eq!(store.reindex().unwrap(), 2);
        assert_eq!(store.stats(&StatsFilter::default()).unwrap().matches, 2);
        // reindex keeps who uploaded what
        let uploader: Option<String> = store
            .conn()
            .query_row(
                "SELECT uploaded_by FROM matches WHERE version = 1425",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(uploader.as_deref(), Some("alice"));
    }

    /// Many threads and two separate connections (like `import` running beside the server)
    /// racing to add the same two replays: each is stored exactly once, with no errors or
    /// partial rows.
    #[test]
    fn test_me_is_the_logged_in_player() {
        let store = Store::open_in_memory().unwrap();
        store.ingest(&fixture(NEW), Path::new("a"), None).unwrap();
        let players: Vec<(String, String)> = store
            .conn()
            .prepare("SELECT player_id, name FROM players ORDER BY player_id")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(players.len(), 2);
        // A one-match tie: the guess is one of them. The log settles it either way.
        let guess = store.facets().unwrap().main_player.unwrap();
        let other = players.iter().find(|p| **p != guess).unwrap().clone();
        store.set_local_player(&other.0).unwrap();
        assert_eq!(store.facets().unwrap().main_player, Some(other));
        // A logged-in account with no stored match yet has no name; fall back to the guess.
        store.set_local_player("999").unwrap();
        assert_eq!(store.facets().unwrap().main_player, Some(guess));
    }

    #[test]
    fn test_concurrent_ingest_is_serialized() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("race.db");
        let stores = [
            Arc::new(Store::open(&path).unwrap()),
            Arc::new(Store::open(&path).unwrap()),
        ];
        let replays = [Arc::new(fixture(OLD)), Arc::new(fixture(NEW))];

        let handles: Vec<_> = (0..16)
            .map(|i| {
                let store = stores[i % 2].clone();
                let replay = replays[(i / 2) % 2].clone();
                std::thread::spawn(move || store.ingest(&replay, Path::new("race"), None))
            })
            .collect();
        let outcomes: Vec<_> = handles
            .into_iter()
            .map(|h| h.join().unwrap().expect("no ingest should fail"))
            .collect();

        let added = outcomes
            .iter()
            .filter(|o| matches!(o, IngestOutcome::Added { .. }))
            .count();
        assert_eq!(added, 2);
        assert_eq!(count(&stores[0], "matches"), 2);
        assert_eq!(count(&stores[0], "players"), 4);
        assert_eq!(count(&stores[0], "replay_blobs"), 2);
    }

    #[test]
    fn test_key_lifecycle() {
        let store = Store::open_in_memory().unwrap();
        assert_eq!(store.active_key_count().unwrap(), 0);
        let key = store.create_key("desktop-jason").unwrap();
        assert!(key.starts_with("mbk_") && key.len() == 4 + 64);
        assert!(
            store.create_key("desktop-jason").is_err(),
            "names are unique"
        );

        assert_eq!(
            store.verify_key(&key).unwrap().as_deref(),
            Some("desktop-jason")
        );
        assert_eq!(store.verify_key("mbk_wrong").unwrap(), None);
        assert_eq!(store.verify_key("").unwrap(), None);

        // Only the hash is stored.
        let stored: Vec<u8> = store
            .conn()
            .query_row("SELECT key_hash FROM upload_keys", [], |r| r.get(0))
            .unwrap();
        assert_ne!(stored, key.as_bytes());

        assert!(store.revoke_key("desktop-jason").unwrap());
        assert!(!store.revoke_key("desktop-jason").unwrap());
        assert_eq!(store.verify_key(&key).unwrap(), None);
        assert_eq!(store.active_key_count().unwrap(), 0);
    }

    /// The two players in the NEW fixture.
    const JTK18: &str = "281474976711196086";
    const BOLSANO: &str = "281474976711476361";

    fn upload(auth: Option<&str>) -> Request<Body> {
        upload_as(auth, JTK18, fixture(NEW))
    }

    fn upload_as(auth: Option<&str>, player: &str, body: Vec<u8>) -> Request<Body> {
        let mut req = Request::post("/api/replays").header(PLAYER_HEADER, player);
        if let Some(auth) = auth {
            req = req.header(header::AUTHORIZATION, auth);
        }
        req.body(Body::from(body)).unwrap()
    }

    fn gzip(bytes: &[u8]) -> Vec<u8> {
        use std::io::Write;
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        gz.write_all(bytes).unwrap();
        gz.finish().unwrap()
    }

    fn name_of(store: &Store, player: &str) -> String {
        store
            .conn()
            .query_row(
                "SELECT name FROM players WHERE player_id = ?1",
                [player],
                |r| r.get(0),
            )
            .unwrap()
    }

    #[tokio::test]
    async fn test_shared_uploads() {
        let store = Arc::new(Store::open_in_memory().unwrap());
        let mine = store.create_key("jtk18-desktop").unwrap();
        let theirs = store.create_key("bolsano-desktop").unwrap();
        let app = router(store.clone(), UploadAuth::Required { admin_token: None });
        let bearer = |k: &str| format!("Bearer {k}");

        // Gzipped upload for my own account: stored with the opponent's name removed.
        let req = upload_as(Some(&bearer(&mine)), JTK18, gzip(&fixture(NEW)));
        assert_eq!(status(&app, req).await, StatusCode::CREATED);
        assert_eq!(name_of(&store, JTK18), "jtk18");
        assert_eq!(name_of(&store, BOLSANO), mechabellum::HIDDEN_NAME);
        assert_eq!(
            store.key_player("jtk18-desktop").unwrap().as_deref(),
            Some(JTK18)
        );
        let stored = store
            .replay_document("20260924--67113734")
            .unwrap()
            .unwrap();
        assert!(
            !stored.contains("<name>Bolsano</name>"),
            "the stored replay has no opponent name"
        );

        // The key is now bound: it can't share games as someone else.
        let req = upload_as(Some(&bearer(&mine)), BOLSANO, fixture(NEW));
        assert_eq!(
            status(&app, req).await,
            StatusCode::OK,
            "bound player is used, claim ignored"
        );
        let other_game = fixture("1425_20241123--67298547_[小疯子]VS[kaltorak18].grbr");
        let req = upload_as(Some(&bearer(&mine)), JTK18, other_game.clone());
        assert_eq!(
            status(&app, req).await,
            StatusCode::CREATED,
            "kaltorak18 is the same account"
        );

        // The opponent shares the same match later: nothing new is stored, but their own name
        // is filled in.
        let req = upload_as(Some(&bearer(&theirs)), BOLSANO, fixture(NEW));
        assert_eq!(status(&app, req).await, StatusCode::OK);
        assert_eq!(name_of(&store, BOLSANO), "Bolsano");
        assert_eq!(store.stats(&StatsFilter::default()).unwrap().matches, 2);

        // A key can only share games its player was in.
        let stranger = store.create_key("stranger").unwrap();
        let req = upload_as(Some(&bearer(&stranger)), "42", fixture(NEW));
        assert_eq!(status(&app, req).await, StatusCode::FORBIDDEN);

        // Edited replays are refused: a new battle whose round 3 army no longer matches that
        // round's fight report. (An edited copy of a stored battle is just a duplicate.)
        let original = String::from_utf8(fixture(NEW)).unwrap();
        let mine_at = original.find(&format!("<id>{JTK18}</id>")).unwrap();
        let round3 = mine_at + original[mine_at..].find("<round>3</round>").unwrap();
        let x = round3 + original[round3..].find("<x>").unwrap() + 3;
        let x_end = x + original[x..].find('<').unwrap();
        let edited = format!("{}{}{}", &original[..x], "12345", &original[x_end..])
            .replace("20260924--67113734", "20260924--99999999");
        let req = upload_as(Some(&bearer(&mine)), JTK18, edited.into_bytes());
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);

        // Ask which matches are missing.
        let req = Request::post("/api/replays/missing")
            .header(header::AUTHORIZATION, bearer(&mine))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                r#"{"battle_ids": ["20260924--67113734", "20990101--1"]}"#,
            ))
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), 1 << 20)
            .await
            .unwrap();
        let missing: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(missing["missing"], serde_json::json!(["20990101--1"]));

        // Ratings for my own match are accepted; the stranger's aren't.
        let ratings = format!(
            r#"[{{"player_id": "{JTK18}", "elo": 1344, "rank_point": 3285, "level": 352, "elo_games": 1583}},
                {{"player_id": "{BOLSANO}", "elo": 900, "rank_point": null, "level": 40, "elo_games": null}}]"#
        );
        for (key, expected) in [(&mine, StatusCode::OK), (&stranger, StatusCode::FORBIDDEN)] {
            let req = Request::post("/api/replays/20260924--67113734/ratings")
                .header(header::AUTHORIZATION, bearer(key))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(ratings.clone()))
                .unwrap();
            assert_eq!(status(&app, req).await, expected);
        }
        assert_eq!(store.ratings("20260924--67113734").unwrap().len(), 2);

        // Purging an uploader removes everything they shared.
        assert_eq!(store.purge_uploader("jtk18-desktop").unwrap(), 2);
        assert_eq!(store.stats(&StatsFilter::default()).unwrap().matches, 0);
        assert!(store.ratings("20260924--67113734").unwrap().is_empty());
    }

    async fn status(app: &Router, req: Request<Body>) -> StatusCode {
        app.clone().oneshot(req).await.unwrap().status()
    }

    #[tokio::test]
    async fn test_upload_auth() {
        let store = Arc::new(Store::open_in_memory().unwrap());
        let key = store.create_key("desktop").unwrap();
        let app = router(
            store.clone(),
            UploadAuth::Required {
                admin_token: Some("s3cret".into()),
            },
        );

        assert_eq!(status(&app, upload(None)).await, StatusCode::UNAUTHORIZED);
        assert_eq!(
            status(&app, upload(Some("Bearer nope"))).await,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            status(&app, upload(Some("s3cret"))).await,
            StatusCode::UNAUTHORIZED,
            "scheme is required"
        );
        assert_eq!(
            status(&app, upload(Some(&format!("Bearer {key}")))).await,
            StatusCode::CREATED
        );
        assert_eq!(
            status(&app, upload(Some("Bearer s3cret"))).await,
            StatusCode::OK,
            "admin token works; replay already present"
        );
        let uploader: String = store
            .conn()
            .query_row("SELECT uploaded_by FROM matches", [], |r| r.get(0))
            .unwrap();
        assert_eq!(uploader, "desktop");

        store.revoke_key("desktop").unwrap();
        assert_eq!(
            status(&app, upload(Some(&format!("Bearer {key}")))).await,
            StatusCode::UNAUTHORIZED
        );

        // Reads stay open.
        let stats = Request::get("/api/stats").body(Body::empty()).unwrap();
        assert_eq!(status(&app, stats).await, StatusCode::OK);
    }

    /// The landing page and the stats page load, and the filter data doesn't name any player.
    #[tokio::test]
    async fn test_website_and_public_facets() {
        let store = Arc::new(Store::open_in_memory().unwrap());
        store.ingest(&fixture(NEW), Path::new("a"), None).unwrap();
        let app = router(store, UploadAuth::Open);

        let page = |path: &'static str| {
            let app = app.clone();
            async move {
                let page = app
                    .oneshot(Request::get(path).body(Body::empty()).unwrap())
                    .await
                    .unwrap();
                assert_eq!(page.status(), StatusCode::OK, "{path}");
                let html = axum::body::to_bytes(page.into_body(), usize::MAX)
                    .await
                    .unwrap();
                String::from_utf8_lossy(&html).into_owned()
            }
        };
        let landing = page("/").await;
        assert!(landing.contains("How it works"));
        assert!(landing.contains("github.com/jtk18/mechabellum-rs"));
        assert!(
            !landing.contains("/api/"),
            "the landing page calls no API yet"
        );
        let stats = page("/stats").await;
        assert!(
            stats.contains("/api/stats"),
            "the stats page is the one with data"
        );
        assert!(stats.contains("/api/stats/games"));
        let matches = page("/matches").await;
        assert!(matches.contains("/api/cards"));
        let cards = page("/api/cards").await;
        assert!(cards.contains("\"total\":1"), "{cards}");
        assert!(!cards.contains(JTK18) && !cards.contains(BOLSANO));
        let games = page("/api/stats/games?match_mode=VS_1_1").await;
        assert!(games.contains("\"deployments\""), "{games}");

        let facets = app
            .oneshot(Request::get("/api/facets").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(facets.status(), StatusCode::OK);
        let body = axum::body::to_bytes(facets.into_body(), usize::MAX)
            .await
            .unwrap();
        let text = String::from_utf8_lossy(&body);
        assert!(text.contains("2324"));
        for private in ["main_player", "jtk18", "Bolsano", JTK18] {
            assert!(!text.contains(private), "{private} in {text}");
        }
    }

    #[tokio::test]
    async fn test_open_uploads_and_bad_bodies() {
        let app = router(Arc::new(Store::open_in_memory().unwrap()), UploadAuth::Open);
        assert_eq!(status(&app, upload(None)).await, StatusCode::CREATED);

        let junk = Request::post("/api/replays")
            .body(Body::from("hello"))
            .unwrap();
        assert_eq!(status(&app, junk).await, StatusCode::BAD_REQUEST);

        let broken = Request::post("/api/replays")
            .body(Body::from(
                "<?xml version=\"1.0\"?><BattleRecord><oops></BattleRecord>",
            ))
            .unwrap();
        assert_eq!(status(&app, broken).await, StatusCode::UNPROCESSABLE_ENTITY);
    }
}
