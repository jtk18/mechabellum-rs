#![allow(dead_code)]

mod cli_args;
mod replays;

use cli_args::Args;
use replays::*;

use clap::Parser;

use std::fs;
use std::io::Write;
use std::path::PathBuf;

fn main() {
    env_logger::init();
    let args = Args::parse();
    let file_or_dir = args
        .file_or_dir
        .expect("You need to pass a '-- <FILE>' in this mode");

    let files: Vec<PathBuf> = if file_or_dir.is_dir() {
        let mut v = Vec::new();
        for maybe_entry in fs::read_dir(file_or_dir).expect("Unable to open directory for reading")
        {
            let entry = maybe_entry.expect("Could not unwrap directory entry");
            let path = entry.path();
            if path.is_file()
                && path
                    .extension()
                    .map(|s| s == "grbr" || s == "xml")
                    .unwrap_or(false)
            {
                v.push(path);
            }
        }
        v
    } else {
        vec![file_or_dir]
    };

    let (records, errors) = BattleRecord::new_from_files(&files, args.delete_original);

    println!(
        "Found {} total replays with {} successfully parsed and {} unsuccessfully",
        records.len() + errors.len(),
        records.len(),
        errors.len()
    );

    if errors.len() > 0 {
        println!("Saving errors to {}", args.error_output_file.display());
        let mut file = fs::File::create_new(args.error_output_file)
            .expect("Couldn't open error output file for writing!");
        let mut other_errors: Vec<ParseError> = Vec::new();
        for error in errors.into_iter() {
            if let ParseError::UnableToParse(_, doc, err) = error {
                let _ = file.write_all(doc.0.as_bytes());
                let _ = file.write_all(&[b'\n']);
                let _ = file.write_all(err.0.as_bytes());
                let _ = file.write_all(&[b'\n']);
            } else {
                other_errors.push(error);
            }
        }

        let _ = file.flush();
    }

    let overviews: Vec<BattleOverview> = records
        .iter()
        .filter_map(|r| {
            r.get_battle_overview()
                .map_err(|e| println!("Error parsing BattleOverview: {e}"))
                .ok()
        })
        .collect();

    println!("Overviews: {overviews:#?}");
}
