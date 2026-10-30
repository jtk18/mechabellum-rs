//! Sign in with Steam to get an upload key for a community server.
//!
//! The app opens the server's `/auth/steam` in the browser with a random `state` and a PKCE
//! challenge. After Steam, the server sends the browser back to this app's own server on
//! 127.0.0.1 with a one-time code. The app checks the state, trades the code and the verifier
//! for a key over HTTPS, saves the key and turns sharing on. The key never passes through the
//! browser, and a code is worthless to anything that doesn't hold the verifier.

use crate::AppState;

use axum::extract::Query;
use axum::response::Html;
use axum::routing::get;
use axum::Router;
use serde::Deserialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_opener::OpenerExt;

use std::time::{Duration, Instant};

/// How long a sign-in started from the app stays open.
const PENDING_TTL: Duration = Duration::from_secs(900);

/// A sign-in the app started and is waiting to finish.
pub struct Pending {
    server: String,
    state: String,
    verifier: String,
    started: Instant,
}

fn random_token() -> Result<String, String> {
    let mut bytes = [0u8; 32];
    getrandom::getrandom(&mut bytes).map_err(|e| format!("no randomness: {e}"))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

/// Open the server's Steam sign-in in the browser.
#[tauri::command]
pub fn steam_sign_in(
    app: AppHandle,
    state: State<'_, AppState>,
    server: String,
) -> Result<(), String> {
    let server = crate::sync::base(&server);
    if !server.starts_with("https://") && !server.starts_with("http://") {
        return Err("The server address needs to start with https://".into());
    }
    let port = state
        .http_url
        .lock()
        .unwrap()
        .as_deref()
        .and_then(|url| url.rsplit(':').next())
        .and_then(|port| port.parse::<u16>().ok())
        .ok_or("This app's local server isn't running, so it can't receive the sign-in.")?;
    let (token, verifier) = (random_token()?, random_token()?);
    let url = format!(
        "{server}/auth/steam?app_port={port}&state={token}&challenge={}",
        mechabellum_server::steam::challenge_for(&verifier)
    );
    *state.sign_in.lock().unwrap() = Some(Pending {
        server,
        state: token,
        verifier,
        started: Instant::now(),
    });
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| format!("Couldn't open the browser: {e}"))
}

/// The route the browser comes back to, on this app's local server.
pub fn routes(app: AppHandle) -> Router {
    Router::new().route(
        "/auth/steam/done",
        get(move |Query(query): Query<Done>| finish(app.clone(), query)),
    )
}

#[derive(Deserialize)]
struct Done {
    code: String,
    state: String,
}

#[derive(Deserialize)]
struct Redeemed {
    key: String,
}

async fn finish(app: AppHandle, done: Done) -> Html<String> {
    let message = match redeem(&app, done).await {
        Ok(server) => {
            let _ = app.emit("steam-signed-in", server.clone());
            format!("You're signed in and sharing with {server}. You can close this tab.")
        }
        // Only a sign-in this app started tells the window; anything can request this page.
        Err(Failed::Ours(e)) => {
            log::warn!("Steam sign-in: {e}");
            let _ = app.emit("steam-sign-in-failed", e.clone());
            format!("Signing in didn't finish: {e}")
        }
        Err(Failed::NotOurs(e)) => format!("Signing in didn't finish: {e}"),
    };
    Html(format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">\
         <meta name=\"referrer\" content=\"no-referrer\"><title>Mechabellum Stats</title>\
         <style>body{{margin:0;background:#0a0d10;color:#eef3f6;font:16px/1.55 system-ui,\
         sans-serif}}main{{max-width:560px;margin:0 auto;padding:40px 16px}}</style></head>\
         <body><main><h1>Mechabellum Stats</h1><p>{}</p></main></body></html>",
        message
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    ))
}

enum Failed {
    /// The sign-in this app started didn't finish.
    Ours(String),
    /// A request that doesn't belong to a sign-in this app is waiting for.
    NotOurs(String),
}

/// Check the state, trade the code for a key, and save it. Returns the server's address.
async fn redeem(app: &AppHandle, done: Done) -> Result<String, Failed> {
    let state = app.state::<AppState>();
    let pending =
        take_matching(&mut state.sign_in.lock().unwrap(), &done.state).map_err(Failed::NotOurs)?;
    let (server, verifier, code) = (pending.server.clone(), pending.verifier, done.code);
    let key = tauri::async_runtime::spawn_blocking(move || trade(&server, &code, &verifier))
        .await
        .map_err(|e| Failed::Ours(e.to_string()))?
        .map_err(Failed::Ours)?;
    let sync = crate::sync::SyncSettings {
        enabled: true,
        server: pending.server.clone(),
        key,
    };
    crate::apply_sync_settings(&state, sync).map_err(Failed::Ours)?;
    Ok(pending.server)
}

/// Take the waiting sign-in if `state` is the one it was started with and it hasn't expired.
/// A request with any other state leaves it waiting, so nothing that can reach this app's local
/// server can cancel a sign-in in progress.
fn take_matching(slot: &mut Option<Pending>, state: &str) -> Result<Pending, String> {
    if slot
        .as_ref()
        .is_some_and(|p| p.started.elapsed() >= PENDING_TTL)
    {
        *slot = None;
    }
    let waiting = "no sign-in is waiting; start again from the app";
    let pending = slot.as_ref().ok_or(waiting)?;
    if !mechabellum_server::steam::same_secret(state, &pending.state) {
        return Err("this sign-in wasn't started by this app".into());
    }
    slot.take().ok_or_else(|| waiting.into())
}

/// Trade a one-time code and its verifier for an upload key.
fn trade(server: &str, code: &str, verifier: &str) -> Result<String, String> {
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(20))
        .build();
    let response = agent
        .post(&format!("{server}/api/auth/redeem"))
        .send_json(serde_json::json!({ "code": code, "verifier": verifier }));
    match response {
        Ok(response) => response
            .into_json::<Redeemed>()
            .map(|r| r.key)
            .map_err(|e| format!("the server's answer couldn't be read: {e}")),
        Err(ureq::Error::Status(_, response)) => {
            let body: serde_json::Value = response.into_json().unwrap_or_default();
            Err(body["error"]
                .as_str()
                .unwrap_or("the server refused the sign-in")
                .to_owned())
        }
        Err(e) => Err(format!("the server couldn't be reached: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mechabellum_server::pseudonym::Pseudonyms;
    use mechabellum_server::steam::{challenge_for, SteamConfig};
    use mechabellum_server::{router, Store, UploadAuth};

    use std::sync::Arc;

    fn pending(server: &str, state: &str, verifier: &str) -> Pending {
        Pending {
            server: server.to_owned(),
            state: state.to_owned(),
            verifier: verifier.to_owned(),
            started: Instant::now(),
        }
    }

    #[test]
    fn test_only_the_waiting_sign_in_finishes() {
        assert!(take_matching(&mut None, "s").is_err());
        // A wrong state is refused and leaves the sign-in waiting for the right one.
        let mut slot = Some(pending("x", "right", "v"));
        assert!(take_matching(&mut slot, "wrong").is_err());
        assert!(slot.is_some());
        assert_eq!(take_matching(&mut slot, "right").unwrap().server, "x");
        assert!(slot.is_none(), "finished once");
        assert!(take_matching(&mut slot, "right").is_err());
        let mut old = pending("x", "right", "v");
        old.started = Instant::now() - PENDING_TTL - Duration::from_secs(1);
        let mut slot = Some(old);
        assert!(take_matching(&mut slot, "right").is_err());
        assert!(slot.is_none(), "an expired sign-in is dropped");
    }

    async fn serve(app: Router) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await });
        base
    }

    /// The desktop half of a sign-in against a real community server over HTTP: start with a
    /// state and challenge, come back from a stand-in Steam that vouches for anyone, trade the
    /// code, and upload with the key.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_trade_a_code_for_a_working_key() {
        let steam = serve(Router::new().route(
            "/openid/login",
            axum::routing::post(|| async {
                "ns:http://specs.openid.net/auth/2.0\nis_valid:true\n"
            }),
        ))
        .await;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let ours = format!("http://{}", listener.local_addr().unwrap());
        let store = Arc::new(Store::open_in_memory().unwrap());
        store.use_pseudonyms(Pseudonyms::from_key([3; 32])).unwrap();
        let mut config = SteamConfig::new(&ours);
        config.openid_endpoint = format!("{steam}/openid/login");
        config.claimed_id_prefix = format!("{steam}/openid/id/");
        let app = router(store.clone(), UploadAuth::Required { admin_token: None })
            .merge(mechabellum_server::steam::routes(store, config).unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await });

        let (state, verifier) = (random_token().unwrap(), random_token().unwrap());
        let result = tokio::task::spawn_blocking(move || {
            let agent = ureq::AgentBuilder::new().redirects(0).build();
            let started = agent
                .get(&format!(
                    "{ours}/auth/steam?app_port=8765&state={state}&challenge={}",
                    challenge_for(&verifier)
                ))
                .call()
                .unwrap();
            assert_eq!(started.status(), 303);
            let cookie = started
                .header("set-cookie")
                .unwrap()
                .split(';')
                .next()
                .unwrap()
                .to_owned();
            let at_steam = started.header("location").unwrap().to_owned();
            let return_to =
                url::form_urlencoded::parse(at_steam.split_once('?').unwrap().1.as_bytes())
                    .find(|(k, _)| k == "openid.return_to")
                    .unwrap()
                    .1
                    .into_owned();
            let claimed = format!("{steam}/openid/id/76561198000000001");
            let back = url::form_urlencoded::Serializer::new(String::new())
                .append_pair("openid.ns", "http://specs.openid.net/auth/2.0")
                .append_pair("openid.mode", "id_res")
                .append_pair("openid.op_endpoint", &format!("{steam}/openid/login"))
                .append_pair("openid.claimed_id", &claimed)
                .append_pair("openid.identity", &claimed)
                .append_pair("openid.return_to", &return_to)
                .append_pair("openid.response_nonce", "2026-10-29T00:00:00Zx")
                .append_pair("openid.assoc_handle", "1")
                .append_pair(
                    "openid.signed",
                    "op_endpoint,claimed_id,identity,return_to,response_nonce,assoc_handle",
                )
                .append_pair("openid.sig", "stand-in")
                .finish();
            let returned = agent
                .get(&format!("{return_to}&{back}"))
                .set("cookie", &cookie)
                .call()
                .unwrap();
            assert_eq!(returned.status(), 303);
            let done = returned.header("location").unwrap().to_owned();
            let query = done
                .strip_prefix("http://127.0.0.1:8765/auth/steam/done?")
                .unwrap();
            let params: std::collections::HashMap<String, String> =
                url::form_urlencoded::parse(query.as_bytes())
                    .into_owned()
                    .collect();

            let mut slot = Some(pending(&ours, &state, &verifier));
            let waiting = take_matching(&mut slot, &params["state"]).unwrap();
            let key = trade(&waiting.server, &params["code"], &waiting.verifier).unwrap();
            // The code was used up.
            let again = trade(&waiting.server, &params["code"], &waiting.verifier);

            let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../../crates/mechabellum/fixtures/battle_record_2324.xml");
            let uploaded = agent
                .post(&format!("{ours}/api/replays"))
                .set("authorization", &format!("Bearer {key}"))
                .set("x-mechabellum-player", "281474976711196086")
                .send_bytes(&std::fs::read(fixture).unwrap())
                .unwrap()
                .status();
            (again, uploaded)
        })
        .await
        .unwrap();
        assert!(result.0.unwrap_err().contains("used"), "a code works once");
        assert_eq!(result.1, 201);
    }
}
