use clap::Parser;

use std::path::PathBuf;

/// Parse Mechabellum replays tester
/// Output some basic run statistics and save any parse errors to a file.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None, args_conflicts_with_subcommands = true)]
pub struct Args {
    /// The replay file or directory containing replays
    pub file_or_dir: Option<PathBuf>,
    /// Path to output any parse errors to
    #[arg(short, long = "error-output", default_value = "parse_errors.mbpe")]
    pub error_output_file: PathBuf,
    /// Save xml file to save some space
    #[arg(short, long = "delete-original")]
    pub delete_original: bool,
}
