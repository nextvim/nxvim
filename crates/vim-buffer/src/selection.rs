use crate::{BufferError, BufferSnapshot, ByteOffset, TextRange};
use text::{Anchor, Bias, Selection};

pub trait SelectionExt {
    /// Resolves the selection into a characterwise `TextRange` (either inclusive or exclusive).
    fn edit_ranges(
        &self,
        snapshot: &BufferSnapshot,
        inclusive: bool,
    ) -> Result<Vec<TextRange>, BufferError>;

    /// Resolves the selection into a characterwise register payload string.
    fn operation_text(
        &self,
        snapshot: &BufferSnapshot,
        inclusive: bool,
    ) -> Result<String, BufferError>;
}

impl SelectionExt for Selection<Anchor> {
    fn edit_ranges(
        &self,
        snapshot: &BufferSnapshot,
        inclusive: bool,
    ) -> Result<Vec<TextRange>, BufferError> {
        let inner = snapshot.as_inner();
        if !inner.can_resolve(&self.start) || !inner.can_resolve(&self.end) {
            return Err(BufferError::InvalidSelectionSet);
        }
        let head = inner.offset_for_anchor(&self.head());
        let anchor = inner.offset_for_anchor(&self.tail());

        let (start, end) = if head <= anchor {
            (head, anchor)
        } else {
            (anchor, head)
        };

        let end = inclusive_end(snapshot, end, inclusive);

        Ok(vec![text_range(start, end)])
    }

    fn operation_text(
        &self,
        snapshot: &BufferSnapshot,
        inclusive: bool,
    ) -> Result<String, BufferError> {
        let ranges = self.edit_ranges(snapshot, inclusive)?;
        let text = ranges
            .first()
            .map(|range| {
                snapshot
                    .chunks_for_range(*range)
                    .unwrap()
                    .collect::<String>()
            })
            .unwrap_or_default();
        Ok(text)
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
