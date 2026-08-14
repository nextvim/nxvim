use crate::movement::Motions;
use crate::search::compile;
use crate::{BufferError, SelectionId};
use onig::Regex;
use std::cmp::Ordering;
use std::collections::HashSet;
use std::sync::Arc;
use sum_tree::Bias;
use text::{Anchor, Buffer, Point, Selection, SelectionGoal, ToOffset, ToPoint};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SelectionCellState {
    pub selected_cell: bool,
    pub selected_line: bool,
    pub at_cursor_head: bool,
    pub at_primary_cursor_head: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SelectionSet {
    pub id: usize,            // Auto-increment counter for generating unique selection IDs
    pub primary: SelectionId, // Primary selection ID
    pub selections: Vec<Selection<Anchor>>,
    pub point: Point,
    pub search: String,
    pub regex: Option<Arc<Regex>>,
    pub anchor: Option<Selection<Anchor>>,
}

// Ensure compatibility with codebases importing SelectionCollection
pub type SelectionCollection = SelectionSet;

impl SelectionSet {
    /// Creates a new empty `SelectionSet`
    pub fn new() -> Self {
        SelectionSet {
            id: 0,
            primary: SelectionId::new(0),
            selections: Vec::new(),
            point: Point { row: 0, column: 0 },
            search: "".to_string(),
            regex: None,
            anchor: None,
        }
    }

    /// Creates a `SelectionSet` from a list of selections, validating invariants.
    pub fn from_selections(
        primary: SelectionId,
        selections: Vec<Selection<Anchor>>,
    ) -> Result<Self, BufferError> {
        let mut ids = HashSet::with_capacity(selections.len());
        let valid = selections
            .first()
            .is_some_and(|selection| SelectionId::new(selection.id) == primary)
            && selections
                .iter()
                .all(|selection| ids.insert(SelectionId::new(selection.id)));
        if !valid {
            return Err(BufferError::InvalidSelectionSet);
        }
        let max_id = selections
            .iter()
            .map(|s| s.id)
            .max()
            .map_or(0, |max_id| max_id + 1);
        Ok(Self {
            id: max_id,
            primary,
            selections,
            point: Point { row: 0, column: 0 },
            search: "".to_string(),
            regex: None,
            anchor: None,
        })
    }

    pub fn primary_id(&self) -> SelectionId {
        self.primary
    }

    pub fn primary(&self) -> &Selection<Anchor> {
        self.selections
            .first()
            .expect("SelectionSet invariant requires a primary selection")
    }

    pub fn selections(&self) -> &[Selection<Anchor>] {
        &self.selections
    }

    pub fn primary_selection(&self) -> &Selection<Anchor> {
        self.primary()
    }

    pub fn replace_primary(&mut self, selection: Selection<Anchor>) -> Result<(), BufferError> {
        if SelectionId::new(selection.id) != self.primary {
            return Err(BufferError::InvalidSelectionSet);
        }
        self.selections[0] = selection;
        Ok(())
    }

    pub fn update(&mut self, _buffer: &Buffer, selection: &Selection<Anchor>) {
        let id = selection.id;
        // 1. Try direct index lookup
        if id < self.selections.len() && self.selections[id].id == id {
            self.selections[id] = selection.clone();
            return;
        }
        // 2. Try nearby indices (in case selections were shifted/reordered slightly)
        let len = self.selections.len();
        if len > 0 {
            let guess = id.min(len - 1);
            let start_idx = guess.saturating_sub(4);
            let end_idx = (guess + 4).min(len);
            for i in start_idx..end_idx {
                if self.selections[i].id == id {
                    self.selections[i] = selection.clone();
                    return;
                }
            }
        }
        // 3. Fallback to full search
        if let Some(selected) = self.selections.iter_mut().find(|s| s.id == id) {
            *selected = selection.clone();
        }
    }

    pub fn len(&self) -> usize {
        self.selections.len()
    }

    pub fn is_empty(&self) -> bool {
        self.selections.is_empty()
    }

    // --- Ported SelectionCollection Methods ---

    pub fn first(&self) -> Option<&Selection<Anchor>> {
        self.selections.first()
    }

    pub fn last(&self) -> Option<&Selection<Anchor>> {
        self.selections.last()
    }

    pub fn has_similar_cursor(&self, cursor: &Selection<Anchor>, buffer: &Buffer) -> bool {
        let head = buffer.offset_for_anchor(&cursor.head());
        let tail = buffer.offset_for_anchor(&cursor.tail());

        self.selections.iter().any(|existing| {
            let existing_head = buffer.offset_for_anchor(&existing.head());
            let existing_tail = buffer.offset_for_anchor(&existing.tail());

            (existing_head == head && existing_tail == tail)
                || (existing_head == tail && existing_tail == head)
        })
    }

    pub fn text(&self, buffer: &Buffer) -> String {
        self.selections
            .iter()
            .map(|selection| selection.text(buffer))
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn rows_in_selection(&self, buffer: &Buffer) -> (u32, u32) {
        let mut start: u32 = buffer.row_count();
        let mut end: u32 = 0;
        for cursor in self.selections.iter() {
            let mut rows = [
                cursor.start.to_point(buffer).row,
                cursor.end.to_point(buffer).row,
            ];
            rows.sort();
            let row_start = rows[0];
            let row_end = rows[1];
            start = std::cmp::min(row_start, start);
            end = std::cmp::max(row_end, end);
        }

        return (start, end);
    }

    pub fn add(&mut self, buffer: &Buffer, offset: usize) -> Selection<Anchor> {
        let sel = Selection {
            id: self.id,
            start: buffer.anchor_at(offset, Bias::Left),
            end: buffer.anchor_at(offset, Bias::Left),
            reversed: false,
            goal: SelectionGoal::None,
        };
        if self.selections.is_empty() {
            self.primary = SelectionId::new(self.id);
        }
        self.selections.push(sel.clone());
        self.id += 1;
        sel
    }

    pub fn begin_block(&mut self, buffer: &Buffer) {
        if let Some(first) = self.first().cloned() {
            self.anchor = Some(first);
            self.sync_block(buffer);
        }
    }

    pub fn sync_block(&mut self, buffer: &Buffer) {
        if self.selections.is_empty() {
            return;
        }

        let Some(anchor_sel) = self.anchor.clone() else {
            return;
        };
        let first_sel = self.selections[0].clone();

        // Compute row and column bounds from both selections' heads and tails
        let mut rows = [
            anchor_sel.start.to_point(buffer).row,
            anchor_sel.end.to_point(buffer).row,
            first_sel.start.to_point(buffer).row,
            first_sel.end.to_point(buffer).row,
        ];
        rows.sort();
        let row_start = rows[0];
        let row_end = rows[3];

        let mut cols = [
            anchor_sel.start.to_point(buffer).column,
            anchor_sel.end.to_point(buffer).column,
            first_sel.start.to_point(buffer).column,
            first_sel.end.to_point(buffer).column,
        ];
        cols.sort();
        let col_start = cols[0];
        let col_end = cols[3];

        let first_id = first_sel.id;
        let first_row = first_sel.head().to_point(buffer).row;
        let cursor_col = first_sel.head().to_point(buffer).column;
        let anchor_col = anchor_sel.start.to_point(buffer).column;
        let reversed = cursor_col < anchor_col;

        // Remove selections that are outside the block row range, except the first selection
        self.selections.retain(|sel| {
            if sel.id == first_id {
                return true;
            }
            let row = sel.head().to_point(buffer).row;
            row >= row_start && row <= row_end
        });

        // Ensure a selection exists on each row within the range (inclusive), except the first row
        for row in row_start..=row_end {
            if row == first_row {
                continue;
            }

            // Find an existing selection on this row (not the first)
            let existing_idx = self
                .selections
                .iter()
                .position(|s| s.id != first_id && s.head().to_point(buffer).row == row);

            let line_len = buffer.line_len(row);
            let s_col = col_start.min(line_len);
            let e_col = col_end.min(line_len);

            let start_pt = Point { row, column: s_col };
            let end_pt = Point { row, column: e_col };
            let start_anchor = buffer.anchor_at(start_pt.to_offset(buffer), Bias::Left);
            let end_anchor = buffer.anchor_at(end_pt.to_offset(buffer), Bias::Left);

            if let Some(idx) = existing_idx {
                let id = self.selections[idx].id;
                self.selections[idx] = Selection {
                    id,
                    start: start_anchor,
                    end: end_anchor,
                    reversed,
                    goal: SelectionGoal::None,
                };
            } else {
                let id = self.id;
                self.id += 1;
                self.selections.push(Selection {
                    id,
                    start: start_anchor,
                    end: end_anchor,
                    reversed,
                    goal: SelectionGoal::None,
                });
            }
        }

        // Finally, update the first selection so it conforms to the block at its row
        let line_len = buffer.line_len(first_row);
        let s_col = col_start.min(line_len);
        let e_col = col_end.min(line_len);
        let start_pt = Point {
            row: first_row,
            column: s_col,
        };
        let end_pt = Point {
            row: first_row,
            column: e_col,
        };
        let start_anchor = buffer.anchor_at(start_pt.to_offset(buffer), Bias::Left);
        let end_anchor = buffer.anchor_at(end_pt.to_offset(buffer), Bias::Left);
        self.selections[0] = Selection {
            id: first_id,
            start: start_anchor,
            end: end_anchor,
            reversed,
            goal: SelectionGoal::None,
        };
    }

    pub fn end_block(&mut self) {
        self.anchor = None;
    }

    pub fn begin_line(&mut self, buffer: &Buffer) {
        self.clear(buffer);
        if let Some(first) = self.first().cloned() {
            self.anchor = Some(first);
            self.sync_line(buffer);
        }
    }

    pub fn sync_line(&mut self, buffer: &Buffer) {
        let Some(current) = self.first().cloned() else {
            return;
        };
        let Some(anchor) = self.anchor.as_ref() else {
            return;
        };

        let head = current.head().to_point(buffer);
        let tail = anchor.head().to_point(buffer);
        let upper_row = head.row.min(tail.row);
        let lower_row = head.row.max(tail.row);

        let upper = Point {
            row: upper_row,
            column: 0,
        };
        let lower = Point {
            row: lower_row,
            column: buffer.line_len(lower_row),
        };
        let upper_anchor = buffer.anchor_at(upper.to_offset(buffer), Bias::Left);
        let lower_anchor = buffer.anchor_at(lower.to_offset(buffer), Bias::Left);

        // Keep the endpoint on the moving cursor's row as the head.
        let reversed = head.row < tail.row;
        self.selections.truncate(1);
        self.selections[0] = Selection {
            id: current.id,
            start: upper_anchor,
            end: lower_anchor,
            reversed,
            goal: SelectionGoal::None,
        };
    }

    pub fn end_line(&mut self) {
        self.anchor = None;
    }

    pub fn clear(&mut self, buffer: &Buffer) {
        self.clear_selections(buffer);
        if let Some(first) = self.first().cloned() {
            self.selections.clear();
            self.selections.push(first);
        }
    }

    pub fn is_selected(&self, row: u32, column: u32, buffer: &Buffer) -> SelectionCellState {
        ResolvedSelectionSet::new(self, buffer).is_selected(row, column)
    }

    pub fn has_selection(&self, buffer: &Buffer) -> bool {
        for cursor in self.selections.iter() {
            if cursor.head().cmp(&cursor.tail(), &buffer) != Ordering::Equal {
                return true;
            }
        }
        return false;
    }

    pub fn clear_selections(&mut self, buffer: &Buffer) {
        for cursor in self.selections.clone().iter() {
            self.update(
                buffer,
                &Selection {
                    id: cursor.id,
                    start: cursor.head(),
                    end: cursor.head(),
                    reversed: false,
                    goal: SelectionGoal::None,
                },
            );
        }
    }

    pub fn move_left(&mut self, anchor: bool, count: u32, buffer: &Buffer) {
        for _ in 0..count {
            let cursors = self.selections.clone();
            for cursor in cursors.iter() {
                let next = cursor.clone().move_left_once(anchor, buffer);
                self.point = next.head().to_point(buffer);
                self.update(buffer, &next);
            }
        }
    }

    pub fn move_right(&mut self, anchor: bool, count: u32, buffer: &Buffer) {
        for _ in 0..count {
            let cursors = self.selections.clone();
            for cursor in cursors.iter() {
                let next = cursor.clone().move_right_once(anchor, buffer);
                self.point = next.head().to_point(buffer);
                self.update(buffer, &next);
            }
        }
    }

    pub fn move_up(&mut self, anchor: bool, count: u32, buffer: &Buffer) {
        for _ in 0..count {
            let cursors = self.selections.clone();
            for cursor in cursors.iter() {
                let next = cursor.move_up_once(anchor, self.point.column, buffer);
                self.update(buffer, &next);
            }
        }
    }

    pub fn move_down(&mut self, anchor: bool, count: u32, buffer: &Buffer) {
        for _ in 0..count {
            let cursors = self.selections.clone();
            for cursor in cursors.iter() {
                let next = cursor.move_down_once(anchor, self.point.column, buffer);
                self.update(buffer, &next);
            }
        }
    }

    pub fn move_to_start_of_line(&mut self, anchor: bool, buffer: &Buffer) {
        let cursors = self.selections.clone();
        for cursor in cursors.iter() {
            let next = cursor.clone().move_to_start_of_line(anchor, buffer);
            self.update(buffer, &next);
        }
    }

    pub fn move_to_start_of_line_non_space(&mut self, anchor: bool, buffer: &Buffer) {
        let cursors = self.selections.clone();
        for cursor in cursors.iter() {
            let next = cursor
                .clone()
                .move_to_start_of_line_non_space(anchor, buffer);
            self.update(buffer, &next);
        }
    }

    pub fn move_to_start_of_previous_line(&mut self, anchor: bool, buffer: &Buffer) {
        let cursors = self.selections.clone();
        for cursor in cursors.iter() {
            let next = cursor
                .clone()
                .move_to_start_of_previous_line(anchor, buffer);
            self.update(buffer, &next);
        }
    }

    pub fn move_to_end_of_previous_line(&mut self, anchor: bool, buffer: &Buffer) {
        let cursors = self.selections.clone();
        for cursor in cursors.iter() {
            let next = cursor.clone().move_to_end_of_previous_line(anchor, buffer);
            self.update(buffer, &next);
        }
    }

    pub fn move_to_start_of_next_line(&mut self, anchor: bool, buffer: &Buffer) {
        let cursors = self.selections.clone();
        for cursor in cursors.iter() {
            let next = cursor.clone().move_to_start_of_next_line(anchor, buffer);
            self.update(buffer, &next);
        }
    }

    pub fn move_to_end_of_next_line(&mut self, anchor: bool, buffer: &Buffer) {
        let cursors = self.selections.clone();
        for cursor in cursors.iter() {
            let next = cursor.clone().move_to_end_of_next_line(anchor, buffer);
            self.update(buffer, &next);
        }
    }

    pub fn move_to_end_of_line(&mut self, anchor: bool, buffer: &Buffer) {
        let cursors = self.selections.clone();
        for cursor in cursors.iter() {
            let next = cursor.clone().move_to_end_of_line(anchor, buffer);
            self.update(buffer, &next);
        }
    }

    pub fn move_to_line(&mut self, anchor: bool, line: u32, buffer: &Buffer) {
        let cursors = self.selections.clone();
        for cursor in cursors.iter() {
            let next = cursor.clone().move_to_line(anchor, line, buffer);
            self.update(buffer, &next);
        }
    }

    pub fn move_within_character(&mut self, anchor: bool, count: u32, ch: char, buffer: &Buffer) {
        let cursors = self.selections.clone();
        for cursor in cursors.iter() {
            let next = cursor
                .clone()
                .move_within_character(anchor, count, ch, buffer);
            self.update(buffer, &next);
        }
    }

    pub fn move_around_character(&mut self, anchor: bool, count: u32, ch: char, buffer: &Buffer) {
        let cursors = self.selections.clone();
        for cursor in cursors.iter() {
            let next = cursor
                .clone()
                .move_around_character(anchor, count, ch, buffer);
            self.update(buffer, &next);
        }
    }

    pub fn find_character(
        &mut self,
        anchor: bool,
        count: u32,
        ch: char,
        forward: bool,
        till: bool,
        buffer: &Buffer,
    ) {
        let cursors = self.selections.clone();
        for cursor in cursors.iter() {
            let next = cursor
                .clone()
                .find_character(anchor, count, ch, forward, till, buffer);
            self.update(buffer, &next);
        }
    }

    pub fn move_to_start_of_document(&mut self, anchor: bool, buffer: &Buffer) {
        let cursors = self.selections.clone();
        for cursor in cursors.iter() {
            self.update(
                buffer,
                &cursor.clone().move_to_start_of_document(anchor, buffer),
            );
        }
    }

    pub fn move_to_end_of_document(&mut self, anchor: bool, buffer: &Buffer) {
        let cursors = self.selections.clone();
        for cursor in cursors.iter() {
            self.update(
                buffer,
                &cursor.clone().move_to_end_of_document(anchor, buffer),
            );
        }
    }

    pub fn move_to_word(&mut self, anchor: bool, count: u32, buffer: &Buffer) {
        for _ in 0..count {
            let cursors = self.selections.clone();
            for cursor in cursors.iter() {
                let next = cursor.clone().move_to_word(anchor, buffer);
                self.update(buffer, &next);
            }
        }
    }

    pub fn move_to_word_end(&mut self, anchor: bool, count: u32, buffer: &Buffer) {
        for _ in 0..count {
            let cursors = self.selections.clone();
            for cursor in cursors.iter() {
                let next = cursor.clone().move_to_word_end(anchor, buffer);
                self.update(buffer, &next);
            }
        }
    }

    pub fn move_to_previous_word(&mut self, anchor: bool, count: u32, buffer: &Buffer) {
        for _ in 0..count {
            let cursors = self.selections.clone();
            for cursor in cursors.iter() {
                let next = cursor.clone().move_to_previous_word(anchor, buffer);
                self.update(buffer, &next);
            }
        }
    }

    pub fn move_to_next_word(&mut self, anchor: bool, count: u32, buffer: &Buffer) {
        for _ in 0..count {
            let cursors = self.selections.clone();
            for cursor in cursors.iter() {
                let next = cursor.clone().move_to_next_word(anchor, buffer);
                self.update(buffer, &next);
            }
        }
    }

    pub fn move_to_next_word_end(&mut self, anchor: bool, count: u32, buffer: &Buffer) {
        for _ in 0..count {
            let cursors = self.selections.clone();
            for cursor in cursors.iter() {
                let next = cursor.clone().move_to_next_word_end(anchor, buffer);
                self.update(buffer, &next);
            }
        }
    }

    pub fn move_to_previous_word_end(&mut self, anchor: bool, count: u32, buffer: &Buffer) {
        for _ in 0..count {
            let cursors = self.selections.clone();
            for cursor in cursors.iter() {
                let next = cursor.clone().move_to_previous_word_end(anchor, buffer);
                self.update(buffer, &next);
            }
        }
    }

    pub fn move_to_big_word(&mut self, anchor: bool, count: u32, buffer: &Buffer) {
        for _ in 0..count {
            let cursors = self.selections.clone();
            for cursor in cursors.iter() {
                let next = cursor.clone().move_to_big_word(anchor, buffer);
                self.update(buffer, &next);
            }
        }
    }

    pub fn move_to_previous_big_word(&mut self, anchor: bool, count: u32, buffer: &Buffer) {
        for _ in 0..count {
            let cursors = self.selections.clone();
            for cursor in cursors.iter() {
                let next = cursor.clone().move_to_previous_big_word(anchor, buffer);
                self.update(buffer, &next);
            }
        }
    }

    pub fn move_to_big_word_end(&mut self, anchor: bool, count: u32, buffer: &Buffer) {
        for _ in 0..count {
            let cursors = self.selections.clone();
            for cursor in cursors.iter() {
                let next = cursor.clone().move_to_big_word_end(anchor, buffer);
                self.update(buffer, &next);
            }
        }
    }

    pub fn move_to_previous_big_word_end(&mut self, anchor: bool, count: u32, buffer: &Buffer) {
        for _ in 0..count {
            let cursors = self.selections.clone();
            for cursor in cursors.iter() {
                let next = cursor.clone().move_to_previous_big_word_end(anchor, buffer);
                self.update(buffer, &next);
            }
        }
    }

    pub fn move_to_previous_paragraph(&mut self, anchor: bool, count: u32, buffer: &Buffer) {
        for _ in 0..count {
            let cursors = self.selections.clone();
            for cursor in cursors.iter() {
                let next = cursor.clone().move_to_previous_paragraph(anchor, buffer);
                self.update(buffer, &next);
            }
        }
    }

    pub fn move_to_next_paragraph(&mut self, anchor: bool, count: u32, buffer: &Buffer) {
        for _ in 0..count {
            let cursors = self.selections.clone();
            for cursor in cursors.iter() {
                let next = cursor.clone().move_to_next_paragraph(anchor, buffer);
                self.update(buffer, &next);
            }
        }
    }

    pub fn move_to_previous_match(&mut self, text: &str, pattern: bool, buffer: &Buffer) {
        if pattern && text != self.search {
            self.search = text.to_string();
            self.regex = compile(self.search.as_str()).map(Arc::new);
        }
        let cursors = self.selections.clone();
        for cursor in cursors.iter() {
            let mut cur = cursor.clone();
            let point = cursor.head().to_point(&buffer);
            if pattern {
                if let Some(ref regex) = self.regex {
                    for _ in 0..(point.row + 1) {
                        if let Some(matched) = cur.move_to_previous_pattern_match(regex, buffer) {
                            self.update(buffer, &matched);
                            break;
                        } else {
                            cur = cur.move_to_previous_line(false, buffer);
                        }
                    }
                }
            } else {
                for _ in 0..(point.row + 1) {
                    if let Some(matched) = cur.move_to_previous_match(text, buffer) {
                        self.update(buffer, &matched);
                        break;
                    } else {
                        cur = cur.move_to_previous_line(false, buffer);
                    }
                }
            }
        }
    }

    pub fn move_to_next_match(&mut self, text: &str, pattern: bool, buffer: &Buffer) {
        if pattern && text != self.search {
            self.search = text.to_string();
            self.regex = compile(self.search.as_str()).map(Arc::new);
        }
        let cursors = self.selections.clone();
        let rows = buffer.row_count();
        for cursor in cursors.iter() {
            let mut cur = cursor.clone();
            let point = cursor.head().to_point(&buffer);

            if pattern {
                if let Some(ref regex) = self.regex {
                    let mut first = true;
                    for _ in point.row..rows {
                        let mut search_cur = if first {
                            first = false;
                            let mut p = cur.head().to_point(buffer);
                            p.column += 1;
                            let offset = buffer.clip_point(p, Bias::Left).to_offset(buffer);
                            let new_head = buffer.anchor_at(offset, Bias::Left);
                            Selection {
                                id: cur.id,
                                start: new_head,
                                end: new_head,
                                reversed: cur.reversed,
                                goal: cur.goal,
                            }
                        } else {
                            cur.clone()
                        };

                        if let Some(matched) = search_cur.move_to_next_pattern_match(regex, buffer)
                        {
                            self.update(buffer, &matched);
                            break;
                        } else {
                            cur = cur.move_to_next_line(false, buffer);
                        }
                    }
                }
            } else {
                let mut first = true;
                for _ in point.row..rows {
                    let mut search_cur = if first {
                        first = false;
                        let mut p = cur.head().to_point(buffer);
                        p.column += 1;
                        let offset = buffer.clip_point(p, Bias::Left).to_offset(buffer);
                        let new_head = buffer.anchor_at(offset, Bias::Left);
                        Selection {
                            id: cur.id,
                            start: new_head,
                            end: new_head,
                            reversed: cur.reversed,
                            goal: cur.goal,
                        }
                    } else {
                        cur.clone()
                    };

                    if let Some(matched) = search_cur.move_to_next_match(text, buffer) {
                        self.update(buffer, &matched);
                        break;
                    } else {
                        cur = cur.move_to_next_line(false, buffer);
                    }
                }
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct ResolvedSelection {
    pub head: Point,
    pub tail: Point,
    pub start: Point,
    pub end: Point,
    pub is_collapsed: bool,
    pub reversed: bool,
}

#[derive(Clone, Debug)]
pub struct ResolvedSelectionSet {
    pub primary_head: Point,
    pub selections: Vec<ResolvedSelection>,
}

impl ResolvedSelectionSet {
    pub fn new(selection_set: &SelectionSet, buffer: &Buffer) -> Self {
        let primary_head = selection_set.primary().head().to_point(buffer);
        let selections = selection_set
            .selections
            .iter()
            .map(|cursor| {
                let head = cursor.head().to_point(buffer);
                let tail = cursor.tail().to_point(buffer);
                let ordering = cursor.head().cmp(&cursor.tail(), buffer);
                let is_collapsed = ordering == Ordering::Equal;
                let (start, end, reversed) = if ordering == Ordering::Less {
                    (head, tail, false)
                } else {
                    (tail, head, true)
                };
                ResolvedSelection {
                    head,
                    tail,
                    start,
                    end,
                    is_collapsed,
                    reversed,
                }
            })
            .collect();
        Self {
            primary_head,
            selections,
        }
    }

    pub fn is_selected(&self, row: u32, column: u32) -> SelectionCellState {
        let at_primary_cursor_head = row == self.primary_head.row && column == self.primary_head.column;
        let mut at_cursor_head = false;
        for cursor in self.selections.iter() {
            at_cursor_head |= row == cursor.head.row && column == cursor.head.column;

            if cursor.is_collapsed {
                continue;
            }

            // If row is outside this selection's vertical bounds, try next selection
            if row < cursor.start.row || row > cursor.end.row {
                continue;
            }

            // Row is within selection's vertical range
            let selected;
            // Horizontal bounds depending on whether we're on boundary rows
            if cursor.start.row == cursor.end.row {
                // Single-line selection
                selected = column >= cursor.start.column && column <= cursor.end.column;
            } else if row == cursor.start.row {
                selected = column >= cursor.start.column;
            } else if row == cursor.end.row {
                selected = column <= cursor.end.column;
            } else {
                selected = true;
            }

            if selected {
                let at_head = if cursor.reversed {
                    row == cursor.end.row && column == cursor.end.column
                } else {
                    row == cursor.start.row && column == cursor.start.column
                };
                let selected_line = true; // row is within [start.row, end.row]
                return SelectionCellState {
                    selected_cell: true,
                    selected_line,
                    at_cursor_head: at_cursor_head || at_head,
                    at_primary_cursor_head,
                };
            }
        }
        SelectionCellState {
            selected_cell: false,
            selected_line: false,
            at_cursor_head,
            at_primary_cursor_head,
        }
    }
}
