//! Sharing your matches with a community server. Only replays go up (the server computes stats
//! from them), with every other player's name removed first. See docs/upload-trust.md.

use flate2::{write::GzEncoder, Compression};
use mechabellum_server::Store;
use serde::{Deserialize, Serialize};

use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SyncSettings {
    pub enabled: bool,
    /// e.g. https://stats.example.com
    pub server: String,
    pub key: String,
}

impl SyncSettings {
    pub fn ready(&self) -> bool {
        self.enabled && !self.server.trim().is_empty() && !self.key.trim().is_empty()
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct SyncStatus {
    pub running: bool,
    /// Matches this server already has, including ones sent now.
    pub shared: usize,
    /// Your matches stored locally. Replays of other people's games (tournaments you watched)
    /// aren't yours to share and aren't counted.
    pub total: usize,
    /// Matches the server won't take (offline games, for example), with why.
    pub rejected: usize,
    pub last_error: Option<String>,
}

/// How many uploads run at once.
const PARALLEL: usize = 4;
/// Largest list the server takes when asking which matches it's missing.
const ASK_CHUNK: usize = 1000;
/// How often to check for new matches when nothing announces them.
const INTERVAL: Duration = Duration::from_secs(300);

pub struct Syncer {
    pub settings: Arc<Mutex<SyncSettings>>,
    pub status: Arc<Mutex<SyncStatus>>,
}

/// A server address in the form sync records use: trimmed, no trailing slash.
pub fn base(server: &str) -> String {
    server.trim().trim_end_matches('/').to_owned()
}

fn gzip(bytes: &[u8]) -> std::io::Result<Vec<u8>> {
    let mut gz = GzEncoder::new(Vec::new(), Compression::default());
    gz.write_all(bytes)?;
    gz.finish()
}

/// Outcome of offering one match.
enum Sent {
    Stored,
    Rejected(String),
}

fn send_one(
    agent: &ureq::Agent,
    settings: &SyncSettings,
    store: &Store,
    me: &str,
    battle_id: &str,
) -> anyhow::Result<Sent> {
    let Some(document) = store.replay_document(battle_id)? else {
        return Ok(Sent::Rejected("no stored replay".into()));
    };
    let shared = mechabellum::hide_other_names(&document, me);
    let body = gzip(shared.as_bytes())?;
    let url = format!("{}/api/replays", base(&settings.server));
    let response = agent
        .post(&url)
        .set("Authorization", &format!("Bearer {}", settings.key.trim()))
        .set(mechabellum_server::PLAYER_HEADER, me)
        .set("Content-Type", "application/octet-stream")
        .send_bytes(&body);
    match response {
        Ok(_) => {}
        // The server looked at it and said no: remember, don't retry.
        Err(ureq::Error::Status(code @ (400 | 403 | 422), resp)) => {
            let reason = resp
                .into_json::<serde_json::Value>()
                .ok()
                .and_then(|v| v["reason"].as_str().map(str::to_owned))
                .unwrap_or_else(|| format!("refused ({code})"));
            return Ok(Sent::Rejected(reason));
        }
        Err(e) => return Err(e.into()),
    }
    let ratings = store.ratings(battle_id)?;
    if !ratings.is_empty() {
        let url = format!("{}/api/replays/{battle_id}/ratings", base(&settings.server));
        // Ratings are a bonus; a failure here doesn't un-share the match.
        let _ = agent
            .post(&url)
            .set("Authorization", &format!("Bearer {}", settings.key.trim()))
            .send_json(&ratings);
    }
    Ok(Sent::Stored)
}

/// One pass: ask what the server lacks, send it. Returns an error when the server is
/// unreachable or refuses the key, so the caller can report and retry later.
fn sync_once(
    agent: &ureq::Agent,
    settings: &SyncSettings,
    store: &Store,
    status: &Mutex<SyncStatus>,
    stop: &AtomicBool,
    progress: &(dyn Fn() + Sync),
) -> anyhow::Result<()> {
    let server = base(&settings.server);
    let me = store
        .facets()?
        .main_player
        .map(|(id, _)| id)
        .ok_or_else(|| anyhow::anyhow!("no matches yet, so it isn't clear which player you are"))?;

    let all = store.battle_ids_for(&me)?;
    let done = store.synced(&server)?;
    let todo: Vec<String> = all
        .iter()
        .filter(|id| !done.contains_key(*id))
        .cloned()
        .collect();
    {
        let mut s = status.lock().unwrap();
        s.total = all.len();
        let ours: std::collections::HashSet<&String> = all.iter().collect();
        let mine = |id: &String| ours.contains(id);
        s.shared = done
            .iter()
            .filter(|(id, rejected)| rejected.is_none() && mine(id))
            .count();
        s.rejected = done
            .iter()
            .filter(|(id, rejected)| rejected.is_some() && mine(id))
            .count();
    }
    progress();

    let mut missing = Vec::new();
    for chunk in todo.chunks(ASK_CHUNK) {
        let answer: serde_json::Value = agent
            .post(&format!("{server}/api/replays/missing"))
            .set("Authorization", &format!("Bearer {}", settings.key.trim()))
            .send_json(serde_json::json!({ "battle_ids": chunk }))?
            .into_json()?;
        let lacking: std::collections::HashSet<String> = answer["missing"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect();
        // Already there (another player shared it, or an earlier run did).
        let present: Vec<&String> = chunk.iter().filter(|id| !lacking.contains(*id)).collect();
        for id in &present {
            store.mark_synced(&server, id, None)?;
        }
        status.lock().unwrap().shared += present.len();
        missing.extend(lacking);
    }
    progress();

    let queue = Mutex::new(missing);
    let failure: Mutex<Option<anyhow::Error>> = Mutex::new(None);
    std::thread::scope(|scope| {
        for _ in 0..PARALLEL {
            scope.spawn(|| loop {
                if stop.load(Ordering::Relaxed) || failure.lock().unwrap().is_some() {
                    return;
                }
                let Some(id) = queue.lock().unwrap().pop() else {
                    return;
                };
                match send_one(agent, settings, store, &me, &id) {
                    Ok(Sent::Stored) => {
                        let _ = store.mark_synced(&server, &id, None);
                        status.lock().unwrap().shared += 1;
                    }
                    Ok(Sent::Rejected(reason)) => {
                        let _ = store.mark_synced(&server, &id, Some(&reason));
                        status.lock().unwrap().rejected += 1;
                    }
                    Err(e) => {
                        *failure.lock().unwrap() = Some(e);
                        return;
                    }
                }
                progress();
            });
        }
    });
    match failure.into_inner().unwrap() {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

impl Syncer {
    /// Run in the background: sync when enabled, whenever `wake` fires (new matches or new
    /// settings) and every few minutes.
    pub fn start(
        store: Arc<Store>,
        settings: SyncSettings,
        wake: Receiver<()>,
        progress: Arc<dyn Fn(&SyncStatus) + Send + Sync>,
    ) -> Syncer {
        let settings = Arc::new(Mutex::new(settings));
        let status = Arc::new(Mutex::new(SyncStatus::default()));
        let syncer = Syncer {
            settings: settings.clone(),
            status: status.clone(),
        };
        std::thread::Builder::new()
            .name("sync".into())
            .spawn(move || {
                let agent = ureq::AgentBuilder::new()
                    .timeout(Duration::from_secs(60))
                    .build();
                let stop = AtomicBool::new(false);
                loop {
                    let current = settings.lock().unwrap().clone();
                    if current.ready() {
                        status.lock().unwrap().running = true;
                        let report = || progress(&status.lock().unwrap());
                        report();
                        let result = sync_once(&agent, &current, &store, &status, &stop, &report);
                        let mut s = status.lock().unwrap();
                        s.running = false;
                        s.last_error = result.err().map(|e| format!("{e:#}"));
                        drop(s);
                        report();
                    }
                    match wake.recv_timeout(INTERVAL) {
                        Ok(()) | Err(RecvTimeoutError::Timeout) => {
                            // Coalesce a burst of wake-ups (a big backfill) into one pass.
                            while wake.try_recv().is_ok() {}
                        }
                        Err(RecvTimeoutError::Disconnected) => return,
                    }
                }
            })
            .expect("spawn sync thread");
        syncer
    }
}
