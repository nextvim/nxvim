use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt, fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{
    fixture::{FIXTURE_SCHEMA_VERSION, FixtureDocument},
    oracle::{
        OracleConfig, OracleError, OracleResponse, PINNED_VIM_PATCH, PINNED_VIM_VERSION,
        run_fixture,
    },
};

pub const SNAPSHOT_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OracleSnapshot {
    pub snapshot_schema_version: u32,
    pub fixture_schema_version: u32,
    pub vim_version: u32,
    pub vim_patch: u32,
    pub results: BTreeMap<String, OracleResponse>,
}

impl OracleSnapshot {
    pub fn from_json_str(json: &str) -> Result<Self, WorkflowError> {
        let snapshot: Self = serde_json::from_str(json).map_err(WorkflowError::Json)?;
        snapshot.validate()?;
        Ok(snapshot)
    }

    pub fn to_pretty_json(&self) -> Result<String, WorkflowError> {
        self.validate()?;
        serde_json::to_string_pretty(self)
            .map(|mut json| {
                json.push('\n');
                json
            })
            .map_err(WorkflowError::Json)
    }

    pub fn validate(&self) -> Result<(), WorkflowError> {
        if self.snapshot_schema_version != SNAPSHOT_SCHEMA_VERSION {
            return Err(WorkflowError::InvalidSnapshot(format!(
                "unsupported snapshot schema {}; expected {SNAPSHOT_SCHEMA_VERSION}",
                self.snapshot_schema_version
            )));
        }
        if self.fixture_schema_version != FIXTURE_SCHEMA_VERSION {
            return Err(WorkflowError::InvalidSnapshot(format!(
                "snapshot uses fixture schema {}; expected {FIXTURE_SCHEMA_VERSION}",
                self.fixture_schema_version
            )));
        }
        if self.vim_version != PINNED_VIM_VERSION || self.vim_patch != PINNED_VIM_PATCH {
            return Err(WorkflowError::InvalidSnapshot(format!(
                "snapshot uses Vim {} patch {}; expected {} patch {}",
                self.vim_version, self.vim_patch, PINNED_VIM_VERSION, PINNED_VIM_PATCH
            )));
        }
        for (fixture_id, response) in &self.results {
            if response_fixture_id(response).is_some_and(|response_id| response_id != fixture_id) {
                return Err(WorkflowError::InvalidSnapshot(format!(
                    "result key {fixture_id:?} does not match response fixture id"
                )));
            }
        }
        Ok(())
    }
}

pub fn generate_snapshot(
    fixtures: &FixtureDocument,
    config: &OracleConfig,
) -> Result<OracleSnapshot, WorkflowError> {
    fixtures.validate().map_err(WorkflowError::Fixtures)?;
    let mut results = BTreeMap::new();
    for fixture in &fixtures.fixtures {
        let response = run_fixture(fixture, config).map_err(|source| WorkflowError::Oracle {
            fixture_id: fixture.id.clone(),
            source,
        })?;
        if response_fixture_id(&response).is_some_and(|response_id| response_id != fixture.id) {
            return Err(WorkflowError::InvalidSnapshot(format!(
                "oracle returned fixture id {:?} while running {:?}",
                response_fixture_id(&response),
                fixture.id
            )));
        }
        results.insert(fixture.id.clone(), response);
    }
    Ok(OracleSnapshot {
        snapshot_schema_version: SNAPSHOT_SCHEMA_VERSION,
        fixture_schema_version: fixtures.schema_version,
        vim_version: PINNED_VIM_VERSION,
        vim_patch: PINNED_VIM_PATCH,
        results,
    })
}

pub fn refresh_snapshot(
    fixtures: &FixtureDocument,
    output: &Path,
    config: &OracleConfig,
) -> Result<(), WorkflowError> {
    let snapshot = generate_snapshot(fixtures, config)?;
    let encoded = snapshot.to_pretty_json()?;
    let temporary = temporary_sibling(output);
    fs::write(&temporary, encoded).map_err(|source| WorkflowError::Write {
        path: temporary.clone(),
        source,
    })?;
    if let Err(source) = fs::rename(&temporary, output) {
        let _ = fs::remove_file(&temporary);
        return Err(WorkflowError::Write {
            path: output.to_owned(),
            source,
        });
    }
    Ok(())
}

pub fn verify_snapshot(
    fixtures: &FixtureDocument,
    expected: &OracleSnapshot,
    config: &OracleConfig,
) -> Result<(), WorkflowError> {
    expected.validate()?;
    let actual = generate_snapshot(fixtures, config)?;
    let fixture_ids: BTreeSet<_> = fixtures
        .fixtures
        .iter()
        .map(|fixture| fixture.id.as_str())
        .collect();
    let snapshot_ids: BTreeSet<_> = expected.results.keys().map(String::as_str).collect();
    if fixture_ids != snapshot_ids {
        return Err(WorkflowError::SnapshotMismatch {
            fixture_ids: fixture_ids.into_iter().map(str::to_owned).collect(),
            snapshot_ids: snapshot_ids.into_iter().map(str::to_owned).collect(),
            changed: Vec::new(),
        });
    }

    let changed: Vec<_> = expected
        .results
        .iter()
        .filter_map(|(id, response)| {
            (actual.results.get(id) != Some(response)).then_some(id.clone())
        })
        .collect();
    if changed.is_empty() {
        Ok(())
    } else {
        Err(WorkflowError::SnapshotMismatch {
            fixture_ids: Vec::new(),
            snapshot_ids: Vec::new(),
            changed,
        })
    }
}

pub fn load_snapshot(path: &Path) -> Result<OracleSnapshot, WorkflowError> {
    let json = fs::read_to_string(path).map_err(|source| WorkflowError::Read {
        path: path.to_owned(),
        source,
    })?;
    OracleSnapshot::from_json_str(&json)
}

fn response_fixture_id(response: &OracleResponse) -> Option<&str> {
    match response {
        OracleResponse::Match { fixture_id, .. }
        | OracleResponse::NoMatch { fixture_id, .. }
        | OracleResponse::Diagnostic { fixture_id, .. }
        | OracleResponse::Unsupported { fixture_id, .. }
        | OracleResponse::ProtocolError { fixture_id, .. } => Some(fixture_id),
        OracleResponse::IncompatibleVim { .. } => None,
    }
}

fn temporary_sibling(output: &Path) -> PathBuf {
    let mut name = output
        .file_name()
        .map_or_else(|| "snapshot".into(), |name| name.to_os_string());
    name.push(format!(".{}.tmp", std::process::id()));
    output.with_file_name(name)
}

#[derive(Debug)]
pub enum WorkflowError {
    Json(serde_json::Error),
    Fixtures(crate::fixture::FixtureValidationError),
    Oracle {
        fixture_id: String,
        source: OracleError,
    },
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    Write {
        path: PathBuf,
        source: std::io::Error,
    },
    InvalidSnapshot(String),
    SnapshotMismatch {
        fixture_ids: Vec<String>,
        snapshot_ids: Vec<String>,
        changed: Vec<String>,
    },
}

impl fmt::Display for WorkflowError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(error) => write!(formatter, "invalid snapshot JSON: {error}"),
            Self::Fixtures(error) => error.fmt(formatter),
            Self::Oracle { fixture_id, source } => {
                write!(
                    formatter,
                    "oracle failed for fixture {fixture_id:?}: {source}"
                )
            }
            Self::Read { path, source } => {
                write!(formatter, "cannot read {}: {source}", path.display())
            }
            Self::Write { path, source } => {
                write!(formatter, "cannot write {}: {source}", path.display())
            }
            Self::InvalidSnapshot(message) => write!(formatter, "invalid snapshot: {message}"),
            Self::SnapshotMismatch {
                fixture_ids,
                snapshot_ids,
                changed,
            } => {
                write!(formatter, "oracle snapshot does not match")?;
                if !fixture_ids.is_empty() || !snapshot_ids.is_empty() {
                    write!(
                        formatter,
                        "; fixture ids={fixture_ids:?}, snapshot ids={snapshot_ids:?}"
                    )?;
                }
                if !changed.is_empty() {
                    write!(formatter, "; changed fixtures={changed:?}")?;
                }
                Ok(())
            }
        }
    }
}

impl Error for WorkflowError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Json(error) => Some(error),
            Self::Fixtures(error) => Some(error),
            Self::Oracle { source, .. } => Some(source),
            Self::Read { source, .. } | Self::Write { source, .. } => Some(source),
            Self::InvalidSnapshot(_) | Self::SnapshotMismatch { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::fixture::FixtureDocument;

    use super::*;

    fn fixtures() -> FixtureDocument {
        FixtureDocument::from_json_str(include_str!("../fixtures/schema-v1.example.json")).unwrap()
    }

    #[test]
    fn generated_snapshot_is_deterministic_and_valid() {
        let first = generate_snapshot(&fixtures(), &OracleConfig::default()).unwrap();
        let second = generate_snapshot(&fixtures(), &OracleConfig::default()).unwrap();
        assert_eq!(first, second);
        let json = first.to_pretty_json().unwrap();
        assert_eq!(OracleSnapshot::from_json_str(&json).unwrap(), first);
    }

    #[test]
    fn verification_detects_changed_results_without_writing() {
        let fixtures = fixtures();
        let mut snapshot = generate_snapshot(&fixtures, &OracleConfig::default()).unwrap();
        snapshot.results.insert(
            "very_magic_capture".into(),
            OracleResponse::NoMatch {
                vim_version: PINNED_VIM_VERSION,
                fixture_id: "very_magic_capture".into(),
            },
        );
        let error = verify_snapshot(&fixtures, &snapshot, &OracleConfig::default()).unwrap_err();
        assert!(matches!(
            error,
            WorkflowError::SnapshotMismatch { changed, .. }
                if changed == vec!["very_magic_capture"]
        ));
    }
}
