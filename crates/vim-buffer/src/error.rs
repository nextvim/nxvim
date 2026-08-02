use crate::{BufferId, Point, TextRange};
use std::{error::Error, fmt, io};

#[derive(Debug)]
pub enum BufferError {
    UnknownBuffer(BufferId),
    Unmodifiable(BufferId),
    InvalidRange(TextRange),
    OffsetOutOfBounds(usize),
    InvalidPoint(Point),
    NotCharBoundary(usize),
    OverlappingEdits,
    ModifiedBuffer(BufferId),
    InvalidLifecycleTransition,
    InvalidSelectionSet,
    DecodeUtf8(std::str::Utf8Error),
    UnsupportedEncoding(String),
    UnsupportedFileFormat,
    Io(io::Error),
    NotImplemented(&'static str),
}

impl fmt::Display for BufferError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl Error for BufferError {}

impl From<io::Error> for BufferError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}
