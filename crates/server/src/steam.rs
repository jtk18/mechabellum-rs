//! Sign in with Steam: how a community member gets an upload key.
//!
//! Steam's OpenID 2.0 login proves which Steam account someone holds. The server checks the
//! assertion with Steam itself (`check_authentication`), optionally checks the account with the
//! Steam Web API (age, Mechabellum playtime), and then issues an upload key for the member's
//! account. The Steam ID is never stored: the account is named by its keyed pseudonym.
//!
//! Two ways to finish:
//! - The desktop app opens `/auth/steam?app_port=..&state=..&challenge=..` in the browser. After
//!   Steam, the browser is sent to the app's own server on 127.0.0.1 with a one-time code and
//!   the app's `state`; the app trades the code for a key at `/api/auth/redeem`, proving with the
//!   verifier behind `challenge` (PKCE, RFC 7636) that it's the app that started the sign-in.
//!   The key never appears in a URL.
//! - Without those, the key is shown once on the page, for the command line.
//!
//! A cookie ties the return from Steam to the browser that left for it, and each sign-in can
//! be finished once.

use crate::{blocking, secrets_equal, ApiError, Store};

use anyhow::Result;
use axum::{
    extract::{Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use base64::Engine;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

/// Mechabellum's Steam app ID.
pub const APP_ID: u32 = 669330;

const OPENID_NS: &str = "http://specs.openid.net/auth/2.0";
const IDENTIFIER_SELECT: &str = "http://specs.openid.net/auth/2.0/identifier_select";
const COOKIE: &str = "mbs_login";
/// How long a sign-in may take at Steam.
const LOGIN_TTL: Duration = Duration::from_secs(600);
/// How long the desktop app has to trade its code for a key.
const CODE_TTL: Duration = Duration::from_secs(300);
/// Sign-ins in progress at once. Starting one costs nothing, so this bounds what a flood holds.
const MAX_PENDING: usize = 2000;
/// Fields Steam signs that the server relies on; an assertion signing fewer is refused.
const SIGNED: [&str; 6] = [
    "op_endpoint",
    "claimed_id",
    "identity",
    "return_to",
    "response_nonce",
    "assoc_handle",
];

#[derive(Clone, Debug)]
pub struct SteamConfig {
    /// The server's public address, such as `https://mechabellum-stats.com`.
    pub public_url: String,
    /// Steam Web API key for the account checks. Without one, signing in to Steam is the only
    /// check.
    pub api_key: Option<String>,
    /// Youngest Steam account that may share, in days (0: no check).
    pub min_account_days: u32,
    /// Least Mechabellum playtime that may share, in minutes (0: no check).
    pub min_playtime_minutes: u32,
    pub openid_endpoint: String,
    pub claimed_id_prefix: String,
    pub api_base: String,
}

impl SteamConfig {
    pub fn new(public_url: &str) -> Self {
        SteamConfig {
            public_url: public_url.trim_end_matches('/').to_owned(),
            api_key: None,
            min_account_days: 0,
            min_playtime_minutes: 0,
            openid_endpoint: "https://steamcommunity.com/openid/login".into(),
            claimed_id_prefix: "https://steamcommunity.com/openid/id/".into(),
            api_base: "https://api.steampowered.com".into(),
        }
    }
}

struct Login {
    started: Instant,
    app: Option<AppReturn>,
}

/// Where the desktop app waits, and what it'll prove.
struct AppReturn {
    port: u16,
    state: String,
    challenge: String,
}

struct Code {
    issued: Instant,
    account: String,
    challenge: String,
}

struct Steam {
    store: Arc<Store>,
    config: SteamConfig,
    logins: Mutex<HashMap<String, Login>>,
    /// One-time codes by their SHA-256.
    codes: Mutex<HashMap<String, Code>>,
    http: ureq::Agent,
    /// Calls to Steam at once, so a flood of returns can't turn into a flood of requests.
    calls: tokio::sync::Semaphore,
}

/// The sign-in routes. Needs pseudonyms on: accounts are named by Steam pseudonyms.
pub fn routes(store: Arc<Store>, config: SteamConfig) -> Result<Router> {
    anyhow::ensure!(
        store.has_pseudonyms(),
        "Steam sign-in needs pseudonyms on (serve --require-auth)"
    );
    if config.api_key.is_none() {
        log::warn!("no Steam Web API key: sign-in checks nothing beyond the Steam login itself");
    }
    let steam = Steam {
        store,
        config,
        logins: Mutex::default(),
        codes: Mutex::default(),
        http: ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(10))
            .redirects(0)
            .build(),
        calls: tokio::sync::Semaphore::new(4),
    };
    Ok(Router::new()
        .route("/auth/steam", get(start))
        .route("/auth/steam/return", get(finish))
        .route("/api/auth/redeem", post(redeem))
        .with_state(Arc::new(steam)))
}

#[derive(Deserialize)]
struct StartQuery {
    app_port: Option<u16>,
    state: Option<String>,
    challenge: Option<String>,
}

async fn start(State(steam): State<Arc<Steam>>, Query(query): Query<StartQuery>) -> Response {
    let app = match (query.app_port, query.state, query.challenge) {
        (None, None, None) => None,
        (Some(port), Some(state), Some(challenge))
            if port >= 1024 && token(&state, 16, 128) && token(&challenge, 43, 43) =>
        {
            Some(AppReturn {
                port,
                state,
                challenge,
            })
        }
        _ => {
            return page(
                StatusCode::BAD_REQUEST,
                "That sign-in link isn't right.",
                None,
            )
        }
    };
    let nonce = random_token();
    {
        let mut logins = steam.logins.lock().unwrap_or_else(PoisonError::into_inner);
        logins.retain(|_, l| l.started.elapsed() < LOGIN_TTL);
        if logins.len() >= MAX_PENDING {
            return page(
                StatusCode::SERVICE_UNAVAILABLE,
                "Too many sign-ins at once. Try again in a minute.",
                None,
            );
        }
        logins.insert(
            nonce.clone(),
            Login {
                started: Instant::now(),
                app,
            },
        );
    }
    let config = &steam.config;
    let query = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("openid.ns", OPENID_NS)
        .append_pair("openid.mode", "checkid_setup")
        .append_pair("openid.return_to", &steam.return_to(&nonce))
        .append_pair("openid.realm", &format!("{}/", config.public_url))
        .append_pair("openid.identity", IDENTIFIER_SELECT)
        .append_pair("openid.claimed_id", IDENTIFIER_SELECT)
        .finish();
    let cookie = format!(
        "{COOKIE}={nonce}; Path=/auth/steam; Max-Age={}; HttpOnly; SameSite=Lax{}",
        LOGIN_TTL.as_secs(),
        steam.secure()
    );
    (
        StatusCode::SEE_OTHER,
        [
            (
                header::LOCATION,
                format!("{}?{query}", config.openid_endpoint),
            ),
            (header::SET_COOKIE, cookie),
            (header::CACHE_CONTROL, "no-store".into()),
        ],
    )
        .into_response()
}

async fn finish(
    State(steam): State<Arc<Steam>>,
    headers: HeaderMap,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let expired = || {
        page(
            StatusCode::BAD_REQUEST,
            "This sign-in has expired or was already used. Start again.",
            None,
        )
    };
    let Some(nonce) = params.get("login") else {
        return expired();
    };
    let login = steam
        .logins
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .remove(nonce);
    let Some(login) = login.filter(|l| l.started.elapsed() < LOGIN_TTL) else {
        return expired();
    };
    if cookie(&headers, COOKIE).as_deref() != Some(nonce.as_str()) {
        return page(
            StatusCode::BAD_REQUEST,
            "Finish signing in in the same browser you started in.",
            None,
        );
    }
    if params.get("openid.mode").map(String::as_str) == Some("cancel") {
        return page(StatusCode::OK, "Signing in was cancelled.", None);
    }
    let steam_id = match steam.verify(&params, nonce).await {
        Ok(id) => id,
        Err(reason) => return page(StatusCode::FORBIDDEN, reason, None),
    };
    if let Err(reason) = steam.check_account(&steam_id).await {
        return page(StatusCode::FORBIDDEN, &reason, None);
    }
    let store = steam.store.clone();
    let account = match blocking(move || store.sign_in(&steam_id)).await {
        Ok(Ok(account)) => account,
        Ok(Err(reason)) => return page(StatusCode::FORBIDDEN, &reason, None),
        Err(e) => return e.into_response(),
    };
    let clear = format!(
        "{COOKIE}=; Path=/auth/steam; Max-Age=0; HttpOnly; SameSite=Lax{}",
        steam.secure()
    );
    match login.app {
        Some(app) => {
            let code = random_token();
            let mut codes = steam.codes.lock().unwrap_or_else(PoisonError::into_inner);
            codes.retain(|_, c| c.issued.elapsed() < CODE_TTL);
            codes.insert(
                sha256_hex(&code),
                Code {
                    issued: Instant::now(),
                    account,
                    challenge: app.challenge,
                },
            );
            let to = format!(
                "http://127.0.0.1:{}/auth/steam/done?code={code}&state={}",
                app.port, app.state
            );
            (
                StatusCode::SEE_OTHER,
                [
                    (header::LOCATION, to),
                    (header::SET_COOKIE, clear),
                    (header::CACHE_CONTROL, "no-store".into()),
                ],
            )
                .into_response()
        }
        None => {
            let store = steam.store.clone();
            match blocking(move || store.create_account_key(&account)).await {
                Ok(key) => {
                    let mut response = page(StatusCode::OK, "You're signed in.", Some(&key));
                    if let Ok(clear) = clear.parse() {
                        response.headers_mut().insert(header::SET_COOKIE, clear);
                    }
                    response
                }
                Err(e) => e.into_response(),
            }
        }
    }
}

#[derive(Deserialize)]
struct RedeemRequest {
    code: String,
    verifier: String,
}

async fn redeem(
    State(steam): State<Arc<Steam>>,
    Json(request): Json<RedeemRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let refused = || {
        ApiError(
            StatusCode::BAD_REQUEST,
            "this sign-in code is used, expired or not yours; sign in again".into(),
        )
    };
    // Removed whatever happens: a code is tried once.
    let code = steam
        .codes
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .remove(&sha256_hex(&request.code));
    let code = code
        .filter(|c| c.issued.elapsed() < CODE_TTL)
        .ok_or_else(refused)?;
    if !secrets_equal(&challenge_for(&request.verifier), &code.challenge) {
        return Err(refused());
    }
    let account = code.account;
    let store = steam.store.clone();
    let key = {
        let account = account.clone();
        blocking(move || store.create_account_key(&account)).await?
    };
    Ok(Json(serde_json::json!({ "key": key, "account": account })))
}

impl Steam {
    fn return_to(&self, nonce: &str) -> String {
        format!("{}/auth/steam/return?login={nonce}", self.config.public_url)
    }

    fn secure(&self) -> &'static str {
        if self.config.public_url.starts_with("https://") {
            "; Secure"
        } else {
            ""
        }
    }

    /// Check a positive assertion from Steam and return the Steam ID it vouches for.
    async fn verify(
        &self,
        params: &HashMap<String, String>,
        nonce: &str,
    ) -> Result<String, &'static str> {
        let refused = "Steam didn't vouch for this sign-in.";
        let get = |k: &str| params.get(k).map(String::as_str);
        if get("openid.ns") != Some(OPENID_NS)
            || get("openid.mode") != Some("id_res")
            || get("openid.op_endpoint") != Some(self.config.openid_endpoint.as_str())
            || get("openid.return_to") != Some(self.return_to(nonce).as_str())
            || get("openid.claimed_id") != get("openid.identity")
            || get("openid.sig").is_none()
        {
            return Err(refused);
        }
        let signed: Vec<&str> = get("openid.signed").unwrap_or("").split(',').collect();
        if !SIGNED.iter().all(|field| signed.contains(field)) {
            return Err(refused);
        }
        let steam_id = get("openid.claimed_id")
            .and_then(|id| id.strip_prefix(self.config.claimed_id_prefix.as_str()))
            .filter(|id| id.len() == 17 && id.bytes().all(|b| b.is_ascii_digit()))
            .ok_or(refused)?
            .to_owned();

        // Steam checks its own signature: every field as received, with the mode changed.
        let form: Vec<(String, String)> = params
            .iter()
            .filter(|(k, _)| k.starts_with("openid.") && *k != "openid.mode")
            .map(|(k, v)| (k.clone(), v.clone()))
            .chain([("openid.mode".into(), "check_authentication".into())])
            .collect();
        let _turn = self.calls.acquire().await.map_err(|_| refused)?;
        let (http, endpoint) = (self.http.clone(), self.config.openid_endpoint.clone());
        let answer = tokio::task::spawn_blocking(move || {
            let pairs: Vec<(&str, &str)> =
                form.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
            let response = http.post(&endpoint).send_form(&pairs).ok()?;
            response.into_string().ok()
        })
        .await;
        match answer {
            Ok(Some(body)) if body.lines().any(|line| line.trim() == "is_valid:true") => {
                Ok(steam_id)
            }
            Ok(Some(_)) => Err(refused),
            _ => Err("Steam couldn't be reached. Try again in a minute."),
        }
    }

    /// The account checks, when there's a Web API key: old enough, and has played Mechabellum.
    async fn check_account(&self, steam_id: &str) -> Result<(), String> {
        let Some(key) = self.config.api_key.clone() else {
            return Ok(());
        };
        let unreachable = || "Steam couldn't be reached. Try again in a minute.".to_owned();
        let _turn = self.calls.acquire().await.map_err(|_| unreachable())?;
        let (http, base, id) = (
            self.http.clone(),
            self.config.api_base.clone(),
            steam_id.to_owned(),
        );
        let (days, minutes) = (
            self.config.min_account_days,
            self.config.min_playtime_minutes,
        );
        tokio::task::spawn_blocking(move || -> Result<(), String> {
            // The key travels in the query string, so these URLs are never logged.
            let fetch = |url: String| -> Result<serde_json::Value, String> {
                let response = http.get(&url).call().map_err(|_| unreachable())?;
                response.into_json().map_err(|_| unreachable())
            };
            if days > 0 {
                let summary = fetch(format!(
                    "{base}/ISteamUser/GetPlayerSummaries/v2/?key={key}&steamids={id}"
                ))?;
                let created = summary["response"]["players"][0]["timecreated"].as_i64();
                let Some(created) = created else {
                    return Err(
                        "Your Steam profile is private, so its age can't be checked. \
                                Make it public, sign in, and switch it back afterwards."
                            .into(),
                    );
                };
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_secs() as i64);
                if now - created < i64::from(days) * 86_400 {
                    return Err(format!(
                        "Steam accounts can share games once they're {days} days old."
                    ));
                }
            }
            if minutes > 0 {
                let games = fetch(format!(
                    "{base}/IPlayerService/GetOwnedGames/v1/?key={key}&steamid={id}\
                     &include_played_free_games=1&appids_filter%5B0%5D={APP_ID}"
                ))?;
                let played = games["response"]["games"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|g| g["appid"].as_u64() == Some(u64::from(APP_ID)))
                    .and_then(|g| g["playtime_forever"].as_u64());
                let Some(played) = played else {
                    return Err(
                        "Steam doesn't show Mechabellum on this account. If your game \
                                details are private, make them public, sign in, and switch \
                                them back afterwards."
                            .into(),
                    );
                };
                if played < u64::from(minutes) {
                    return Err(format!(
                        "Sharing opens after {} hours of Mechabellum.",
                        minutes.div_ceil(60)
                    ));
                }
            }
            Ok(())
        })
        .await
        .map_err(|_| unreachable())?
    }
}

/// Compare two secrets in constant time, for the desktop app's check of its sign-in state.
pub fn same_secret(a: &str, b: &str) -> bool {
    secrets_equal(a, b)
}

/// The PKCE S256 challenge for a verifier: unpadded base64url of its SHA-256.
pub fn challenge_for(verifier: &str) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

fn token(s: &str, min: usize, max: usize) -> bool {
    (min..=max).contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn random_token() -> String {
    let mut bytes = [0u8; 32];
    getrandom::getrandom(&mut bytes).expect("the system's random number generator");
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn sha256_hex(s: &str) -> String {
    Sha256::digest(s.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(k, _)| *k == name)
        .map(|(_, v)| v.to_owned())
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// A small page in the website's colors, with a message and, after a sign-in from a browser,
/// the new key.
fn page(status: StatusCode, message: &str, key: Option<&str>) -> Response {
    let key = key.map_or_else(String::new, |key| {
        format!(
            r#"<p>Your upload key, shown this once:</p>
<p><code id="key">{}</code> <button type="button" onclick="navigator.clipboard.writeText(document.getElementById('key').textContent)">Copy</button></p>
<p>Paste it into the desktop app's Sharing settings, or send it with uploads as
<code>Authorization: Bearer &lt;key&gt;</code>. Signing in again makes a new key; each account
keeps a few, for a few devices.</p>"#,
            escape(key)
        )
    });
    let body = format!(
        r#"<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="referrer" content="no-referrer">
<title>Sign in · Mechabellum Community Stats</title>
<style>
:root {{ color-scheme: dark; }}
body {{ margin: 0; background: #0a0d10; color: #eef3f6; font: 16px/1.55 Bahnschrift, "DIN Alternate", "Segoe UI", Roboto, system-ui, sans-serif; }}
main {{ max-width: 640px; margin: 0 auto; padding: 40px 16px; }}
h1 {{ font-size: 24px; border-bottom: 2px solid #d95926; padding-bottom: 10px; }}
code {{ background: #12171b; border: 1px solid rgb(160 190 210 / 0.14); border-radius: 6px; padding: 2px 6px; word-break: break-all; }}
button {{ font: inherit; color: inherit; background: #12171b; border: 1px solid rgb(160 190 210 / 0.3); border-radius: 6px; padding: 2px 10px; cursor: pointer; }}
a {{ color: #6cc6e0; }}
</style></head>
<body><main><h1>Mechabellum Community Stats</h1><p>{}</p>{key}<p><a href="/">Back to the site</a></p></main></body></html>"#,
        escape(message)
    );
    (
        status,
        [
            (header::CACHE_CONTROL, "no-store"),
            (header::REFERRER_POLICY, "no-referrer"),
        ],
        Html(body),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{pseudonym::Pseudonyms, router, UploadAuth};
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    use std::collections::HashSet;

    const PLAYER: &str = "76561198000000001";
    const OURS: &str = "http://stats.test";
    const SECRET: &str = "stand-in signing secret";

    /// What the stand-in Steam answers about the signed-in account.
    #[derive(Clone)]
    struct Profile {
        created: Option<i64>,
        minutes: Option<u64>,
    }

    struct FakeSteam {
        base: String,
        seen: Arc<Mutex<HashSet<String>>>,
    }

    fn signature(fields: &[(String, String)], signed: &str) -> String {
        let mut text = String::from(SECRET);
        for name in signed.split(',') {
            let key = format!("openid.{name}");
            let value = fields
                .iter()
                .find(|(k, _)| *k == key)
                .map_or("", |(_, v)| v.as_str());
            text.push_str(&format!("{name}:{value}\n"));
        }
        sha256_hex(&text)
    }

    /// A stand-in for Steam: `check_authentication` recomputes the signature over the signed
    /// fields and refuses a response nonce it has seen before, as an OpenID provider must; the
    /// Web API answers from `profile`.
    async fn fake_steam(profile: Profile) -> FakeSteam {
        let seen: Arc<Mutex<HashSet<String>>> = Arc::default();
        let check = {
            let seen = seen.clone();
            move |body: String| async move {
                let fields: Vec<(String, String)> = url::form_urlencoded::parse(body.as_bytes())
                    .map(|(k, v)| (k.into_owned(), v.into_owned()))
                    .collect();
                let get = |k: &str| {
                    fields
                        .iter()
                        .find(|(name, _)| name == k)
                        .map(|(_, v)| v.clone())
                };
                let signed = get("openid.signed").unwrap_or_default();
                let fresh = seen
                    .lock()
                    .unwrap()
                    .insert(get("openid.response_nonce").unwrap_or_default());
                let valid = get("openid.mode").as_deref() == Some("check_authentication")
                    && get("openid.sig") == Some(signature(&fields, &signed))
                    && fresh;
                format!("ns:{OPENID_NS}\nis_valid:{valid}\n")
            }
        };
        let summaries = {
            let profile = profile.clone();
            move || async move {
                let mut player = serde_json::json!({ "steamid": PLAYER });
                if let Some(created) = profile.created {
                    player["timecreated"] = created.into();
                }
                Json(serde_json::json!({ "response": { "players": [player] } }))
            }
        };
        let games = move || async move {
            let games: Vec<_> = profile
                .minutes
                .map(|m| serde_json::json!({ "appid": APP_ID, "playtime_forever": m }))
                .into_iter()
                .collect();
            Json(serde_json::json!({ "response": { "games": games } }))
        };
        let app = Router::new()
            .route("/openid/login", post(check))
            .route("/ISteamUser/GetPlayerSummaries/v2/", get(summaries))
            .route("/IPlayerService/GetOwnedGames/v1/", get(games));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await });
        FakeSteam { base, seen }
    }

    impl FakeSteam {
        fn config(&self) -> SteamConfig {
            let mut config = SteamConfig::new(OURS);
            config.openid_endpoint = format!("{}/openid/login", self.base);
            config.claimed_id_prefix = format!("{}/openid/id/", self.base);
            config.api_base = self.base.clone();
            config
        }

        /// The fields Steam sends back with a positive assertion for `steam_id`.
        fn assertion(&self, return_to: &str, steam_id: &str) -> Vec<(String, String)> {
            let claimed = format!("{}/openid/id/{steam_id}", self.base);
            let signed = SIGNED.join(",");
            let mut fields: Vec<(String, String)> = [
                ("openid.ns", OPENID_NS.to_owned()),
                ("openid.mode", "id_res".into()),
                ("openid.op_endpoint", format!("{}/openid/login", self.base)),
                ("openid.claimed_id", claimed.clone()),
                ("openid.identity", claimed),
                ("openid.return_to", return_to.to_owned()),
                (
                    "openid.response_nonce",
                    format!("2026-10-29T00:00:00Z{}", random_token()),
                ),
                ("openid.assoc_handle", "1234567890".into()),
                ("openid.signed", signed.clone()),
            ]
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v))
            .collect();
            let sig = signature(&fields, &signed);
            fields.push(("openid.sig".into(), sig));
            fields
        }
    }

    fn server(fake: &FakeSteam, config: SteamConfig) -> (Arc<Store>, Router) {
        let store = Arc::new(Store::open_in_memory().unwrap());
        store.use_pseudonyms(Pseudonyms::from_key([5; 32])).unwrap();
        let _ = fake;
        let app = router(store.clone(), UploadAuth::Required { admin_token: None })
            .merge(routes(store.clone(), config).unwrap());
        (store, app)
    }

    async fn send(app: &Router, req: Request<Body>) -> Response {
        app.clone().oneshot(req).await.unwrap()
    }

    fn header_of(resp: &Response, name: header::HeaderName) -> String {
        resp.headers()
            .get(name)
            .map(|v| v.to_str().unwrap().to_owned())
            .unwrap_or_default()
    }

    async fn text(resp: Response) -> String {
        let body = axum::body::to_bytes(resp.into_body(), 1 << 20)
            .await
            .unwrap();
        String::from_utf8_lossy(&body).into_owned()
    }

    /// Start a sign-in; returns the login cookie and the return address Steam was given.
    async fn start_login(app: &Router, query: &str) -> (String, String) {
        let resp = send(
            app,
            Request::get(format!("/auth/steam{query}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::SEE_OTHER);
        let cookie = header_of(&resp, header::SET_COOKIE);
        let cookie = cookie.split(';').next().unwrap().to_owned();
        let location = header_of(&resp, header::LOCATION);
        let query = location.split_once('?').unwrap().1;
        let return_to = url::form_urlencoded::parse(query.as_bytes())
            .find(|(k, _)| k == "openid.return_to")
            .unwrap()
            .1
            .into_owned();
        (cookie, return_to)
    }

    /// Come back from Steam with `fields`, as the browser would.
    async fn come_back(
        app: &Router,
        return_to: &str,
        fields: &[(String, String)],
        cookie: Option<&str>,
    ) -> Response {
        let extra = url::form_urlencoded::Serializer::new(String::new())
            .extend_pairs(fields)
            .finish();
        let path = return_to.strip_prefix(OURS).unwrap();
        let mut req = Request::get(format!("{path}&{extra}"));
        if let Some(cookie) = cookie {
            req = req.header(header::COOKIE, cookie);
        }
        send(app, req.body(Body::empty()).unwrap()).await
    }

    fn app_query(state: &str, verifier: &str) -> String {
        format!(
            "?app_port=8765&state={state}&challenge={}",
            challenge_for(verifier)
        )
    }

    async fn redeem_code(app: &Router, code: &str, verifier: &str) -> Response {
        let body = serde_json::json!({ "code": code, "verifier": verifier }).to_string();
        send(
            app,
            Request::post("/api/auth/redeem")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
    }

    /// Sign in as the desktop app would, and trade the code for a key.
    async fn desktop_sign_in(fake: &FakeSteam, app: &Router) -> (String, String) {
        let (state, verifier) = (random_token(), random_token());
        let (cookie, return_to) = start_login(app, &app_query(&state, &verifier)).await;
        let resp = come_back(
            app,
            &return_to,
            &fake.assertion(&return_to, PLAYER),
            Some(&cookie),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::SEE_OTHER, "{}", text(resp).await);
        let to = header_of(&resp, header::LOCATION);
        let query = to
            .strip_prefix("http://127.0.0.1:8765/auth/steam/done?")
            .unwrap();
        let params: HashMap<String, String> = url::form_urlencoded::parse(query.as_bytes())
            .into_owned()
            .collect();
        assert_eq!(params["state"], state);
        let resp = redeem_code(app, &params["code"], &verifier).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let answer: serde_json::Value = serde_json::from_str(&text(resp).await).unwrap();
        let key = answer["key"].as_str().unwrap().to_owned();
        // A code works once.
        let again = redeem_code(app, &params["code"], &verifier).await;
        assert_eq!(again.status(), StatusCode::BAD_REQUEST);
        (key, answer["account"].as_str().unwrap().to_owned())
    }

    fn upload(key: &str) -> Request<Body> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../mechabellum/fixtures");
        Request::post("/api/replays")
            .header(header::AUTHORIZATION, format!("Bearer {key}"))
            .header(crate::PLAYER_HEADER, "281474976711196086")
            .body(Body::from(
                std::fs::read(dir.join("battle_record_2324.xml")).unwrap(),
            ))
            .unwrap()
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_desktop_sign_in() {
        let fake = fake_steam(Profile {
            created: None,
            minutes: None,
        })
        .await;
        let (store, app) = server(&fake, fake.config());
        let (key, account) = desktop_sign_in(&fake, &app).await;
        assert!(account.starts_with("steam-"), "{account}");

        assert_eq!(send(&app, upload(&key)).await.status(), StatusCode::CREATED);
        let uploader: String = store
            .conn()
            .query_row("SELECT uploaded_by FROM matches", [], |r| r.get(0))
            .unwrap();
        assert_eq!(uploader, account, "uploads are recorded under the account");
        assert!(store.key_player(&account).unwrap().is_some());

        // No Steam ID is stored anywhere.
        for table in ["accounts", "upload_keys", "matches", "meta"] {
            let rows: Vec<String> = store
                .conn()
                .prepare(&format!("SELECT * FROM {table}"))
                .unwrap()
                .query_map([], |r| {
                    let n = r.as_ref().column_count();
                    (0..n)
                        .map(|i| {
                            r.get::<_, rusqlite::types::Value>(i)
                                .map(|v| format!("{v:?}"))
                        })
                        .collect::<rusqlite::Result<String>>()
                })
                .unwrap()
                .map(Result::unwrap)
                .collect();
            assert!(!rows.concat().contains(PLAYER), "{table}");
        }

        // Each sign-in is another device's key; past five, the oldest stops working.
        let mut keys = vec![key];
        for _ in 0..5 {
            keys.push(desktop_sign_in(&fake, &app).await.0);
        }
        assert_eq!(store.verify_key(&keys[0]).unwrap(), None);
        for key in &keys[1..] {
            assert_eq!(
                store.verify_key(key).unwrap().as_deref(),
                Some(account.as_str())
            );
        }
        assert_eq!(store.list_accounts().unwrap()[0].active_keys, 5);

        // A disabled account can't sign in, and its keys stop at once.
        assert!(store.set_account_disabled(&account, true).unwrap());
        assert_eq!(store.verify_key(&keys[5]).unwrap(), None);
        let (cookie, return_to) = start_login(&app, "").await;
        let resp = come_back(
            &app,
            &return_to,
            &fake.assertion(&return_to, PLAYER),
            Some(&cookie),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        assert!(store.set_account_disabled(&account, false).unwrap());
        assert!(store.verify_key(&keys[5]).unwrap().is_some());
    }

    /// From a browser, the key is shown once on the page.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_browser_sign_in_shows_the_key() {
        let fake = fake_steam(Profile {
            created: None,
            minutes: None,
        })
        .await;
        let (_, app) = server(&fake, fake.config());
        let (cookie, return_to) = start_login(&app, "").await;
        let resp = come_back(
            &app,
            &return_to,
            &fake.assertion(&return_to, PLAYER),
            Some(&cookie),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(header_of(&resp, header::CACHE_CONTROL), "no-store");
        let page = text(resp).await;
        let at = page.find("mbk_").unwrap();
        let key: String = page[at..]
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        assert_eq!(send(&app, upload(&key)).await.status(), StatusCode::CREATED);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_forged_and_replayed_sign_ins_are_refused() {
        let fake = fake_steam(Profile {
            created: None,
            minutes: None,
        })
        .await;
        let (store, app) = server(&fake, fake.config());
        let set = |fields: &mut Vec<(String, String)>, key: &str, value: String| {
            fields.iter_mut().find(|(k, _)| k == key).unwrap().1 = value;
        };

        // Coming back without the cookie, or with another browser's.
        let (cookie, return_to) = start_login(&app, "").await;
        let fields = fake.assertion(&return_to, PLAYER);
        let resp = come_back(&app, &return_to, &fields, None).await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let (_, other) = start_login(&app, "").await;
        let resp = come_back(&app, &other, &fake.assertion(&other, PLAYER), Some(&cookie)).await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        // Each sign-in is finished once: the first try above used this one up.
        let resp = come_back(&app, &return_to, &fields, Some(&cookie)).await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        type Forge = Box<dyn Fn(&FakeSteam, &mut Vec<(String, String)>)>;
        let forgeries: Vec<(&str, Forge)> = vec![
            (
                "another Steam ID, signature unchanged",
                Box::new(move |fake, f| {
                    let other = format!("{}/openid/id/76561198000000002", fake.base);
                    set(f, "openid.claimed_id", other.clone());
                    set(f, "openid.identity", other);
                }),
            ),
            (
                "claimed_id and identity disagree",
                Box::new(move |fake, f| {
                    set(
                        f,
                        "openid.identity",
                        format!("{}/openid/id/76561198000000002", fake.base),
                    );
                }),
            ),
            (
                "another provider",
                Box::new(move |_, f| {
                    set(
                        f,
                        "openid.op_endpoint",
                        "https://evil.test/openid/login".into(),
                    );
                }),
            ),
            (
                "the claimed ID left unsigned",
                Box::new(move |_, f| {
                    set(
                        f,
                        "openid.signed",
                        "op_endpoint,identity,return_to,response_nonce,assoc_handle".into(),
                    );
                }),
            ),
            (
                "an ID that isn't Steam's",
                Box::new(move |_, f| {
                    let other = "https://evil.test/openid/id/76561198000000001".to_owned();
                    set(f, "openid.claimed_id", other.clone());
                    set(f, "openid.identity", other);
                }),
            ),
            (
                "a forged signature",
                Box::new(move |_, f| set(f, "openid.sig", "00".repeat(32))),
            ),
            (
                "not a positive assertion",
                Box::new(move |_, f| set(f, "openid.mode", "setup_needed".into())),
            ),
        ];
        for (what, forge) in forgeries {
            let (cookie, return_to) = start_login(&app, "").await;
            let mut fields = fake.assertion(&return_to, PLAYER);
            forge(&fake, &mut fields);
            let resp = come_back(&app, &return_to, &fields, Some(&cookie)).await;
            assert_eq!(resp.status(), StatusCode::FORBIDDEN, "{what}");
        }

        // An assertion made for one sign-in can't finish another.
        let (_, first) = start_login(&app, "").await;
        let stolen = fake.assertion(&first, PLAYER);
        let (cookie, second) = start_login(&app, "").await;
        let resp = come_back(&app, &second, &stolen, Some(&cookie)).await;
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);

        // Steam refuses an assertion it has already confirmed once.
        let (cookie, return_to) = start_login(&app, "").await;
        let fields = fake.assertion(&return_to, PLAYER);
        let nonce = fields
            .iter()
            .find(|(k, _)| k == "openid.response_nonce")
            .unwrap()
            .1
            .clone();
        fake.seen.lock().unwrap().insert(nonce);
        let resp = come_back(&app, &return_to, &fields, Some(&cookie)).await;
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);

        // A code needs the verifier behind its challenge, and a wrong one burns it.
        let (state, verifier) = (random_token(), random_token());
        let (cookie, return_to) = start_login(&app, &app_query(&state, &verifier)).await;
        let resp = come_back(
            &app,
            &return_to,
            &fake.assertion(&return_to, PLAYER),
            Some(&cookie),
        )
        .await;
        let to = header_of(&resp, header::LOCATION);
        let code = to.split("code=").nth(1).unwrap().split('&').next().unwrap();
        let wrong = redeem_code(&app, code, &random_token()).await;
        assert_eq!(wrong.status(), StatusCode::BAD_REQUEST);
        let late = redeem_code(&app, code, &verifier).await;
        assert_eq!(late.status(), StatusCode::BAD_REQUEST);

        // Malformed desktop parameters, and an address that isn't a loopback port.
        for query in [
            "?app_port=80&state=aaaaaaaaaaaaaaaa&challenge=x".to_owned(),
            app_query("short", &verifier),
            format!("?app_port=8765&state={state}"),
            "?app_port=8765&state=aaaaaaaaaaaaaaaa%2F%2Fevil.test&challenge=\
             aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                .to_owned(),
        ] {
            let resp = send(
                &app,
                Request::get(format!("/auth/steam{query}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await;
            assert_eq!(resp.status(), StatusCode::BAD_REQUEST, "{query}");
        }
        // The PKCE case signed in to Steam for real, so an account exists; no key was issued.
        assert!(store.list_keys().unwrap().is_empty(), "nothing got in");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_account_checks() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let old = Some(now - 400 * 86_400);
        let cases = [
            (
                Profile {
                    created: old,
                    minutes: Some(600),
                },
                true,
                "",
            ),
            (
                Profile {
                    created: Some(now - 86_400),
                    minutes: Some(600),
                },
                false,
                "days old",
            ),
            (
                Profile {
                    created: None,
                    minutes: Some(600),
                },
                false,
                "private",
            ),
            (
                Profile {
                    created: old,
                    minutes: None,
                },
                false,
                "doesn't show Mechabellum",
            ),
            (
                Profile {
                    created: old,
                    minutes: Some(30),
                },
                false,
                "2 hours",
            ),
        ];
        for (profile, allowed, says) in cases {
            let fake = fake_steam(profile).await;
            let mut config = fake.config();
            config.api_key = Some("test-key".into());
            config.min_account_days = 30;
            config.min_playtime_minutes = 120;
            let (store, app) = server(&fake, config);
            let (cookie, return_to) = start_login(&app, "").await;
            let resp = come_back(
                &app,
                &return_to,
                &fake.assertion(&return_to, PLAYER),
                Some(&cookie),
            )
            .await;
            let status = resp.status();
            let page = text(resp).await;
            if allowed {
                assert_eq!(status, StatusCode::OK, "{page}");
            } else {
                assert_eq!(status, StatusCode::FORBIDDEN, "{page}");
                assert!(page.contains(says), "{says}: {page}");
                assert!(store.list_accounts().unwrap().is_empty());
            }
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_steam_down() {
        let fake = fake_steam(Profile {
            created: None,
            minutes: None,
        })
        .await;
        let mut config = fake.config();
        // Nothing listens on port 9 of the loopback address.
        config.openid_endpoint = "http://127.0.0.1:9/openid/login".into();
        config.claimed_id_prefix = format!("{}/openid/id/", fake.base);
        let (_, app) = server(&fake, config);
        let (cookie, return_to) = start_login(&app, "").await;
        let mut fields = fake.assertion(&return_to, PLAYER);
        fields
            .iter_mut()
            .find(|(k, _)| k == "openid.op_endpoint")
            .unwrap()
            .1 = "http://127.0.0.1:9/openid/login".into();
        let resp = come_back(&app, &return_to, &fields, Some(&cookie)).await;
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        assert!(text(resp).await.contains("couldn't be reached"));
    }
}
