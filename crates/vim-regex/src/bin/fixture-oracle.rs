use std::{env, error::Error, fs, path::PathBuf, process::ExitCode};

use vim_regex::{
    fixture::FixtureDocument,
    oracle::OracleConfig,
    workflow::{load_snapshot, refresh_snapshot, verify_snapshot},
};

fn main() -> ExitCode {
    match run() {
        Ok(message) => {
            println!("{message}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("fixture-oracle: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<String, Box<dyn Error>> {
    let mut arguments = env::args_os().skip(1);
    let command = arguments.next().ok_or(USAGE)?;
    let fixture_path = PathBuf::from(arguments.next().ok_or(USAGE)?);
    let snapshot_path = PathBuf::from(arguments.next().ok_or(USAGE)?);
    if arguments.next().is_some() {
        return Err(USAGE.into());
    }

    let fixture_json = fs::read_to_string(&fixture_path)?;
    let fixtures = FixtureDocument::from_json_str(&fixture_json)?;
    let config = OracleConfig::default();

    match command.to_str() {
        Some("refresh") => {
            refresh_snapshot(&fixtures, &snapshot_path, &config)?;
            Ok(format!(
                "refreshed {} fixture result(s) in {}",
                fixtures.fixtures.len(),
                snapshot_path.display()
            ))
        }
        Some("verify") => {
            let snapshot = load_snapshot(&snapshot_path)?;
            verify_snapshot(&fixtures, &snapshot, &config)?;
            Ok(format!(
                "verified {} fixture result(s) against {}",
                fixtures.fixtures.len(),
                snapshot_path.display()
            ))
        }
        _ => Err(USAGE.into()),
    }
}

const USAGE: &str = "usage: fixture-oracle <refresh|verify> <fixtures.json> <oracle-snapshot.json>";
