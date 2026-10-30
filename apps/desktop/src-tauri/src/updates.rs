//! Updates from the project's GitHub releases, through Tauri's updater plugin.
//!
//! tauri.conf.json (`plugins.updater`) names the endpoint, the `latest.json` the release workflow
//! uploads, and the public key every update must be signed with. A check that finds a newer
//! version keeps it here until the user installs it from the banner; installing downloads it,
//! verifies its signature, replaces the app and restarts. Failures (offline, no published release
//! yet, a private repository answering 404) are logged and shown as one line in Settings, never
//! as a dialog.

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// How often a running app looks for a release while automatic checks are on.
const CHECK_EVERY: Duration = Duration::from_secs(4 * 60 * 60);
/// The first automatic check waits this long after start, so it doesn't compete with opening
/// the database and the first stats queries.
const FIRST_CHECK_AFTER: Duration = Duration::from_secs(10);
/// How often the timer wakes to see whether a check is due. It compares wall-clock times, so a
/// computer that slept through the interval checks within a minute of waking.
const TICK: Duration = Duration::from_secs(60);
/// Written to the app data folder just before an update installs, holding the version it
/// replaces. The next start reads it, opens the window and says what was updated.
const MARKER: &str = "updated-from";

#[derive(Debug, Clone, Default, Serialize)]
pub struct UpdateStatus {
    /// The version running now.
    pub current: String,
    /// False when this copy can't update itself: it runs from a cargo build folder, or on macOS
    /// from outside an app bundle. Installing there would replace the build folder.
    pub supported: bool,
    /// Check shortly after start and every few hours.
    pub auto_check: bool,
    pub checking: bool,
    /// Downloading, verifying or installing.
    pub installing: bool,
    /// Download progress from 0 to 1 while installing, when the server sent a length.
    pub progress: Option<f64>,
    /// A newer release, ready to install.
    pub available: Option<Available>,
    /// When the last check finished, in seconds since 1970.
    pub checked_at: Option<u64>,
    /// Why the last check or install failed, as a sentence. The log has the whole error.
    pub error: Option<String>,
    /// On the first start after an update, the version it replaced.
    pub updated_from: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Available {
    pub version: String,
    /// The release notes latest.json carries; often empty.
    pub notes: Option<String>,
    /// When it was published, in seconds since 1970.
    pub published: Option<i64>,
}

impl From<&Update> for Available {
    fn from(update: &Update) -> Self {
        Available {
            version: update.version.clone(),
            notes: update.body.clone().filter(|n| !n.trim().is_empty()),
            published: update.date.map(|d| d.unix_timestamp()),
        }
    }
}

pub struct Updates {
    status: Mutex<UpdateStatus>,
    /// The update the last check found, held for the install button.
    pending: Mutex<Option<Update>>,
    /// When the last check started, automatic or not; the timer counts from it.
    last_attempt: Mutex<Option<SystemTime>>,
    marker: PathBuf,
}

impl Updates {
    pub fn new(current: String, auto_check: bool, data_dir: &Path) -> Self {
        let marker = data_dir.join(MARKER);
        let updated_from = take_marker(&marker, &current);
        let supported = match std::env::current_exe() {
            Ok(exe) => {
                let ok = updatable(&exe);
                if !ok {
                    log::info!(
                        "updates are off: {} is a build, not an installed app",
                        exe.display()
                    );
                }
                ok
            }
            Err(e) => {
                log::warn!("updates are off: can't tell where the app runs from: {e}");
                false
            }
        };
        Updates {
            status: Mutex::new(UpdateStatus {
                current,
                supported,
                auto_check,
                updated_from,
                ..Default::default()
            }),
            pending: Mutex::new(None),
            last_attempt: Mutex::new(None),
            marker,
        }
    }

    pub fn status(&self) -> UpdateStatus {
        self.status.lock().unwrap().clone()
    }

    pub fn set_auto_check(&self, on: bool) {
        self.status.lock().unwrap().auto_check = on;
    }

    fn emit(&self, app: &AppHandle) {
        let _ = app.emit("update-status", self.status());
    }

    fn due(&self, now: SystemTime) -> bool {
        {
            let status = self.status.lock().unwrap();
            if !status.supported || !status.auto_check {
                return false;
            }
        }
        is_due(*self.last_attempt.lock().unwrap(), now)
    }
}

/// Whether an automatic check is due, given when the last one started.
fn is_due(last: Option<SystemTime>, now: SystemTime) -> bool {
    // A clock set back makes the last check look like it's in the future; check rather than
    // wait for the clock to catch up.
    last.is_none_or(|at| now.duration_since(at).map_or(true, |d| d >= CHECK_EVERY))
}

/// Whether the copy at `exe` may update itself. The updater replaces what holds the executable:
/// on macOS the `.app` around `Contents/MacOS`, elsewhere the installed program. A binary run
/// from a cargo target folder (cargo marks its root with CACHEDIR.TAG) is a build, and on macOS
/// installing over it would replace the build folder, so builds never check.
fn updatable(exe: &Path) -> bool {
    let in_build_folder = exe
        .ancestors()
        .skip(1)
        .any(|dir| dir.join("CACHEDIR.TAG").is_file());
    let in_bundle = !cfg!(target_os = "macos") || {
        let macos = exe.parent();
        macos.is_some_and(|dir| dir.ends_with("Contents/MacOS"))
            && macos
                .and_then(Path::parent)
                .and_then(Path::parent)
                .and_then(Path::extension)
                .is_some_and(|ext| ext == "app")
    };
    in_bundle && !in_build_folder
}

/// The version an update replaced, from the marker its install left, which is removed. A marker
/// naming the version running now means the install never happened (a cancelled installer, say),
/// so it's dropped without a report.
fn take_marker(marker: &Path, current: &str) -> Option<String> {
    let from = std::fs::read_to_string(marker).ok()?;
    let _ = std::fs::remove_file(marker);
    let from = from.trim();
    if from.is_empty() || from == current {
        log::info!("an update to replace {from} didn't finish; still running {current}");
        return None;
    }
    log::info!("updated from {from} to {current}");
    Some(from.to_owned())
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// A failed check as a sentence for the Settings line.
fn describe_check(e: &tauri_plugin_updater::Error) -> String {
    use tauri_plugin_updater::Error as E;
    let reason = match e {
        // Any answer but success: no published release yet, or a private repository's 404.
        E::ReleaseNotFound => "no published release was found at the update address".to_owned(),
        E::Reqwest(e) if e.is_connect() || e.is_timeout() || e.is_request() => {
            "the update server couldn't be reached".to_owned()
        }
        E::TargetNotFound(_) | E::TargetsNotFound(_) => {
            "the newest release has no download for this system".to_owned()
        }
        other => other.to_string(),
    };
    format!("Couldn't check for updates: {reason}.")
}

/// A failed install as a sentence for the banner and the Settings line.
fn describe_install(version: &str, e: &tauri_plugin_updater::Error) -> String {
    use tauri_plugin_updater::Error as E;
    let reason = match e {
        // A different key, or a file changed after signing.
        E::Minisign(_) => {
            "the download has no valid signature from this app's key, so nothing was installed"
                .to_owned()
        }
        E::Reqwest(_) | E::Network(_) => "the download didn't finish".to_owned(),
        other => other.to_string(),
    };
    format!("Couldn't install {version}: {reason}.")
}

/// Ask the endpoint for a newer release. One check runs at a time; a call while a check or an
/// install is under way returns without checking.
pub async fn check(app: &AppHandle) {
    let updates = app.state::<Updates>();
    {
        let mut status = updates.status.lock().unwrap();
        if !status.supported || status.checking || status.installing {
            return;
        }
        status.checking = true;
    }
    *updates.last_attempt.lock().unwrap() = Some(SystemTime::now());
    updates.emit(app);

    let result = match app.updater() {
        Ok(updater) => updater.check().await,
        Err(e) => Err(e),
    };
    let outcome = match result {
        Ok(Some(update)) => {
            log::info!(
                "update check: {} is available, running {}",
                update.version,
                update.current_version
            );
            let available = Available::from(&update);
            *updates.pending.lock().unwrap() = Some(update);
            Ok(Some(available))
        }
        Ok(None) => {
            log::info!("update check: no newer release");
            *updates.pending.lock().unwrap() = None;
            Ok(None)
        }
        Err(e) => {
            log::warn!("update check failed: {e}");
            Err(describe_check(&e))
        }
    };
    {
        let mut status = updates.status.lock().unwrap();
        status.checking = false;
        status.checked_at = Some(unix_now());
        match outcome {
            Ok(available) => {
                status.available = available;
                status.error = None;
            }
            // A failed check keeps any update an earlier check found.
            Err(e) => status.error = Some(e),
        }
    }
    updates.emit(app);
}

/// Download the update the last check found, verify its signature against the key in
/// tauri.conf.json, install it and restart. On Windows the installer takes over and the updater
/// exits the app itself.
pub async fn install(app: &AppHandle) -> Result<(), String> {
    let updates = app.state::<Updates>();
    let Some(update) = updates.pending.lock().unwrap().clone() else {
        return Err("There's no update to install. Check again.".into());
    };
    {
        let mut status = updates.status.lock().unwrap();
        if status.installing {
            return Err("The update is already installing.".into());
        }
        status.installing = true;
        status.progress = None;
        status.error = None;
    }
    updates.emit(app);
    log::info!(
        "downloading update {} from {}",
        update.version,
        update.download_url
    );

    match download_and_install(app, &updates, &update).await {
        Ok(()) => {
            log::info!("installed {}; restarting", update.version);
            app.request_restart();
            Ok(())
        }
        Err(e) => {
            log::warn!("installing update {}: {e}", update.version);
            let _ = std::fs::remove_file(&updates.marker);
            let message = describe_install(&update.version, &e);
            {
                let mut status = updates.status.lock().unwrap();
                status.installing = false;
                status.progress = None;
                status.error = Some(message.clone());
            }
            updates.emit(app);
            Err(message)
        }
    }
}

async fn download_and_install(
    app: &AppHandle,
    updates: &Updates,
    update: &Update,
) -> Result<(), tauri_plugin_updater::Error> {
    let mut received = 0u64;
    let mut shown = 0.0f64;
    // `download` checks the signature once the last byte is in and fails if it doesn't match.
    let bytes = update
        .download(
            |chunk, total| {
                received += chunk as u64;
                let Some(total) = total.filter(|&t| t > 0) else {
                    return;
                };
                let fraction = (received as f64 / total as f64).min(1.0);
                if fraction - shown >= 0.01 || fraction >= 1.0 {
                    shown = fraction;
                    updates.status.lock().unwrap().progress = Some(fraction);
                    updates.emit(app);
                }
            },
            || log::info!("update downloaded; checking its signature"),
        )
        .await?;
    log::info!("update {} signature verified", update.version);

    // Written before installing: on Windows the installer exits the app.
    if let Err(e) = std::fs::write(&updates.marker, &update.current_version) {
        log::warn!("writing {}: {e}", updates.marker.display());
    }
    let update = update.clone();
    tauri::async_runtime::spawn_blocking(move || update.install(bytes))
        .await
        .map_err(|e| tauri_plugin_updater::Error::Io(std::io::Error::other(e.to_string())))?
}

/// Check soon after start, then each time the last check is `CHECK_EVERY` old, while automatic
/// checks are on.
pub fn start_timer(app: AppHandle) -> std::io::Result<()> {
    std::thread::Builder::new()
        .name("update-check".into())
        .spawn(move || {
            std::thread::sleep(FIRST_CHECK_AFTER);
            loop {
                if app.state::<Updates>().due(SystemTime::now()) {
                    tauri::async_runtime::block_on(check(&app));
                }
                std::thread::sleep(TICK);
            }
        })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn due_after_the_interval_or_a_clock_set_back() {
        let now = SystemTime::now();
        assert!(is_due(None, now));
        assert!(!is_due(Some(now - Duration::from_secs(60)), now));
        assert!(is_due(Some(now - CHECK_EVERY), now));
        assert!(is_due(Some(now + Duration::from_secs(60)), now));
    }

    #[test]
    fn builds_never_update() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target");
        std::fs::create_dir_all(target.join("debug")).unwrap();
        std::fs::write(target.join("CACHEDIR.TAG"), "Signature: x").unwrap();
        assert!(!updatable(&target.join("debug/mechabellum-desktop")));
        assert!(!updatable(&target.join(
            "debug/bundle/macos/Mechabellum Stats.app/Contents/MacOS/app"
        )));

        let installed = dir
            .path()
            .join("Applications/Mechabellum Stats.app/Contents/MacOS/app");
        assert!(updatable(&installed));
        // Outside a bundle, macOS would replace the executable's folder.
        let loose = dir.path().join("bin/mechabellum-desktop");
        assert_eq!(updatable(&loose), !cfg!(target_os = "macos"));
    }

    #[test]
    fn marker_reports_once_and_only_a_real_change() {
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join(MARKER);
        assert_eq!(take_marker(&marker, "0.1.1"), None);

        std::fs::write(&marker, "0.1.0").unwrap();
        assert_eq!(take_marker(&marker, "0.1.1").as_deref(), Some("0.1.0"));
        assert!(!marker.exists());
        assert_eq!(take_marker(&marker, "0.1.1"), None);

        // The install didn't happen: same version still running.
        std::fs::write(&marker, "0.1.1\n").unwrap();
        assert_eq!(take_marker(&marker, "0.1.1"), None);
        assert!(!marker.exists());
    }
}
