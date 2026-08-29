//! Vim runtime-path syntax source loading.

use std::ffi::OsStr;
use std::fmt;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::process::Command;

/// Ordered roots corresponding to Vim's `'runtimepath'` option.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RuntimePath {
    roots: Vec<PathBuf>,
}

/// A syntax source loaded from a Vim runtime directory.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyntaxSource {
    pub path: PathBuf,
    pub text: String,
}

#[derive(Debug)]
pub enum LoadError {
    InvalidFiletype(String),
    NotFound {
        filetype: String,
        searched: Vec<PathBuf>,
    },
    Read {
        path: PathBuf,
        source: io::Error,
    },
    VimDiscovery {
        executable: PathBuf,
        source: io::Error,
    },
    VimDiscoveryFailed {
        executable: PathBuf,
        status: Option<i32>,
        stderr: String,
    },
    EmptyVimRuntime {
        executable: PathBuf,
    },
}

impl RuntimePath {
    pub fn new(roots: impl IntoIterator<Item = PathBuf>) -> Self {
        Self {
            roots: roots.into_iter().collect(),
        }
    }

    pub fn roots(&self) -> &[PathBuf] {
        &self.roots
    }

    /// Uses `$VIMRUNTIME` as one explicit runtime root when it is set.
    pub fn from_environment() -> Option<Self> {
        std::env::var_os("VIMRUNTIME").map(|root| Self::new([PathBuf::from(root)]))
    }

    /// Asks a Vim executable for `$VIMRUNTIME`.
    ///
    /// This is intended for compatibility tests and development tools. Editor
    /// integration should construct the runtime path from its own configuration.
    pub fn discover_with_vim(executable: impl AsRef<OsStr>) -> Result<Self, LoadError> {
        let executable = PathBuf::from(executable.as_ref());
        let output = Command::new(&executable)
            .args([
                "--clean",
                "--not-a-term",
                "-Nu",
                "NONE",
                "-n",
                "-es",
                "-V1",
                "+echo $VIMRUNTIME",
                "+qa!",
            ])
            .output()
            .map_err(|source| LoadError::VimDiscovery {
                executable: executable.clone(),
                source,
            })?;

        if !output.status.success() {
            return Err(LoadError::VimDiscoveryFailed {
                executable,
                status: output.status.code(),
                stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
            });
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let runtime = stdout
            .lines()
            .chain(stderr.lines())
            .map(str::trim)
            .find(|line| !line.is_empty() && PathBuf::from(line).join("syntax").is_dir());
        let Some(runtime) = runtime else {
            return Err(LoadError::EmptyVimRuntime { executable });
        };
        Ok(Self::new([PathBuf::from(runtime)]))
    }

    /// Loads the first `syntax/{filetype}.vim` found in runtime-path order.
    pub fn load_syntax(&self, filetype: &str) -> Result<SyntaxSource, LoadError> {
        validate_filetype(filetype)?;
        let relative = PathBuf::from("syntax").join(format!("{filetype}.vim"));
        let searched = self
            .roots
            .iter()
            .map(|root| root.join(&relative))
            .collect::<Vec<_>>();

        for path in &searched {
            match fs::read_to_string(path) {
                Ok(text) => {
                    return Ok(SyntaxSource {
                        path: path.clone(),
                        text,
                    });
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(source) => {
                    return Err(LoadError::Read {
                        path: path.clone(),
                        source,
                    });
                }
            }
        }

        Err(LoadError::NotFound {
            filetype: filetype.to_owned(),
            searched,
        })
    }
}

fn validate_filetype(filetype: &str) -> Result<(), LoadError> {
    let valid = !filetype.is_empty()
        && !filetype.starts_with('.')
        && !filetype.contains("..")
        && filetype
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'));
    if valid {
        Ok(())
    } else {
        Err(LoadError::InvalidFiletype(filetype.to_owned()))
    }
}

impl fmt::Display for LoadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidFiletype(filetype) => write!(formatter, "invalid filetype {filetype:?}"),
            Self::NotFound { filetype, searched } => write!(
                formatter,
                "syntax/{filetype}.vim was not found; searched {}",
                searched
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Self::Read { path, source } => {
                write!(formatter, "failed to read {}: {source}", path.display())
            }
            Self::VimDiscovery { executable, source } => write!(
                formatter,
                "failed to run {} to discover Vim runtime: {source}",
                executable.display()
            ),
            Self::VimDiscoveryFailed {
                executable,
                status,
                stderr,
            } => write!(
                formatter,
                "{} failed while discovering Vim runtime (status {status:?}): {stderr}",
                executable.display()
            ),
            Self::EmptyVimRuntime { executable } => write!(
                formatter,
                "{} returned an empty Vim runtime path",
                executable.display()
            ),
        }
    }
}

impl std::error::Error for LoadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Read { source, .. } | Self::VimDiscovery { source, .. } => Some(source),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temporary_runtime() -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "vim-syntax-runtime-{}-{unique}",
            std::process::id()
        ))
    }

    #[test]
    fn loads_c_syntax_from_an_explicit_runtime() {
        let root = temporary_runtime();
        let syntax = root.join("syntax");
        fs::create_dir_all(&syntax).unwrap();
        fs::write(syntax.join("c.vim"), "syn keyword cType int char\n").unwrap();

        let source = RuntimePath::new([root.clone()]).load_syntax("c").unwrap();
        assert_eq!(source.path, root.join("syntax/c.vim"));
        assert_eq!(source.text, "syn keyword cType int char\n");

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_filetype_path_traversal() {
        let error = RuntimePath::default().load_syntax("../../etc/passwd");
        assert!(matches!(error, Err(LoadError::InvalidFiletype(_))));
    }

    /// Development smoke test for Vim's real `runtime/syntax/c.vim`.
    #[test]
    #[ignore = "requires a Vim executable and its runtime files"]
    fn loads_installed_vim_c_syntax() {
        let runtime = RuntimePath::from_environment()
            .map(Ok)
            .unwrap_or_else(|| RuntimePath::discover_with_vim("vim"))
            .unwrap();
        let source = runtime.load_syntax("c").unwrap();
        assert!(source.text.contains("cType"));
        assert!(source.path.ends_with(Path::new("syntax/c.vim")));
    }
}
