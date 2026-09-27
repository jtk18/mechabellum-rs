//! Replay ingestion and stats API. Runs standalone (`mechabellum-server serve`) or embedded in the
//! desktop app via [`router`].

mod store;

pub use store::{IngestOutcome, MatchRow, Stats, StatsFilter, Store, UploadKey, UsageStat};

use axum::{
    body::Bytes,
    extract::{DefaultBodyLimit, Query, State},
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
        return Ok(Some("admin".into()));
    }
    let store = state.store.clone();
    blocking(move || store.verify_key(&presented))
        .await?
        .map(Some)
        .ok_or_else(unauthorized)
}

async fn upload_replay(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<(StatusCode, Json<IngestOutcome>), ApiError> {
    let uploader = authenticate(&state, &headers).await?;
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
    Ok((status, Json(outcome)))
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

    fn upload(auth: Option<&str>) -> Request<Body> {
        let mut req = Request::post("/api/replays");
        if let Some(auth) = auth {
            req = req.header(header::AUTHORIZATION, auth);
        }
        req.body(Body::from(fixture(NEW))).unwrap()
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
