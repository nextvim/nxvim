use std::fmt;

use vim_buffer::BufferError;
use vim_script::{runtime::RuntimeError, source::Diagnostic};

#[derive(Debug)]
pub enum EditorError {
    Buffer(BufferError),
    Diagnostics {
        stage: &'static str,
        diagnostics: Vec<Diagnostic>,
    },
    Runtime(RuntimeError),
    State(&'static str),
}

impl fmt::Display for EditorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Buffer(error) => write!(formatter, "buffer error: {error}"),
            Self::Diagnostics { stage, diagnostics } => {
                write!(formatter, "Vimscript {stage} failed")?;
                if let Some(first) = diagnostics.first() {
                    write!(formatter, ": {}", first.message)?;
                }
                Ok(())
            }
            Self::Runtime(error) => {
                if let Some(code) = &error.code {
                    write!(formatter, "{code}: {}", error.message)
                } else {
                    formatter.write_str(&error.message)
                }
            }
            Self::State(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for EditorError {}

impl From<BufferError> for EditorError {
    fn from(error: BufferError) -> Self {
        Self::Buffer(error)
    }
}

impl From<RuntimeError> for EditorError {
    fn from(error: RuntimeError) -> Self {
        Self::Runtime(error)
    }
}
