use crate::{SelectionId, TextExtent, TextRange};
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edit {
    pub range: TextRange,
    pub replacement: Arc<str>,
}

impl Edit {
    pub fn replace(range: TextRange, replacement: impl Into<Arc<str>>) -> Self {
        Self {
            range,
            replacement: replacement.into(),
        }
    }

    pub fn insert(offset: crate::ByteOffset, text: impl Into<Arc<str>>) -> Self {
        Self::replace(
            TextRange {
                start: offset,
                end: offset,
            },
            text,
        )
    }

    pub fn delete(range: TextRange) -> Self {
        Self::replace(range, Arc::<str>::from(""))
    }
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

pub(crate) fn summaries_for_patch(
    before: &text::BufferSnapshot,
    after: &text::BufferSnapshot,
    patch: &text::Patch<usize>,
) -> Vec<EditSummary> {
    patch
        .edits()
        .iter()
        .map(|edit| EditSummary {
            old_range: text_range(edit.old.clone()),
            new_range: text_range(edit.new.clone()),
            old_extent: extent(before, edit.old.clone()),
            new_extent: extent(after, edit.new.clone()),
        })
        .collect()
}

fn text_range(range: std::ops::Range<usize>) -> TextRange {
    TextRange {
        start: crate::ByteOffset(range.start),
        end: crate::ByteOffset(range.end),
    }
}

fn extent(snapshot: &text::BufferSnapshot, range: std::ops::Range<usize>) -> TextExtent {
    let summary = snapshot.text_summary_for_range::<text::TextSummary, _>(range);
    TextExtent {
        bytes: summary.len,
        lines: summary.lines.row,
        last_line_bytes: summary.lines.column,
    }
}
