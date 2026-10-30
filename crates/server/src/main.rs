use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use mechabellum_server::pseudonym::Pseudonyms;
use mechabellum_server::{router, IngestOutcome, Store, UploadAuth};

use rayon::prelude::*;
use std::net::SocketAddr;
use std::path::PathBuf;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// Accepts Mechabellum replays and serves aggregate stats
#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    /// SQLite database file
    #[arg(
        long,
        env = "MECHABELLUM_DB",
        default_value = "mechabellum.db",
        global = true
    )]
    db: PathBuf,
    /// The secret key for pseudonymous player IDs (64 hex digits). Default: `pseudonym.key`
    /// beside the database. `serve --require-auth` creates one if the database has never been
    /// pseudonymized; after that every command needs the same key. Back it up with the database.
    #[arg(long, env = "MECHABELLUM_PSEUDONYM_KEY_FILE", global = true)]
    pseudonym_key_file: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Run the HTTP API
    Serve {
        #[arg(long, env = "MECHABELLUM_BIND", default_value = "127.0.0.1:8080")]
        bind: SocketAddr,
        /// Require an upload key (see `keys add`) or the admin token for uploads
        #[arg(long, env = "MECHABELLUM_REQUIRE_AUTH")]
        require_auth: bool,
        /// Admin upload token, accepted in addition to database keys. Setting it implies
        /// --require-auth
        #[arg(long, env = "MECHABELLUM_UPLOAD_TOKEN", hide_env_values = true)]
        upload_token: Option<String>,
        /// The server's public address (such as https://mechabellum-stats.com). With
        /// --require-auth, it turns on Sign in with Steam at /auth/steam
        #[arg(long, env = "MECHABELLUM_PUBLIC_URL")]
        public_url: Option<String>,
        /// File holding a Steam Web API key, for the account checks below. Without one, signing
        /// in to Steam is the only check
        #[arg(long, env = "MECHABELLUM_STEAM_API_KEY_FILE")]
        steam_api_key_file: Option<PathBuf>,
        /// Youngest Steam account that may sign in, in days (needs the API key; 0: no check)
        #[arg(long, env = "MECHABELLUM_STEAM_MIN_ACCOUNT_DAYS", default_value_t = 30)]
        steam_min_account_days: u32,
        /// Least Mechabellum playtime that may sign in, in hours (needs the API key; 0: no check)
        #[arg(
            long,
            env = "MECHABELLUM_STEAM_MIN_PLAYTIME_HOURS",
            default_value_t = 2
        )]
        steam_min_playtime_hours: u32,
        /// Steam's OpenID endpoint, replaced only to test against a stand-in
        #[arg(long, hide = true)]
        steam_openid_url: Option<String>,
        /// Steam Web API base address, replaced only to test against a stand-in
        #[arg(long, hide = true)]
        steam_api_url: Option<String>,
    },
    /// Load every .grbr/.xml replay in a directory into the database (read-only on the directory)
    Import { dir: PathBuf },
    /// Rebuild all stats from the stored replays, e.g. after a parser upgrade
    Reindex,
    /// Remove every match an upload key shared (then revoke the key with `keys revoke`)
    Purge {
        /// The key's name
        uploader: String,
    },
    /// List shared matches whose players' copies disagree
    Disputes,
    /// List uploaders whose results sit far from everyone else's
    Outliers,
    /// Manage upload keys
    Keys {
        #[command(subcommand)]
        command: KeysCommand,
    },
    /// Manage members who signed in with Steam
    Accounts {
        #[command(subcommand)]
        command: AccountsCommand,
    },
}

#[derive(Subcommand, Debug)]
enum AccountsCommand {
    /// List accounts (names and dates; no Steam IDs are stored)
    List,
    /// Stop an account's uploads and sign-ins; its keys stop working at once
    Disable { name: String },
    /// Allow a disabled account again
    Enable { name: String },
}

#[derive(Subcommand, Debug)]
enum KeysCommand {
    /// Create a key and print it. It can't be shown again
    Add { name: String },
    /// List keys (names and dates only)
    List,
    /// Revoke a key; uploads using it are rejected immediately
    Revoke { name: String },
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let args = Args::parse();
    let store = Arc::new(Store::open(&args.db)?);
    store.upgrade()?;

    // A community server keeps no account IDs or names. Once a database is pseudonymized,
    // every command must use the same key, so nothing raw is ever stored in it again.
    let community = matches!(
        &args.command,
        Command::Serve { require_auth, upload_token, .. }
            if *require_auth || upload_token.as_deref().is_some_and(|t| !t.trim().is_empty())
    );
    let pseudonymized = store.pseudonym_check()?.is_some();
    if community || pseudonymized {
        let key_file = args.pseudonym_key_file.clone().unwrap_or_else(|| {
            args.db
                .parent()
                .unwrap_or(std::path::Path::new("."))
                .join("pseudonym.key")
        });
        store.use_pseudonyms(Pseudonyms::load(&key_file, !pseudonymized)?)?;
    }

    match args.command {
        Command::Serve {
            bind,
            require_auth,
            upload_token,
            public_url,
            steam_api_key_file,
            steam_min_account_days,
            steam_min_playtime_hours,
            steam_openid_url,
            steam_api_url,
        } => {
            let admin_token = upload_token.filter(|t| !t.trim().is_empty());
            let auth = if require_auth || admin_token.is_some() {
                if admin_token.is_none() && store.active_key_count()? == 0 {
                    bail!(
                        "uploads require auth but no credentials exist; create one with \
                         `mechabellum-server keys add <name>` or set MECHABELLUM_UPLOAD_TOKEN"
                    );
                }
                UploadAuth::Required { admin_token }
            } else {
                UploadAuth::Open
            };
            let uploads = match auth {
                UploadAuth::Open => "open",
                UploadAuth::Required { .. } => "require a key",
            };
            let community = matches!(auth, UploadAuth::Required { .. });
            let mut app = router(store.clone(), auth);
            if let Some(public_url) = public_url.filter(|_| community) {
                let mut steam = mechabellum_server::steam::SteamConfig::new(&public_url);
                if let Some(file) = steam_api_key_file {
                    let key = std::fs::read_to_string(&file)
                        .with_context(|| format!("reading {}", file.display()))?;
                    steam.api_key = Some(key.trim().to_owned()).filter(|k| !k.is_empty());
                }
                steam.min_account_days = steam_min_account_days;
                steam.min_playtime_minutes = steam_min_playtime_hours * 60;
                if let Some(url) = steam_openid_url {
                    steam.claimed_id_prefix = format!(
                        "{}/id/",
                        url.trim_end_matches('/').trim_end_matches("/login")
                    );
                    steam.openid_endpoint = url;
                }
                if let Some(url) = steam_api_url {
                    steam.api_base = url.trim_end_matches('/').to_owned();
                }
                log::info!("Sign in with Steam at {}/auth/steam", steam.public_url);
                app = app.merge(mechabellum_server::steam::routes(store, steam)?);
            }
            // Socket activation (systemd, or a host such as uredo): serve the
            // Unix socket handed over as fd 3 instead of binding `bind`.
            if let Some(listener) = activated_unix_listener()? {
                log::info!(
                    "listening on the activated socket (db {}, uploads {uploads})",
                    args.db.display()
                );
                axum::serve(listener, app).await?;
            } else {
                let listener = tokio::net::TcpListener::bind(bind)
                    .await
                    .with_context(|| format!("binding {bind}"))?;
                log::info!(
                    "listening on http://{bind} (db {}, uploads {uploads})",
                    args.db.display()
                );
                axum::serve(listener, app).await?;
            }
        }
        Command::Import { dir } => {
            let paths: Vec<PathBuf> = std::fs::read_dir(&dir)?
                .map(|entry| entry.map(|e| e.path()))
                .collect::<std::io::Result<_>>()?;
            let (added, duplicate, failed) = (
                AtomicUsize::new(0),
                AtomicUsize::new(0),
                AtomicUsize::new(0),
            );
            // Parse on every core; Store::ingest only serializes the database write.
            paths
                .par_iter()
                .filter(|p| p.extension().is_some_and(|e| e == "grbr" || e == "xml"))
                .for_each(|path| {
                    let counter = match std::fs::read(path)
                        .map_err(Into::into)
                        .and_then(|bytes| store.ingest(&bytes, path, Some("import")))
                    {
                        Ok(IngestOutcome::Added { .. }) => &added,
                        Ok(IngestOutcome::Duplicate { .. }) => &duplicate,
                        Err(e) => {
                            log::warn!("{}: {e:#}", path.display());
                            &failed
                        }
                    };
                    counter.fetch_add(1, Ordering::Relaxed);
                });
            println!(
                "added {}, already present {}, failed {}",
                added.into_inner(),
                duplicate.into_inner(),
                failed.into_inner()
            );
        }
        Command::Reindex => println!("reindexed {} replays", store.reindex()?),
        Command::Purge { uploader } => {
            println!(
                "removed {} matches shared by {uploader}",
                store.purge_uploader(&uploader)?
            );
        }
        Command::Outliers => {
            let found = store.outliers(&Default::default())?;
            if found.is_empty() {
                println!("no outliers");
            }
            for o in found {
                println!(
                    "{}: {} won {:.0}% of {} games; everyone else {:.0}% (z = {:.1})",
                    o.name,
                    o.uploader,
                    o.win_rate * 100.0,
                    o.games,
                    o.others_win_rate * 100.0,
                    o.z
                );
            }
        }
        Command::Disputes => {
            let (corroborated, disputed) = store.corroboration()?;
            println!("{corroborated} corroborated, {disputed} disputed");
            for d in store.disputes()? {
                println!("{}", d.battle_id);
                for c in d.copies {
                    println!(
                        "  {} (player {}) at {}: {}",
                        c.uploaded_by,
                        c.player_id,
                        c.at,
                        &c.fingerprint[..12]
                    );
                }
            }
        }
        Command::Accounts { command } => match command {
            AccountsCommand::List => {
                for a in store.list_accounts()? {
                    println!(
                        "{}\tcreated {}\tsigned in {}\t{}\t{} active keys{}",
                        a.name,
                        a.created_at,
                        a.signed_in_at.as_deref().unwrap_or("-"),
                        if a.bound { "bound" } else { "not bound yet" },
                        a.active_keys,
                        a.disabled_at
                            .map(|at| format!("\tdisabled {at}"))
                            .unwrap_or_default()
                    );
                }
            }
            AccountsCommand::Disable { name } => {
                if !store.set_account_disabled(&name, true)? {
                    bail!("no account named {name:?}");
                }
                println!("disabled {name}");
            }
            AccountsCommand::Enable { name } => {
                if !store.set_account_disabled(&name, false)? {
                    bail!("no account named {name:?}");
                }
                println!("enabled {name}");
            }
        },
        Command::Keys { command } => match command {
            KeysCommand::Add { name } => {
                let key = store.create_key(&name)?;
                println!("{key}");
                eprintln!("Upload key for {name:?}. Store it now; it can't be shown again.");
            }
            KeysCommand::List => {
                for key in store.list_keys()? {
                    let status = match &key.revoked_at {
                        Some(at) => format!("revoked {at}"),
                        None => "active".into(),
                    };
                    println!("{}\tcreated {}\t{status}", key.name, key.created_at);
                }
            }
            KeysCommand::Revoke { name } => {
                if !store.revoke_key(&name)? {
                    bail!("no active key named {name:?}");
                }
                println!("revoked {name}");
            }
        },
    }
    Ok(())
}

/// The Unix socket passed by socket activation (`LISTEN_FDS=1`, fd 3), if any.
#[cfg(unix)]
fn activated_unix_listener() -> Result<Option<tokio::net::UnixListener>> {
    use std::os::fd::FromRawFd;
    let fds = std::env::var("LISTEN_FDS").ok();
    let pid_ok = std::env::var("LISTEN_PID")
        .map(|p| p == std::process::id().to_string())
        .unwrap_or(true);
    if fds.as_deref() != Some("1") || !pid_ok {
        return Ok(None);
    }
    // SAFETY: by the socket-activation protocol fd 3 is ours and open; it is
    // taken exactly once.
    let std_listener = unsafe { std::os::unix::net::UnixListener::from_raw_fd(3) };
    std_listener
        .set_nonblocking(true)
        .context("activated fd 3 is not a Unix listening socket")?;
    Ok(Some(tokio::net::UnixListener::from_std(std_listener)?))
}

#[cfg(not(unix))]
fn activated_unix_listener() -> Result<Option<tokio::net::TcpListener>> {
    Ok(None)
}
