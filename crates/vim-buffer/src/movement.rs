use crate::search::TextSearch;
use std::cmp::Ordering;
use sum_tree::Bias;
use text::{Anchor, Buffer, Point, Selection, SelectionGoal, ToOffset, ToPoint};
use vim_regex::Regex;

pub trait BufferText {
    fn row_text(&self, row: u32) -> String;
}

/// Returns the greatest valid UTF-8 boundary at or before `offset`.
fn floor_char_boundary(text: &str, offset: usize) -> usize {
    let mut offset = offset.min(text.len());
    while offset > 0 && !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

/// Returns the start of the character immediately before `offset`.
fn previous_char_boundary(text: &str, offset: usize) -> usize {
    let offset = floor_char_boundary(text, offset);
    text[..offset]
        .char_indices()
        .next_back()
        .map_or(0, |(index, _)| index)
}

/// Returns the starts of sentences following a word-ending period on `row`.
/// A period at the end of a line starts the next sentence on the following row.
fn sentence_starts(buffer: &Buffer, row: u32) -> Vec<Point> {
    let text = buffer.row_text(row);
    let mut starts = Vec::new();

    for (period, ch) in text.char_indices() {
        if ch != '.' || period == 0 {
            continue;
        }
        let Some(previous) = text[..period].chars().next_back() else {
            continue;
        };
        if !(previous.is_alphanumeric() || previous == '_') {
            continue;
        }

        let after_period = period + ch.len_utf8();
        if after_period == text.len() {
            starts.push(Point::new(row + 1, 0));
        } else if text[after_period..].starts_with(' ') {
            starts.push(Point::new(row, (after_period + 1) as u32));
        }
    }

    starts
}

impl BufferText for Buffer {
    fn row_text(&self, row: u32) -> String {
        let start = Point::new(row, 0).to_offset(self);
        let end = Point::new(row, self.line_len(row)).to_offset(self);
        self.as_rope().chunks_in_range(start..end).collect()
    }
}

impl BufferText for text::BufferSnapshot {
    fn row_text(&self, row: u32) -> String {
        let start = Point::new(row, 0).to_offset(self);
        let end = Point::new(row, self.line_len(row)).to_offset(self);
        self.as_rope().chunks_in_range(start..end).collect()
    }
}

pub trait Motions {
    fn text(&self, buffer: &Buffer) -> String;

    fn move_to_start_of_document(&self, anchor: bool, buffer: &Buffer) -> Selection<Anchor>;
    fn move_to_end_of_document(&self, anchor: bool, buffer: &Buffer) -> Selection<Anchor>;

    fn move_left_once(&self, anchor: bool, buffer: &Buffer) -> Selection<Anchor>;
    fn move_right_once(&self, anchor: bool, buffer: &Buffer) -> Selection<Anchor>;
    fn move_up_once(&self, anchor: bool, column: u32, buffer: &Buffer) -> Selection<Anchor>;
    fn move_down_once(&self, anchor: bool, column: u32, buffer: &Buffer) -> Selection<Anchor>;

    fn move_to_start_of_line(&self, anchor: bool, buffer: &Buffer) -> Selection<Anchor>;
    fn move_to_start_of_line_non_space(&self, anchor: bool, buffer: &Buffer) -> Selection<Anchor>;
    fn move_to_end_of_line(&self, anchor: bool, buffer: &Buffer) -> Selection<Anchor>;
    fn move_to_line(&self, anchor: bool, line: u32, buffer: &Buffer) -> Selection<Anchor>;
    fn move_to_previous_line(&self, anchor: bool, buffer: &Buffer) -> Selection<Anchor>;
    fn move_to_next_line(&self, anchor: bool, buffer: &Buffer) -> Selection<Anchor>;
    fn move_to_start_of_previous_line(&self, anchor: bool, buffer: &Buffer) -> Selection<Anchor>;
    fn move_to_end_of_previous_line(&self, anchor: bool, buffer: &Buffer) -> Selection<Anchor>;
    fn move_to_start_of_next_line(&self, anchor: bool, buffer: &Buffer) -> Selection<Anchor>;
    fn move_to_end_of_next_line(&self, anchor: bool, buffer: &Buffer) -> Selection<Anchor>;

    fn find_character(
        &self,
        anchor: bool,
        count: u32,
        ch: char,
        forward: bool,
        till: bool,
        buffer: &Buffer,
    ) -> Selection<Anchor>;

    // Word motions
    fn move_to_word(&self, anchor: bool, buffer: &Buffer) -> Selection<Anchor>;
    fn move_to_word_end(&self, anchor: bool, buffer: &Buffer) -> Selection<Anchor>;
    fn move_to_previous_word(&self, anchor: bool, buffer: &Buffer) -> Selection<Anchor>;
    fn move_to_next_word(&self, anchor: bool, buffer: &Buffer) -> Selection<Anchor>;
    fn move_to_next_word_end(&self, anchor: bool, buffer: &Buffer) -> Selection<Anchor>;
    fn move_to_previous_word_end(&self, anchor: bool, buffer: &Buffer) -> Selection<Anchor>;
    fn move_to_big_word(&self, anchor: bool, buffer: &Buffer) -> Selection<Anchor>;
    fn move_to_previous_big_word(&self, anchor: bool, buffer: &Buffer) -> Selection<Anchor>;
    fn move_to_big_word_end(&self, anchor: bool, buffer: &Buffer) -> Selection<Anchor>;
    fn move_to_previous_big_word_end(&self, anchor: bool, buffer: &Buffer) -> Selection<Anchor>;

    // Paragraph motions
    fn move_to_previous_paragraph(&self, anchor: bool, buffer: &Buffer) -> Selection<Anchor>;
    fn move_to_next_paragraph(&self, anchor: bool, buffer: &Buffer) -> Selection<Anchor>;

    // Sentence motions
    fn move_to_previous_sentence(&self, anchor: bool, buffer: &Buffer) -> Selection<Anchor>;
    fn move_to_next_sentence(&self, anchor: bool, buffer: &Buffer) -> Selection<Anchor>;

    fn move_to_previous_match(&mut self, text: &str, buffer: &Buffer) -> Option<Selection<Anchor>>;
    fn move_to_next_match(&mut self, text: &str, buffer: &Buffer) -> Option<Selection<Anchor>>;
    fn move_to_next_match_within(
        &mut self,
        search: &str,
        buffer: &Buffer,
        rows: u32,
    ) -> Option<Selection<Anchor>>;
    fn move_to_previous_pattern_match(
        &mut self,
        search: &Regex,
        buffer: &Buffer,
    ) -> Option<Selection<Anchor>>;
    fn move_to_next_pattern_match(
        &mut self,
        search: &Regex,
        buffer: &Buffer,
    ) -> Option<Selection<Anchor>>;

    fn move_within_character(
        &self,
        anchor: bool,
        count: u32,
        ch: char,
        buffer: &Buffer,
    ) -> Selection<Anchor>;

    fn move_around_character(
        &self,
        anchor: bool,
        count: u32,
        ch: char,
        buffer: &Buffer,
    ) -> Selection<Anchor>;
}

impl Motions for Selection<Anchor> {
    fn text(&self, buffer: &Buffer) -> String {
        let head = self.head();
        let tail = self.tail();
        if head.cmp(&tail, buffer) == Ordering::Equal {
            return String::new();
        }

        let (start, end) = if head.cmp(&tail, buffer) == Ordering::Less {
            (head.bias_left(buffer), tail.bias_right(buffer))
        } else {
            (tail.bias_left(buffer), head.bias_right(buffer))
        };
        let start = buffer.offset_for_anchor(&start);
        let end = buffer.clip_offset(buffer.offset_for_anchor(&end) + 1, Bias::Right);

        buffer.as_rope().chunks_in_range(start..end).collect()
    }

    fn move_to_start_of_document(&self, anchor: bool, buffer: &Buffer) -> Selection<Anchor> {
        let point = Point { row: 0, column: 0 };
        let offset = point.to_offset(&buffer);
        let new_head = buffer.anchor_at(offset, Bias::Left);
        return Selection {
            id: self.id,
            start: new_head,
            end: if anchor { self.tail() } else { new_head },
            reversed: true,
            goal: SelectionGoal::None,
        };
    }

    fn move_to_end_of_document(&self, anchor: bool, buffer: &Buffer) -> Selection<Anchor> {
        let point = Point {
            row: buffer.row_count(),
            column: 0,
        };
        let offset = point.to_offset(&buffer);
        let new_head = buffer.anchor_at(offset, Bias::Left);
        return Selection {
            id: self.id,
            start: new_head,
            end: if anchor { self.tail() } else { new_head },
            reversed: true,
            goal: SelectionGoal::None,
        };
    }

    fn move_left_once(&self, anchor: bool, buffer: &Buffer) -> Selection<Anchor> {
        let mut point = self.head().to_point(&buffer);
        if point.column != 0 {
            let row_text = buffer.row_text(point.row);
            let current_col = floor_char_boundary(&row_text, point.column as usize);
            if let Some(ch) = row_text[..current_col].chars().next_back() {
                point.column = point.column.saturating_sub(ch.len_utf8() as u32);
            } else {
                point.column = point.column.saturating_sub(1);
            }
        } else if point.row > 0 {
            point.row = point.row.saturating_sub(1);
            point.column = buffer.line_len(point.row);
        }
        let mut offset = buffer.offset_for_anchor(&buffer.anchor_at(&point, Bias::Left));
        offset = buffer.clip_offset(offset, Bias::Left);
        let new_head = buffer.anchor_at(offset, Bias::Left);

        Selection {
            id: self.id,
            start: new_head,
            end: if anchor { self.tail() } else { new_head },
            reversed: true,
            goal: SelectionGoal::None,
        }
    }

    fn move_right_once(&self, anchor: bool, buffer: &Buffer) -> Selection<Anchor> {
        let mut point = self.head().to_point(&buffer);
        let row_text = buffer.row_text(point.row);
        let l = row_text.len() as u32;
        if point.column < l {
            let current_col = floor_char_boundary(&row_text, point.column as usize);
            if let Some(ch) = row_text[current_col..].chars().next() {
                point.column += ch.len_utf8() as u32;
            } else {
                point.column += 1;
            }
        } else {
            point.row += 1;
            point.column = 0;
        }
        let mut offset = buffer.offset_for_anchor(&buffer.anchor_at(&point, Bias::Right));
        offset = buffer.clip_offset(offset, Bias::Right);
        let new_head = buffer.anchor_at(offset, Bias::Right);
        Selection {
            id: self.id,
            start: new_head,
            end: if anchor { self.tail() } else { new_head },
            reversed: true,
            goal: SelectionGoal::None,
        }
    }

    fn move_to_start_of_line(&self, anchor: bool, buffer: &Buffer) -> Selection<Anchor> {
        let mut point = self.head().to_point(&buffer);
        point.column = 0;
        point = buffer.clip_point(point, self.head().bias);
        let offset = point.to_offset(&buffer);
        let new_head = buffer.anchor_at(offset, Bias::Left);
        Selection {
            id: self.id,
            start: new_head,
            end: if anchor { self.tail() } else { new_head },
            reversed: true,
            goal: SelectionGoal::None,
        }
    }

    fn move_to_start_of_line_non_space(&self, anchor: bool, buffer: &Buffer) -> Selection<Anchor> {
        let mut point = self.head().to_point(&buffer);
        let line_text = buffer.row_text(point.row);
        let mut first_non_space = 0;
        for (idx, ch) in line_text.char_indices() {
            if !ch.is_whitespace() {
                first_non_space = idx;
                break;
            }
        }
        point.column = first_non_space as u32;
        point = buffer.clip_point(point, self.head().bias);
        let offset = point.to_offset(&buffer);
        let new_head = buffer.anchor_at(offset, Bias::Left);
        Selection {
            id: self.id,
            start: new_head,
            end: if anchor { self.tail() } else { new_head },
            reversed: true,
            goal: SelectionGoal::None,
        }
    }

    fn move_to_end_of_line(&self, anchor: bool, buffer: &Buffer) -> Selection<Anchor> {
        let mut point = self.head().to_point(buffer);
        point.column = buffer.line_len(point.row);
        point = buffer.clip_point(point, self.head().bias);
        let offset = point.to_offset(&buffer);
        let new_head = buffer.anchor_at(offset, Bias::Left);
        Selection {
            id: self.id,
            start: new_head,
            end: if anchor { self.tail() } else { new_head },
            reversed: true,
            goal: SelectionGoal::None,
        }
    }

    fn move_to_line(&self, anchor: bool, line: u32, buffer: &Buffer) -> Selection<Anchor> {
        let mut point = self.head().to_point(buffer);
        point.row = line
            .saturating_sub(1)
            .min(buffer.row_count().saturating_sub(1));
        point.column = 0;
        point = buffer.clip_point(point, self.head().bias);
        let offset = point.to_offset(buffer);
        let new_head = buffer.anchor_at(offset, Bias::Left);
        Selection {
            id: self.id,
            start: new_head,
            end: if anchor { self.tail() } else { new_head },
            reversed: false,
            goal: SelectionGoal::None,
        }
    }

    fn move_up_once(&self, anchor: bool, column: u32, buffer: &Buffer) -> Selection<Anchor> {
        let mut point = self.head().to_point(buffer);
        point.row = point.row.saturating_sub(1);
        point.column = column.min(buffer.line_len(point.row));
        point = buffer.clip_point(point, self.head().bias);
        let new_head = buffer.anchor_at(point.to_offset(buffer), Bias::Left);

        Selection {
            id: self.id,
            start: new_head,
            end: if anchor { self.tail() } else { new_head },
            reversed: true,
            goal: SelectionGoal::None,
        }
    }

    fn move_down_once(&self, anchor: bool, column: u32, buffer: &Buffer) -> Selection<Anchor> {
        let mut point = self.head().to_point(buffer);
        point.row = point
            .row
            .saturating_add(1)
            .min(buffer.row_count().saturating_sub(1));
        point.column = column.min(buffer.line_len(point.row));
        point = buffer.clip_point(point, self.head().bias);
        let new_head = buffer.anchor_at(point.to_offset(buffer), Bias::Left);

        Selection {
            id: self.id,
            start: new_head,
            end: if anchor { self.tail() } else { new_head },
            reversed: true,
            goal: SelectionGoal::None,
        }
    }

    fn move_to_previous_line(&self, anchor: bool, buffer: &Buffer) -> Selection<Anchor> {
        let cursor = self.move_to_start_of_line(anchor, buffer);
        cursor.move_left_once(anchor, buffer)
    }

    fn move_to_next_line(&self, anchor: bool, buffer: &Buffer) -> Selection<Anchor> {
        let cursor = self.move_to_end_of_line(anchor, buffer);
        cursor.move_right_once(anchor, buffer)
    }

    fn move_to_start_of_previous_line(&self, anchor: bool, buffer: &Buffer) -> Selection<Anchor> {
        let cursor = self.move_to_previous_line(anchor, buffer);
        cursor.move_to_start_of_line(anchor, buffer)
    }

    fn move_to_end_of_previous_line(&self, anchor: bool, buffer: &Buffer) -> Selection<Anchor> {
        self.move_to_previous_line(anchor, buffer)
    }

    fn move_to_start_of_next_line(&self, anchor: bool, buffer: &Buffer) -> Selection<Anchor> {
        self.move_to_next_line(anchor, buffer)
    }

    fn move_to_end_of_next_line(&self, anchor: bool, buffer: &Buffer) -> Selection<Anchor> {
        let cursor = self.move_to_next_line(anchor, buffer);
        cursor.move_to_end_of_line(anchor, buffer)
    }

    fn find_character(
        &self,
        anchor: bool,
        count: u32,
        ch: char,
        forward: bool,
        till: bool,
        buffer: &Buffer,
    ) -> Selection<Anchor> {
        let mut point = self.head().to_point(buffer);
        let line_text = buffer.row_text(point.row);
        let mut found_count = 0;
        if forward {
            let current_idx = floor_char_boundary(&line_text, point.column as usize);
            let start_idx = line_text[current_idx..]
                .chars()
                .next()
                .map_or(line_text.len(), |c| current_idx + c.len_utf8());
            if start_idx < line_text.len() {
                for (idx, c) in line_text[start_idx..].char_indices() {
                    if c == ch {
                        found_count += 1;
                        if found_count == count {
                            let match_idx = start_idx + idx;
                            if till {
                                if let Some(prev_c) = line_text[..match_idx].chars().next_back() {
                                    point.column = (match_idx - prev_c.len_utf8()) as u32;
                                } else {
                                    point.column = match_idx as u32;
                                }
                            } else {
                                point.column = match_idx as u32;
                            }
                            break;
                        }
                    }
                }
            }
        } else {
            let end_idx = floor_char_boundary(&line_text, point.column as usize);
            if end_idx > 0 {
                for (idx, c) in line_text[..end_idx].char_indices().rev() {
                    if c == ch {
                        found_count += 1;
                        if found_count == count {
                            if till {
                                point.column = (idx + c.len_utf8()) as u32;
                            } else {
                                point.column = idx as u32;
                            }
                            break;
                        }
                    }
                }
            }
        }
        if found_count == count {
            point = buffer.clip_point(point, self.head().bias);
            let offset = point.to_offset(buffer);
            let new_head = buffer.anchor_at(offset, Bias::Left);
            return Selection {
                id: self.id,
                start: new_head,
                end: if anchor { self.tail() } else { new_head },
                reversed: true,
                goal: SelectionGoal::None,
            };
        }
        // not found: return original selection unchanged
        self.clone()
    }

    fn move_to_word(&self, anchor: bool, buffer: &Buffer) -> Selection<Anchor> {
        let mut point = self.head().to_point(buffer);
        let text = buffer.row_text(point.row);
        let _previous_column = point.column;
        if let Some(word) = text.as_str().find_word(point.column as usize) {
            point.column = word.0 as u32;
        } else {
            point.column = 0;
        }
        let offset = point.to_offset(buffer);
        let new_head = buffer.anchor_at(offset, Bias::Left);
        Selection {
            id: self.id,
            start: new_head,
            end: if anchor { self.tail() } else { new_head },
            reversed: true,
            goal: SelectionGoal::None,
        }
    }

    fn move_to_word_end(&self, anchor: bool, buffer: &Buffer) -> Selection<Anchor> {
        let mut point = self.head().to_point(buffer);
        let text = buffer.row_text(point.row);
        let previous_column = point.column;
        if let Some(word) = text.as_str().find_next_word_end(point.column as usize) {
            point.column = previous_char_boundary(&text, word.1) as u32;
        } else {
            point.column = buffer.line_len(point.row);
        }
        if point.column == previous_column {
            return self.move_right_once(anchor, buffer);
        }
        let mut offset = point.to_offset(buffer);
        offset = buffer.clip_offset(offset, Bias::Left);
        let new_head = buffer.anchor_at(offset, Bias::Left);
        Selection {
            id: self.id,
            end: new_head,
            start: if anchor { self.tail() } else { new_head },
            reversed: false,
            goal: SelectionGoal::None,
        }
    }

    fn move_to_previous_word(&self, anchor: bool, buffer: &Buffer) -> Selection<Anchor> {
        let mut point = self.head().to_point(buffer);
        let text = buffer.row_text(point.row);
        let previous_column = point.column;
        if let Some(word) = text.as_str().find_previous_word(point.column as usize) {
            point.column = word.0 as u32;
        } else {
            point.column = 0;
        }
        if point.column == previous_column {
            return self.move_left_once(anchor, buffer);
        }
        let offset = point.to_offset(buffer);
        let new_head = buffer.anchor_at(offset, Bias::Left);
        Selection {
            id: self.id,
            start: new_head,
            end: if anchor { self.tail() } else { new_head },
            reversed: true,
            goal: SelectionGoal::None,
        }
    }

    fn move_to_next_word(&self, anchor: bool, buffer: &Buffer) -> Selection<Anchor> {
        let mut point = self.head().to_point(buffer);
        let text = buffer.row_text(point.row);
        let previous_column = point.column;
        if let Some(word) = text.as_str().find_next_word(point.column as usize) {
            point.column = word.0 as u32;
        } else {
            point.column = buffer.line_len(point.row);
        }
        if point.column == previous_column {
            return self.move_right_once(anchor, buffer);
        }
        let mut offset = point.to_offset(buffer);
        offset = buffer.clip_offset(offset, Bias::Left);
        let new_head = buffer.anchor_at(offset, Bias::Right);
        Selection {
            id: self.id,
            end: new_head,
            start: if anchor { self.tail() } else { new_head },
            reversed: false,
            goal: SelectionGoal::None,
        }
    }

    fn move_to_next_word_end(&self, anchor: bool, buffer: &Buffer) -> Selection<Anchor> {
        let mut point = self.head().to_point(buffer);
        let text = buffer.row_text(point.row);
        if let Some(word) = text.as_str().find_next_word_end(point.column as usize) {
            point.column = previous_char_boundary(&text, word.1) as u32;
        } else {
            point.column = buffer.line_len(point.row);
        }
        let mut offset = point.to_offset(buffer);
        offset = buffer.clip_offset(offset, Bias::Left);
        let new_head = buffer.anchor_at(offset, Bias::Left);
        Selection {
            id: self.id,
            end: new_head,
            start: if anchor { self.tail() } else { new_head },
            reversed: false,
            goal: SelectionGoal::None,
        }
    }

    fn move_to_previous_word_end(&self, anchor: bool, buffer: &Buffer) -> Selection<Anchor> {
        let mut point = self.head().to_point(buffer);
        let text = buffer.row_text(point.row);
        let previous_column = point.column;
        if let Some(word) = text.as_str().find_previous_word_end(point.column as usize) {
            point.column = previous_char_boundary(&text, word.1) as u32;
        } else {
            point.column = 0;
        }
        if point.column == previous_column {
            return self.move_left_once(anchor, buffer);
        }
        let offset = point.to_offset(buffer);
        let new_head = buffer.anchor_at(offset, Bias::Left);
        Selection {
            id: self.id,
            start: new_head,
            end: if anchor { self.tail() } else { new_head },
            reversed: true,
            goal: SelectionGoal::None,
        }
    }

    fn move_to_big_word(&self, anchor: bool, buffer: &Buffer) -> Selection<Anchor> {
        let mut point = self.head().to_point(buffer);
        let text = buffer.row_text(point.row);
        if let Some(word) = text.as_str().find_next_big_word(point.column as usize) {
            point.column = word.0 as u32;
        } else {
            point.column = text.len() as u32;
        }
        let offset = point.to_offset(buffer);
        let new_head = buffer.anchor_at(offset, Bias::Left);
        Selection {
            id: self.id,
            start: new_head,
            end: if anchor { self.tail() } else { new_head },
            reversed: false,
            goal: SelectionGoal::None,
        }
    }

    fn move_to_previous_big_word(&self, anchor: bool, buffer: &Buffer) -> Selection<Anchor> {
        let mut point = self.head().to_point(buffer);
        let text = buffer.row_text(point.row);
        if let Some(word) = text.as_str().find_previous_big_word(point.column as usize) {
            point.column = word.0 as u32;
        } else {
            point.column = 0;
        }
        let offset = point.to_offset(buffer);
        let new_head = buffer.anchor_at(offset, Bias::Left);
        Selection {
            id: self.id,
            start: new_head,
            end: if anchor { self.tail() } else { new_head },
            reversed: true,
            goal: SelectionGoal::None,
        }
    }

    fn move_to_big_word_end(&self, anchor: bool, buffer: &Buffer) -> Selection<Anchor> {
        let mut point = self.head().to_point(buffer);
        let text = buffer.row_text(point.row);
        if let Some(word) = text.as_str().find_next_big_word_end(point.column as usize) {
            point.column = previous_char_boundary(&text, word.1) as u32;
        } else {
            point.column = text.len() as u32;
        }
        let offset = point.to_offset(buffer);
        let new_head = buffer.anchor_at(offset, Bias::Left);
        Selection {
            id: self.id,
            start: new_head,
            end: if anchor { self.tail() } else { new_head },
            reversed: false,
            goal: SelectionGoal::None,
        }
    }

    fn move_to_previous_big_word_end(&self, anchor: bool, buffer: &Buffer) -> Selection<Anchor> {
        let mut point = self.head().to_point(buffer);
        let text = buffer.row_text(point.row);
        if let Some(word) = text
            .as_str()
            .find_previous_big_word_end(point.column as usize)
        {
            point.column = previous_char_boundary(&text, word.1) as u32;
        } else {
            point.column = 0;
        }
        let offset = point.to_offset(buffer);
        let new_head = buffer.anchor_at(offset, Bias::Left);
        Selection {
            id: self.id,
            start: new_head,
            end: if anchor { self.tail() } else { new_head },
            reversed: true,
            goal: SelectionGoal::None,
        }
    }

    fn move_to_previous_paragraph(&self, anchor: bool, buffer: &Buffer) -> Selection<Anchor> {
        let mut point = self.head().to_point(buffer);
        point.column = 0;
        let mut target_point = point.clone();
        let mut has_target = false;
        while point.row > 0 {
            point.row -= 1;
            if buffer.line_len(point.row) == 0 {
                target_point = point.clone();
                has_target = true;
            } else if has_target {
                break;
            }
        }
        let final_point = if has_target { target_point } else { point };
        let mut offset = final_point.to_offset(buffer);
        offset = buffer.clip_offset(offset, Bias::Right);
        let new_head = buffer.anchor_at(offset, Bias::Left);
        Selection {
            id: self.id,
            start: new_head,
            end: if anchor { self.tail() } else { new_head },
            reversed: true,
            goal: SelectionGoal::None,
        }
    }

    fn move_to_next_paragraph(&self, anchor: bool, buffer: &Buffer) -> Selection<Anchor> {
        let mut point = self.head().to_point(buffer);
        point.column = 0;
        let mut target_point = point.clone();
        let mut has_target = false;
        while point.row < buffer.row_count() {
            point.row += 1;
            if buffer.line_len(point.row) == 0 {
                target_point = point.clone();
                has_target = true;
            } else if has_target {
                break;
            }
        }
        let final_point = if has_target { target_point } else { point };
        let offset = final_point.to_offset(buffer);
        let new_head = buffer.anchor_at(offset, Bias::Left);
        Selection {
            id: self.id,
            start: new_head,
            end: if anchor { self.tail() } else { new_head },
            reversed: true,
            goal: SelectionGoal::None,
        }
    }

    fn move_to_previous_sentence(&self, anchor: bool, buffer: &Buffer) -> Selection<Anchor> {
        let point = self.head().to_point(buffer);

        // A sentence ends with a word-ending period followed by a space.
        // An empty line is also treated as a sentence
        // boundary, the same way it terminates a paragraph. Every candidate
        // boundary is compared directly against the original point, so a
        // boundary is only used if it's strictly before where we started.
        let mut row = point.row;
        let target_point = loop {
            let candidate = if buffer.line_len(row) == 0 {
                let candidate = Point::new(row + 1, 0);
                (candidate < point).then_some(candidate)
            } else {
                sentence_starts(buffer, row)
                    .into_iter()
                    .rev()
                    .find(|candidate| *candidate < point)
            };

            if let Some(candidate) = candidate {
                break candidate;
            }
            if row == 0 {
                break Point::new(0, 0);
            }
            row -= 1;
        };

        let mut offset = target_point.to_offset(buffer);
        offset = buffer.clip_offset(offset, Bias::Right);
        let new_head = buffer.anchor_at(offset, Bias::Left);
        Selection {
            id: self.id,
            start: new_head,
            end: if anchor { self.tail() } else { new_head },
            reversed: true,
            goal: SelectionGoal::None,
        }
    }

    fn move_to_next_sentence(&self, anchor: bool, buffer: &Buffer) -> Selection<Anchor> {
        let point = self.head().to_point(buffer);

        // Mirrors `move_to_previous_sentence`, scanning forward instead of
        // backward: a sentence boundary is a word-ending period followed by
        // a space, or an empty line. Every candidate
        // boundary is compared directly against the original point, so a
        // boundary is only used if it's strictly after where we started.
        let mut row = point.row;
        let target_point = loop {
            if row >= buffer.row_count() {
                break Point::new(buffer.row_count(), 0);
            }

            let candidate = if buffer.line_len(row) == 0 {
                let candidate = Point::new(row + 1, 0);
                (candidate > point).then_some(candidate)
            } else {
                sentence_starts(buffer, row)
                    .into_iter()
                    .find(|candidate| *candidate > point)
            };

            if let Some(candidate) = candidate {
                break candidate;
            }
            row += 1;
        };

        let offset = target_point.to_offset(buffer);
        let new_head = buffer.anchor_at(offset, Bias::Left);
        Selection {
            id: self.id,
            start: new_head,
            end: if anchor { self.tail() } else { new_head },
            reversed: true,
            goal: SelectionGoal::None,
        }
    }

    fn move_to_previous_match(
        &mut self,
        search: &str,
        buffer: &Buffer,
    ) -> Option<Selection<Anchor>> {
        let mut point = self.head().to_point(buffer);
        let line_text = buffer.row_text(point.row);
        if let Some(matched) = line_text
            .to_string()
            .as_str()
            .find_previous_match(search, point.column as usize)
        {
            point.column = matched.0 as u32;
            point = buffer.clip_point(point, self.head().bias);
            let offset = point.to_offset(buffer);
            let new_head = buffer.anchor_at(offset, Bias::Left);
            return Some(Selection {
                id: self.id,
                start: new_head,
                end: new_head,
                reversed: true,
                goal: SelectionGoal::None,
            });
        }
        return None;
    }

    fn move_to_next_match(&mut self, search: &str, buffer: &Buffer) -> Option<Selection<Anchor>> {
        let mut point = self.head().to_point(buffer);
        let line_text = buffer.row_text(point.row);
        if let Some(matched) = line_text
            .to_string()
            .as_str()
            .find_next_match(search, point.column as usize)
        {
            point.column = matched.0 as u32;
            point = buffer.clip_point(point, self.head().bias);
            let offset = point.to_offset(buffer);
            let new_head = buffer.anchor_at(offset, Bias::Left);
            return Some(Selection {
                id: self.id,
                start: new_head,
                end: new_head,
                reversed: true,
                goal: SelectionGoal::None,
            });
        }
        return None;
    }

    fn move_to_next_match_within(
        &mut self,
        search: &str,
        buffer: &Buffer,
        rows: u32,
    ) -> Option<Selection<Anchor>> {
        let mut cursor = self.clone();
        let mut p = cursor.head().to_point(buffer);
        p.column += 1;
        let offset = buffer.clip_point(p, Bias::Left).to_offset(buffer);
        let new_head = buffer.anchor_at(offset, Bias::Left);
        let mut first_cursor = Selection {
            id: cursor.id,
            start: new_head,
            end: new_head,
            reversed: cursor.reversed,
            goal: cursor.goal,
        };
        if let Some(matched) = first_cursor.move_to_next_match(search, buffer) {
            return Some(matched);
        }

        for _ in 0..rows {
            let current_row = cursor.head().to_point(buffer).row;
            if current_row + 1 >= buffer.row_count() {
                cursor = cursor.move_to_start_of_document(false, buffer);
            } else {
                cursor = cursor.move_to_start_of_next_line(false, buffer);
            }

            let mut point = cursor.head().to_point(buffer);
            let line_text = buffer.row_text(point.row);
            let Some((column, _, _)) = line_text.find_string(search).into_iter().next() else {
                continue;
            };

            point.column = column as u32;
            point = buffer.clip_point(point, cursor.head().bias);
            let new_head = buffer.anchor_at(point.to_offset(buffer), Bias::Left);
            return Some(Selection {
                id: cursor.id,
                start: new_head,
                end: new_head,
                reversed: true,
                goal: SelectionGoal::None,
            });
        }

        None
    }

    fn move_to_previous_pattern_match(
        &mut self,
        search: &Regex,
        buffer: &Buffer,
    ) -> Option<Selection<Anchor>> {
        let mut point = self.head().to_point(buffer);
        let line_text = buffer.row_text(point.row);
        if let Some(matched) = line_text
            .to_string()
            .as_str()
            .find_previous_pattern_match(search, point.column as usize)
        {
            point.column = matched.0 as u32;
            point = buffer.clip_point(point, self.head().bias);
            let offset = point.to_offset(buffer);
            let new_head = buffer.anchor_at(offset, Bias::Left);
            return Some(Selection {
                id: self.id,
                start: new_head,
                end: new_head,
                reversed: true,
                goal: SelectionGoal::None,
            });
        }
        return None;
    }

    fn move_to_next_pattern_match(
        &mut self,
        search: &Regex,
        buffer: &Buffer,
    ) -> Option<Selection<Anchor>> {
        let mut point = self.head().to_point(buffer);
        let line_text = buffer.row_text(point.row);
        if let Some(matched) = line_text
            .to_string()
            .as_str()
            .find_next_pattern_match(search, point.column as usize)
        {
            point.column = matched.0 as u32;
            point = buffer.clip_point(point, self.head().bias);
            let offset = point.to_offset(buffer);
            let new_head = buffer.anchor_at(offset, Bias::Left);
            return Some(Selection {
                id: self.id,
                start: new_head,
                end: new_head,
                reversed: true,
                goal: SelectionGoal::None,
            });
        }
        return None;
    }

    fn move_within_character(
        &self,
        _anchor: bool,
        _count: u32,
        ch: char,
        buffer: &Buffer,
    ) -> Selection<Anchor> {
        let (start_ch, end_ch) = match ch {
            '{' | '}' => ('{', '}'),
            '[' | ']' => ('[', ']'),
            '(' | ')' => ('(', ')'),
            '\'' => ('\'', '\''),
            '"' => ('"', '"'),
            _ => ('`', '`'),
        };

        let start_sel = self.find_character(false, 1, start_ch, false, false, buffer);
        let start_pos = start_sel.head().to_point(buffer);

        let end_sel = start_sel.find_character(true, 1, end_ch, true, false, buffer);
        let end_pos = end_sel.head().to_point(buffer);

        if start_pos == self.head().to_point(buffer)
            || end_pos == start_pos
            || start_pos.row != end_pos.row
        {
            return self.clone();
        }

        let start_col = start_pos.column + 1;
        let end_col = end_pos.column.saturating_sub(1);

        let start_offset = buffer
            .clip_point(
                Point {
                    row: start_pos.row,
                    column: start_col,
                },
                Bias::Right,
            )
            .to_offset(buffer);
        let end_offset = buffer
            .clip_point(
                Point {
                    row: end_pos.row,
                    column: end_col,
                },
                Bias::Left,
            )
            .to_offset(buffer);

        let start_anchor = buffer.anchor_at(start_offset, Bias::Left);
        let end_anchor = buffer.anchor_at(end_offset, Bias::Right);

        Selection {
            id: self.id,
            start: start_anchor,
            end: end_anchor,
            reversed: false,
            goal: SelectionGoal::None,
        }
    }

    fn move_around_character(
        &self,
        _anchor: bool,
        _count: u32,
        ch: char,
        buffer: &Buffer,
    ) -> Selection<Anchor> {
        let (start_ch, end_ch) = match ch {
            '{' | '}' => ('{', '}'),
            '[' | ']' => ('[', ']'),
            '(' | ')' => ('(', ')'),
            '\'' => ('\'', '\''),
            '"' => ('"', '"'),
            _ => ('`', '`'),
        };

        let start_sel = self.find_character(false, 1, start_ch, false, false, buffer);
        let start_pos = start_sel.head().to_point(buffer);

        let end_sel = start_sel.find_character(true, 1, end_ch, true, false, buffer);
        let end_pos = end_sel.head().to_point(buffer);

        if start_pos == self.head().to_point(buffer)
            || end_pos == start_pos
            || start_pos.row != end_pos.row
        {
            return self.clone();
        }

        let start_col = start_pos.column;
        let end_col = end_pos.column;

        let start_offset = buffer
            .clip_point(
                Point {
                    row: start_pos.row,
                    column: start_col,
                },
                Bias::Right,
            )
            .to_offset(buffer);
        let end_offset = buffer
            .clip_point(
                Point {
                    row: end_pos.row,
                    column: end_col,
                },
                Bias::Left,
            )
            .to_offset(buffer);

        let start_anchor = buffer.anchor_at(start_offset, Bias::Left);
        let end_anchor = buffer.anchor_at(end_offset, Bias::Right);

        Selection {
            id: self.id,
            start: start_anchor,
            end: end_anchor,
            reversed: false,
            goal: SelectionGoal::None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SelectionCollection;
    use clock::ReplicaId;
    use text::BufferId;

    fn selection(
        buffer: &Buffer,
        id: usize,
        start: usize,
        end: usize,
        reversed: bool,
    ) -> Selection<Anchor> {
        Selection {
            id,
            start: buffer.anchor_at(start, Bias::Left),
            end: buffer.anchor_at(end, Bias::Left),
            reversed,
            goal: SelectionGoal::None,
        }
    }

    #[test]
    fn selection_text_normalizes_direction_and_uses_inclusive_endpoints() {
        let buffer = Buffer::new(ReplicaId::LOCAL, BufferId::new(1).unwrap(), "abcdef");

        assert_eq!(selection(&buffer, 0, 1, 2, false).text(&buffer), "bc");
        assert_eq!(selection(&buffer, 0, 1, 2, true).text(&buffer), "bc");
        assert_eq!(selection(&buffer, 0, 2, 2, false).text(&buffer), "");
    }

    #[test]
    fn similar_cursor_check_ignores_range_direction() {
        let buffer = Buffer::new(ReplicaId::LOCAL, BufferId::new(1).unwrap(), "abcdef");
        let mut selections = SelectionCollection::new();
        selections.selections = vec![selection(&buffer, 0, 1, 3, false)];

        assert!(selections.has_similar_cursor(&selection(&buffer, 1, 1, 3, false), &buffer));
        assert!(selections.has_similar_cursor(&selection(&buffer, 1, 3, 1, false), &buffer));
        assert!(!selections.has_similar_cursor(&selection(&buffer, 1, 2, 4, false), &buffer));

        selections
            .selections
            .push(selection(&buffer, 1, 3, 1, true));
        selections.collapse_overlapping_cursors(&buffer);
        assert_eq!(selections.len(), 1);
        assert_eq!(selections.primary().id, 0);
    }

    #[test]
    fn collection_text_joins_non_empty_selections() {
        let buffer = Buffer::new(ReplicaId::LOCAL, BufferId::new(1).unwrap(), "abcdef");
        let mut selections = SelectionCollection::new();
        selections.selections = vec![
            selection(&buffer, 0, 0, 1, false),
            selection(&buffer, 1, 3, 4, false),
        ];

        assert_eq!(selections.text(&buffer), "ab\nde");
    }

    #[test]
    fn next_match_within_searches_only_the_requested_following_rows() {
        let buffer = Buffer::new(
            ReplicaId::LOCAL,
            BufferId::new(1).unwrap(),
            "zero\none\ntarget",
        );
        let mut cursor = selection(&buffer, 0, 0, 0, false);

        assert!(
            cursor
                .move_to_next_match_within("target", &buffer, 1)
                .is_none()
        );

        let matched = cursor
            .move_to_next_match_within("target", &buffer, 2)
            .expect("match should be found two rows below");
        assert_eq!(matched.head().to_point(&buffer), Point::new(2, 0));
    }

    #[test]
    fn test_move_within_character() {
        let buffer = Buffer::new(ReplicaId::LOCAL, BufferId::new(1).unwrap(), "a {hello} b");
        let cursor = selection(&buffer, 0, 4, 4, false);
        let result = cursor.move_within_character(false, 1, '{', &buffer);
        assert_eq!(result.text(&buffer), "hello");
    }

    #[test]
    fn test_move_around_character() {
        let buffer = Buffer::new(ReplicaId::LOCAL, BufferId::new(1).unwrap(), "a {hello} b");
        let cursor = selection(&buffer, 0, 4, 4, false);
        let result = cursor.move_around_character(false, 1, '{', &buffer);
        assert_eq!(result.text(&buffer), "{hello}");
    }

    #[test]
    fn test_move_within_character_visual() {
        let buffer = Buffer::new(ReplicaId::LOCAL, BufferId::new(1).unwrap(), "a {hello} b");
        let cursor = Selection {
            id: 0,
            start: buffer.anchor_at(4, Bias::Left),
            end: buffer.anchor_at(0, Bias::Left),
            reversed: true,
            goal: SelectionGoal::None,
        };
        let result = cursor.move_within_character(true, 1, '{', &buffer);
        assert_eq!(result.text(&buffer), "hello");
    }

    #[test]
    fn utf8_motions_never_slice_inside_a_character() {
        let buffer = Buffer::new(ReplicaId::LOCAL, BufferId::new(1).unwrap(), "α😀x café");
        let cursor = selection(&buffer, 0, 0, 0, false);

        let next = cursor.move_right_once(false, &buffer);
        assert_eq!(next.head().to_point(&buffer), Point::new(0, 2));

        let found = cursor.find_character(true, 1, 'x', true, false, &buffer);
        assert_eq!(found.head().to_point(&buffer), Point::new(0, 6));

        let previous = found.move_left_once(false, &buffer);
        assert_eq!(previous.head().to_point(&buffer), Point::new(0, 2));
    }

    #[test]
    fn utf8_word_end_stays_on_character_boundary() {
        let buffer = Buffer::new(ReplicaId::LOCAL, BufferId::new(1).unwrap(), "café тест");
        let cursor = selection(&buffer, 0, 0, 0, false);

        let end = cursor.move_to_word_end(false, &buffer);
        assert_eq!(end.head().to_point(&buffer), Point::new(0, 3));
        assert!(
            buffer
                .as_rope()
                .is_char_boundary(end.head().to_point(&buffer).to_offset(&buffer))
        );
    }

    #[test]
    fn test_word_end_motions() {
        let buffer = Buffer::new(
            ReplicaId::LOCAL,
            BufferId::new(1).unwrap(),
            "hello world\nfoo bar",
        );
        // Start at 'h' (index 0)
        let mut cursor = selection(&buffer, 0, 0, 0, false);

        // Move to word end -> should be 'o' of hello (index 4)
        cursor = cursor.move_to_word_end(false, &buffer);
        assert_eq!(cursor.head().to_point(&buffer), Point::new(0, 4));

        // Move to word end again -> should be 'd' of world (index 10)
        cursor = cursor.move_to_word_end(false, &buffer);
        assert_eq!(cursor.head().to_point(&buffer), Point::new(0, 10));

        // Move to word end again -> should go to end of line (line 0, index 11)
        cursor = cursor.move_to_word_end(false, &buffer);
        assert_eq!(cursor.head().to_point(&buffer), Point::new(0, 11));

        // Move to word end again -> should go to start of next line (line 1, index 0)
        cursor = cursor.move_to_word_end(false, &buffer);
        assert_eq!(cursor.head().to_point(&buffer), Point::new(1, 0));

        // Move to word end again -> should go to 'o' of foo (line 1, index 2)
        cursor = cursor.move_to_word_end(false, &buffer);
        assert_eq!(cursor.head().to_point(&buffer), Point::new(1, 2));

        // Move to previous word end -> should go to start of line (line 1, index 0)
        cursor = cursor.move_to_previous_word_end(false, &buffer);
        assert_eq!(cursor.head().to_point(&buffer), Point::new(1, 0));

        // Move to previous word end again -> should cross line to end of line 0 (line 0, index 11)
        cursor = cursor.move_to_previous_word_end(false, &buffer);
        assert_eq!(cursor.head().to_point(&buffer), Point::new(0, 11));

        // Move to previous word end again -> should go to 'd' of world (line 0, index 10)
        cursor = cursor.move_to_previous_word_end(false, &buffer);
        assert_eq!(cursor.head().to_point(&buffer), Point::new(0, 10));

        // Move to previous word end again -> should go to 'o' of hello (line 0, index 4)
        cursor = cursor.move_to_previous_word_end(false, &buffer);
        assert_eq!(cursor.head().to_point(&buffer), Point::new(0, 4));
    }

    #[test]
    fn test_move_to_previous_sentence_within_a_line() {
        let buffer = Buffer::new(
            ReplicaId::LOCAL,
            BufferId::new(1).unwrap(),
            "One. Two. Three.",
        );
        // "One. " -> 0..5, "Two. " -> 5..10, "Three." starts at 10.

        // From inside "Three", the previous sentence boundary is the start
        // of "Three" itself.
        let cursor = selection(&buffer, 0, 13, 13, false);
        let cursor = cursor.move_to_previous_sentence(false, &buffer);
        assert_eq!(cursor.head().to_point(&buffer), Point::new(0, 10));

        // Repeating from exactly the start of "Three" goes back to the start
        // of "Two".
        let cursor = cursor.move_to_previous_sentence(false, &buffer);
        assert_eq!(cursor.head().to_point(&buffer), Point::new(0, 5));

        // Repeating from exactly the start of "Two" goes back to the start
        // of "One" (the beginning of the document).
        let cursor = cursor.move_to_previous_sentence(false, &buffer);
        assert_eq!(cursor.head().to_point(&buffer), Point::new(0, 0));

        // Already at the start of the document: stays put.
        let cursor = cursor.move_to_previous_sentence(false, &buffer);
        assert_eq!(cursor.head().to_point(&buffer), Point::new(0, 0));
    }

    #[test]
    fn test_move_to_previous_sentence_requires_period_and_space() {
        // "Mr. Smith" should not be treated as two sentences ending at "Mr."
        // when there's no word before point ending in ". " earlier than
        // "Really"; but "Really." followed by a space is a real boundary.
        let buffer = Buffer::new(
            ReplicaId::LOCAL,
            BufferId::new(1).unwrap(),
            "Really. Ok then.",
        );
        let cursor = selection(&buffer, 0, 12, 12, false);
        let cursor = cursor.move_to_previous_sentence(false, &buffer);
        assert_eq!(cursor.head().to_point(&buffer), Point::new(0, 8));
    }

    #[test]
    fn test_move_to_previous_sentence_crosses_lines() {
        let buffer = Buffer::new(
            ReplicaId::LOCAL,
            BufferId::new(1).unwrap(),
            "Line one. Line two.\nLine three. Line four.",
        );
        // Row 0 ("Line one. Line two.") is 19 characters, plus a newline,
        // so row 1 starts at offset 20.

        // From inside "Line four" on row 1, the previous boundary is the
        // start of "Line four" on the same row.
        let cursor = selection(&buffer, 0, 20 + 15, 20 + 15, false);
        let cursor = cursor.move_to_previous_sentence(false, &buffer);
        assert_eq!(cursor.head().to_point(&buffer), Point::new(1, 12));

        // From the start of "Line four", the previous boundary is the start
        // of "Line three", still on row 1.
        let cursor = cursor.move_to_previous_sentence(false, &buffer);
        assert_eq!(cursor.head().to_point(&buffer), Point::new(1, 0));

        // From the start of row 1, cross back onto row 0's second sentence.
        let cursor = cursor.move_to_previous_sentence(false, &buffer);
        assert_eq!(cursor.head().to_point(&buffer), Point::new(0, 10));
    }

    #[test]
    fn test_move_to_previous_sentence_stops_at_empty_line() {
        let buffer = Buffer::new(
            ReplicaId::LOCAL,
            BufferId::new(1).unwrap(),
            "Line one. Line two.\n\nLine three.",
        );
        // Row 0 is 19 characters (offsets 0..19) followed by a newline at 19.
        // Row 1 is empty, its own newline is at offset 20, so row 2 starts at
        // offset 21.

        // From inside "Line three" (row 2), there is no sentence-ending
        // punctuation before point on row 2, but row 1 is empty, so the
        // previous sentence boundary is the start of row 2.
        let cursor = selection(&buffer, 0, 21 + 8, 21 + 8, false);
        let cursor = cursor.move_to_previous_sentence(false, &buffer);
        assert_eq!(cursor.head().to_point(&buffer), Point::new(2, 0));

        // Pressing it again lands on the empty row itself, matching how
        // paragraph motions stop at blank lines.
        let cursor = cursor.move_to_previous_sentence(false, &buffer);
        assert_eq!(cursor.head().to_point(&buffer), Point::new(1, 0));

        // Pressing it once more crosses the empty line and lands on the last
        // sentence of row 0.
        let cursor = cursor.move_to_previous_sentence(false, &buffer);
        assert_eq!(cursor.head().to_point(&buffer), Point::new(0, 10));
    }

    #[test]
    fn test_move_to_next_sentence_within_a_line() {
        let buffer = Buffer::new(
            ReplicaId::LOCAL,
            BufferId::new(1).unwrap(),
            "One. Two. Three.",
        );
        // "One. " -> 0..5, "Two. " -> 5..10, "Three." starts at 10.

        // From the start of "One", the next boundary is the start of "Two".
        let cursor = selection(&buffer, 0, 0, 0, false);
        let cursor = cursor.move_to_next_sentence(false, &buffer);
        assert_eq!(cursor.head().to_point(&buffer), Point::new(0, 5));

        // From the start of "Two", the next boundary is the start of
        // "Three".
        let cursor = cursor.move_to_next_sentence(false, &buffer);
        assert_eq!(cursor.head().to_point(&buffer), Point::new(0, 10));

        // From the start of "Three", there's no sentence left, so it moves
        // to the end of the document.
        let cursor = cursor.move_to_next_sentence(false, &buffer);
        assert_eq!(cursor.head().to_point(&buffer), Point::new(0, 16));

        // Already at the end of the document: stays put.
        let cursor = cursor.move_to_next_sentence(false, &buffer);
        assert_eq!(cursor.head().to_point(&buffer), Point::new(0, 16));
    }

    #[test]
    fn test_move_to_next_sentence_requires_period_and_space() {
        let buffer = Buffer::new(
            ReplicaId::LOCAL,
            BufferId::new(1).unwrap(),
            "Really. Ok then.",
        );
        let cursor = selection(&buffer, 0, 0, 0, false);
        let cursor = cursor.move_to_next_sentence(false, &buffer);
        assert_eq!(cursor.head().to_point(&buffer), Point::new(0, 8));
    }

    #[test]
    fn test_move_to_next_sentence_crosses_lines() {
        let buffer = Buffer::new(
            ReplicaId::LOCAL,
            BufferId::new(1).unwrap(),
            "Line one. Line two.\nLine three. Line four.",
        );
        // Row 0 ("Line one. Line two.") is 19 characters, plus a newline,
        // so row 1 starts at offset 20.

        // From the start of "Line one", the next boundary is the start of
        // "Line two", still on row 0.
        let cursor = selection(&buffer, 0, 0, 0, false);
        let cursor = cursor.move_to_next_sentence(false, &buffer);
        assert_eq!(cursor.head().to_point(&buffer), Point::new(0, 10));

        // From the start of "Line two", the next boundary crosses onto row
        // 1, to the start of "Line three".
        let cursor = cursor.move_to_next_sentence(false, &buffer);
        assert_eq!(cursor.head().to_point(&buffer), Point::new(1, 0));

        // From the start of "Line three", the next boundary is the start of
        // "Line four", still on row 1.
        let cursor = cursor.move_to_next_sentence(false, &buffer);
        assert_eq!(cursor.head().to_point(&buffer), Point::new(1, 12));
    }

    #[test]
    fn test_move_to_next_sentence_stops_at_empty_line() {
        let buffer = Buffer::new(
            ReplicaId::LOCAL,
            BufferId::new(1).unwrap(),
            "Line one. Line two.\n\nLine three.",
        );
        // Row 0 is 19 characters (offsets 0..19) followed by a newline at 19.
        // Row 1 is empty, its own newline is at offset 20, so row 2 starts at
        // offset 21.

        // From the start of "Line two" (row 0), the next boundary lands on
        // the empty row itself, matching how paragraph motions stop at
        // blank lines.
        let cursor = selection(&buffer, 0, 10, 10, false);
        let cursor = cursor.move_to_next_sentence(false, &buffer);
        assert_eq!(cursor.head().to_point(&buffer), Point::new(1, 0));

        // Pressing it again crosses the empty line and lands on the start
        // of "Line three".
        let cursor = cursor.move_to_next_sentence(false, &buffer);
        assert_eq!(cursor.head().to_point(&buffer), Point::new(2, 0));
    }

    #[test]
    fn test_pattern_match_start_of_line() {
        let buffer = Buffer::new(
            ReplicaId::LOCAL,
            BufferId::new(1).unwrap(),
            "abc\npattern_here\ndef",
        );
        let mut selections = SelectionCollection::new();
        selections
            .selections
            .push(selection(&buffer, 0, 0, 0, false));

        // Search forward for "pattern" (starts at first character of the second line)
        selections.move_to_next_match("pattern", true, &buffer);

        // The selection should have moved to the start of "pattern_here" (line 1, column 0)
        let head_pt = selections.selections[0].head().to_point(&buffer);
        assert_eq!(head_pt, Point::new(1, 0));
    }
}
