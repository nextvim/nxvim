use crate::WindowId;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiError {
    UnknownWindow(WindowId),
    WindowNotInLayout(WindowId),
    WindowNotVisible(WindowId),
    DuplicateWindowInLayout(WindowId),
    FloatingWindowInLayout(WindowId),
    EmptyLayout,
    CannotCloseFinalEditorWindow,
}

impl fmt::Display for UiError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownWindow(id) => write!(formatter, "unknown window {id}"),
            Self::WindowNotInLayout(id) => {
                write!(formatter, "window {id} is not in the tiled layout")
            }
            Self::WindowNotVisible(id) => write!(formatter, "window {id} is not visible"),
            Self::DuplicateWindowInLayout(id) => {
                write!(formatter, "window {id} occurs more than once in the layout")
            }
            Self::FloatingWindowInLayout(id) => {
                write!(
                    formatter,
                    "floating window {id} cannot be used in a tiled layout"
                )
            }
            Self::EmptyLayout => formatter.write_str("a tiled layout must contain a window"),
            Self::CannotCloseFinalEditorWindow => {
                formatter.write_str("cannot close the final editor window")
            }
        }
    }
}

impl std::error::Error for UiError {}

pub type UiResult<T> = Result<T, UiError>;
