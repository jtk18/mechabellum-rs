use clap::Parser;

use std::path::PathBuf;

/// Parse Mechabellum replays and gather stats
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Args {
    /// The replay file or directory containing replays
    pub file_or_dir: PathBuf,
}