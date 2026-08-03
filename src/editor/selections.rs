//! Selection state backed by `vim-buffer` primitives.
//!
//! This is intentionally a mirror of the legacy selection collection. Motion
//! parity will be added incrementally; this module establishes the new types
//! and the conversion boundary without changing the active editor yet.

use onig::Regex;
use text::{Point, Selection, SelectionGoal};
use vim_buffer::{BufferSnapshot, ByteOffset, SelectionId, SelectionKind, VimSelection};

pub trait VimSelectionText {
    fn text(&self, snapshot: &BufferSnapshot) -> Result<String, vim_buffer::BufferError>;
}

impl VimSelectionText for VimSelection {
    fn text(&self, snapshot: &BufferSnapshot) -> Result<String, vim_buffer::BufferError> {
        match self.operation_text(snapshot)? {
            vim_buffer::OperationText::Characterwise(text)
            | vim_buffer::OperationText::Linewise(text) => Ok(text),
            vim_buffer::OperationText::Blockwise(parts) => Ok(parts.join("\n")),
        }
    }
}

/// Collection equivalent of the legacy `SelectionCollection`, using the
/// editor-agnostic selection type from `vim-buffer`.
#[derive(Clone, Debug, PartialEq)]
pub struct VimSelectionCollection {
    pub id: usize,
    pub selections: Vec<VimSelection>,
    pub point: text::Point,
    pub search: String,
    pub anchor: Option<VimSelection>,
}

impl Default for VimSelectionCollection {
    fn default() -> Self {
        Self::new()
    }
}

pub trait VimSelectionMotions {
    fn with_head(
        &self,
        snapshot: &BufferSnapshot,
        offset: usize,
        extend: bool,
    ) -> Result<Self, vim_buffer::BufferError>
    where
        Self: Sized;
    fn move_left(
        &self,
        snapshot: &BufferSnapshot,
        extend: bool,
    ) -> Result<Self, vim_buffer::BufferError>
    where
        Self: Sized;
    fn move_right(
        &self,
        snapshot: &BufferSnapshot,
        extend: bool,
    ) -> Result<Self, vim_buffer::BufferError>
    where
        Self: Sized;
    fn move_up(
        &self,
        snapshot: &BufferSnapshot,
        extend: bool,
    ) -> Result<Self, vim_buffer::BufferError>
    where
        Self: Sized;
    fn move_down(
        &self,
        snapshot: &BufferSnapshot,
        extend: bool,
    ) -> Result<Self, vim_buffer::BufferError>
    where
        Self: Sized;
    fn move_to_start_of_line_non_space(
        &self,
        snapshot: &BufferSnapshot,
        extend: bool,
    ) -> Result<Self, vim_buffer::BufferError>
    where
        Self: Sized;
    fn move_to_start_of_document(
        &self,
        snapshot: &BufferSnapshot,
        extend: bool,
    ) -> Result<Self, vim_buffer::BufferError>
    where
        Self: Sized;
    fn move_to_end_of_document(
        &self,
        snapshot: &BufferSnapshot,
        extend: bool,
    ) -> Result<Self, vim_buffer::BufferError>
    where
        Self: Sized;
    fn move_to_start_of_line(
        &self,
        snapshot: &BufferSnapshot,
        extend: bool,
    ) -> Result<Self, vim_buffer::BufferError>
    where
        Self: Sized;
    fn move_to_end_of_line(
        &self,
        snapshot: &BufferSnapshot,
        extend: bool,
    ) -> Result<Self, vim_buffer::BufferError>
    where
        Self: Sized;
    fn move_to_line(
        &self,
        snapshot: &BufferSnapshot,
        row: u32,
        extend: bool,
    ) -> Result<Self, vim_buffer::BufferError>
    where
        Self: Sized;
}

impl VimSelectionMotions for VimSelection {
    fn with_head(
        &self,
        snapshot: &BufferSnapshot,
        offset: usize,
        extend: bool,
    ) -> Result<Self, vim_buffer::BufferError> {
        let offset = snapshot.validate_offset(vim_buffer::ByteOffset(offset))?;
        let head = snapshot.as_inner().anchor_before(offset);
        let inner = self.as_inner();
        Ok(Self::new(
            Selection {
                id: inner.id,
                start: head,
                end: if extend { inner.tail() } else { head },
                reversed: extend && inner.reversed,
                goal: SelectionGoal::None,
            },
            self.kind(),
            self.is_inclusive(),
        ))
    }

    fn move_left(
        &self,
        snapshot: &BufferSnapshot,
        extend: bool,
    ) -> Result<Self, vim_buffer::BufferError> {
        let offset = self.head_offset(snapshot)?.0;
        let target = if offset == 0 {
            0
        } else {
            let point = snapshot.offset_to_point(vim_buffer::ByteOffset(offset))?;
            if point.column > 0 {
                let line = snapshot
                    .as_inner()
                    .text_for_range(
                        snapshot.point_to_offset(text::Point::new(point.row, 0))?.0..offset,
                    )
                    .collect::<String>();
                offset.saturating_sub(line.chars().next_back().map_or(1, char::len_utf8))
            } else {
                snapshot
                    .point_to_offset(text::Point::new(
                        point.row - 1,
                        snapshot.line_len(point.row - 1)?,
                    ))?
                    .0
            }
        };
        self.with_head(snapshot, target, extend)
    }

    fn move_right(
        &self,
        snapshot: &BufferSnapshot,
        extend: bool,
    ) -> Result<Self, vim_buffer::BufferError> {
        let offset = self.head_offset(snapshot)?.0;
        let target = if offset >= snapshot.len_bytes() {
            snapshot.len_bytes()
        } else {
            let ch = snapshot
                .as_inner()
                .text_for_range(offset..snapshot.len_bytes())
                .collect::<String>();
            offset + ch.chars().next().map_or(1, char::len_utf8)
        };
        self.with_head(snapshot, target.min(snapshot.len_bytes()), extend)
    }

    fn move_up(
        &self,
        snapshot: &BufferSnapshot,
        extend: bool,
    ) -> Result<Self, vim_buffer::BufferError> {
        let point = snapshot.offset_to_point(self.head_offset(snapshot)?)?;
        let row = point.row.saturating_sub(1);
        let column = point.column.min(snapshot.line_len(row)?);
        self.with_head(
            snapshot,
            snapshot.point_to_offset(text::Point::new(row, column))?.0,
            extend,
        )
    }

    fn move_down(
        &self,
        snapshot: &BufferSnapshot,
        extend: bool,
    ) -> Result<Self, vim_buffer::BufferError> {
        let point = snapshot.offset_to_point(self.head_offset(snapshot)?)?;
        let row = (point.row + 1).min(snapshot.row_count().saturating_sub(1));
        let column = point.column.min(snapshot.line_len(row)?);
        self.with_head(
            snapshot,
            snapshot.point_to_offset(text::Point::new(row, column))?.0,
            extend,
        )
    }

    fn move_to_start_of_line_non_space(
        &self,
        snapshot: &BufferSnapshot,
        extend: bool,
    ) -> Result<Self, vim_buffer::BufferError> {
        let point = snapshot.offset_to_point(self.head_offset(snapshot)?)?;
        let line_start = snapshot.point_to_offset(text::Point::new(point.row, 0))?.0;
        let line_end = snapshot.line_len(point.row)? as usize + line_start;
        let line = snapshot
            .as_inner()
            .text_for_range(line_start..line_end)
            .collect::<String>();
        let whitespace = line.len() - line.trim_start().len();
        self.with_head(snapshot, line_start + whitespace, extend)
    }

    fn move_to_start_of_document(
        &self,
        snapshot: &BufferSnapshot,
        extend: bool,
    ) -> Result<Self, vim_buffer::BufferError> {
        self.with_head(snapshot, 0, extend)
    }

    fn move_to_end_of_document(
        &self,
        snapshot: &BufferSnapshot,
        extend: bool,
    ) -> Result<Self, vim_buffer::BufferError> {
        self.with_head(snapshot, snapshot.len_bytes(), extend)
    }

    fn move_to_start_of_line(
        &self,
        snapshot: &BufferSnapshot,
        extend: bool,
    ) -> Result<Self, vim_buffer::BufferError> {
        let point = snapshot.offset_to_point(self.head_offset(snapshot)?)?;
        self.with_head(
            snapshot,
            snapshot.point_to_offset(text::Point::new(point.row, 0))?.0,
            extend,
        )
    }

    fn move_to_end_of_line(
        &self,
        snapshot: &BufferSnapshot,
        extend: bool,
    ) -> Result<Self, vim_buffer::BufferError> {
        let point = snapshot.offset_to_point(self.head_offset(snapshot)?)?;
        self.with_head(
            snapshot,
            snapshot
                .point_to_offset(text::Point::new(point.row, snapshot.line_len(point.row)?))?
                .0,
            extend,
        )
    }

    fn move_to_line(
        &self,
        snapshot: &BufferSnapshot,
        row: u32,
        extend: bool,
    ) -> Result<Self, vim_buffer::BufferError> {
        let column = snapshot
            .offset_to_point(self.head_offset(snapshot)?)?
            .column;
        let row = row.min(snapshot.row_count().saturating_sub(1));
        self.with_head(
            snapshot,
            snapshot
                .point_to_offset(text::Point::new(row, column.min(snapshot.line_len(row)?)))?
                .0,
            extend,
        )
    }
}

impl VimSelectionCollection {
    pub fn new() -> Self {
        Self {
            id: 0,
            selections: Vec::new(),
            point: text::Point::new(0, 0),
            search: String::new(),
            anchor: None,
        }
    }

    pub fn first(&self) -> Option<&VimSelection> {
        self.selections.first()
    }

    pub fn last(&self) -> Option<&VimSelection> {
        self.selections.last()
    }

    pub fn add_caret(
        &mut self,
        snapshot: &BufferSnapshot,
        offset: usize,
    ) -> Result<VimSelection, vim_buffer::BufferError> {
        let selection = VimSelection::caret(
            SelectionId::new(self.id),
            snapshot,
            vim_buffer::ByteOffset(offset),
        )?;
        self.id += 1;
        self.point = snapshot.offset_to_point(vim_buffer::ByteOffset(offset))?;
        self.selections.push(selection.clone());
        Ok(selection)
    }

    pub fn replace(&mut self, selection: VimSelection) -> bool {
        if let Some(existing) = self
            .selections
            .iter_mut()
            .find(|existing| existing.id() == selection.id())
        {
            *existing = selection;
            true
        } else {
            false
        }
    }

    pub fn has_similar_cursor(
        &self,
        candidate: &VimSelection,
        snapshot: &BufferSnapshot,
    ) -> Result<bool, vim_buffer::BufferError> {
        let head = candidate.head_offset(snapshot)?.0;
        let anchor = snapshot.as_inner().offset_for_anchor(&candidate.anchor());
        Ok(self.selections.iter().any(|existing| {
            let existing_head = existing.head_offset(snapshot).ok().map(|offset| offset.0);
            let existing_anchor = snapshot.as_inner().offset_for_anchor(&existing.anchor());
            matches!(existing_head, Some(offset) if offset == head) && existing_anchor == anchor
                || matches!(existing_head, Some(offset) if offset == anchor)
                    && existing_anchor == head
        }))
    }

    pub fn text(&self, snapshot: &BufferSnapshot) -> Result<String, vim_buffer::BufferError> {
        let mut texts = Vec::new();
        for selection in &self.selections {
            let text = selection.text(snapshot)?;
            if !text.is_empty() {
                texts.push(text);
            }
        }
        Ok(texts.join("\n"))
    }

    pub fn rows_in_selection(
        &self,
        snapshot: &BufferSnapshot,
    ) -> Result<Option<(u32, u32)>, vim_buffer::BufferError> {
        let mut rows = None;
        for selection in &self.selections {
            let head = snapshot.offset_to_point(selection.head_offset(snapshot)?)?;
            let anchor = snapshot.offset_to_point(vim_buffer::ByteOffset(
                snapshot.as_inner().offset_for_anchor(&selection.anchor()),
            ))?;
            let range = (head.row.min(anchor.row), head.row.max(anchor.row));
            rows = Some(rows.map_or(range, |current: (u32, u32)| {
                (current.0.min(range.0), current.1.max(range.1))
            }));
        }
        Ok(rows)
    }

    pub fn clear_selections(&mut self, snapshot: &BufferSnapshot) {
        for selection in &mut self.selections {
            if let Ok(offset) = selection.head_offset(snapshot) {
                let anchor = snapshot.as_inner().anchor_before(offset.0);
                *selection = VimSelection::new(
                    Selection {
                        id: selection.id().get(),
                        start: anchor,
                        end: anchor,
                        reversed: false,
                        goal: SelectionGoal::None,
                    },
                    SelectionKind::Characterwise,
                    false,
                );
            }
        }
    }

    pub fn is_selected(
        &self,
        row: u32,
        column: u32,
        snapshot: &BufferSnapshot,
    ) -> (bool, bool, bool) {
        let point = Point::new(row, column);
        let mut selected = false;
        let mut selected_line = false;
        let mut at_cursor = false;
        for selection in &self.selections {
            let Ok(head) = selection
                .head_offset(snapshot)
                .and_then(|offset| snapshot.offset_to_point(offset))
            else {
                continue;
            };
            let anchor_offset = snapshot.as_inner().offset_for_anchor(&selection.anchor());
            let Ok(anchor) = snapshot.offset_to_point(ByteOffset(anchor_offset)) else {
                continue;
            };
            at_cursor |= point == head;
            let start = head.min(anchor);
            let end = head.max(anchor);
            selected |= point >= start && point < end;
            selected_line |= start.row < end.row && row >= start.row && row <= end.row;
        }
        (selected, selected_line, at_cursor)
    }

    pub fn has_selection(&self, snapshot: &BufferSnapshot) -> bool {
        self.selections.iter().any(|selection| {
            selection
                .head_offset(snapshot)
                .ok()
                .zip(
                    snapshot
                        .as_inner()
                        .can_resolve(&selection.anchor())
                        .then(|| snapshot.as_inner().offset_for_anchor(&selection.anchor())),
                )
                .is_some_and(|(head, anchor)| head.0 != anchor)
        })
    }

    fn update_each<F>(
        &mut self,
        snapshot: &BufferSnapshot,
        mut motion: F,
    ) -> Result<(), vim_buffer::BufferError>
    where
        F: FnMut(&VimSelection, &BufferSnapshot) -> Result<VimSelection, vim_buffer::BufferError>,
    {
        let current = self.selections.clone();
        for selection in current {
            let moved = motion(&selection, snapshot)?;
            self.replace(moved);
        }
        if let Some(selection) = self.first() {
            self.point = snapshot.offset_to_point(selection.head_offset(snapshot)?)?;
        }
        Ok(())
    }

    pub fn move_left(
        &mut self,
        snapshot: &BufferSnapshot,
        count: u32,
        extend: bool,
    ) -> Result<(), vim_buffer::BufferError> {
        for _ in 0..count {
            self.update_each(snapshot, |selection, snapshot| {
                selection.move_left(snapshot, extend)
            })?;
        }
        Ok(())
    }

    pub fn move_right(
        &mut self,
        snapshot: &BufferSnapshot,
        count: u32,
        extend: bool,
    ) -> Result<(), vim_buffer::BufferError> {
        for _ in 0..count {
            self.update_each(snapshot, |selection, snapshot| {
                selection.move_right(snapshot, extend)
            })?;
        }
        Ok(())
    }

    pub fn move_up(
        &mut self,
        snapshot: &BufferSnapshot,
        count: u32,
        extend: bool,
    ) -> Result<(), vim_buffer::BufferError> {
        for _ in 0..count {
            self.update_each(snapshot, |selection, snapshot| {
                selection.move_up(snapshot, extend)
            })?;
        }
        Ok(())
    }

    pub fn move_down(
        &mut self,
        snapshot: &BufferSnapshot,
        count: u32,
        extend: bool,
    ) -> Result<(), vim_buffer::BufferError> {
        for _ in 0..count {
            self.update_each(snapshot, |selection, snapshot| {
                selection.move_down(snapshot, extend)
            })?;
        }
        Ok(())
    }

    pub fn move_to_start_of_line_non_space(
        &mut self,
        snapshot: &BufferSnapshot,
        extend: bool,
    ) -> Result<(), vim_buffer::BufferError> {
        self.update_each(snapshot, |selection, snapshot| {
            selection.move_to_start_of_line_non_space(snapshot, extend)
        })
    }

    pub fn move_to_start_of_document(
        &mut self,
        snapshot: &BufferSnapshot,
        extend: bool,
    ) -> Result<(), vim_buffer::BufferError> {
        self.update_each(snapshot, |selection, snapshot| {
            selection.move_to_start_of_document(snapshot, extend)
        })
    }

    pub fn move_to_end_of_document(
        &mut self,
        snapshot: &BufferSnapshot,
        extend: bool,
    ) -> Result<(), vim_buffer::BufferError> {
        self.update_each(snapshot, |selection, snapshot| {
            selection.move_to_end_of_document(snapshot, extend)
        })
    }

    pub fn move_to_start_of_line(
        &mut self,
        snapshot: &BufferSnapshot,
        extend: bool,
    ) -> Result<(), vim_buffer::BufferError> {
        self.update_each(snapshot, |selection, snapshot| {
            selection.move_to_start_of_line(snapshot, extend)
        })
    }

    pub fn move_to_end_of_line(
        &mut self,
        snapshot: &BufferSnapshot,
        extend: bool,
    ) -> Result<(), vim_buffer::BufferError> {
        self.update_each(snapshot, |selection, snapshot| {
            selection.move_to_end_of_line(snapshot, extend)
        })
    }

    pub fn move_to_line(
        &mut self,
        snapshot: &BufferSnapshot,
        row: u32,
        extend: bool,
    ) -> Result<(), vim_buffer::BufferError> {
        self.update_each(snapshot, |selection, snapshot| {
            selection.move_to_line(snapshot, row, extend)
        })
    }

    pub fn move_to_previous_line(
        &mut self,
        snapshot: &BufferSnapshot,
        extend: bool,
    ) -> Result<(), vim_buffer::BufferError> {
        self.update_each(snapshot, |selection, snapshot| {
            let point = snapshot.offset_to_point(selection.head_offset(snapshot)?)?;
            selection.move_to_line(snapshot, point.row.saturating_sub(1), extend)
        })
    }

    pub fn move_to_next_line(
        &mut self,
        snapshot: &BufferSnapshot,
        extend: bool,
    ) -> Result<(), vim_buffer::BufferError> {
        self.update_each(snapshot, |selection, snapshot| {
            let point = snapshot.offset_to_point(selection.head_offset(snapshot)?)?;
            selection.move_to_line(
                snapshot,
                (point.row + 1).min(snapshot.row_count().saturating_sub(1)),
                extend,
            )
        })
    }

    pub fn move_to_start_of_previous_line(
        &mut self,
        snapshot: &BufferSnapshot,
        extend: bool,
    ) -> Result<(), vim_buffer::BufferError> {
        self.move_to_previous_line(snapshot, extend)?;
        self.move_to_start_of_line(snapshot, extend)
    }

    pub fn move_to_end_of_previous_line(
        &mut self,
        snapshot: &BufferSnapshot,
        extend: bool,
    ) -> Result<(), vim_buffer::BufferError> {
        self.move_to_previous_line(snapshot, extend)?;
        self.move_to_end_of_line(snapshot, extend)
    }

    pub fn move_to_start_of_next_line(
        &mut self,
        snapshot: &BufferSnapshot,
        extend: bool,
    ) -> Result<(), vim_buffer::BufferError> {
        self.move_to_next_line(snapshot, extend)?;
        self.move_to_start_of_line(snapshot, extend)
    }

    pub fn move_to_end_of_next_line(
        &mut self,
        snapshot: &BufferSnapshot,
        extend: bool,
    ) -> Result<(), vim_buffer::BufferError> {
        self.move_to_next_line(snapshot, extend)?;
        self.move_to_end_of_line(snapshot, extend)
    }

    pub fn find_character(
        &mut self,
        snapshot: &BufferSnapshot,
        count: u32,
        character: char,
        forward: bool,
        till: bool,
        extend: bool,
    ) -> Result<(), vim_buffer::BufferError> {
        let text = snapshot.chunks().collect::<String>();
        self.update_each(snapshot, |selection, snapshot| {
            let offset = selection.head_offset(snapshot)?.0;
            let mut matches = if forward {
                text[offset.min(text.len())..]
                    .char_indices()
                    .filter(|(_, ch)| *ch == character)
                    .map(|(index, _)| index + offset.min(text.len()))
                    .collect::<Vec<_>>()
            } else {
                text[..offset.min(text.len())]
                    .char_indices()
                    .filter(|(_, ch)| *ch == character)
                    .map(|(index, _)| index)
                    .collect::<Vec<_>>()
            };
            if !forward {
                matches.reverse();
            }
            let Some(mut target) = matches.into_iter().nth(count.saturating_sub(1) as usize) else {
                return Ok(selection.clone());
            };
            if till {
                let moved = if forward {
                    selection.move_left(snapshot, false)?
                } else {
                    selection.move_right(snapshot, false)?
                };
                target = moved.head_offset(snapshot)?.0;
            }
            selection.with_head(snapshot, target, extend)
        })
    }

    fn word_target(
        selection: &VimSelection,
        snapshot: &BufferSnapshot,
        forward: bool,
        end: bool,
        big: bool,
    ) -> Result<ByteOffset, vim_buffer::BufferError> {
        let text = snapshot.chunks().collect::<String>();
        let offset = selection.head_offset(snapshot)?.0.min(text.len());
        let is_word = |ch: char| {
            if big {
                !ch.is_whitespace()
            } else {
                ch.is_alphanumeric() || ch == '_'
            }
        };
        let chars = text.char_indices().collect::<Vec<_>>();
        let mut index = chars
            .iter()
            .position(|(index, _)| *index >= offset)
            .unwrap_or(chars.len());
        if forward {
            while index < chars.len() && !is_word(chars[index].1) {
                index += 1;
            }
            if end {
                while index + 1 < chars.len() && is_word(chars[index + 1].1) {
                    index += 1;
                }
            }
        } else {
            index = index.saturating_sub(1);
            while index > 0 && !is_word(chars[index].1) {
                index -= 1;
            }
            while index > 0 && is_word(chars[index - 1].1) {
                index -= 1;
            }
        }
        Ok(ByteOffset(
            chars.get(index).map_or(text.len(), |(offset, _)| *offset),
        ))
    }

    pub fn move_to_word(
        &mut self,
        snapshot: &BufferSnapshot,
        extend: bool,
    ) -> Result<(), vim_buffer::BufferError> {
        self.update_each(snapshot, |selection, snapshot| {
            selection.with_head(
                snapshot,
                Self::word_target(selection, snapshot, true, false, false)?.0,
                extend,
            )
        })
    }

    pub fn move_to_word_end(
        &mut self,
        snapshot: &BufferSnapshot,
        extend: bool,
    ) -> Result<(), vim_buffer::BufferError> {
        self.update_each(snapshot, |selection, snapshot| {
            selection.with_head(
                snapshot,
                Self::word_target(selection, snapshot, true, true, false)?.0,
                extend,
            )
        })
    }

    pub fn move_to_previous_word(
        &mut self,
        snapshot: &BufferSnapshot,
        extend: bool,
    ) -> Result<(), vim_buffer::BufferError> {
        self.update_each(snapshot, |selection, snapshot| {
            selection.with_head(
                snapshot,
                Self::word_target(selection, snapshot, false, false, false)?.0,
                extend,
            )
        })
    }

    pub fn move_to_big_word(
        &mut self,
        snapshot: &BufferSnapshot,
        extend: bool,
    ) -> Result<(), vim_buffer::BufferError> {
        self.update_each(snapshot, |selection, snapshot| {
            selection.with_head(
                snapshot,
                Self::word_target(selection, snapshot, true, false, true)?.0,
                extend,
            )
        })
    }

    pub fn move_to_big_word_end(
        &mut self,
        snapshot: &BufferSnapshot,
        extend: bool,
    ) -> Result<(), vim_buffer::BufferError> {
        self.update_each(snapshot, |selection, snapshot| {
            selection.with_head(
                snapshot,
                Self::word_target(selection, snapshot, true, true, true)?.0,
                extend,
            )
        })
    }

    pub fn move_to_previous_paragraph(
        &mut self,
        snapshot: &BufferSnapshot,
        extend: bool,
    ) -> Result<(), vim_buffer::BufferError> {
        self.update_each(snapshot, |selection, snapshot| {
            let mut row = snapshot
                .offset_to_point(selection.head_offset(snapshot)?)?
                .row;
            while row > 0 {
                row -= 1;
                if snapshot.line_len(row)? == 0 {
                    break;
                }
            }
            selection.move_to_line(snapshot, row, extend)
        })
    }

    pub fn move_to_next_paragraph(
        &mut self,
        snapshot: &BufferSnapshot,
        extend: bool,
    ) -> Result<(), vim_buffer::BufferError> {
        self.update_each(snapshot, |selection, snapshot| {
            let mut row = snapshot
                .offset_to_point(selection.head_offset(snapshot)?)?
                .row;
            while row + 1 < snapshot.row_count() {
                row += 1;
                if snapshot.line_len(row)? == 0 {
                    break;
                }
            }
            selection.move_to_line(snapshot, row, extend)
        })
    }

    fn replace_with_range(
        &mut self,
        snapshot: &BufferSnapshot,
        selection: &VimSelection,
        start: ByteOffset,
        end: ByteOffset,
        kind: SelectionKind,
        inclusive: bool,
    ) -> Result<(), vim_buffer::BufferError> {
        snapshot.validate_offset(start)?;
        snapshot.validate_offset(end)?;
        let start_anchor = snapshot.as_inner().anchor_before(start.0);
        let end_anchor = snapshot.as_inner().anchor_before(end.0);
        self.replace(VimSelection::new(
            Selection {
                id: selection.id().get(),
                start: start_anchor,
                end: end_anchor,
                reversed: end < start,
                goal: SelectionGoal::None,
            },
            kind,
            inclusive,
        ));
        Ok(())
    }

    pub fn select_linewise(
        &mut self,
        snapshot: &BufferSnapshot,
    ) -> Result<(), vim_buffer::BufferError> {
        let current = self.selections.clone();
        for selection in current {
            let range = selection.edit_ranges(snapshot)?.into_iter().next();
            let Some(range) = range else { continue };
            self.replace_with_range(
                snapshot,
                &selection,
                range.start,
                range.end,
                SelectionKind::Linewise,
                false,
            )?;
        }
        Ok(())
    }

    pub fn select_blockwise(
        &mut self,
        snapshot: &BufferSnapshot,
        inclusive: bool,
    ) -> Result<(), vim_buffer::BufferError> {
        let current = self.selections.clone();
        for selection in current {
            let start = snapshot.offset_to_point(selection.head_offset(snapshot)?)?;
            let end = snapshot.offset_to_point(ByteOffset(
                snapshot.as_inner().offset_for_anchor(&selection.anchor()),
            ))?;
            let start_offset = snapshot.point_to_offset(Point::new(
                start.row.min(end.row),
                start.column.min(end.column),
            ))?;
            let end_offset = snapshot.point_to_offset(Point::new(
                start.row.max(end.row),
                start.column.max(end.column),
            ))?;
            self.replace_with_range(
                snapshot,
                &selection,
                start_offset,
                end_offset,
                SelectionKind::Blockwise,
                inclusive,
            )?;
        }
        Ok(())
    }

    fn select_match(
        &mut self,
        snapshot: &BufferSnapshot,
        start: usize,
        end: usize,
    ) -> Result<bool, vim_buffer::BufferError> {
        let Some(selection) = self.first().cloned() else {
            return Ok(false);
        };
        self.replace_with_range(
            snapshot,
            &selection,
            ByteOffset(start),
            ByteOffset(end),
            SelectionKind::Characterwise,
            false,
        )?;
        Ok(true)
    }

    pub fn move_to_next_match(
        &mut self,
        snapshot: &BufferSnapshot,
        query: &str,
    ) -> Result<bool, vim_buffer::BufferError> {
        if query.is_empty() {
            return Ok(false);
        }
        let text = snapshot.chunks().collect::<String>();
        let head = self
            .first()
            .and_then(|selection| selection.head_offset(snapshot).ok())
            .map_or(0, |offset| offset.0.saturating_add(1));
        let found = text[head.min(text.len())..]
            .find(query)
            .map(|offset| offset + head.min(text.len()))
            .or_else(|| text[..head.min(text.len())].find(query));
        let Some(start) = found else { return Ok(false) };
        self.search = query.to_owned();
        self.select_match(snapshot, start, start + query.len())
    }

    pub fn move_to_previous_match(
        &mut self,
        snapshot: &BufferSnapshot,
        query: &str,
    ) -> Result<bool, vim_buffer::BufferError> {
        if query.is_empty() {
            return Ok(false);
        }
        let text = snapshot.chunks().collect::<String>();
        let head = self
            .first()
            .and_then(|selection| selection.head_offset(snapshot).ok())
            .map_or(text.len(), |offset| offset.0);
        let Some(start) = text[..head.min(text.len())].rfind(query) else {
            return Ok(false);
        };
        self.search = query.to_owned();
        self.select_match(snapshot, start, start + query.len())
    }

    pub fn move_to_next_pattern_match(
        &mut self,
        snapshot: &BufferSnapshot,
        regex: &Regex,
    ) -> Result<bool, vim_buffer::BufferError> {
        let text = snapshot.chunks().collect::<String>();
        let head = self
            .first()
            .and_then(|selection| selection.head_offset(snapshot).ok())
            .map_or(0, |offset| offset.0.saturating_add(1));
        let start_at = head.min(text.len());
        let found = regex
            .find(&text[start_at..])
            .map(|(start, end)| (start + start_at, end + start_at))
            .or_else(|| {
                regex
                    .find(&text[..start_at])
                    .map(|(start, end)| (start, end))
            });
        let Some((start, end)) = found else {
            return Ok(false);
        };
        self.select_match(snapshot, start, end)
    }

    pub fn move_to_previous_pattern_match(
        &mut self,
        snapshot: &BufferSnapshot,
        regex: &Regex,
    ) -> Result<bool, vim_buffer::BufferError> {
        let text = snapshot.chunks().collect::<String>();
        let head = self
            .first()
            .and_then(|selection| selection.head_offset(snapshot).ok())
            .map_or(text.len(), |offset| offset.0);
        let mut found = None;
        for (start, end) in regex.find_iter(&text[..head.min(text.len())]) {
            found = Some((start, end));
        }
        let Some((start, end)) = found else {
            return Ok(false);
        };
        self.select_match(snapshot, start, end)
    }

    pub fn selection_set(&self) -> Result<vim_buffer::SelectionSet, vim_buffer::BufferError> {
        let primary = self
            .selections
            .first()
            .map(|selection| selection.id())
            .ok_or(vim_buffer::BufferError::InvalidSelectionSet)?;
        vim_buffer::SelectionSet::new(primary, self.selections.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot() -> BufferSnapshot {
        let buffer = vim_buffer::Buffer::new(
            vim_buffer::BufferId::new(1).unwrap(),
            clock::ReplicaId::default(),
            "one\ntwo",
        );
        buffer.snapshot()
    }

    #[test]
    fn creates_vim_caret_and_selection_set() {
        let snapshot = snapshot();
        let mut selections = VimSelectionCollection::new();
        selections.add_caret(&snapshot, 0).unwrap();
        assert_eq!(
            selections.first().unwrap().kind(),
            SelectionKind::Characterwise
        );
        assert_eq!(selections.selection_set().unwrap().len(), 1);
    }

    #[test]
    fn clear_selections_keeps_carets() {
        let snapshot = snapshot();
        let mut selections = VimSelectionCollection::new();
        selections.add_caret(&snapshot, 0).unwrap();
        selections.clear_selections(&snapshot);
        assert!(!selections.has_selection(&snapshot));
    }

    #[test]
    fn vertical_and_first_non_space_motions_preserve_column() {
        let buffer = vim_buffer::Buffer::new(
            vim_buffer::BufferId::new(2).unwrap(),
            clock::ReplicaId::default(),
            "  one\n  two\nthree",
        );
        let snapshot = buffer.snapshot();
        let mut selections = VimSelectionCollection::new();
        selections.add_caret(&snapshot, 2).unwrap();

        selections.move_down(&snapshot, 1, false).unwrap();
        assert_eq!(
            selections
                .first()
                .unwrap()
                .head_offset(&snapshot)
                .unwrap()
                .0,
            8
        );

        selections
            .move_to_start_of_line_non_space(&snapshot, false)
            .unwrap();
        assert_eq!(
            selections
                .first()
                .unwrap()
                .head_offset(&snapshot)
                .unwrap()
                .0,
            8
        );
    }

    #[test]
    fn string_and_pattern_search_select_matches() {
        let buffer = vim_buffer::Buffer::new(
            vim_buffer::BufferId::new(3).unwrap(),
            clock::ReplicaId::default(),
            "alpha beta alpha",
        );
        let snapshot = buffer.snapshot();
        let mut selections = VimSelectionCollection::new();
        selections.add_caret(&snapshot, 0).unwrap();

        assert!(selections.move_to_next_match(&snapshot, "alpha").unwrap());
        assert_eq!(selections.text(&snapshot).unwrap(), "alpha");
        assert!(
            selections
                .move_to_previous_match(&snapshot, "alpha")
                .unwrap()
        );
        assert_eq!(selections.text(&snapshot).unwrap(), "alpha");
        assert!(
            selections
                .move_to_next_pattern_match(&snapshot, &Regex::new("beta").unwrap())
                .unwrap()
        );
        assert_eq!(selections.text(&snapshot).unwrap(), "beta");
    }

    #[test]
    fn linewise_and_blockwise_selections_expose_operation_text() {
        let buffer = vim_buffer::Buffer::new(
            vim_buffer::BufferId::new(4).unwrap(),
            clock::ReplicaId::default(),
            "abc\ndef",
        );
        let snapshot = buffer.snapshot();
        let mut selections = VimSelectionCollection::new();
        selections.add_caret(&snapshot, 1).unwrap();
        selections.select_linewise(&snapshot).unwrap();
        assert_eq!(selections.first().unwrap().kind(), SelectionKind::Linewise);
        assert_eq!(selections.text(&snapshot).unwrap(), "abc\n");

        selections.select_blockwise(&snapshot, true).unwrap();
        assert_eq!(selections.first().unwrap().kind(), SelectionKind::Blockwise);
        assert_eq!(selections.text(&snapshot).unwrap(), "b");
    }
}
