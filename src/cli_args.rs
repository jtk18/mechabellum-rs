use anyhow::Result;
use clap::{Parser, Subcommand};

use std::path::PathBuf;

use crate::gui;

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
    #[arg(short)]
    pub delete_original: bool,
    #[clap(subcommand)]
    pub subcommand: Option<AppSubcommands>,
}

#[derive(Debug, Subcommand, Clone)]
pub enum AppSubcommands {
    /// A Graphical User Interface (GUI) to navigate replays with
    Gui,
}

pub trait CommandDispatch {
    fn run_command(&self) -> Result<()>;
}

impl CommandDispatch for AppSubcommands {
    fn run_command(&self) -> Result<()> {
        match self {
            AppSubcommands::Gui => gui::start_gui(),
        }
    }
}
