use crate::controller::actions::{Action, Mode};
use crate::editor::Editor;
use crate::editor::display::{self};
use crate::editor::selections::{Motions, SelectionCollection};
use crate::services::clipboard::ClipboardKind;

use crate::editor::display::display_map::DisplayMap;
use crate::editor::display::highlight::Highlights;
use clock::ReplicaId;
use rope::Point;
use std::{cmp::Ordering, collections::HashMap, io, sync::Arc, sync::atomic::AtomicU64};
use sum_tree::Bias;
use text::{Anchor, Buffer, BufferId, BufferSnapshot, Selection, SelectionGoal, ToOffset, ToPoint};

pub trait BufferText {
    fn row_text(&self, row: u32) -> String;
}

impl BufferText for Buffer {
    fn row_text(&self, row: u32) -> String {
        let start = Point::new(row, 0).to_offset(self);
        let end = Point::new(row, self.line_len(row)).to_offset(self);
        self.as_rope().chunks_in_range(start..end).collect()
    }
}

impl BufferText for BufferSnapshot {
    fn row_text(&self, row: u32) -> String {
        let start = Point::new(row, 0).to_offset(self);
        let end = Point::new(row, self.line_len(row)).to_offset(self);
        self.as_rope().chunks_in_range(start..end).collect()
    }
}

pub struct Document {
    pub id: usize,
    selections: SelectionCollection,
    mode: Mode,
    pub folds: Vec<display::fold_map::Fold>,
    pub display_map: DisplayMap,
    pub hl: Highlights,
    pub latest_hl_task_id: Arc<AtomicU64>,
    pub latest_wrap_task_id: Arc<AtomicU64>,
    pub latest_parse_task_id: Arc<AtomicU64>,
    pub latest_index_task_id: Arc<AtomicU64>,
    pub current_hl_task_id: u64,
    pub current_wrap_task_id: u64,
    pub current_parse_task_id: u64,
    pub current_index_task_id: u64,
    pub show_pattern_match: bool,
    pub show_scrollbar: bool,
    pub show_gutter: bool,
    pub gutter_width: usize,
    pub should_sync: bool,
    pub marks: HashMap<char, Anchor>,
}

impl Document {
    pub fn clear(&mut self, buffer: &Buffer) {
        self.selections = SelectionCollection::new();
        self.selections.add(buffer, 0);
        self.mode = Mode::Normal;
        self.folds.clear();
        self.display_map = DisplayMap::new(buffer.snapshot().clone(), None);
        self.hl.clear();
        self.gutter_width = 0;
        self.should_sync = true;
        self.marks.clear();
    }

    pub fn new_with_buffer(id: usize, buffer: &Buffer, file_path: &str) -> Self {
        let mut selections = SelectionCollection::new();
        selections.add(buffer, 0);
        let hl = Highlights::new(file_path);
        let display_map = DisplayMap::new(buffer.snapshot().clone(), None);

        Self {
            id,
            selections,
            mode: Mode::Normal,
            folds: Vec::new(),
            display_map,
            hl,
            latest_hl_task_id: Arc::new(AtomicU64::new(0)),
            latest_wrap_task_id: Arc::new(AtomicU64::new(0)),
            latest_parse_task_id: Arc::new(AtomicU64::new(0)),
            latest_index_task_id: Arc::new(AtomicU64::new(0)),
            current_hl_task_id: 0,
            current_wrap_task_id: 0,
            current_parse_task_id: 0,
            current_index_task_id: 0,
            show_pattern_match: true,
            show_scrollbar: true,
            show_gutter: true,
            gutter_width: 0,
            should_sync: true,
            marks: HashMap::new(),
        }
    }

    pub fn mode(&self) -> Mode {
        return self.mode;
    }

    pub fn new_line(&self, buffer: &Buffer) -> &str {
        buffer.line_ending().as_str()
    }

    pub fn undo(&mut self, buffer: &mut Buffer, count: u32) {
        for _ in 0..count {
            buffer.undo();
        }
    }

    pub fn redo(&mut self, buffer: &mut Buffer, count: u32) {
        for _ in 0..count {
            buffer.redo();
        }
    }

    pub fn fold(
        &mut self,
        buffer: &Buffer,
        _count: u32,
        editor: &Editor,
        syntax_tree: Option<&crate::services::treesitter::SyntaxTree>,
    ) {
        if let Some(syntax_tree) = syntax_tree {
            let mut seen_ranges = std::collections::HashSet::new();
            let mut updated_selections = Vec::new();
            for selection in self.selections.selections.iter() {
                let head_point = selection.head().to_point(buffer);
                let head_offset = head_point.to_offset(buffer);
                if let Some(block) = syntax_tree.enclosing_block_at_byte(head_offset) {
                    let mut start_offset = block.byte_range.start;
                    let mut end_offset = block.byte_range.end;

                    let first_char = buffer
                        .text_for_range(start_offset..start_offset + 1)
                        .next()
                        .and_then(|s| s.chars().next());
                    let last_char = if end_offset > 0 {
                        buffer
                            .text_for_range(end_offset - 1..end_offset)
                            .next()
                            .and_then(|s| s.chars().next())
                    } else {
                        None
                    };

                    if let (Some(fc), Some(lc)) = (first_char, last_char) {
                        if (fc == '{' && lc == '}')
                            || (fc == '[' && lc == ']')
                            || (fc == '(' && lc == ')')
                        {
                            start_offset += 1;
                            end_offset -= 1;
                        }
                    }

                    let start_point = start_offset.to_point(buffer);
                    let end_point = end_offset.to_point(buffer);

                    if !editor.fold_multiline_only
                        || (block.end_position.row > block.start_position.row
                            && start_point.row < end_point.row)
                    {
                        let range = block.byte_range.clone();
                        if seen_ranges.insert(range) {
                            let fold = display::fold_map::Fold {
                                start: start_point,
                                end: end_point,
                            };
                            if !self.folds.contains(&fold) {
                                self.folds.push(fold);
                            }
                            let target_offset = block.byte_range.start;
                            let target_anchor =
                                buffer.anchor_at(&target_offset.to_point(buffer), Bias::Left);
                            let new_sel = Selection {
                                id: selection.id,
                                start: target_anchor.clone(),
                                end: target_anchor,
                                reversed: false,
                                goal: SelectionGoal::None,
                            };
                            updated_selections.push(new_sel);
                        }
                    }
                }
            }
            for new_sel in updated_selections {
                self.selections.update(buffer, &new_sel);
            }
            if let Some(first) = self.selections.first() {
                self.selections.point = first.head().to_point(buffer);
            }
        }
    }

    pub fn unfold(
        &mut self,
        buffer: &Buffer,
        _count: u32,
        editor: &Editor,
        syntax_tree: Option<&crate::services::treesitter::SyntaxTree>,
    ) {
        let mut to_remove = Vec::new();
        for selection in self.selections.selections.iter() {
            let head_point = selection.head().to_point(buffer);
            for (idx, fold) in self.folds.iter().enumerate() {
                if (head_point >= fold.start && head_point <= fold.end)
                    || head_point.row == fold.start.row
                {
                    to_remove.push(idx);
                }
            }
        }
        to_remove.sort_unstable();
        to_remove.dedup();
        for idx in to_remove.into_iter().rev() {
            self.folds.remove(idx);
        }
    }

    pub fn snap_selections_to_folds(&mut self, buffer: &Buffer, action: &Action) {
        if self.folds.is_empty() {
            return;
        }

        // Detect direction based on motion/action
        let moving_right = match action {
            Action::MoveRight { .. }
            | Action::MoveDown { .. }
            | Action::MoveToWord { .. }
            | Action::MoveToWordEnd { .. }
            | Action::MoveToBigWord { .. }
            | Action::MoveToEndOfLine { .. }
            | Action::MoveToEndOfDocument { .. }
            | Action::MoveToEndOfNextLine { .. } => true,
            _ => false,
        };

        let is_move_right = matches!(action, Action::MoveRight { .. });

        let mut updated_selections = Vec::new();
        for selection in &self.selections.selections {
            let head = selection.head().to_point(buffer);
            let mut new_head = head;
            for fold in &self.folds {
                if head >= fold.start && head < fold.end {
                    new_head = if moving_right {
                        if is_move_right && head == fold.start {
                            fold.start
                        } else {
                            fold.end
                        }
                    } else {
                        fold.start
                    };
                    break;
                }
            }

            if new_head != head {
                let anchor_pos = selection.tail().to_point(buffer);
                let mut new_anchor = anchor_pos;
                if anchor_pos == head {
                    new_anchor = new_head;
                }

                let new_sel = Selection {
                    id: selection.id,
                    start: buffer.anchor_at(&new_anchor, Bias::Left),
                    end: buffer.anchor_at(&new_head, Bias::Left),
                    reversed: new_head < new_anchor,
                    goal: selection.goal,
                };
                updated_selections.push(new_sel);
            }
        }

        for new_sel in updated_selections {
            self.selections.update(buffer, &new_sel);
        }

        if let Some(first) = self.selections.first() {
            self.selections.point = first.head().to_point(buffer);
        }
    }

    pub fn enter_mode(&mut self, buffer: &Buffer, mode: Mode) {
        if self.mode == mode {
            self.clear_selections(buffer);
            return;
        }

        if self.mode == Mode::VisualBlock {
            self.selections.end_block();
        }
        if self.mode == Mode::VisualLine {
            self.selections.end_line();
        }

        self.mode = mode;

        if self.mode == Mode::VisualBlock {
            self.selections.begin_block(buffer);
        }
        if self.mode == Mode::VisualLine {
            self.selections.begin_line(buffer);
        }
    }

    pub fn current_mode(&self) -> Mode {
        return self.mode.clone();
    }

    pub fn sync(&mut self, buffer: &Buffer) {
        if self.mode == Mode::VisualBlock {
            self.selections.sync_block(buffer);
        }
        if self.mode == Mode::VisualLine {
            self.selections.sync_line(buffer);
        }
    }

    pub fn select_similar(&mut self, buffer: &Buffer) {
        if !self.has_selection(buffer) {
            let cursors = self.selections.selections.clone();
            for cursor in cursors.iter() {
                let start_sel = cursor.move_to_word(false, buffer);
                let end_sel = cursor.move_to_word_end(false, buffer);
                let next = Selection {
                    id: cursor.id,
                    start: start_sel.head(),
                    end: end_sel.head(),
                    reversed: false,
                    goal: SelectionGoal::None,
                };
                self.selections.update(buffer, &next);
            }
        } else {
            let cursor = self.selection();
            let selected_text = cursor.text(buffer);
            if let Some(mut next_match) = cursor.clone().move_to_next_match_within(
                selected_text.as_str(),
                buffer,
                buffer.row_count(),
            ) {
                for _ in 0..selected_text.len().saturating_sub(1) {
                    next_match = next_match.move_right_once(true, buffer);
                }

                let next_cursor = Selection {
                    id: cursor.id,
                    start: next_match.head(),
                    end: next_match.tail(),
                    reversed: false,
                    goal: SelectionGoal::None,
                };
                if self.selections.has_similar_cursor(&next_cursor, buffer) {
                    return;
                }

                let sel = self.add_selection(buffer);
                self.selections.update(
                    buffer,
                    &Selection {
                        id: sel.id,
                        start: cursor.head(),
                        end: cursor.tail(),
                        reversed: false,
                        goal: SelectionGoal::None,
                    },
                );
                self.selections.update(buffer, &next_cursor);
            }
        }
    }

    pub fn apply_action(
        &mut self,
        buffer: &mut Buffer,
        action: &Action,
        editor: &Editor,
        syntax_tree: Option<&crate::services::treesitter::SyntaxTree>,
    ) {
        let mut action_owned = action.clone();
        if self.mode.is_visual() {
            action_owned = action_owned.with_select(true);
        }
        let action = &action_owned;

        let mut next_action = Action::NoOp;
        match action {
            Action::InsertNewLineMotion { .. }
            | Action::Change { .. }
            | Action::ChangeLine { .. }
            | Action::ChangeMotion { .. } => next_action = Action::SetToInsert,
            _ => {}
        }

        // These actions immediately elevates mode to Insert
        if self.mode == Mode::VisualBlock {
            match action {
                Action::Delete { .. } | Action::DeleteMotion { .. } => {
                    next_action = Action::SetToInsert
                }
                _ => {}
            }
        }
        // These actions immediately drops mode back to Normal
        if self.mode.is_visual() {
            match action {
                Action::Yank { .. } | Action::YankLine { .. } | Action::YankMotion { .. } => {
                    next_action = Action::SetToNormal
                }
                _ => {}
            }
        }

        match action {
            Action::Clear => {
                self.clear_selections(buffer);
                self.enter_mode(buffer, Mode::Normal);
                return;
            }
            Action::SelectSimilar => {
                self.select_similar(buffer);
                return;
            }
            Action::SetToNormal => {
                self.enter_mode(buffer, Mode::Normal);
                return;
            }
            Action::SetToInsert => {
                self.enter_mode(buffer, Mode::Insert);
                return;
            }
            Action::SetToAppend => {
                let cursors = self.selections.selections.clone();
                for cursor in cursors.iter() {
                    let point = cursor.head().to_point(buffer);
                    let row_len = buffer.line_len(point.row);
                    if point.column < row_len {
                        self.selections.move_right(false, 1, buffer);
                    }
                }
                self.enter_mode(buffer, Mode::Insert);
                return;
            }
            Action::SetToAppendEndOfLine => {
                self.selections.move_to_end_of_line(false, buffer);
                self.enter_mode(buffer, Mode::Insert);
                return;
            }
            Action::SetToOpenLineBelow { count } => {
                let count = *count;
                self.selections.move_to_end_of_line(false, buffer);
                let current_row = self.selections.first().unwrap().head().to_point(buffer).row;
                for _ in 0..count {
                    self.insert_text(buffer, &self.new_line(buffer).to_string());
                }
                let target_point = Point {
                    row: current_row + 1,
                    column: 0,
                };
                let target_anchor = buffer.anchor_at(target_point.to_offset(buffer), Bias::Left);
                self.selections.clear(buffer);
                let first = self.selections.first().unwrap().clone();
                let next = Selection {
                    id: first.id,
                    start: target_anchor.clone(),
                    end: target_anchor,
                    reversed: false,
                    goal: SelectionGoal::None,
                };
                self.selections.point = target_point;
                self.selections.update(buffer, &next);
                self.enter_mode(buffer, Mode::Insert);
                return;
            }
            Action::SetToOpenLineAbove { count } => {
                let count = *count;
                self.selections.move_to_start_of_line(false, buffer);
                let current_row = self.selections.first().unwrap().head().to_point(buffer).row;
                for _ in 0..count {
                    self.insert_text(buffer, &self.new_line(buffer).to_string());
                }
                let target_point = Point {
                    row: current_row,
                    column: 0,
                };
                let target_anchor = buffer.anchor_at(target_point.to_offset(buffer), Bias::Left);
                self.selections.clear(buffer);
                let first = self.selections.first().unwrap().clone();
                let next = Selection {
                    id: first.id,
                    start: target_anchor.clone(),
                    end: target_anchor,
                    reversed: false,
                    goal: SelectionGoal::None,
                };
                self.selections.point = target_point;
                self.selections.update(buffer, &next);
                self.enter_mode(buffer, Mode::Insert);
                return;
            }
            Action::SetToVisual => {
                self.enter_mode(buffer, Mode::Visual);
                return;
            }
            Action::SetToInsertStartOfLineNonSpace => {
                self.selections
                    .move_to_start_of_line_non_space(false, buffer);
                self.enter_mode(buffer, Mode::Insert);
                return;
            }
            Action::SetToVisualLine => {
                self.enter_mode(buffer, Mode::VisualLine);
                return;
            }
            Action::SetToVisualBlock => {
                self.enter_mode(buffer, Mode::VisualBlock);
                return;
            }
            Action::SetToCommand
            | Action::SetToCommandSearchForward
            | Action::SetToCommandSearchBackward => {
                self.enter_mode(buffer, Mode::Command);
                return;
            }
            Action::MoveLeft { count, select } => {
                self.selections.move_left(*select, *count, buffer);
            }
            Action::MoveRight { count, select } => {
                self.selections.move_right(*select, *count, buffer);
            }
            Action::MoveUp { count, select } => {
                self.selections.move_up(*select, *count, buffer);
            }
            Action::MoveDown { count, select } => {
                self.selections.move_down(*select, *count, buffer);
            }
            Action::MoveToPreviousWord { select, count } => self
                .selections
                .move_to_previous_word(*select, *count, buffer),
            Action::MoveToWord { select, count } => {
                self.selections.move_to_next_word(*select, *count, buffer)
            }
            Action::MoveToPreviousWordEnd { select, count } => self
                .selections
                .move_to_previous_word_end(*select, *count, buffer),
            Action::MoveToWordEnd { select, count } => {
                self.selections.move_to_word_end(*select, *count, buffer)
            }
            Action::MoveToBigWord { select, count } => {
                self.selections.move_to_big_word(*select, *count, buffer)
            }
            Action::MoveToPreviousBigWord { select, count } => self
                .selections
                .move_to_previous_big_word(*select, *count, buffer),
            Action::MoveToBigWordEnd { select, count } => self
                .selections
                .move_to_big_word_end(*select, *count, buffer),
            Action::MoveToPreviousBigWordEnd { select, count } => self
                .selections
                .move_to_previous_big_word_end(*select, *count, buffer),
            Action::MoveToPreviousParagraph { select, count } => self
                .selections
                .move_to_previous_paragraph(*select, *count, buffer),
            Action::MoveToNextParagraph { select, count } => self
                .selections
                .move_to_next_paragraph(*select, *count, buffer),
            Action::MoveToPreviousCharacter {
                select,
                count,
                ch,
                till,
            } => self
                .selections
                .find_character(*select, *count, *ch, false, *till, buffer),
            Action::MoveToNextCharacter {
                select,
                count,
                ch,
                till,
            } => self
                .selections
                .find_character(*select, *count, *ch, true, *till, buffer),
            Action::SearchBackward { count } => {
                for _ in 0..*count {
                    self.selections.move_to_previous_match(&editor.search_pattern, true, buffer);
                }
            }
            Action::SearchForward { count } => {
                for _ in 0..*count {
                    self.selections.move_to_next_match(&editor.search_pattern, true, buffer);
                }
            }
            Action::MoveWithinCharacter { count, ch } => {
                let select = self.current_mode().is_visual();
                let cursors = self.selections.selections.clone();
                for cursor in cursors.iter() {
                    let mut updated = false;
                    if *ch == 'w' {
                        let start_sel = cursor.move_to_word(false, buffer);
                        let end_sel = cursor.move_to_word_end(false, buffer);
                        let next = Selection {
                            id: cursor.id,
                            start: start_sel.head(),
                            end: end_sel.head(),
                            reversed: false,
                            goal: SelectionGoal::None,
                        };
                        self.selections.update(buffer, &next);
                        updated = true;
                    } else if *ch == 'p' {
                        let prev_p = cursor
                            .move_to_previous_paragraph(false, buffer)
                            .head()
                            .to_point(buffer);
                        let next_p = cursor
                            .move_to_next_paragraph(false, buffer)
                            .head()
                            .to_point(buffer);
                        let start_row = if prev_p.row < buffer.row_count()
                            && buffer.line_len(prev_p.row) == 0
                        {
                            prev_p.row + 1
                        } else {
                            prev_p.row
                        };
                        let end_row = if next_p.row > 0 && buffer.line_len(next_p.row) == 0 {
                            next_p.row - 1
                        } else {
                            next_p.row
                        };
                        let start_offset = Point {
                            row: start_row,
                            column: 0,
                        }
                        .to_offset(buffer);
                        let end_offset = Point {
                            row: end_row,
                            column: buffer.line_len(end_row),
                        }
                        .to_offset(buffer)
                        .saturating_sub(1);
                        let next = Selection {
                            id: cursor.id,
                            start: buffer.anchor_at(start_offset, Bias::Left),
                            end: buffer.anchor_at(end_offset, Bias::Right),
                            reversed: false,
                            goal: SelectionGoal::None,
                        };
                        self.selections.update(buffer, &next);
                        updated = true;
                    } else if editor.tree_sitter {
                        if let Some(syntax_tree) = syntax_tree {
                            let byte = buffer.offset_for_anchor(&cursor.head());
                            if let Some((start_node, end_node)) =
                                syntax_tree.delimiter_boundaries_at_byte(byte)
                            {
                                let matches_ch = match ch {
                                    '{' | '}' => start_node.kind == "{",
                                    '(' | ')' => start_node.kind == "(",
                                    '[' | ']' => start_node.kind == "[",
                                    '"' => start_node.kind == "\"",
                                    '\'' => start_node.kind == "'",
                                    '`' => start_node.kind == "`",
                                    't' | '<' | '>' => {
                                        start_node.kind == "<"
                                            || start_node.kind == "start_tag"
                                            || start_node.kind == "jsx_opening_element"
                                    }
                                    _ => false,
                                };
                                if matches_ch {
                                    let start_offset = start_node.byte_range.end;
                                    let end_offset = end_node.byte_range.start.saturating_sub(1);
                                    let start_anchor = buffer.anchor_at(start_offset, Bias::Left);
                                    let end_anchor = buffer.anchor_at(end_offset, Bias::Right);
                                    let next = Selection {
                                        id: cursor.id,
                                        start: start_anchor,
                                        end: end_anchor,
                                        reversed: false,
                                        goal: SelectionGoal::None,
                                    };
                                    self.selections.update(buffer, &next);
                                    updated = true;
                                }
                            }
                        }
                    }
                    if !updated {
                        let next = cursor.move_within_character(select, *count, *ch, buffer);
                        self.selections.update(buffer, &next);
                    }
                }
            }
            Action::MoveAroundCharacter { count, ch } => {
                let select = self.current_mode().is_visual();
                let cursors = self.selections.selections.clone();
                for cursor in cursors.iter() {
                    let mut updated = false;
                    if *ch == 'w' {
                        let start_sel = cursor.move_to_word(false, buffer);
                        let next_word_head = cursor.move_to_next_word(false, buffer).head();
                        let next_word_offset = buffer.offset_for_anchor(&next_word_head);
                        let end_offset =
                            buffer.clip_offset(next_word_offset.saturating_sub(1), Bias::Left);
                        let next = Selection {
                            id: cursor.id,
                            start: start_sel.head(),
                            end: buffer.anchor_at(end_offset, Bias::Right),
                            reversed: false,
                            goal: SelectionGoal::None,
                        };
                        self.selections.update(buffer, &next);
                        updated = true;
                    } else if *ch == 'p' {
                        let prev_p = cursor
                            .move_to_previous_paragraph(false, buffer)
                            .head()
                            .to_point(buffer);
                        let next_p = cursor
                            .move_to_next_paragraph(false, buffer)
                            .head()
                            .to_point(buffer);
                        let start_row = if prev_p.row < buffer.row_count()
                            && buffer.line_len(prev_p.row) == 0
                        {
                            prev_p.row + 1
                        } else {
                            prev_p.row
                        };
                        let end_row = next_p.row;
                        let start_offset = Point {
                            row: start_row,
                            column: 0,
                        }
                        .to_offset(buffer);
                        let end_offset = Point {
                            row: end_row,
                            column: buffer.line_len(end_row),
                        }
                        .to_offset(buffer);
                        let next = Selection {
                            id: cursor.id,
                            start: buffer.anchor_at(start_offset, Bias::Left),
                            end: buffer.anchor_at(end_offset, Bias::Right),
                            reversed: false,
                            goal: SelectionGoal::None,
                        };
                        self.selections.update(buffer, &next);
                        updated = true;
                    } else if editor.tree_sitter {
                        if let Some(syntax_tree) = syntax_tree {
                            let byte = buffer.offset_for_anchor(&cursor.head());
                            if let Some((start_node, end_node)) =
                                syntax_tree.delimiter_boundaries_at_byte(byte)
                            {
                                let matches_ch = match ch {
                                    '{' | '}' => start_node.kind == "{",
                                    '(' | ')' => start_node.kind == "(",
                                    '[' | ']' => start_node.kind == "[",
                                    '"' => start_node.kind == "\"",
                                    '\'' => start_node.kind == "'",
                                    '`' => start_node.kind == "`",
                                    't' | '<' | '>' => {
                                        start_node.kind == "<"
                                            || start_node.kind == "start_tag"
                                            || start_node.kind == "jsx_opening_element"
                                    }
                                    _ => false,
                                };
                                if matches_ch {
                                    let start_offset = start_node.byte_range.start;
                                    let end_offset = end_node.byte_range.end.saturating_sub(1);
                                    let start_anchor = buffer.anchor_at(start_offset, Bias::Left);
                                    let end_anchor = buffer.anchor_at(end_offset, Bias::Right);
                                    let next = Selection {
                                        id: cursor.id,
                                        start: start_anchor,
                                        end: end_anchor,
                                        reversed: false,
                                        goal: SelectionGoal::None,
                                    };
                                    self.selections.update(buffer, &next);
                                    updated = true;
                                }
                            }
                        }
                    }
                    if !updated {
                        let next = cursor.move_around_character(select, *count, *ch, buffer);
                        self.selections.update(buffer, &next);
                    }
                }
            }

            Action::MoveToNextFunction { select, count } => {
                if editor.tree_sitter && *count > 0 {
                    if let Some(syntax_tree) = syntax_tree {
                        self.selections.move_to_syntax_target(
                            *select,
                            *count,
                            syntax_tree,
                            buffer,
                            |tree, byte| tree.next_function_after_byte(byte),
                        );
                    }
                }
            }
            Action::MoveToPreviousFunction { select, count } => {
                if editor.tree_sitter && *count > 0 {
                    if let Some(syntax_tree) = syntax_tree {
                        self.selections.move_to_syntax_target(
                            *select,
                            *count,
                            syntax_tree,
                            buffer,
                            |tree, byte| tree.previous_function_before_byte(byte),
                        );
                    }
                }
            }
            Action::MoveToNextBlock { select, count } => {
                if editor.tree_sitter && *count > 0 {
                    if let Some(syntax_tree) = syntax_tree {
                        self.selections.move_to_syntax_target(
                            *select,
                            *count,
                            syntax_tree,
                            buffer,
                            |tree, byte| tree.next_block_after_byte(byte),
                        );
                    }
                }
            }
            Action::MoveToPreviousBlock { select, count } => {
                if editor.tree_sitter && *count > 0 {
                    if let Some(syntax_tree) = syntax_tree {
                        self.selections.move_to_syntax_target(
                            *select,
                            *count,
                            syntax_tree,
                            buffer,
                            |tree, byte| tree.previous_block_before_byte(byte),
                        );
                    }
                }
            }
            Action::MoveToBlockStart { select, count } => {
                if editor.tree_sitter && *count > 0 {
                    if let Some(syntax_tree) = syntax_tree {
                        self.selections.move_to_syntax_target(
                            *select,
                            *count,
                            syntax_tree,
                            buffer,
                            |tree, byte| tree.block_start_at_byte(byte),
                        );
                    }
                }
            }
            Action::MoveToBlockEnd { select, count } => {
                if editor.tree_sitter && *count > 0 {
                    if let Some(syntax_tree) = syntax_tree {
                        self.selections.move_to_syntax_target_end(
                            *select,
                            *count,
                            syntax_tree,
                            buffer,
                            |tree, byte| tree.block_end_at_byte(byte),
                        );
                    }
                }
            }
            Action::MoveToNextClass { select, count } => {
                if editor.tree_sitter && *count > 0 {
                    if let Some(syntax_tree) = syntax_tree {
                        self.selections.move_to_syntax_target(
                            *select,
                            *count,
                            syntax_tree,
                            buffer,
                            |tree, byte| tree.next_class_after_byte(byte),
                        );
                    }
                }
            }
            Action::MoveToPreviousClass { select, count } => {
                if editor.tree_sitter && *count > 0 {
                    if let Some(syntax_tree) = syntax_tree {
                        self.selections.move_to_syntax_target(
                            *select,
                            *count,
                            syntax_tree,
                            buffer,
                            |tree, byte| tree.previous_class_before_byte(byte),
                        );
                    }
                }
            }
            Action::MoveToNextArgument { select, count } => {
                if editor.tree_sitter && *count > 0 {
                    if let Some(syntax_tree) = syntax_tree {
                        self.selections.move_to_syntax_target(
                            *select,
                            *count,
                            syntax_tree,
                            buffer,
                            |tree, byte| tree.next_argument_after_byte(byte),
                        );
                    }
                }
            }
            Action::MoveToPreviousArgument { select, count } => {
                if editor.tree_sitter && *count > 0 {
                    if let Some(syntax_tree) = syntax_tree {
                        self.selections.move_to_syntax_target(
                            *select,
                            *count,
                            syntax_tree,
                            buffer,
                            |tree, byte| tree.previous_argument_before_byte(byte),
                        );
                    }
                }
            }

            Action::MarkSet { ch } => {
                let head = self.selection().head();
                self.marks.insert(*ch, head);
            }
            Action::MarkJump { ch, select } => {
                if let Some(anchor) = self.marks.get(ch).cloned() {
                    let cursors = self.selections.selections.clone();
                    for cursor in cursors.iter() {
                        let start = if *select {
                            cursor.start.clone()
                        } else {
                            anchor.clone()
                        };
                        let next = Selection {
                            id: cursor.id,
                            start,
                            end: anchor.clone(),
                            reversed: *select && (buffer.offset_for_anchor(&anchor) < buffer.offset_for_anchor(&cursor.start)),
                            goal: SelectionGoal::None,
                        };
                        self.selections.update(buffer, &next);
                    }
                }
            }

            Action::MoveToStartOfDocument { select, count } => {
                self.selections.move_to_start_of_document(*select, buffer)
            }
            Action::MoveToEndOfDocument { select, count } => {
                self.selections.move_to_end_of_document(*select, buffer)
            }
            Action::MoveToStartOfLine { select, count } => {
                self.selections.move_to_start_of_line(*select, buffer)
            }
            Action::MoveToStartOfLineNonSpace { select, count } => self
                .selections
                .move_to_start_of_line_non_space(*select, buffer),
            Action::MoveToEndOfLine { select, count } => {
                self.selections.move_to_end_of_line(*select, buffer)
            }
            Action::MoveToStartOfPreviousLine { select, count } => self
                .selections
                .move_to_start_of_previous_line(*select, buffer),
            Action::MoveToEndOfPreviousLine { select, count } => self
                .selections
                .move_to_end_of_previous_line(*select, buffer),
            Action::MoveToStartOfNextLine { select, count } => {
                self.selections.move_to_start_of_next_line(*select, buffer)
            }
            Action::MoveToEndOfNextLine { select, count } => {
                self.selections.move_to_end_of_next_line(*select, buffer)
            }
            Action::MovePageUp { count, select } => {
                let page_size = self
                    .display_map
                    .snapshot()
                    .visible_rows
                    .saturating_sub(4)
                    .max(1);
                self.selections.move_up(*select, page_size * *count, buffer);
            }
            Action::MovePageDown { count, select } => {
                let page_size = self
                    .display_map
                    .snapshot()
                    .visible_rows
                    .saturating_sub(4)
                    .max(1);
                self.selections
                    .move_down(*select, page_size * *count, buffer);
            }
            Action::ScrollHalfPageUp { count } => {
                let half_page_size = (self
                    .display_map
                    .snapshot()
                    .visible_rows
                    .saturating_sub(4)
                    .max(2)
                    / 2)
                .max(1);
                self.selections
                    .move_up(false, half_page_size * *count, buffer);
            }
            Action::ScrollHalfPageDown { count } => {
                let half_page_size = (self
                    .display_map
                    .snapshot()
                    .visible_rows
                    .saturating_sub(4)
                    .max(2)
                    / 2)
                .max(1);
                self.selections
                    .move_down(false, half_page_size * *count, buffer);
            }
            Action::MoveToScreenTop { select, count } => {
                let display_snapshot = self.display_map.snapshot();
                let target_point = display_snapshot.display_point_to_point(
                    display::display_map::DisplayPoint::new(display_snapshot.scroll_y, 0),
                );
                self.selections
                    .move_to_line(*select, target_point.row, buffer);
            }
            Action::MoveToScreenMiddle { select, count } => {
                let display_snapshot = self.display_map.snapshot();
                let middle_display_row =
                    display_snapshot.scroll_y + display_snapshot.visible_rows / 2;
                let target_point = display_snapshot.display_point_to_point(
                    display::display_map::DisplayPoint::new(middle_display_row, 0),
                );
                self.selections
                    .move_to_line(*select, target_point.row, buffer);
            }
            Action::MoveToScreenBottom { select, count } => {
                let display_snapshot = self.display_map.snapshot();
                let bottom_display_row =
                    display_snapshot.scroll_y + display_snapshot.visible_rows.saturating_sub(1);
                let target_point = display_snapshot.display_point_to_point(
                    display::display_map::DisplayPoint::new(bottom_display_row, 0),
                );
                self.selections
                    .move_to_line(*select, target_point.row, buffer);
            }
            Action::InsertText(text) => {
                self.delete_text(buffer, 0);
                self.insert_text(buffer, text);
            }
            Action::DeleteChar { count } | Action::Delete { count } => {
                let text = if self.selections.has_selection(buffer) {
                    self.selections.text(buffer)
                } else {
                    let head_offset = buffer.offset_for_anchor(&self.selection().head());
                    let end_offset = buffer.clip_offset(head_offset + *count as usize, Bias::Right);
                    buffer
                        .as_rope()
                        .chunks_in_range(head_offset..end_offset)
                        .collect()
                };
                editor.services.clipboard.borrow_mut().set_text(&text);

                if self.delete_text(buffer, 0) {
                    //
                } else {
                    for _ in 0..*count {
                        self.delete_text(buffer, 1);
                    }
                }
            }
            Action::DeleteCharBefore { count } => {
                let text = if self.selections.has_selection(buffer) {
                    self.selections.text(buffer)
                } else {
                    let head_offset = buffer.offset_for_anchor(&self.selection().head());
                    let start_offset = if head_offset >= *count as usize {
                        head_offset - *count as usize
                    } else {
                        0
                    };
                    buffer
                        .as_rope()
                        .chunks_in_range(start_offset..head_offset)
                        .collect()
                };
                editor.services.clipboard.borrow_mut().set_text(&text);

                if self.delete_text(buffer, 0) {
                    //
                } else {
                    for _ in 0..*count {
                        self.selections.move_left(false, 1, buffer);
                        self.delete_text(buffer, 1);
                    }
                }
            }
            Action::DeleteLines {
                start_line,
                end_line,
            } => {
                let start_row = start_line.saturating_sub(1);
                let end_row = end_line.saturating_sub(1);

                let max_row = buffer.row_count().saturating_sub(1);
                let start_row = std::cmp::min(start_row, max_row);
                let end_row = std::cmp::min(end_row, max_row);
                let start_row = std::cmp::min(start_row, end_row);

                let start_offset = Point::new(start_row, 0).to_offset(buffer);
                let end_offset = if end_row + 1 < buffer.row_count() {
                    Point::new(end_row + 1, 0).to_offset(buffer)
                } else {
                    Point::new(end_row, buffer.line_len(end_row)).to_offset(buffer)
                };

                let text: String = buffer
                    .as_rope()
                    .chunks_in_range(start_offset..end_offset)
                    .collect();
                editor.services.clipboard.borrow_mut().set_lines(text);

                buffer.edit([(start_offset..end_offset, "")]);
            }
            Action::YankLines {
                start_line,
                end_line,
            } => {
                let start_row = start_line.saturating_sub(1);
                let end_row = end_line.saturating_sub(1);

                let max_row = buffer.row_count().saturating_sub(1);
                let start_row = std::cmp::min(start_row, max_row);
                let end_row = std::cmp::min(end_row, max_row);
                let start_row = std::cmp::min(start_row, end_row);

                let start_offset = Point::new(start_row, 0).to_offset(buffer);
                let end_offset = if end_row + 1 < buffer.row_count() {
                    Point::new(end_row + 1, 0).to_offset(buffer)
                } else {
                    Point::new(end_row, buffer.line_len(end_row)).to_offset(buffer)
                };

                let text: String = buffer
                    .as_rope()
                    .chunks_in_range(start_offset..end_offset)
                    .collect();
                editor.services.clipboard.borrow_mut().set_lines(text);
            }
            Action::DeleteLine { count } | Action::ChangeLine { count } => {
                let selections = self.selections.selections.clone();
                let point = self.selections.point;
                let anchor = self.selections.anchor.clone();

                self.selections.move_to_start_of_line(false, buffer);
                if *count > 1 {
                    self.selections
                        .move_down(true, count.saturating_sub(1), buffer);
                }
                self.selections.move_to_end_of_line(true, buffer);

                let mut text = self.selections.text(buffer);
                if !text.ends_with('\n') {
                    text.push('\n');
                }
                editor.services.clipboard.borrow_mut().set_lines(text);

                self.selections.selections = selections;
                self.selections.point = point;
                self.selections.anchor = anchor;

                self.delete_current_line(buffer, *count);
            }
            Action::JoinLines { count } => {
                let count = *count;
                let lines_to_join = if count <= 1 { 2 } else { count };
                let newlines_to_remove = lines_to_join - 1;

                let current_row = self.selections.first().unwrap().head().to_point(buffer).row;
                let total_rows = buffer.row_count();
                let actual_removes = std::cmp::min(
                    newlines_to_remove as usize,
                    (total_rows.saturating_sub(1) - current_row) as usize,
                );

                let mut target_col = None;

                for _ in 0..actual_removes {
                    let current_line_len = buffer.line_len(current_row);
                    if target_col.is_none() {
                        target_col = Some(current_line_len);
                    }

                    let end_of_current = Point {
                        row: current_row,
                        column: current_line_len,
                    }
                    .to_offset(buffer);
                    let next_line_text = buffer.row_text(current_row + 1);
                    let leading_whitespace_len = next_line_text
                        .chars()
                        .take_while(|c| c.is_whitespace())
                        .map(|c| c.len_utf8())
                        .sum::<usize>();

                    let delete_start = end_of_current;
                    let delete_end = end_of_current + 1 + leading_whitespace_len;

                    let current_line_text = buffer.row_text(current_row);
                    let ends_with_space = current_line_text.as_str().ends_with(char::is_whitespace);
                    let next_first_non_space = next_line_text.as_str().trim_start().chars().next();

                    let replacement = if ends_with_space || next_first_non_space.is_none() {
                        ""
                    } else {
                        " "
                    };

                    buffer.edit([(delete_start..delete_end, replacement)]);
                }

                if let Some(col) = target_col {
                    let target_point = Point {
                        row: current_row,
                        column: col,
                    };
                    let target_anchor =
                        buffer.anchor_at(target_point.to_offset(buffer), Bias::Left);
                    self.selections.clear(buffer);
                    let first = self.selections.first().unwrap().clone();
                    let next = Selection {
                        id: first.id,
                        start: target_anchor.clone(),
                        end: target_anchor,
                        reversed: false,
                        goal: SelectionGoal::None,
                    };
                    self.selections.point = target_point;
                    self.selections.update(buffer, &next);
                }
            }
            Action::ChangeMotion { count, motion } | Action::DeleteMotion { count, motion } => {
                let mut motion = (**motion).clone();
                let is_textobject = match &motion {
                    Action::MoveToWord { .. }
                    | Action::MoveToNextParagraph { .. }
                    | Action::MoveToEndOfLine { .. }
                    | Action::MoveWithinCharacter { .. }
                    | Action::MoveAroundCharacter { .. } => true,
                    _ => false,
                };

                match &mut motion {
                    Action::MoveUp { select, .. }
                    | Action::MoveDown { select, .. }
                    | Action::MoveLeft { select, .. }
                    | Action::MoveRight { select, .. }
                    | Action::MoveToPreviousWord { select, .. }
                    | Action::MoveToWord { select, .. }
                    | Action::MoveToPreviousWordEnd { select, .. }
                    | Action::MoveToWordEnd { select, .. }
                    | Action::MoveToStartOfDocument { select, .. }
                    | Action::MoveToEndOfDocument { select, .. }
                    | Action::MoveToStartOfLine { select, .. }
                    | Action::MoveToStartOfLineNonSpace { select, .. }
                    | Action::MoveToEndOfLine { select, .. }
                    | Action::MoveToPreviousParagraph { select, .. }
                    | Action::MoveToNextParagraph { select, .. }
                    | Action::MoveToPreviousCharacter { select, .. }
                    | Action::MoveToNextCharacter { select, .. }
                    | Action::MarkJump { select, .. } => *select = true,
                    _ => {}
                }

                let selections = self.selections.selections.clone();
                let point = self.selections.point;
                let anchor = self.selections.anchor.clone();

                for _ in 0..*count {
                    self.apply_action(buffer, &motion, editor, syntax_tree);
                }

                let text = self.selections.text(buffer);
                editor.services.clipboard.borrow_mut().set_text(text);

                self.selections.selections = selections;
                self.selections.point = point;
                self.selections.anchor = anchor;

                if is_textobject {
                    let inclusive = matches!(
                        motion,
                        Action::MoveWithinCharacter { .. } | Action::MoveAroundCharacter { .. }
                    );
                    for _idx in 0..*count {
                        self.apply_action(buffer, &motion, editor, syntax_tree);
                        self.delete_text_object(buffer, inclusive);
                    }
                } else {
                    for _ in 0..*count {
                        self.apply_action(buffer, &motion, editor, syntax_tree);
                        self.delete_text(buffer, 0);
                    }
                }
            }
            Action::Change { count } => {
                let text = self.selections.text(buffer);
                if !text.is_empty() {
                    editor.services.clipboard.borrow_mut().set_text(text);
                }
                self.delete_text(buffer, 0);
            }
            Action::InsertNewLine { count } => {
                let text = self.selections.text(buffer);
                if !text.is_empty() {
                    editor.services.clipboard.borrow_mut().set_text(text);
                }
                self.delete_text(buffer, 0);
                for _ in 0..*count {
                    self.insert_text(buffer, &self.new_line(buffer).to_string());
                }
            }
            Action::InsertNewLineMotion { count, motion } => {
                let mut motion = (**motion).clone();
                for _ in 0..*count {
                    self.apply_action(buffer, &motion, editor, syntax_tree);
                    self.insert_text(buffer, &self.new_line(buffer).to_string());
                    motion = Action::NoOp;
                }
                self.selections.move_left(false, 1, buffer);
            }
            Action::InsertTab => {
                for _ in 0..4 {
                    self.insert_text(buffer, " ");
                }
            }
            Action::YankMotion { count, motion } => {
                self.yank_motion(buffer, *count, motion, editor, syntax_tree);
            }
            Action::YankLine { count } => {
                self.yank_current_line(buffer, *count, editor);
            }
            Action::Put { count } => {
                self.paste(buffer, *count, editor);
            }
            Action::Undo { count } => self.undo(buffer, *count),
            Action::Redo { count } => self.redo(buffer, *count),
            Action::Fold { count } => {
                self.fold(buffer, *count, editor, syntax_tree);
            }
            Action::Unfold { count } => {
                self.unfold(buffer, *count, editor, syntax_tree);
            }
            Action::NoOp | Action::Quit => {
                return;
            }
            _ => {}
        }

        self.apply_action(buffer, &next_action, editor, syntax_tree);
        self.snap_selections_to_folds(buffer, action);
        self.sync(buffer);
    }

    fn yank_motion(
        &mut self,
        buffer: &mut Buffer,
        count: u32,
        motion: &Action,
        editor: &Editor,
        syntax_tree: Option<&crate::services::treesitter::SyntaxTree>,
    ) {
        let mut motion = motion.clone();
        match &mut motion {
            Action::MoveUp { select, .. }
            | Action::MoveDown { select, .. }
            | Action::MoveLeft { select, .. }
            | Action::MoveRight { select, .. }
            | Action::MoveToPreviousWord { select, .. }
            | Action::MoveToWord { select, .. }
            | Action::MoveToPreviousWordEnd { select, .. }
            | Action::MoveToWordEnd { select, .. }
            | Action::MoveToStartOfDocument { select, .. }
            | Action::MoveToEndOfDocument { select, .. }
            | Action::MoveToStartOfLine { select, .. }
            | Action::MoveToStartOfLineNonSpace { select, .. }
            | Action::MoveToEndOfLine { select, .. }
            | Action::MoveToPreviousParagraph { select, .. }
            | Action::MoveToNextParagraph { select, .. }
            | Action::MoveToPreviousCharacter { select, .. }
            | Action::MoveToNextCharacter { select, .. }
            | Action::MarkJump { select, .. } => *select = true,
            _ => {}
        }

        let selections = self.selections.selections.clone();
        let point = self.selections.point;
        let anchor = self.selections.anchor.clone();

        for _ in 0..count {
            self.apply_action(buffer, &motion, editor, syntax_tree);
        }
        let text = self.selections.text(buffer);
        editor.services.clipboard.borrow_mut().set_text(text);

        self.selections.selections = selections;
        self.selections.point = point;
        self.selections.anchor = anchor;
    }

    fn yank_current_line(&mut self, buffer: &Buffer, count: u32, editor: &Editor) {
        let selections = self.selections.selections.clone();
        let point = self.selections.point;
        let anchor = self.selections.anchor.clone();

        self.selections.move_to_start_of_line(false, buffer);
        if count > 1 {
            self.selections
                .move_down(true, count.saturating_sub(1), buffer);
        }
        self.selections.move_to_end_of_line(true, buffer);

        let mut text = self.selections.text(buffer);
        if !text.ends_with('\n') {
            text.push('\n');
        }
        editor.services.clipboard.borrow_mut().set_lines(text);

        self.selections.selections = selections;
        self.selections.point = point;
        self.selections.anchor = anchor;
    }

    fn paste(&mut self, buffer: &mut Buffer, count: u32, editor: &Editor) {
        let clipboard = editor.services.clipboard.borrow();
        if clipboard.is_empty() || count == 0 {
            return;
        }
        let text = clipboard.text().to_string();
        let kind = clipboard.kind();
        drop(clipboard);

        match kind {
            ClipboardKind::Character | ClipboardKind::Block => {
                self.selections.move_right(false, 1, buffer);
                for _ in 0..count {
                    self.insert_text(buffer, &text);
                }
            }
            ClipboardKind::Line => {
                let cursor_row = self.selection().head().to_point(buffer).row;
                let has_next_line = cursor_row + 1 < buffer.row_count();
                if has_next_line {
                    self.selections.move_to_start_of_next_line(false, buffer);
                } else {
                    self.selections.move_to_end_of_line(false, buffer);
                    self.insert_text(buffer, &self.new_line(buffer).to_string());
                }
                for _ in 0..count {
                    self.insert_text(buffer, &text);
                }
            }
        }
    }

    fn insert_text(&mut self, buffer: &mut Buffer, text: &str) {
        let cursors = self.selections.selections.clone();
        for cursor in cursors.iter() {
            let start = buffer.offset_for_anchor(&cursor.head());
            buffer.edit([(start..start, text)]);

            let new_offset = buffer.clip_offset(start + text.len(), Bias::Left);
            let new_head = buffer.anchor_at(new_offset, Bias::Left);
            self.selections.update(
                buffer,
                &Selection {
                    id: cursor.id,
                    start: new_head,
                    end: new_head,
                    reversed: false,
                    goal: SelectionGoal::None,
                },
            );
        }
    }

    fn delete_text(&mut self, buffer: &mut Buffer, count: usize) -> bool {
        let mut delete_count = 0;
        let cursors = self.selections.selections.clone();
        for cursor in cursors.iter() {
            let (start, mut end) = {
                let (cs, ce) = if cursor.head().cmp(&cursor.tail(), buffer) == Ordering::Less {
                    (
                        cursor.head().bias_left(buffer),
                        cursor.tail().bias_right(buffer),
                    )
                } else {
                    (
                        cursor.tail().bias_left(buffer),
                        cursor.head().bias_right(buffer),
                    )
                };

                let start = buffer.offset_for_anchor(&cs);
                let mut end = buffer.offset_for_anchor(&ce);
                if start != end {
                    end = buffer.clip_offset(end + 1, Bias::Right);
                }
                (start, end)
            };

            if count != 0 {
                end = buffer.clip_offset(end + count, Bias::Right);
            }

            if start != end {
                delete_count += 1;
                self.remove_overlapping_folds(buffer, start, end);
                buffer.edit([(start..end, "")]);
            }
        }
        return delete_count > 0;
    }

    fn delete_text_object(&mut self, buffer: &mut Buffer, inclusive: bool) -> bool {
        let mut delete_count = 0;
        let cursors = self.selections.selections.clone();
        for cursor in cursors.iter() {
            let (start, mut end) = {
                let (cs, ce) = if cursor.head().cmp(&cursor.tail(), buffer) == Ordering::Less {
                    (
                        cursor.head().bias_left(buffer),
                        cursor.tail().bias_right(buffer),
                    )
                } else {
                    (
                        cursor.tail().bias_left(buffer),
                        cursor.head().bias_right(buffer),
                    )
                };

                let start = buffer.offset_for_anchor(&cs);
                let mut end = buffer.offset_for_anchor(&ce);
                if inclusive && start != end {
                    end = buffer.clip_offset(end + 1, Bias::Right);
                }
                (start, end)
            };

            if start != end {
                delete_count += 1;
                buffer.edit([(start..end, "")]);
            }
        }
        return delete_count > 0;
    }

    pub fn delete_current_line(&mut self, buffer: &mut Buffer, count: u32) {
        if self.delete_text(buffer, 0) {
            return;
        }
        for _ in 0..count {
            let cursors = self.selections.selections.clone();
            for cursor in cursors.iter() {
                let mut point = cursor.head().to_point(buffer);
                let (start, end) = {
                    point.column = 0;
                    let start = buffer.offset_for_anchor(&buffer.anchor_at(&point, Bias::Left));
                    if point.row < buffer.row_count() {
                        point.row += 1;
                    } else {
                        point.column = buffer.line_len(point.row);
                    }
                    let end = buffer.clip_offset(
                        buffer.offset_for_anchor(&buffer.anchor_at(&point, Bias::Right)),
                        Bias::Right,
                    );
                    (start, end)
                };
                if start != end {
                    self.remove_overlapping_folds(buffer, start, end);
                    buffer.edit([(start..end, "")]);
                }
            }
        }
    }

    fn remove_overlapping_folds(&mut self, buffer: &Buffer, start: usize, end: usize) {
        self.folds.retain(|fold| {
            let fold_start = fold.start.to_offset(buffer);
            let fold_end = fold.end.to_offset(buffer);
            !(end > fold_start.saturating_sub(1) && start < fold_end + 1)
        });
    }

    pub fn selection(&self) -> Selection<Anchor> {
        self.selections.first().unwrap().clone()
    }

    pub fn add_selection(&mut self, buffer: &Buffer) -> Selection<Anchor> {
        self.selections.add(buffer, 0)
    }

    pub fn selections(&self) -> &SelectionCollection {
        &self.selections
    }

    pub fn selections_mut(&mut self) -> &mut SelectionCollection {
        &mut self.selections
    }

    pub fn clear_selections(&mut self, buffer: &Buffer) {
        self.selections.clear(buffer);
    }

    pub fn has_selection(&self, buffer: &Buffer) -> bool {
        self.selections.has_selection(buffer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::buffers::{BufferManager, TextBuffer};
    use crate::services::treesitter::TreeSitterParser;
    use crate::services::treesitter::grammars::Grammar;

    const MAIN_WIN: usize = crate::ui::WindowId::MainWindow as usize;

    struct TestEnv {
        editor: Editor,
        buffer_manager: BufferManager,
        ui: crate::ui::Ui,
    }

    impl TestEnv {
        fn new() -> Self {
            let mut editor = Editor::new().unwrap();
            let mut buffer_manager = BufferManager::new();
            buffer_manager.add_buffer_for_path("").unwrap();
            let mut ui = crate::ui::Ui::new();
            let active_buf = &buffer_manager.buffers[0];
            if let Some(win) = ui.windows.get_mut(&MAIN_WIN) {
                win.buffer_id = Some(active_buf.id);
                win.doc = Some(Document::new_with_buffer(
                    active_buf.id,
                    &active_buf.buffer,
                    &active_buf.file_path,
                ));
            }
            Self {
                editor,
                buffer_manager,
                ui,
            }
        }

        fn apply_action(&mut self, action: &Action) {
            self.editor
                .apply_active_action(&mut self.ui, &mut self.buffer_manager, action);
        }

        fn doc(&self) -> &Document {
            self.ui
                .windows
                .get(&MAIN_WIN)
                .unwrap()
                .doc
                .as_ref()
                .unwrap()
        }

        fn doc_mut(&mut self) -> &mut Document {
            self.ui
                .windows
                .get_mut(&MAIN_WIN)
                .unwrap()
                .doc
                .as_mut()
                .unwrap()
        }

        fn buffer(&self) -> &text::Buffer {
            &self.buffer_manager.buffers[0].buffer
        }
    }

    #[test]
    fn consecutive_insert_text_actions_leave_cursor_after_inserted_text() {
        let mut env = TestEnv::new();

        let buffer = &env.buffer_manager.buffers[0].buffer;
        let doc = env
            .ui
            .windows
            .get_mut(&MAIN_WIN)
            .unwrap()
            .doc
            .as_mut()
            .unwrap();
        doc.enter_mode(buffer, Mode::Insert);
        env.apply_action(&Action::InsertText("abc".into()));
        env.apply_action(&Action::MoveLeft {
            select: false,
            count: 2,
        });
        env.apply_action(&Action::InsertText("x".into()));
        env.apply_action(&Action::InsertText("y".into()));

        assert_eq!(&env.buffer().row_text(0), "axybc");
        assert_eq!(
            env.doc().selection().head().to_point(env.buffer()).column,
            3
        );
    }

    #[test]
    fn newline_and_tab_insertions_do_not_advance_twice() {
        let mut env = TestEnv::new();

        let buffer = &env.buffer_manager.buffers[0].buffer;
        let doc = env
            .ui
            .windows
            .get_mut(&MAIN_WIN)
            .unwrap()
            .doc
            .as_mut()
            .unwrap();
        doc.enter_mode(buffer, Mode::Insert);
        env.apply_action(&Action::InsertText("abc".into()));
        env.apply_action(&Action::MoveLeft {
            select: false,
            count: 2,
        });
        env.apply_action(&Action::InsertNewLine { count: 1 });

        assert_eq!(&env.buffer().row_text(0), "a");
        assert_eq!(&env.buffer().row_text(1), "bc");
        assert_eq!(
            env.doc().selection().head().to_point(env.buffer()),
            Point::new(1, 0)
        );

        let mut env2 = TestEnv::new();
        let buffer2 = &env2.buffer_manager.buffers[0].buffer;
        let doc2 = env2
            .ui
            .windows
            .get_mut(&MAIN_WIN)
            .unwrap()
            .doc
            .as_mut()
            .unwrap();
        doc2.enter_mode(buffer2, Mode::Insert);
        env2.apply_action(&Action::InsertText("abc".into()));
        env2.apply_action(&Action::MoveLeft {
            select: false,
            count: 2,
        });
        env2.apply_action(&Action::InsertTab);

        assert_eq!(&env2.buffer().row_text(0), "a    bc");
        assert_eq!(
            env2.doc().selection().head().to_point(env2.buffer()).column,
            5
        );
    }

    #[test]
    fn yank_motion_copies_selection_and_paste_inserts_after_cursor() {
        let mut env = TestEnv::new();

        env.apply_action(&Action::InsertText("abcde".into()));
        env.apply_action(&Action::MoveLeft {
            select: false,
            count: 4,
        });

        env.apply_action(&Action::YankMotion {
            count: 1,
            motion: Box::new(Action::MoveRight {
                select: true,
                count: 1,
            }),
        });

        assert_eq!(env.editor.services.clipboard.borrow().text(), "bc");
        assert_eq!(
            env.doc().selection().head().to_point(env.buffer()).column,
            1
        );

        env.apply_action(&Action::Put { count: 1 });

        assert_eq!(&env.buffer().row_text(0), "abbccde");
    }

    #[test]
    fn test_marks_set_and_jump() {
        let mut env = TestEnv::new();

        env.apply_action(&Action::InsertText("abcde".into()));
        env.apply_action(&Action::MoveLeft {
            select: false,
            count: 3,
        });

        env.apply_action(&Action::MarkSet { ch: 'a' });

        env.apply_action(&Action::MoveToStartOfDocument { select: false, count: 1 });
        assert_eq!(
            env.doc().selection().head().to_point(env.buffer()).column,
            0
        );

        env.apply_action(&Action::MarkJump { ch: 'a', select: false });
        assert_eq!(
            env.doc().selection().head().to_point(env.buffer()).column,
            2
        );

        env.apply_action(&Action::MoveToStartOfDocument { select: false, count: 1 });
        env.apply_action(&Action::MarkJump { ch: 'a', select: true });
        assert_eq!(
            env.doc().selection().head().to_point(env.buffer()).column,
            2
        );
        assert_eq!(
            env.doc().selection().start.to_point(env.buffer()).column,
            0
        );
    }

    #[test]
    fn test_select_similar() {
        let mut env = TestEnv::new();

        env.apply_action(&Action::InsertText("hello hello hello".into()));
        env.apply_action(&Action::MoveLeft {
            select: false,
            count: 11,
        });

        env.apply_action(&Action::SelectSimilar);
        assert_eq!(
            env.doc().selection().start.to_point(env.buffer()).column,
            6
        );
        assert_eq!(
            env.doc().selection().end.to_point(env.buffer()).column,
            10
        );

        env.apply_action(&Action::SelectSimilar);
        assert_eq!(env.doc().selections().selections.len(), 2);

        env.apply_action(&Action::SelectSimilar);
        assert_eq!(env.doc().selections().selections.len(), 3);
    }

    #[test]
    fn yank_current_line_and_paste_create_a_line_below() {
        let mut env = TestEnv::new();

        env.apply_action(&Action::InsertText("abc".into()));
        env.apply_action(&Action::MoveLeft {
            select: false,
            count: 1,
        });

        env.apply_action(&Action::YankLine { count: 1 });
        assert_eq!(
            env.editor.services.clipboard.borrow().text(),
            "abc
"
        );
        assert_eq!(
            env.editor.services.clipboard.borrow().kind(),
            ClipboardKind::Line
        );

        env.apply_action(&Action::Put { count: 1 });
        assert_eq!(&env.buffer().row_text(0), "abc");
        assert_eq!(&env.buffer().row_text(1), "abc");
    }

    #[test]
    fn test_join_lines() {
        let mut env = TestEnv::new();

        env.apply_action(&Action::InsertText(
            "line 1
  line 2
line 3"
                .into(),
        ));
        // Move back to line 1
        env.apply_action(&Action::MoveUp {
            select: false,
            count: 2,
        });

        // Join line 1 and line 2
        env.apply_action(&Action::JoinLines { count: 1 });

        assert_eq!(&env.buffer().row_text(0), "line 1 line 2");
        assert_eq!(&env.buffer().row_text(1), "line 3");

        // Verify cursor is on the space
        assert_eq!(
            env.doc().selection().head().to_point(env.buffer()),
            Point { row: 0, column: 6 }
        );
    }

    #[test]
    fn test_delete_around_character() {
        let mut env = TestEnv::new();

        env.apply_action(&Action::InsertText("a (hello) b".into()));
        // Move cursor inside parens
        env.apply_action(&Action::MoveLeft {
            select: false,
            count: 7,
        });

        // Execute DeleteMotion around '('
        env.apply_action(&Action::DeleteMotion {
            count: 1,
            motion: Box::new(Action::MoveAroundCharacter { count: 1, ch: '(' }),
        });

        assert_eq!(&env.buffer().row_text(0), "a  b");
    }

    #[test]
    fn test_delete_word() {
        let mut env = TestEnv::new();

        env.apply_action(&Action::InsertText("abc def".into()));
        env.apply_action(&Action::MoveLeft {
            select: false,
            count: 7,
        });

        env.apply_action(&Action::DeleteMotion {
            count: 1,
            motion: Box::new(Action::MoveToWord {
                count: 1,
                select: false,
            }),
        });

        assert_eq!(&env.buffer().row_text(0), "def");
    }

    #[test]
    fn test_delete_inner_word() {
        let mut env = TestEnv::new();

        env.apply_action(&Action::InsertText("abc def ghi".into()));
        // Move to 'e' in 'def'
        env.apply_action(&Action::MoveLeft {
            select: false,
            count: 6,
        });

        // diw
        env.apply_action(&Action::DeleteMotion {
            count: 1,
            motion: Box::new(Action::MoveWithinCharacter { count: 1, ch: 'w' }),
        });

        assert_eq!(&env.buffer().row_text(0), "abc  ghi");
    }

    #[test]
    fn test_delete_around_word() {
        let mut env = TestEnv::new();

        env.apply_action(&Action::InsertText("abc def ghi".into()));
        // Move to 'e' in 'def'
        env.apply_action(&Action::MoveLeft {
            select: false,
            count: 6,
        });

        // daw
        env.apply_action(&Action::DeleteMotion {
            count: 1,
            motion: Box::new(Action::MoveAroundCharacter { count: 1, ch: 'w' }),
        });

        assert_eq!(&env.buffer().row_text(0), "abc ghi");
    }

    #[test]
    fn test_treesitter_folding() {
        let mut env = TestEnv::new();

        let text = "fn main() {
    let x = 1;
    let y = 2;
}";
        env.buffer_manager.buffers[0] = TextBuffer::new_with_text(text);
        if let Some(win) = env.ui.windows.get_mut(&MAIN_WIN) {
            let active_buf = &env.buffer_manager.buffers[0];
            win.buffer_id = Some(active_buf.id);
            win.doc = Some(Document::new_with_buffer(
                active_buf.id,
                &active_buf.buffer,
                &active_buf.file_path,
            ));
        }

        env.buffer_manager.buffers[0].grammar = Some(Grammar::Rust);

        let mut parser = TreeSitterParser::new(Grammar::Rust).unwrap();
        let tree = parser.parse(env.buffer().snapshot(), None).unwrap();
        env.buffer_manager.buffers[0].syntax_tree = Some(tree);

        env.apply_action(&Action::MoveDown {
            select: false,
            count: 1,
        });

        env.apply_action(&Action::Fold { count: 1 });

        assert_eq!(env.doc().folds.len(), 1);
        let fold = &env.doc().folds[0];
        assert_eq!(fold.start.row, 0);
        assert_eq!(fold.start.column, 11);
        assert_eq!(fold.end.row, 3);
        assert_eq!(fold.end.column, 0);

        let head = env.doc().selection().head().to_point(env.buffer());
        assert_eq!(head.row, 0);
        assert_eq!(head.column, 10);

        env.apply_action(&Action::Unfold { count: 1 });
        assert_eq!(env.doc().folds.len(), 0);
    }

    #[test]
    fn test_fold_multiline_only() {
        let mut env = TestEnv::new();

        let text = "fn main() { let x = 1; }";
        env.buffer_manager.buffers[0] = TextBuffer::new_with_text(text);
        if let Some(win) = env.ui.windows.get_mut(&MAIN_WIN) {
            let active_buf = &env.buffer_manager.buffers[0];
            win.buffer_id = Some(active_buf.id);
            win.doc = Some(Document::new_with_buffer(
                active_buf.id,
                &active_buf.buffer,
                &active_buf.file_path,
            ));
        }

        env.buffer_manager.buffers[0].grammar = Some(Grammar::Rust);

        let mut parser = TreeSitterParser::new(Grammar::Rust).unwrap();
        let tree = parser.parse(env.buffer().snapshot(), None).unwrap();
        env.buffer_manager.buffers[0].syntax_tree = Some(tree);

        // Move to the inside of the block (e.g. column 15)
        env.apply_action(&Action::MoveRight {
            select: false,
            count: 15,
        });

        // With fold_multiline_only = true, fold should not work
        env.editor.fold_multiline_only = true;
        env.apply_action(&Action::Fold { count: 1 });
        assert_eq!(env.doc().folds.len(), 0);

        // With fold_multiline_only = false, fold should work
        env.editor.fold_multiline_only = false;
        env.apply_action(&Action::Fold { count: 1 });
        assert_eq!(env.doc().folds.len(), 1);

        let head = env.doc().selection().head().to_point(env.buffer());
        assert_eq!(head.row, 0);
        assert_eq!(head.column, 10);
    }

    #[test]
    fn test_fold_deletion() {
        let mut env = TestEnv::new();

        let text = "line 1
line 2
line 3
line 4";
        env.buffer_manager.buffers[0] = TextBuffer::new_with_text(text);
        if let Some(win) = env.ui.windows.get_mut(&MAIN_WIN) {
            let active_buf = &env.buffer_manager.buffers[0];
            win.buffer_id = Some(active_buf.id);
            win.doc = Some(Document::new_with_buffer(
                active_buf.id,
                &active_buf.buffer,
                &active_buf.file_path,
            ));
        }

        let fold = display::fold_map::Fold {
            start: Point::new(1, 0),
            end: Point::new(2, 6),
        };
        env.doc_mut().folds.push(fold);
        assert_eq!(env.doc().folds.len(), 1);

        // Manually place cursor at Point::new(1, 0)
        let anchor = env.buffer_manager.buffers[0]
            .buffer
            .anchor_at(&Point::new(1, 0), Bias::Left);
        let selection = Selection {
            id: 0,
            start: anchor.clone(),
            end: anchor,
            reversed: false,
            goal: SelectionGoal::None,
        };
        let buffer = &env.buffer_manager.buffers[0].buffer;
        let doc = env
            .ui
            .windows
            .get_mut(&MAIN_WIN)
            .unwrap()
            .doc
            .as_mut()
            .unwrap();
        doc.selections.update(buffer, &selection);

        env.apply_action(&Action::Delete { count: 1 });

        assert_eq!(env.doc().folds.len(), 0);
    }

    #[test]
    fn test_fold_deletion_by_boundary_delete() {
        let mut env = TestEnv::new();

        let text = "fn main() {\n    let x = 1;\n}";
        env.buffer_manager.buffers[0] = TextBuffer::new_with_text(text);
        if let Some(win) = env.ui.windows.get_mut(&MAIN_WIN) {
            let active_buf = &env.buffer_manager.buffers[0];
            win.buffer_id = Some(active_buf.id);
            win.doc = Some(Document::new_with_buffer(
                active_buf.id,
                &active_buf.buffer,
                &active_buf.file_path,
            ));
        }

        // Fold starts after '{' (offset 11) and ends before '}' (offset 26)
        let fold = display::fold_map::Fold {
            start: Point::new(0, 11),
            end: Point::new(2, 0),
        };
        env.doc_mut().folds.push(fold.clone());
        assert_eq!(env.doc().folds.len(), 1);

        // Place cursor at '}' (row 2, col 0)
        let anchor = env.buffer_manager.buffers[0]
            .buffer
            .anchor_at(&Point::new(2, 0), Bias::Left);
        let selection = Selection {
            id: 0,
            start: anchor.clone(),
            end: anchor,
            reversed: false,
            goal: SelectionGoal::None,
        };
        let buffer = &env.buffer_manager.buffers[0].buffer;
        let doc = env
            .ui
            .windows
            .get_mut(&MAIN_WIN)
            .unwrap()
            .doc
            .as_mut()
            .unwrap();
        doc.selections.update(buffer, &selection);

        // Delete '}' forward should delete the fold
        env.apply_action(&Action::Delete { count: 1 });
        assert_eq!(env.doc().folds.len(), 0);
    }

    #[test]
    fn test_document_clear() {
        let mut env = TestEnv::new();
        let text = "some text\nwith folds and cursors";
        env.buffer_manager.buffers[0] = TextBuffer::new_with_text(text);

        let active_buf = &env.buffer_manager.buffers[0];
        if let Some(win) = env.ui.windows.get_mut(&MAIN_WIN) {
            win.buffer_id = Some(active_buf.id);
            win.doc = Some(Document::new_with_buffer(
                active_buf.id,
                &active_buf.buffer,
                &active_buf.file_path,
            ));
        }

        // Add a fold
        let fold = display::fold_map::Fold {
            start: Point::new(0, 5),
            end: Point::new(1, 0),
        };
        env.doc_mut().folds.push(fold);
        assert_eq!(env.doc().folds.len(), 1);

        // Clear the buffer and document
        let buf = &mut env.buffer_manager.buffers[0];
        buf.clear();
        let doc = env
            .ui
            .windows
            .get_mut(&MAIN_WIN)
            .unwrap()
            .doc
            .as_mut()
            .unwrap();
        doc.clear(&buf.buffer);

        // Verify everything was reset
        assert_eq!(buf.buffer.snapshot().text(), "");
        assert_eq!(doc.folds.len(), 0);
        assert_eq!(doc.mode, Mode::Normal);
        assert_eq!(doc.selections.text(&buf.buffer), "");
    }
}
