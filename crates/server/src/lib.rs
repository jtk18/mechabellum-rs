//! Replay ingestion and stats API. Runs standalone (`mechabellum-server serve`) or embedded in the
//! desktop app via [`router`].

mod store;

pub use store::{IngestOutcome, MatchRow, Stats, StatsFilter, Store, UsageStat};

use axum::{
    body::Bytes,
    extract::{DefaultBodyLimit, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde::Deserialize;

use std::path::Path;
use std::sync::Arc;

/// Largest accepted upload. Replays are usually 0.3-2 MB.
const MAX_REPLAY_BYTES: usize = 32 * 1024 * 1024;

#[derive(Clone)]
struct AppState {
    store: Arc<Store>,
    upload_token: Option<Arc<str>>,
}

/// Build the API. When `upload_token` is set, `POST /api/replays` requires
/// `Authorization: Bearer <token>`. Reads stay open.
pub fn router(store: Arc<Store>, upload_token: Option<String>) -> Router {
    let state = AppState {
        store,
        upload_token: upload_token.filter(|t| !t.is_empty()).map(Into::into),
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

async fn upload_replay(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<(StatusCode, Json<IngestOutcome>), ApiError> {
    if let Some(token) = &state.upload_token {
        let presented = headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "));
        if presented != Some(token.as_ref()) {
            return Err(ApiError(
                StatusCode::UNAUTHORIZED,
                "missing or invalid upload token".into(),
            ));
        }
    }
    if mechabellum::BattleRecord::extract_document(&body).is_none() {
        return Err(ApiError(
            StatusCode::BAD_REQUEST,
            "body is not a Mechabellum replay (.grbr or .xml)".into(),
        ));
    }
    let store = state.store.clone();
    let outcome = blocking(move || store.ingest(&body, Path::new("<upload>"))).await?;
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

    fn fixture(name: &str) -> Vec<u8> {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../mechabellum/fixtures");
        std::fs::read(dir.join(name)).unwrap()
    }

    #[test]
    fn test_ingest_dedupes_and_counts() {
        let store = Store::open_in_memory().unwrap();
        let replay = fixture("battle_record_2324.xml");
        let first = store.ingest(&replay, Path::new("a")).unwrap();
        assert!(matches!(first, IngestOutcome::Added { .. }));
        let second = store.ingest(&replay, Path::new("b")).unwrap();
        assert!(matches!(second, IngestOutcome::Duplicate { .. }));
        store
            .ingest(
                &fixture("1425_20241123--67298547_[小疯子]VS[kaltorak18].grbr"),
                Path::new("c"),
            )
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
    }

    #[tokio::test]
    async fn test_upload_requires_token() {
        let app = router(
            Arc::new(Store::open_in_memory().unwrap()),
            Some("s3cret".into()),
        );
        let upload = |auth: Option<&str>| {
            let mut req = Request::post("/api/replays");
            if let Some(auth) = auth {
                req = req.header(header::AUTHORIZATION, auth);
            }
            req.body(Body::from(fixture("battle_record_2324.xml")))
                .unwrap()
        };

        let resp = app.clone().oneshot(upload(None)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        let resp = app
            .clone()
            .oneshot(upload(Some("Bearer s3cret")))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
        let resp = app
            .oneshot(Request::get("/api/stats").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }
}
