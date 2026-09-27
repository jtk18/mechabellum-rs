#![allow(dead_code)]

mod cli_args;

use cli_args::{Args, Command};

use clap::Parser;

use std::fs;
use std::io::Write;
use std::path::PathBuf;

use mechabellum::*;

fn main() {
    env_logger::init();
    let args = Args::parse();

    if let Some(Command::Validate { dir }) = &args.command {
        let mut checked = 0;
        let mut flagged = 0;
        let mut counts = std::collections::BTreeMap::<&str, usize>::new();
        for entry in fs::read_dir(dir).expect("Unable to read directory") {
            let path = entry.expect("directory entry").path();
            if !path.extension().is_some_and(|e| e == "grbr" || e == "xml") {
                continue;
            }
            let Ok(record) = BattleRecord::new_from_file(&path, false) else {
                continue;
            };
            checked += 1;
            let problems = record.validate();
            if !problems.is_empty() {
                flagged += 1;
                for p in &problems {
                    *counts.entry(p.code).or_default() += 1;
                }
                println!("{}: {}", path.display(), problems[0].detail);
            }
        }
        println!("checked {checked}, flagged {flagged}, by check: {counts:?}");
        return;
    }

    if let Some(Command::Board { replay, round }) = &args.command {
        let record = BattleRecord::new_from_file(replay, false).expect("Unable to read replay");
        let detail = record.match_detail();
        let index = match round {
            Some(r) => r.checked_sub(1).expect("rounds start at 1"),
            None => detail.rounds.len().saturating_sub(1),
        };
        match board::Board::from_round(&detail, index) {
            Some(board) => println!("{board}"),
            None => {
                eprintln!("The replay has {} rounds.", detail.rounds.len());
                std::process::exit(1);
            }
        }
        return;
    }

    if let Some(Command::CheckBoard { file }) = &args.command {
        let text = if file.as_os_str() == "-" {
            let mut text = String::new();
            std::io::Read::read_to_string(&mut std::io::stdin(), &mut text)
                .expect("Unable to read stdin");
            text
        } else {
            fs::read_to_string(file).expect("Unable to read file")
        };
        match board::Board::parse(&text) {
            Ok(board) => {
                let units: usize = board.sides.iter().map(|s| s.units.len()).sum();
                println!("OK: {} sides, {units} units", board.sides.len());
            }
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        }
        return;
    }

    if let Some(Command::GenNames { game_dir, output }) = args.command {
        let names =
            GameNames::extract_from_install(&game_dir).expect("Unable to read game install");
        let count: usize = names.categories.values().map(|ids| ids.len()).sum();
        let json = serde_json::to_string_pretty(&names).expect("Unable to serialize names");
        fs::write(&output, json).expect("Unable to write names file");
        println!(
            "Wrote {count} names from version {} to {}",
            names.version,
            output.display()
        );
        return;
    }

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

    if !errors.is_empty() {
        println!("Saving errors to {}", args.error_output_file.display());
        let mut file = fs::File::create_new(args.error_output_file)
            .expect("Couldn't open error output file for writing!");
        let mut other_errors: Vec<ParseError> = Vec::new();
        for error in errors.into_iter() {
            if let ParseError::UnableToParse(_, doc, err) = error {
                let _ = file.write_all(doc.0.as_bytes());
                let _ = file.write_all(b"\n");
                let _ = file.write_all(err.0.as_bytes());
                let _ = file.write_all(b"\n");
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
