use crate::{BufferError, BufferSnapshot, ByteOffset, TextRange};
use std::collections::HashMap;
use text::Anchor;

#[derive(Clone, Debug, Default)]
pub struct MarkSet {
    marks: HashMap<char, Anchor>,
}

impl MarkSet {
    pub fn get(&self, name: char) -> Option<&Anchor> {
        self.marks.get(&name)
    }

    pub fn set(&mut self, name: char, anchor: Anchor) -> Result<Option<Anchor>, BufferError> {
        if !is_buffer_mark(name) {
            return Err(BufferError::InvalidMark(name));
        }
        Ok(self.marks.insert(name, anchor))
    }

    pub fn remove(&mut self, name: char) -> Result<Option<Anchor>, BufferError> {
        if !is_buffer_mark(name) || name == '\'' {
            return Err(BufferError::InvalidMark(name));
        }
        Ok(self.marks.remove(&name))
    }

    pub fn clear_local(&mut self) {
        self.marks.retain(|name, _| !name.is_ascii_lowercase());
    }

    pub fn resolve(&self, name: char, snapshot: &BufferSnapshot) -> Option<ByteOffset> {
        let anchor = self.marks.get(&name)?;
        snapshot
            .as_inner()
            .can_resolve(anchor)
            .then(|| ByteOffset(snapshot.as_inner().offset_for_anchor(anchor)))
    }

    pub(crate) fn remove_marks_on_deleted_lines(
        &mut self,
        before: &BufferSnapshot,
        deleted_ranges: &[TextRange],
    ) {
        self.marks.retain(|_, anchor| {
            if !before.as_inner().can_resolve(anchor) {
                return false;
            }
            let offset = before.as_inner().offset_for_anchor(anchor);
            let point = before.as_inner().offset_to_point(offset);
            let line_start = before
                .as_inner()
                .point_to_offset(text::Point::new(point.row, 0));
            let line_end = if point.row + 1 < before.row_count() {
                before
                    .as_inner()
                    .point_to_offset(text::Point::new(point.row + 1, 0))
            } else {
                before.len_bytes()
            };
            !deleted_ranges.iter().any(|range| {
                !range.is_empty() && range.start.0 <= line_start && range.end.0 >= line_end
            })
        });
    }
}

fn is_buffer_mark(name: char) -> bool {
    name.is_ascii_lowercase() || matches!(name, '\'' | '[' | ']' | '<' | '>' | '^' | '.')
}

