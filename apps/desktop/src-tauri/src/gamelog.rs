//! Ratings from the game's own log. Replays don't record MMR, but when a match starts the game
//! logs a message with the battle ID and each player's `eloScore`, `rankPoint`, and level.
//! The log only holds the current and previous session, so it's read as it grows.

use mechabellum_server::{Rating, Store};
use serde_json::Value;

use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Folders where Unity puts Mechabellum's `Player.log`, most likely first.
pub fn log_dirs(steam_libraries: &[PathBuf]) -> Vec<PathBuf> {
    let home =
        std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from);
    let mut dirs = Vec::new();
    if let Some(home) = &home {
        if cfg!(windows) {
            dirs.push(home.join("AppData/LocalLow/GameRiver/Mechabellum"));
        } else if cfg!(target_os = "macos") {
            dirs.push(home.join("Library/Logs/GameRiver/Mechabellum"));
        } else {
            dirs.push(home.join(".config/unity3d/GameRiver/Mechabellum"));
        }
    }
    // Linux running the Windows build under Proton.
    for library in steam_libraries {
        dirs.push(library.join(
            "steamapps/compatdata/669330/pfx/drive_c/users/steamuser/AppData/LocalLow/GameRiver/Mechabellum",
        ));
    }
    dirs
}

/// The first JSON value in `text` starting at `start` (an opening bracket), respecting strings.
fn json_span(text: &str, start: usize) -> Option<&str> {
    let bytes = text.as_bytes();
    let (mut depth, mut in_string, mut escaped) = (0i32, false, false);
    for (i, &b) in bytes.iter().enumerate().skip(start) {
        if in_string {
            match b {
                _ if escaped => escaped = false,
                b'\\' => escaped = true,
                b'"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match b {
            b'"' => in_string = true,
            b'[' | b'{' => depth += 1,
            b']' | b'}' => {
                depth -= 1;
                if depth == 0 {
                    return text.get(start..=i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Match-start ratings in a chunk of log text: (battle ID, ratings).
/// The player ID of the account that logged in, from the last `[C] receive server id : <id>`
/// line. That's the player using this machine, whoever appears in the most replays.
pub fn parse_logged_in(text: &str) -> Option<String> {
    text.lines()
        .rev()
        .filter_map(|line| line.split_once("receive server id :"))
        .map(|(_, id)| id.trim())
        .find(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()))
        .map(str::to_owned)
}

pub fn parse_ratings(text: &str) -> Vec<(String, Vec<Rating>)> {
    let mut found = Vec::new();
    for line in text.lines() {
        if !line.contains("\"BattleID\"") || !line.contains("\"playeres\"") {
            continue;
        }
        let Some(start) = line.find('[') else {
            continue;
        };
        let Some(value) =
            json_span(line, start).and_then(|j| serde_json::from_str::<Value>(j).ok())
        else {
            continue;
        };
        for message in value.as_array().into_iter().flatten() {
            let Some(battle) = message["info"]["BattleID"].as_str() else {
                continue;
            };
            let ratings: Vec<Rating> = message["playeres"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|p| {
                    let info = &p["baseInfo"];
                    let id = info["userid"].as_str()?;
                    // Recorded opponents (Star Expedition) have no real account.
                    (id != "1").then(|| Rating {
                        player_id: id.to_owned(),
                        elo: info["eloScore"].as_i64(),
                        rank_point: info["rankPoint"].as_i64(),
                        level: info["level"].as_i64(),
                        elo_games: info["eloTimes"].as_i64(),
                    })
                })
                .collect();
            if !ratings.is_empty() {
                found.push((battle.to_owned(), ratings));
            }
        }
    }
    found
}

/// Read the previous session's log once, then follow the current one as it grows. Runs until
/// `stop` is set.
pub fn follow(dirs: Vec<PathBuf>, store: Arc<Store>, stop: Arc<AtomicBool>) {
    // The game creates its log on first launch, which may come after this app starts.
    let mut announced = false;
    let dir = loop {
        if let Some(dir) = dirs.iter().find(|d| d.join("Player.log").is_file()) {
            break dir.clone();
        }
        if !announced {
            log::info!("no Mechabellum Player.log yet; checking every 5 s");
            announced = true;
        }
        if stop.load(Ordering::Relaxed) {
            return;
        }
        std::thread::sleep(Duration::from_secs(5));
    };
    log::info!("reading ratings from {}", dir.join("Player.log").display());
    let save = |text: &str| {
        if let Some(id) = parse_logged_in(text) {
            if let Err(e) = store.set_local_player(&id) {
                log::warn!("saving the logged-in player: {e:#}");
            }
        }
        for (battle, ratings) in parse_ratings(text) {
            if let Err(e) = store.put_ratings(&battle, &ratings) {
                log::warn!("saving ratings for {battle}: {e:#}");
            }
        }
    };
    if let Ok(previous) = std::fs::read(dir.join("Player-prev.log")) {
        save(&String::from_utf8_lossy(&previous));
    }

    let current = dir.join("Player.log");
    let mut offset = 0u64;
    let mut partial = String::new();
    while !stop.load(Ordering::Relaxed) {
        if let Err(e) = read_new(&current, &mut offset, &mut partial, &save) {
            log::debug!("reading {}: {e}", current.display());
        }
        std::thread::sleep(Duration::from_secs(5));
    }
}

/// Read what was appended since `offset`. The game starts a fresh log each session, so a
/// shorter file means start over.
fn read_new(
    path: &Path,
    offset: &mut u64,
    partial: &mut String,
    save: &dyn Fn(&str),
) -> std::io::Result<()> {
    let len = std::fs::metadata(path)?.len();
    if len < *offset {
        *offset = 0;
        partial.clear();
    }
    if len == *offset {
        return Ok(());
    }
    let mut file = std::fs::File::open(path)?;
    file.seek(SeekFrom::Start(*offset))?;
    let mut bytes = Vec::with_capacity((len - *offset) as usize);
    file.take(len - *offset).read_to_end(&mut bytes)?;
    *offset += bytes.len() as u64;
    partial.push_str(&String::from_utf8_lossy(&bytes));
    // Only complete lines; keep the rest for next time.
    if let Some(end) = partial.rfind('\n') {
        let complete: String = partial.drain(..=end).collect();
        save(&complete);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_match_start() {
        let line = r#"[{ "info": { "SystemSeed": 1, "BattleID": "20260927--67216808", "MapID": 5 }, "playeres": [ { "baseInfo": { "userid": "1", "name": "CodeX", "eloScore": 384, "rankPoint": 1, "level": 1 } }, { "baseInfo": { "userid": "281474976711196086", "name": "jtk18 [x]", "eloScore": 1344, "rankPoint": 3285, "level": 352, "eloTimes": 1583 } } ], "state": 0 }] trailing text"#;
        let text = format!("noise\n{line}\nmore noise\n");
        let found = parse_ratings(&text);
        assert_eq!(found.len(), 1);
        let (battle, ratings) = &found[0];
        assert_eq!(battle, "20260927--67216808");
        assert_eq!(
            ratings,
            &vec![Rating {
                player_id: "281474976711196086".into(),
                elo: Some(1344),
                rank_point: Some(3285),
                level: Some(352),
                elo_games: Some(1583),
            }],
            "the recorded opponent (userid 1) is skipped"
        );
    }

    #[test]
    fn test_follows_appends_and_new_sessions() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Player.log");
        let line = |battle: &str, elo: i64| {
            format!(
                r#"[{{ "info": {{ "BattleID": "{battle}" }}, "playeres": [ {{ "baseInfo": {{ "userid": "7", "eloScore": {elo} }} }} ] }}]"#
            )
        };
        let seen = std::cell::RefCell::new(Vec::new());
        let save = |text: &str| {
            seen.borrow_mut()
                .extend(parse_ratings(text).into_iter().map(|(b, _)| b))
        };
        let (mut offset, mut partial) = (0, String::new());

        std::fs::write(&path, format!("{}\n", line("a--1", 1))).unwrap();
        read_new(&path, &mut offset, &mut partial, &save).unwrap();
        // Half a line, then the rest.
        let second = line("b--2", 2);
        let (head, tail) = second.split_at(10);
        std::fs::write(&path, format!("{}\n{head}", line("a--1", 1))).unwrap();
        read_new(&path, &mut offset, &mut partial, &save).unwrap();
        std::fs::write(&path, format!("{}\n{second}\n", line("a--1", 1))).unwrap();
        read_new(&path, &mut offset, &mut partial, &save).unwrap();
        // A new session starts a shorter log.
        std::fs::write(&path, format!("{}\n", line("c--3", 3))).unwrap();
        read_new(&path, &mut offset, &mut partial, &save).unwrap();
        let _ = tail;
        assert_eq!(*seen.borrow(), vec!["a--1", "b--2", "c--3"]);
    }

    #[test]
    fn test_logged_in_player_is_the_last_login() {
        let log = "noise\n[C] receive server id : 281474976711196086\nmore\n\
                   [C] receive server id : 42\r\n[C] receive server id : \n";
        assert_eq!(parse_logged_in(log).as_deref(), Some("42"));
        assert_eq!(parse_logged_in("nothing here"), None);
    }
}
