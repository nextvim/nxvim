use crate::{BufferId, ChangedTick, Revision};
use std::ops::Range;

#[derive(Clone)]
pub struct BufferSnapshot {
    pub(crate) id: BufferId,

    pub(crate) changedtick: ChangedTick,
    pub(crate) inner: text::BufferSnapshot,
}

impl BufferSnapshot {
    pub fn id(&self) -> BufferId {
        self.id
    }

    pub fn revision(&self) -> &Revision {
        &self.inner.version
    }

    pub fn changedtick(&self) -> ChangedTick {
        self.changedtick
    }

    pub fn len_bytes(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    pub fn row_count(&self) -> u32 {
        self.inner.row_count()
    }

    pub fn line_len(&self, row: u32) -> u32 {
        self.inner.line_len(row)
    }

    pub fn line_ending(&self) -> text::LineEnding {
        self.inner.line_ending()
    }

    pub fn text_for_range(&self, range: Range<usize>) -> text::Chunks<'_> {
        self.inner.text_for_range(range)
    }

    pub fn as_inner(&self) -> &text::BufferSnapshot {
        &self.inner
    }

    pub fn into_inner(self) -> text::BufferSnapshot {
        self.inner
    }
}
