//! Replay ingestion and stats API. Runs standalone (`mechabellum-server serve`) or embedded in the
//! desktop app via [`router`].

mod store;

pub use store::{
    Facets, IngestOutcome, MatchRow, Rating, Rejection, Stats, StatsFilter, Store, TechStat, Trend,
    TrendCell, UnitTechStats, UploadKey, UsageStat, VersionTotals, VersionTrends,
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
        .route("/health", get(|| async { "ok" }))
        .route("/api/replays", get(list_replays).post(upload_replay))
        .route("/api/stats", get(stats))
        .route("/api/stats/versions", get(version_trends))
        .route("/api/stats/techs", get(tech_stats))
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
            if !store.missing(&[battle_id.clone()])?.is_empty()
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

async fn stats(
    State(state): State<AppState>,
    Query(filter): Query<StatsFilter>,
) -> Result<Json<Stats>, ApiError> {
    let store = state.store.clone();
    Ok(Json(blocking(move || store.stats(&filter)).await?))
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
        let versions: Vec<u32> = trends.versions.iter().map(|v| v.version).collect();
        assert_eq!(versions, vec![1425, 2324]);
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
