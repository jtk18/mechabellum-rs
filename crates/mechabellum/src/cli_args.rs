use clap::{Parser, Subcommand};

use std::path::PathBuf;

/// Parse Mechabellum replays tester
/// Output some basic run statistics and save any parse errors to a file.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None, args_conflicts_with_subcommands = true)]
pub struct Args {
    #[command(subcommand)]
    pub command: Option<Command>,
    /// The replay file or directory containing replays
    pub file_or_dir: Option<PathBuf>,
    /// Path to output any parse errors to
    #[arg(short, long = "error-output", default_value = "parse_errors.mbpe")]
    pub error_output_file: PathBuf,
    /// Save xml file to save some space
    #[arg(short, long = "delete-original")]
    pub delete_original: bool,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Check every replay in a folder for inconsistencies (what the server rejects)
    Validate {
        /// Folder of .grbr/.xml replays (read only)
        dir: PathBuf,
    },
    /// Print a round of a replay as board code (see docs/board-code.md)
    Board {
        /// A .grbr or .xml replay
        replay: PathBuf,
        /// 1-based round; the last round by default
        #[arg(short, long)]
        round: Option<usize>,
    },
    /// Check board code for mistakes, reading it from a file or "-" for stdin
    CheckBoard { file: PathBuf },
    /// Extract the ID -> name table from a Mechabellum install
    GenNames {
        /// Game install directory (steamapps/common/Mechabellum). Defaults to the Steam install
        /// on this machine.
        #[arg(long, default_value_os_t = mechabellum::install::default_game_dir())]
        game_dir: PathBuf,
        /// Where to write the table
        #[arg(short, long, default_value = concat!(env!("CARGO_MANIFEST_DIR"), "/data/names.json"))]
        output: PathBuf,
    },
}
