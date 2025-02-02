mod cli_args;
mod replays;

use cli_args::Args;
use replays::BattleRecord;

use clap::Parser;

use std::fs;
use std::path::PathBuf;

fn main() {
    env_logger::init();
    let args = Args::parse();

    let files: Vec<PathBuf> = if args.file_or_dir.is_dir() {
        let mut v = Vec::new();
        for maybe_entry in fs::read_dir(args.file_or_dir).expect("Unable to open directory for reading") {
            let entry = maybe_entry.expect("Could not unwrap directory entry");
            let path = entry.path();
            if path.is_file() && path.extension().map(|s| s == "grbr").unwrap_or(false) {
                v.push(path);
            }
        }
        v
    } else {
        vec![args.file_or_dir]
    };

    println!("FILES {:?}", files);

    let records = match BattleRecord::new_from_files(&files) {
        Ok(records) => records,
        Err(e) => panic!("One or more of the files passed in could not be parsed: {e}")
    };

    println!("Records {:#?}", records);
}
