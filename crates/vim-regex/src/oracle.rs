use std::{
    error::Error,
    fmt, fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

use serde::{Deserialize, Serialize};
use wait_timeout::ChildExt;

use crate::fixture::{ByteRange, Fixture};

pub const PINNED_VIM_VERSION: u32 = 902;
pub const PINNED_VIM_PATCH: u32 = 843;

static REQUEST_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OracleConfig {
    pub vim: PathBuf,
    pub script: PathBuf,
    pub timeout: Duration,
}

impl Default for OracleConfig {
    fn default() -> Self {
        Self {
            vim: PathBuf::from("vim"),
            script: Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("oracle")
                .join("run-fixture.vim"),
            timeout: Duration::from_secs(5),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum OracleResponse {
    Match {
        vim_version: u32,
        fixture_id: String,
        range: ByteRange,
        /// `matchlist()` values: whole match followed by captures 1 through 9.
        capture_texts: Vec<String>,
    },
    NoMatch {
        vim_version: u32,
        fixture_id: String,
    },
    Diagnostic {
        vim_version: u32,
        fixture_id: String,
        /// Stable Vim error identifier such as `E54`; localized text is omitted.
        code: String,
    },
    Unsupported {
        vim_version: u32,
        fixture_id: String,
        reason: String,
    },
    IncompatibleVim {
        vim_version: u32,
        required_patch: u32,
        message: String,
    },
    ProtocolError {
        vim_version: u32,
        fixture_id: String,
        code: String,
    },
}

impl OracleResponse {
    pub fn vim_version(&self) -> u32 {
        match self {
            Self::Match { vim_version, .. }
            | Self::NoMatch { vim_version, .. }
            | Self::Diagnostic { vim_version, .. }
            | Self::Unsupported { vim_version, .. }
            | Self::IncompatibleVim { vim_version, .. }
            | Self::ProtocolError { vim_version, .. } => *vim_version,
        }
    }
}

pub fn run_fixture(
    fixture: &Fixture,
    config: &OracleConfig,
) -> Result<OracleResponse, OracleError> {
    let paths = TemporaryOracleFiles::new();
    let request = serde_json::to_vec(fixture).map_err(OracleError::SerializeRequest)?;
    fs::write(&paths.input, request).map_err(OracleError::WriteRequest)?;

    let mut child = Command::new(&config.vim)
        .args([
            "--clean",
            "--not-a-term",
            "-N",
            "-es",
            "-X",
            "-i",
            "NONE",
            "-u",
            "NONE",
            "-U",
            "NONE",
            "-S",
        ])
        .arg(&config.script)
        .env("VIM_REGEX_ORACLE_INPUT", &paths.input)
        .env("VIM_REGEX_ORACLE_OUTPUT", &paths.output)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|source| OracleError::Spawn {
            executable: config.vim.clone(),
            source,
        })?;

    let status = match child
        .wait_timeout(config.timeout)
        .map_err(OracleError::Wait)?
    {
        Some(status) => status,
        None => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(OracleError::Timeout(config.timeout));
        }
    };

    let output = fs::read(&paths.output).map_err(|source| OracleError::MissingOutput {
        status: status.code(),
        source,
    })?;
    let response: OracleResponse =
        serde_json::from_slice(&output).map_err(OracleError::InvalidResponse)?;

    match &response {
        OracleResponse::IncompatibleVim {
            vim_version,
            required_patch,
            ..
        } => Err(OracleError::IncompatibleVim {
            actual_version: *vim_version,
            required_version: PINNED_VIM_VERSION,
            required_patch: *required_patch,
        }),
        OracleResponse::ProtocolError { code, .. } => Err(OracleError::Protocol(code.clone())),
        _ if !status.success() => Err(OracleError::ProcessFailed(status.code())),
        _ if response.vim_version() != PINNED_VIM_VERSION => Err(OracleError::IncompatibleVim {
            actual_version: response.vim_version(),
            required_version: PINNED_VIM_VERSION,
            required_patch: PINNED_VIM_PATCH,
        }),
        _ => Ok(response),
    }
}

#[derive(Debug)]
pub enum OracleError {
    SerializeRequest(serde_json::Error),
    WriteRequest(std::io::Error),
    Spawn {
        executable: PathBuf,
        source: std::io::Error,
    },
    Wait(std::io::Error),
    Timeout(Duration),
    MissingOutput {
        status: Option<i32>,
        source: std::io::Error,
    },
    InvalidResponse(serde_json::Error),
    ProcessFailed(Option<i32>),
    IncompatibleVim {
        actual_version: u32,
        required_version: u32,
        required_patch: u32,
    },
    Protocol(String),
}

impl fmt::Display for OracleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SerializeRequest(error) => write!(formatter, "cannot serialize fixture: {error}"),
            Self::WriteRequest(error) => write!(formatter, "cannot write oracle request: {error}"),
            Self::Spawn { executable, source } => write!(
                formatter,
                "cannot start Vim oracle at {}: {source}",
                executable.display()
            ),
            Self::Wait(error) => write!(formatter, "cannot wait for Vim oracle: {error}"),
            Self::Timeout(timeout) => write!(formatter, "Vim oracle timed out after {timeout:?}"),
            Self::MissingOutput { status, source } => write!(
                formatter,
                "Vim oracle exited with {status:?} without valid output: {source}"
            ),
            Self::InvalidResponse(error) => {
                write!(formatter, "invalid Vim oracle response: {error}")
            }
            Self::ProcessFailed(status) => {
                write!(
                    formatter,
                    "Vim oracle process failed with status {status:?}"
                )
            }
            Self::IncompatibleVim {
                actual_version,
                required_version,
                required_patch,
            } => write!(
                formatter,
                "incompatible Vim {actual_version}; oracle requires version {required_version} patch {required_patch} exactly"
            ),
            Self::Protocol(code) => write!(formatter, "Vim oracle protocol error: {code}"),
        }
    }
}

impl Error for OracleError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::SerializeRequest(error) | Self::InvalidResponse(error) => Some(error),
            Self::WriteRequest(error) | Self::Wait(error) => Some(error),
            Self::Spawn { source, .. } | Self::MissingOutput { source, .. } => Some(source),
            Self::Timeout(_)
            | Self::ProcessFailed(_)
            | Self::IncompatibleVim { .. }
            | Self::Protocol(_) => None,
        }
    }
}

struct TemporaryOracleFiles {
    input: PathBuf,
    output: PathBuf,
}

impl TemporaryOracleFiles {
    fn new() -> Self {
        let id = REQUEST_ID.fetch_add(1, Ordering::Relaxed);
        let prefix = format!("vim-regex-oracle-{}-{id}", std::process::id());
        let directory = std::env::temp_dir();
        Self {
            input: directory.join(format!("{prefix}.request.json")),
            output: directory.join(format!("{prefix}.response.json")),
        }
    }
}

impl Drop for TemporaryOracleFiles {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.input);
        let _ = fs::remove_file(&self.output);
    }
}

#[cfg(test)]
mod tests {
    use crate::fixture::FixtureDocument;

    use super::*;

    #[test]
    fn pinned_vim_executes_the_checked_in_fixture() {
        let document =
            FixtureDocument::from_json_str(include_str!("../fixtures/schema-v1.example.json"))
                .unwrap();
        let response = run_fixture(&document.fixtures[0], &OracleConfig::default()).unwrap();
        assert_eq!(
            response,
            OracleResponse::Match {
                vim_version: PINNED_VIM_VERSION,
                fixture_id: "very_magic_capture".into(),
                range: ByteRange { start: 3, end: 7 },
                capture_texts: vec![
                    "word".into(),
                    "word".into(),
                    "".into(),
                    "".into(),
                    "".into(),
                    "".into(),
                    "".into(),
                    "".into(),
                    "".into(),
                    "".into(),
                ],
            }
        );
    }

    #[test]
    fn unsupported_fixture_state_is_structured() {
        let mut document =
            FixtureDocument::from_json_str(include_str!("../fixtures/schema-v1.example.json"))
                .unwrap();
        document.fixtures[0].features.push("visual-area".into());
        let response = run_fixture(&document.fixtures[0], &OracleConfig::default()).unwrap();
        assert!(matches!(response, OracleResponse::Unsupported { .. }));
    }

    #[test]
    fn missing_vim_is_reported_clearly() {
        let document =
            FixtureDocument::from_json_str(include_str!("../fixtures/schema-v1.example.json"))
                .unwrap();
        let config = OracleConfig {
            vim: PathBuf::from("definitely-not-a-vim-executable"),
            ..OracleConfig::default()
        };
        assert!(matches!(
            run_fixture(&document.fixtures[0], &config),
            Err(OracleError::Spawn { .. })
        ));
    }

    #[test]
    fn timeout_is_reported_and_process_is_killed() {
        let document =
            FixtureDocument::from_json_str(include_str!("../fixtures/schema-v1.example.json"))
                .unwrap();
        let config = OracleConfig {
            timeout: Duration::ZERO,
            ..OracleConfig::default()
        };
        assert!(matches!(
            run_fixture(&document.fixtures[0], &config),
            Err(OracleError::Timeout(_))
        ));
    }
}
