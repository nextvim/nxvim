use crate::{BufferError, BufferSnapshot, ByteOffset, SelectionId, TextRange};
use text::{Anchor, Selection};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SelectionKind {
    #[default]
    Characterwise,
    Linewise,
    Blockwise,
}

/// Text shaped for a Vim register by an operator selection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OperationText {
    Characterwise(String),
    Linewise(String),
    Blockwise(Vec<String>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct VimSelection {
    inner: Selection<Anchor>,
    kind: SelectionKind,
    inclusive: bool,
}

impl VimSelection {
    pub fn new(inner: Selection<Anchor>, kind: SelectionKind, inclusive: bool) -> Self {
        Self {
            inner,
            kind,
            inclusive,
        }
    }

    pub fn id(&self) -> SelectionId {
        SelectionId::new(self.inner.id)
    }

    pub fn anchor(&self) -> Anchor {
        self.inner.tail()
    }

    pub fn head(&self) -> Anchor {
        self.inner.head()
    }

    pub fn kind(&self) -> SelectionKind {
        self.kind
    }

    pub fn is_inclusive(&self) -> bool {
        self.inclusive
    }

    pub fn as_inner(&self) -> &Selection<Anchor> {
        &self.inner
    }

    pub fn into_inner(self) -> Selection<Anchor> {
        self.inner
    }

    pub fn edit_ranges(&self, snapshot: &BufferSnapshot) -> Result<Vec<TextRange>, BufferError> {
        let inner = snapshot.as_inner();
        if !inner.can_resolve(&self.inner.start) || !inner.can_resolve(&self.inner.end) {
            return Err(BufferError::InvalidSelectionSet);
        }
        let head = inner.offset_for_anchor(&self.head());
        let anchor = inner.offset_for_anchor(&self.anchor());
        let (start, end) = if head <= anchor {
            (head, anchor)
        } else {
            (anchor, head)
        };
        match self.kind {
            SelectionKind::Characterwise => {
                let end = inclusive_end(snapshot, end, self.inclusive);
                Ok(vec![text_range(start, end)])
            }
            SelectionKind::Linewise => {
                let start_row = inner.offset_to_point(start).row;
                let end_row = inner.offset_to_point(end).row;
                let start = inner.point_to_offset(text::Point::new(start_row, 0));
                let end = if end_row + 1 < inner.row_count() {
                    inner.point_to_offset(text::Point::new(end_row + 1, 0))
                } else {
                    inner.len()
                };
                Ok(vec![text_range(start, end)])
            }
            SelectionKind::Blockwise => {
                let start_point = inner.offset_to_point(start);
                let end_point = inner.offset_to_point(end);
                let left = start_point.column.min(end_point.column);
                let right = start_point.column.max(end_point.column);
                let mut ranges = Vec::new();
                for row in start_point.row..=end_point.row {
                    let line_len = inner.line_len(row);
                    let start_point = text::Point::new(row, left.min(line_len));
                    let end_point = text::Point::new(row, right.min(line_len));
                    let start = snapshot.point_to_offset(start_point)?.0;
                    let end = snapshot.point_to_offset(end_point)?.0;
                    let include_endpoint = self.inclusive && end_point.column < line_len;
                    ranges.push(text_range(
                        start,
                        inclusive_end(snapshot, end, include_endpoint),
                    ));
                }
                Ok(ranges)
            }
        }
    }

    /// Resolves this selection into the payload Vim would place in a register.
    ///
    /// Linewise payloads always end in a newline, including the final line of a
    /// buffer without an existing final newline. Blockwise payloads retain one
    /// fragment per selected buffer row, including empty fragments on short rows.
    pub fn operation_text(&self, snapshot: &BufferSnapshot) -> Result<OperationText, BufferError> {
        let ranges = self.edit_ranges(snapshot)?;
        let mut fragments = Vec::with_capacity(ranges.len());
        for range in ranges {
            fragments.push(snapshot.chunks_for_range(range)?.collect::<String>());
        }
        match self.kind {
            SelectionKind::Characterwise => Ok(OperationText::Characterwise(
                fragments.into_iter().next().unwrap_or_default(),
            )),
            SelectionKind::Linewise => {
                let mut text = fragments.into_iter().next().unwrap_or_default();
                if !text.ends_with('\n') {
                    text.push('\n');
                }
                Ok(OperationText::Linewise(text))
            }
            SelectionKind::Blockwise => Ok(OperationText::Blockwise(fragments)),
        }
    }
}

fn inclusive_end(snapshot: &BufferSnapshot, end: usize, inclusive: bool) -> usize {
    if inclusive && end < snapshot.len_bytes() {
        snapshot
            .as_inner()
            .as_rope()
            .ceil_char_boundary(end.saturating_add(1))
    } else {
        end
    }
}

fn text_range(start: usize, end: usize) -> TextRange {
    TextRange {
        start: ByteOffset(start),
        end: ByteOffset(end),
    }
}
