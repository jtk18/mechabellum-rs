//! Backfills and then watches a replay folder, feeding new replays into the store.
//! The folder is only ever read.

use mechabellum_server::{IngestOutcome, Store};
use notify::{EventKind, RecursiveMode, Watcher};
use rayon::prelude::*;
use serde::Serialize;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::Arc;
use std::time::{Duration, Instant, UNIX_EPOCH};

/// How long a file must go without changes before it is read. The game may still be writing it.
const QUIET: Duration = Duration::from_millis(1500);
/// Give up on a file that never becomes a complete replay after this many tries.
const MAX_ATTEMPTS: u32 = 20;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum WatchEvent {
    BackfillProgress {
        done: usize,
        total: usize,
    },
    Ingested {
        file: String,
        outcome: IngestOutcome,
    },
    Failed {
        file: String,
        error: String,
    },
}

pub type EventSink = Arc<dyn Fn(WatchEvent) + Send + Sync>;

pub struct ReplayWatcher {
    // Dropping the watcher closes the channel, which ends the worker thread.
    _watcher: notify::RecommendedWatcher,
    stop: Arc<AtomicBool>,
}

impl Drop for ReplayWatcher {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

pub fn is_replay(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("grbr") || e.eq_ignore_ascii_case("xml"))
}

enum FileResult {
    Skipped,
    Done,
    /// No complete replay in the file yet.
    Incomplete,
    Failed,
}

fn process_file(store: &Store, path: &Path, sink: &EventSink, final_attempt: bool) -> FileResult {
    let file = path.display().to_string();
    let fail = |error: String| {
        sink(WatchEvent::Failed {
            file: file.clone(),
            error,
        });
        FileResult::Failed
    };
    let Ok(meta) = std::fs::metadata(path) else {
        // Deleted or renamed since the event.
        return FileResult::Skipped;
    };
    let size = meta.len();
    let modified = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs() as i64);
    match store.file_seen(&file, size, modified) {
        Ok(true) => return FileResult::Skipped,
        Ok(false) => {}
        Err(e) => return fail(format!("{e:#}")),
    }

    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) => return fail(e.to_string()),
    };
    if mechabellum::BattleRecord::extract_document(&bytes).is_none() && !final_attempt {
        return FileResult::Incomplete;
    }
    match store.ingest(&bytes, path, None) {
        Ok(outcome) => {
            let battle_id = match &outcome {
                IngestOutcome::Added { battle_id } | IngestOutcome::Duplicate { battle_id } => {
                    battle_id.clone()
                }
            };
            let _ = store.mark_file(&file, size, modified, Ok(&battle_id));
            sink(WatchEvent::Ingested { file, outcome });
            FileResult::Done
        }
        Err(e) => {
            let error = format!("{e:#}");
            let _ = store.mark_file(&file, size, modified, Err(&error));
            fail(error)
        }
    }
}

impl ReplayWatcher {
    /// Start watching `dir`. Existing replays are backfilled on a background thread first;
    /// files that were already read, with the same size and modification time, are skipped
    /// without parsing.
    pub fn start(dir: &Path, store: Arc<Store>, sink: EventSink) -> anyhow::Result<Self> {
        let (tx, rx) = mpsc::channel::<PathBuf>();
        let mut watcher =
            notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
                if let Ok(event) = res {
                    if matches!(event.kind, EventKind::Create(_) | EventKind::Modify(_)) {
                        for path in event.paths.into_iter().filter(|p| is_replay(p)) {
                            let _ = tx.send(path);
                        }
                    }
                }
            })?;
        // Watch before backfilling so nothing written in between is missed.
        watcher.watch(dir, RecursiveMode::NonRecursive)?;

        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let dir = dir.to_path_buf();
        std::thread::Builder::new()
            .name("replay-watcher".into())
            .spawn(move || {
                let mut existing: Vec<PathBuf> = std::fs::read_dir(&dir)
                    .map(|entries| {
                        entries
                            .flatten()
                            .map(|e| e.path())
                            .filter(|p| p.is_file() && is_replay(p))
                            .collect()
                    })
                    .unwrap_or_default();
                existing.sort();
                // Parse on every core; Store::ingest only serializes the database write.
                let total = existing.len();
                let started = Instant::now();
                let done = AtomicUsize::new(0);
                existing.par_iter().for_each(|path| {
                    if stopped.load(Ordering::Relaxed) {
                        return;
                    }
                    process_file(&store, path, &sink, true);
                    let finished = done.fetch_add(1, Ordering::Relaxed) + 1;
                    if finished % 25 == 0 || finished == total {
                        sink(WatchEvent::BackfillProgress {
                            done: finished,
                            total,
                        });
                    }
                });
                if stopped.load(Ordering::Relaxed) {
                    return;
                }
                log::info!(
                    "backfill: {total} replay files checked in {:.2?}",
                    started.elapsed()
                );
                if total == 0 {
                    sink(WatchEvent::BackfillProgress { done: 0, total: 0 });
                }

                // path -> (last change, attempts)
                let mut pending: HashMap<PathBuf, (Instant, u32)> = HashMap::new();
                while !stopped.load(Ordering::Relaxed) {
                    match rx.recv_timeout(Duration::from_millis(500)) {
                        Ok(path) => {
                            let attempts = pending.get(&path).map_or(0, |(_, a)| *a);
                            pending.insert(path, (Instant::now(), attempts));
                        }
                        Err(RecvTimeoutError::Timeout) => {}
                        Err(RecvTimeoutError::Disconnected) => return,
                    }
                    let ready: Vec<PathBuf> = pending
                        .iter()
                        .filter(|(_, (changed, _))| changed.elapsed() >= QUIET)
                        .map(|(path, _)| path.clone())
                        .collect();
                    for path in ready {
                        let (_, attempts) = pending.remove(&path).unwrap();
                        let last_try = attempts + 1 >= MAX_ATTEMPTS;
                        if let FileResult::Incomplete = process_file(&store, &path, &sink, last_try)
                        {
                            pending.insert(path, (Instant::now(), attempts + 1));
                        }
                    }
                }
            })?;

        Ok(ReplayWatcher {
            _watcher: watcher,
            stop,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../crates/mechabellum/fixtures")
            .join(name)
    }

    fn wait_for(events: &Mutex<Vec<WatchEvent>>, what: impl Fn(&WatchEvent) -> bool) -> bool {
        let deadline = Instant::now() + Duration::from_secs(20);
        while Instant::now() < deadline {
            if events.lock().unwrap().iter().any(&what) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        false
    }

    #[test]
    fn test_backfill_then_watch() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::copy(
            fixture("battle_record_2324.xml"),
            dir.path().join("2324_a.xml"),
        )
        .unwrap();
        std::fs::write(dir.path().join("notes.txt"), "not a replay").unwrap();

        let store = Arc::new(Store::open_in_memory().unwrap());
        let events = Arc::new(Mutex::new(Vec::new()));
        let seen = events.clone();
        let sink: EventSink = Arc::new(move |e| seen.lock().unwrap().push(e));
        let watcher = ReplayWatcher::start(dir.path(), store.clone(), sink).unwrap();

        assert!(wait_for(&events, |e| matches!(
            e,
            WatchEvent::BackfillProgress { done: 1, total: 1 }
        )));
        assert_eq!(store.match_count().unwrap(), 1);

        // A new replay written in two steps, like a game still saving it: only a complete file
        // gets ingested.
        let bytes = std::fs::read(fixture(
            "1425_20241123--67298547_[小疯子]VS[kaltorak18].grbr",
        ))
        .unwrap();
        let new_file = dir.path().join("1425_b.grbr");
        std::fs::write(&new_file, &bytes[..bytes.len() / 3]).unwrap();
        std::thread::sleep(Duration::from_millis(300));
        std::fs::write(&new_file, &bytes).unwrap();

        assert!(
            wait_for(&events, |e| matches!(
                e,
                WatchEvent::Ingested {
                    outcome: IngestOutcome::Added { .. },
                    file
                } if file.ends_with("1425_b.grbr")
            )),
            "new replay was not picked up: {:?}",
            events.lock().unwrap()
        );
        assert_eq!(store.match_count().unwrap(), 2);
        assert!(!events
            .lock()
            .unwrap()
            .iter()
            .any(|e| matches!(e, WatchEvent::Failed { .. })));
        drop(watcher);

        // A restart skips files it has already read, without parsing them again.
        let events = Arc::new(Mutex::new(Vec::new()));
        let seen = events.clone();
        let sink: EventSink = Arc::new(move |e| seen.lock().unwrap().push(e));
        let _watcher = ReplayWatcher::start(dir.path(), store.clone(), sink).unwrap();
        assert!(wait_for(&events, |e| matches!(
            e,
            WatchEvent::BackfillProgress { done: 2, total: 2 }
        )));
        assert!(!events
            .lock()
            .unwrap()
            .iter()
            .any(|e| matches!(e, WatchEvent::Ingested { .. })));
    }
}
