use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use mechabellum_server::{router, IngestOutcome, Store, UploadAuth};

use std::net::SocketAddr;
use std::path::PathBuf;
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
    },
    /// Load every .grbr/.xml replay in a directory into the database (read-only on the directory)
    Import { dir: PathBuf },
    /// Rebuild all stats from the stored replays, e.g. after a parser upgrade
    Reindex,
    /// Manage upload keys
    Keys {
        #[command(subcommand)]
        command: KeysCommand,
    },
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

    match args.command {
        Command::Serve {
            bind,
            require_auth,
            upload_token,
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
            let listener = tokio::net::TcpListener::bind(bind)
                .await
                .with_context(|| format!("binding {bind}"))?;
            log::info!(
                "listening on http://{bind} (db {}, uploads {})",
                args.db.display(),
                match auth {
                    UploadAuth::Open => "open",
                    UploadAuth::Required { .. } => "require a key",
                }
            );
            axum::serve(listener, router(store, auth)).await?;
        }
        Command::Import { dir } => {
            let (mut added, mut duplicate, mut failed) = (0, 0, 0);
            for entry in std::fs::read_dir(&dir)? {
                let path = entry?.path();
                let is_replay = path.extension().is_some_and(|e| e == "grbr" || e == "xml");
                if !is_replay {
                    continue;
                }
                match std::fs::read(&path)
                    .map_err(Into::into)
                    .and_then(|bytes| store.ingest(&bytes, &path, Some("import")))
                {
                    Ok(IngestOutcome::Added { .. }) => added += 1,
                    Ok(IngestOutcome::Duplicate { .. }) => duplicate += 1,
                    Err(e) => {
                        failed += 1;
                        log::warn!("{}: {e:#}", path.display());
                    }
                }
            }
            println!("added {added}, already present {duplicate}, failed {failed}");
        }
        Command::Reindex => println!("reindexed {} replays", store.reindex()?),
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
