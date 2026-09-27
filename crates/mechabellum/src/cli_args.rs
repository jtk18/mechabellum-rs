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
    /// Extract the ID -> name table from a Mechabellum install
    GenNames {
        /// Game install directory (contains Mechabellum_Data and ProjectDatas)
        #[arg(
            long,
            default_value = r"C:\Program Files (x86)\Steam\steamapps\common\Mechabellum"
        )]
        game_dir: PathBuf,
        /// Where to write the table
        #[arg(short, long, default_value = concat!(env!("CARGO_MANIFEST_DIR"), "/data/names.json"))]
        output: PathBuf,
    },
}
