use std::{env, error::Error, fs, path::PathBuf, process::ExitCode};

use vim_regex::{FixtureDocument, compare_oracle_snapshot, load_snapshot};

fn main() -> ExitCode {
    match run() {
        Ok(success) => {
            if success {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        Err(error) => {
            eprintln!("fixture-report: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<bool, Box<dyn Error>> {
    let mut arguments = env::args_os().skip(1);
    let fixture_path = PathBuf::from(arguments.next().ok_or(USAGE)?);
    let snapshot_path = PathBuf::from(arguments.next().ok_or(USAGE)?);
    if arguments.next().is_some() {
        return Err(USAGE.into());
    }

    let fixtures = FixtureDocument::from_json_str(&fs::read_to_string(&fixture_path)?)?;
    let snapshot = load_snapshot(&snapshot_path)?;
    let report = compare_oracle_snapshot(&fixtures, &snapshot);

    println!("corpus: {}", fixture_path.display());
    println!(
        "oracle: Vim {} patch {} ({})",
        snapshot.vim_version,
        snapshot.vim_patch,
        snapshot_path.display()
    );
    println!("report kind: oracle expectation agreement (not Rust compatibility)");
    println!("{}", report.summary());
    println!("by tier:");
    for (tier, counts) in &report.by_tier {
        println!(
            "  {tier:?}: {} passed, {} failed, {} unsupported, {} excluded",
            counts.passed, counts.failed, counts.unsupported, counts.excluded
        );
    }
    println!("by feature:");
    for (feature, counts) in &report.by_feature {
        println!(
            "  {feature}: {} passed, {} failed, {} unsupported, {} excluded",
            counts.passed, counts.failed, counts.unsupported, counts.excluded
        );
    }
    for case in report.cases.iter().filter(|case| {
        matches!(
            case.status,
            vim_regex::CaseStatus::Failed | vim_regex::CaseStatus::Unsupported
        )
    }) {
        println!(
            "  {:?} {}: {}",
            case.status,
            case.fixture_id,
            case.details.as_deref().unwrap_or("no details")
        );
    }
    Ok(report.is_success())
}

const USAGE: &str = "usage: fixture-report <fixtures.json> <oracle-snapshot.json>";
