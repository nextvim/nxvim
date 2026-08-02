use crate::{SelectionId, TextExtent, TextRange};
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edit {
    pub range: TextRange,
    pub replacement: Arc<str>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditOrigin {
    User,
    InsertMode,
    VimScript,
    Formatter,
    Reload,
    Undo,
    Redo,
    External,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlannedEdit {
    pub selection: Option<SelectionId>,
    pub edit: Edit,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditSummary {
    pub old_range: TextRange,
    pub new_range: TextRange,
    pub old_extent: TextExtent,
    pub new_extent: TextExtent,
}
