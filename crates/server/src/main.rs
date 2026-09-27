use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use mechabellum_server::{router, IngestOutcome, Store};

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
        /// Require `Authorization: Bearer <token>` on uploads
        #[arg(long, env = "MECHABELLUM_UPLOAD_TOKEN", hide_env_values = true)]
        upload_token: Option<String>,
    },
    /// Load every .grbr/.xml replay in a directory into the database (read-only on the directory)
    Import { dir: PathBuf },
    /// Rebuild all stats from the stored replays, e.g. after a parser upgrade
    Reindex,
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let args = Args::parse();
    let store = Arc::new(Store::open(&args.db)?);

    match args.command {
        Command::Serve { bind, upload_token } => {
            let listener = tokio::net::TcpListener::bind(bind)
                .await
                .with_context(|| format!("binding {bind}"))?;
            log::info!(
                "listening on http://{bind} (db {}, uploads {})",
                args.db.display(),
                if upload_token.is_some() {
                    "need a token"
                } else {
                    "open"
                }
            );
            axum::serve(listener, router(store, upload_token)).await?;
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
                    .and_then(|bytes| store.ingest(&bytes, &path))
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
    }
    Ok(())
}
