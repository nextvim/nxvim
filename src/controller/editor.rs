use crate::model::{BufferState, WindowState};
use display_map::DisplayPoint;
use std::cmp::Ordering;
use sum_tree::Bias;
use text::{Point, Selection, SelectionGoal, ToOffset, ToPoint};
use vim_buffer::{Buffer, Motions, SelectionSet};
use vim_input::{Action, Mode};

pub struct Editor;

impl Editor {
    pub fn new() -> Self {
        Self
    }

    /// Executes an action by mutably accessing the buffer and both of its contexts.
    pub fn execute(
        &self,
        mode: Mode,
        action: &Action,
        buffer: &mut Buffer,
        buffer_context: &mut BufferState,
        buffer_display_context: &mut WindowState,
        services: &mut crate::app::services::Services,
    ) -> Result<Option<Mode>, Box<dyn std::error::Error>> {
        // Ensure there is at least one selection
        if buffer_display_context.selections.selections.is_empty() {
            buffer_display_context
                .selections
                .add(buffer.as_text_buffer(), 0);
        }

        let new_mode = self.apply_action(
            mode,
            action,
            buffer,
            buffer_context,
            buffer_display_context,
            services,
        );

        Ok(new_mode)
    }

    pub fn enter_mode(
        &self,
        mode: Mode,
        previous_mode: Mode,
        buffer: &mut Buffer,
        _buffer_context: &mut BufferState,
        buffer_display_context: &mut WindowState,
    ) {
        if previous_mode == mode {
            buffer_display_context
                .selections
                .clear_selections(buffer.as_text_buffer());
            return;
        }

        if previous_mode == Mode::VisualBlock {
            buffer_display_context.selections.end_block();
        }
        if previous_mode == Mode::VisualLine {
            buffer_display_context.selections.end_line();
        }

        if mode == Mode::VisualBlock {
            buffer_display_context
                .selections
                .begin_block(buffer.as_text_buffer());
        }
        if mode == Mode::VisualLine {
            buffer_display_context
                .selections
                .begin_line(buffer.as_text_buffer());
        }
    }

    pub fn sync(
        &self,
        mode: Mode,
        buffer: &mut Buffer,
        _buffer_context: &mut BufferState,
        buffer_display_context: &mut WindowState,
    ) {
        if mode == Mode::VisualBlock {
            buffer_display_context
                .selections
                .sync_block(buffer.as_text_buffer());
        }
        if mode == Mode::VisualLine {
            buffer_display_context
                .selections
                .sync_line(buffer.as_text_buffer());
        }
    }

    fn apply_action(
        &self,
        mode: Mode,
        action: &Action,
        buffer: &mut Buffer,
        buffer_context: &mut BufferState,
        buffer_display_context: &mut WindowState,
        services: &mut crate::app::services::Services,
    ) -> Option<Mode> {
        let mut action_owned = action.clone();
        if mode.is_visual() {
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
        if mode == Mode::VisualBlock {
            match action {
                Action::Delete { .. } | Action::DeleteMotion { .. } => {
                    next_action = Action::SetToInsert
                }
                _ => {}
            }
        }
        // These actions immediately drops mode back to Normal
        if mode.is_visual() {
            match action {
                Action::Yank { .. } | Action::YankLine { .. } | Action::YankMotion { .. } => {
                    next_action = Action::SetToNormal
                }
                _ => {}
            }
        }

        let next_mode = None;
        match action {
            Action::Clear => {
                buffer_display_context
                    .selections
                    .clear(buffer.as_text_buffer());
                return Some(Mode::Normal);
            }
            Action::SelectSimilar => {
                // No-op for now
                return None;
            }
            Action::SetToNormal => {
                self.enter_mode(
                    Mode::Normal,
                    mode,
                    buffer,
                    buffer_context,
                    buffer_display_context,
                );
                return Some(Mode::Normal);
            }
            Action::SetToInsert => {
                return Some(Mode::Insert);
            }
            Action::SetToAppend => {
                let cursors = buffer_display_context.selections.selections.clone();
                for cursor in cursors.iter() {
                    let point = cursor.head().to_point(buffer.as_text_buffer());
                    let row_len = buffer.as_text_buffer().line_len(point.row);
                    if point.column < row_len {
                        buffer_display_context.selections.move_right(
                            false,
                            1,
                            buffer.as_text_buffer(),
                        );
                    }
                }
                self.enter_mode(
                    Mode::Insert,
                    mode,
                    buffer,
                    buffer_context,
                    buffer_display_context,
                );
                return Some(Mode::Insert);
            }
            Action::SetToAppendEndOfLine => {
                buffer_display_context
                    .selections
                    .move_to_end_of_line(false, buffer.as_text_buffer());
                self.enter_mode(
                    Mode::Insert,
                    mode,
                    buffer,
                    buffer_context,
                    buffer_display_context,
                );
                return Some(Mode::Insert);
            }
            Action::SetToOpenLineBelow { count } => {
                let count = *count;
                buffer_display_context
                    .selections
                    .move_to_end_of_line(false, buffer.as_text_buffer());
                let current_row = buffer_display_context
                    .selections
                    .first()
                    .unwrap()
                    .head()
                    .to_point(buffer.as_text_buffer())
                    .row;
                for _ in 0..count {
                    self.insert_text(
                        buffer,
                        &mut buffer_display_context.selections,
                        &self.new_line(buffer),
                    );
                }
                let target_point = Point {
                    row: current_row + 1,
                    column: 0,
                };
                let target_anchor = buffer
                    .as_text_buffer()
                    .anchor_at(target_point.to_offset(buffer.as_text_buffer()), Bias::Left);
                buffer_display_context
                    .selections
                    .clear(buffer.as_text_buffer());
                let first = buffer_display_context.selections.first().unwrap().clone();
                let next = Selection {
                    id: first.id,
                    start: target_anchor.clone(),
                    end: target_anchor,
                    reversed: false,
                    goal: SelectionGoal::None,
                };
                buffer_display_context.selections.point = target_point;
                buffer_display_context
                    .selections
                    .update(buffer.as_text_buffer(), &next);
                self.enter_mode(
                    Mode::Insert,
                    mode,
                    buffer,
                    buffer_context,
                    buffer_display_context,
                );
                return Some(Mode::Insert);
            }
            Action::SetToOpenLineAbove { count } => {
                let count = *count;
                buffer_display_context
                    .selections
                    .move_to_start_of_line(false, buffer.as_text_buffer());
                let current_row = buffer_display_context
                    .selections
                    .first()
                    .unwrap()
                    .head()
                    .to_point(buffer.as_text_buffer())
                    .row;
                for _ in 0..count {
                    self.insert_text(
                        buffer,
                        &mut buffer_display_context.selections,
                        &self.new_line(buffer),
                    );
                }
                let target_point = Point {
                    row: current_row,
                    column: 0,
                };
                let target_anchor = buffer
                    .as_text_buffer()
                    .anchor_at(target_point.to_offset(buffer.as_text_buffer()), Bias::Left);
                buffer_display_context
                    .selections
                    .clear(buffer.as_text_buffer());
                let first = buffer_display_context.selections.first().unwrap().clone();
                let next = Selection {
                    id: first.id,
                    start: target_anchor.clone(),
                    end: target_anchor,
                    reversed: false,
                    goal: SelectionGoal::None,
                };
                buffer_display_context.selections.point = target_point;
                buffer_display_context
                    .selections
                    .update(buffer.as_text_buffer(), &next);
                self.enter_mode(
                    Mode::Insert,
                    mode,
                    buffer,
                    buffer_context,
                    buffer_display_context,
                );
                return Some(Mode::Insert);
            }
            Action::SetToVisual => {
                self.enter_mode(
                    Mode::Visual,
                    mode,
                    buffer,
                    buffer_context,
                    buffer_display_context,
                );
                return Some(Mode::Visual);
            }
            Action::SetToInsertStartOfLineNonSpace => {
                buffer_display_context
                    .selections
                    .move_to_start_of_line_non_space(false, buffer.as_text_buffer());
                self.enter_mode(
                    Mode::Insert,
                    mode,
                    buffer,
                    buffer_context,
                    buffer_display_context,
                );
                return Some(Mode::Insert);
            }
            Action::SetToVisualLine => {
                self.enter_mode(
                    Mode::VisualLine,
                    Mode::Normal,
                    buffer,
                    buffer_context,
                    buffer_display_context,
                );
                return Some(Mode::VisualLine);
            }
            Action::SetToVisualBlock => {
                self.enter_mode(
                    Mode::VisualBlock,
                    Mode::Normal,
                    buffer,
                    buffer_context,
                    buffer_display_context,
                );
                return Some(Mode::VisualBlock);
            }
            Action::SetToCommand
            | Action::SetToCommandSearchForward
            | Action::SetToCommandSearchBackward => {
                return Some(Mode::Command);
            }
            Action::MarkSet { ch } => {
                let head = buffer_display_context.selections.primary().head();
                _ = buffer.set_mark_anchor(*ch, head);
            }
            Action::MarkJump { ch, select } => {
                if let Some(anchor) = buffer.marks().get(*ch) {
                    let primary = buffer_display_context.selections.primary();
                    let start = if *select {
                        primary.start.clone()
                    } else {
                        anchor.clone()
                    };
                    let new_selection = Selection {
                        id: primary.id,
                        start: start.clone(),
                        end: anchor.clone(),
                        reversed: *select
                            && (buffer.as_text_buffer().offset_for_anchor(&anchor)
                                < buffer.as_text_buffer().offset_for_anchor(&primary.start)),
                        goal: SelectionGoal::None,
                    };
                    buffer_display_context
                        .selections
                        .update(buffer.as_text_buffer(), &new_selection);
                }
            }
            Action::MoveLeft { count, select } => {
                buffer_display_context.selections.move_left(
                    *select,
                    *count,
                    buffer.as_text_buffer(),
                );
            }
            Action::MoveRight { count, select } => {
                buffer_display_context.selections.move_right(
                    *select,
                    *count,
                    buffer.as_text_buffer(),
                );
            }
            Action::MoveUp { count, select } => {
                buffer_display_context
                    .selections
                    .move_up(*select, *count, buffer.as_text_buffer());
            }
            Action::MoveDown { count, select } => {
                buffer_display_context.selections.move_down(
                    *select,
                    *count,
                    buffer.as_text_buffer(),
                );
            }
            Action::MoveToPreviousWord { select, count } => {
                buffer_display_context.selections.move_to_previous_word(
                    *select,
                    *count,
                    buffer.as_text_buffer(),
                );
            }
            Action::MoveToWord { select, count } => {
                buffer_display_context.selections.move_to_next_word(
                    *select,
                    *count,
                    buffer.as_text_buffer(),
                );
            }
            Action::MoveToPreviousWordEnd { select, count } => {
                buffer_display_context.selections.move_to_previous_word_end(
                    *select,
                    *count,
                    buffer.as_text_buffer(),
                );
            }
            Action::MoveToWordEnd { select, count } => {
                buffer_display_context.selections.move_to_word_end(
                    *select,
                    *count,
                    buffer.as_text_buffer(),
                );
            }
            Action::MoveToBigWord { select, count } => {
                buffer_display_context.selections.move_to_big_word(
                    *select,
                    *count,
                    buffer.as_text_buffer(),
                );
            }
            Action::MoveToPreviousBigWord { select, count } => {
                buffer_display_context.selections.move_to_previous_big_word(
                    *select,
                    *count,
                    buffer.as_text_buffer(),
                );
            }
            Action::MoveToBigWordEnd { select, count } => {
                buffer_display_context.selections.move_to_big_word_end(
                    *select,
                    *count,
                    buffer.as_text_buffer(),
                );
            }
            Action::MoveToPreviousBigWordEnd { select, count } => {
                buffer_display_context
                    .selections
                    .move_to_previous_big_word_end(*select, *count, buffer.as_text_buffer());
            }
            Action::MoveToPreviousParagraph { select, count } => {
                buffer_display_context
                    .selections
                    .move_to_previous_paragraph(*select, *count, buffer.as_text_buffer());
            }
            Action::MoveToNextParagraph { select, count } => {
                buffer_display_context.selections.move_to_next_paragraph(
                    *select,
                    *count,
                    buffer.as_text_buffer(),
                );
            }
            Action::MoveToPreviousCharacter {
                select,
                count,
                ch,
                till,
            } => {
                buffer_display_context.selections.find_character(
                    *select,
                    *count,
                    *ch,
                    false,
                    *till,
                    buffer.as_text_buffer(),
                );
            }
            Action::MoveToNextCharacter {
                select,
                count,
                ch,
                till,
            } => {
                buffer_display_context.selections.find_character(
                    *select,
                    *count,
                    *ch,
                    true,
                    *till,
                    buffer.as_text_buffer(),
                );
            }
            Action::SearchBackward { count } => {
                let search_pattern = buffer_display_context.selections.search.clone();
                for _ in 0..*count {
                    buffer_display_context.selections.move_to_previous_match(
                        &search_pattern,
                        true,
                        buffer.as_text_buffer(),
                    );
                }
            }
            Action::SearchForward { count } => {
                let search_pattern = buffer_display_context.selections.search.clone();
                for _ in 0..*count {
                    buffer_display_context.selections.move_to_next_match(
                        &search_pattern,
                        true,
                        buffer.as_text_buffer(),
                    );
                }
            }
            Action::MoveWithinCharacter { count, ch } => {
                let select = mode.is_visual();
                let cursors = buffer_display_context.selections.selections.clone();
                for cursor in cursors.iter() {
                    let mut updated = false;
                    if *ch == 'w' {
                        let start_sel = cursor.move_to_word(false, buffer.as_text_buffer());
                        let end_sel = cursor.move_to_word_end(false, buffer.as_text_buffer());
                        let next = Selection {
                            id: cursor.id,
                            start: start_sel.head(),
                            end: end_sel.head(),
                            reversed: false,
                            goal: SelectionGoal::None,
                        };
                        buffer_display_context
                            .selections
                            .update(buffer.as_text_buffer(), &next);
                        updated = true;
                    } else if *ch == 'p' {
                        let prev_p = cursor
                            .move_to_previous_paragraph(false, buffer.as_text_buffer())
                            .head()
                            .to_point(buffer.as_text_buffer());
                        let next_p = cursor
                            .move_to_next_paragraph(false, buffer.as_text_buffer())
                            .head()
                            .to_point(buffer.as_text_buffer());
                        let start_row = if prev_p.row < buffer.as_text_buffer().row_count()
                            && buffer.as_text_buffer().line_len(prev_p.row) == 0
                        {
                            prev_p.row + 1
                        } else {
                            prev_p.row
                        };
                        let end_row = if next_p.row > 0
                            && buffer.as_text_buffer().line_len(next_p.row) == 0
                        {
                            next_p.row - 1
                        } else {
                            next_p.row
                        };
                        let start_offset = Point {
                            row: start_row,
                            column: 0,
                        }
                        .to_offset(buffer.as_text_buffer());
                        let end_offset = Point {
                            row: end_row,
                            column: buffer.as_text_buffer().line_len(end_row),
                        }
                        .to_offset(buffer.as_text_buffer())
                        .saturating_sub(1);
                        let next = Selection {
                            id: cursor.id,
                            start: buffer.as_text_buffer().anchor_at(start_offset, Bias::Left),
                            end: buffer.as_text_buffer().anchor_at(end_offset, Bias::Right),
                            reversed: false,
                            goal: SelectionGoal::None,
                        };
                        buffer_display_context
                            .selections
                            .update(buffer.as_text_buffer(), &next);
                        updated = true;
                    }
                    if !updated {
                        let next = cursor.move_within_character(
                            select,
                            *count,
                            *ch,
                            buffer.as_text_buffer(),
                        );
                        buffer_display_context
                            .selections
                            .update(buffer.as_text_buffer(), &next);
                    }
                }
            }
            Action::MoveAroundCharacter { count, ch } => {
                let select = mode.is_visual();
                let cursors = buffer_display_context.selections.selections.clone();
                for cursor in cursors.iter() {
                    let mut updated = false;
                    if *ch == 'w' {
                        let start_sel = cursor.move_to_word(false, buffer.as_text_buffer());
                        let next_word_head = cursor
                            .move_to_next_word(false, buffer.as_text_buffer())
                            .head();
                        let next_word_offset =
                            buffer.as_text_buffer().offset_for_anchor(&next_word_head);
                        let end_offset = buffer
                            .as_text_buffer()
                            .clip_offset(next_word_offset.saturating_sub(1), Bias::Left);
                        let next = Selection {
                            id: cursor.id,
                            start: start_sel.head(),
                            end: buffer.as_text_buffer().anchor_at(end_offset, Bias::Right),
                            reversed: false,
                            goal: SelectionGoal::None,
                        };
                        buffer_display_context
                            .selections
                            .update(buffer.as_text_buffer(), &next);
                        updated = true;
                    } else if *ch == 'p' {
                        let prev_p = cursor
                            .move_to_previous_paragraph(false, buffer.as_text_buffer())
                            .head()
                            .to_point(buffer.as_text_buffer());
                        let next_p = cursor
                            .move_to_next_paragraph(false, buffer.as_text_buffer())
                            .head()
                            .to_point(buffer.as_text_buffer());
                        let start_row = if prev_p.row < buffer.as_text_buffer().row_count()
                            && buffer.as_text_buffer().line_len(prev_p.row) == 0
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
                        .to_offset(buffer.as_text_buffer());
                        let end_offset = Point {
                            row: end_row,
                            column: buffer.as_text_buffer().line_len(end_row),
                        }
                        .to_offset(buffer.as_text_buffer());
                        let next = Selection {
                            id: cursor.id,
                            start: buffer.as_text_buffer().anchor_at(start_offset, Bias::Left),
                            end: buffer.as_text_buffer().anchor_at(end_offset, Bias::Right),
                            reversed: false,
                            goal: SelectionGoal::None,
                        };
                        buffer_display_context
                            .selections
                            .update(buffer.as_text_buffer(), &next);
                        updated = true;
                    }
                    if !updated {
                        let next = cursor.move_around_character(
                            select,
                            *count,
                            *ch,
                            buffer.as_text_buffer(),
                        );
                        buffer_display_context
                            .selections
                            .update(buffer.as_text_buffer(), &next);
                    }
                }
            }

            Action::MoveToStartOfDocument { select, .. } => {
                buffer_display_context
                    .selections
                    .move_to_start_of_document(*select, buffer.as_text_buffer());
            }
            Action::MoveToEndOfDocument { select, .. } => {
                buffer_display_context
                    .selections
                    .move_to_end_of_document(*select, buffer.as_text_buffer());
            }
            Action::MoveToStartOfLine { select, .. } => {
                buffer_display_context
                    .selections
                    .move_to_start_of_line(*select, buffer.as_text_buffer());
            }
            Action::MoveToStartOfLineNonSpace { select, .. } => {
                buffer_display_context
                    .selections
                    .move_to_start_of_line_non_space(*select, buffer.as_text_buffer());
            }
            Action::MoveToEndOfLine { select, .. } => {
                buffer_display_context
                    .selections
                    .move_to_end_of_line(*select, buffer.as_text_buffer());
            }
            Action::MoveToStartOfPreviousLine { select, .. } => {
                buffer_display_context
                    .selections
                    .move_to_start_of_previous_line(*select, buffer.as_text_buffer());
            }
            Action::MoveToEndOfPreviousLine { select, .. } => {
                buffer_display_context
                    .selections
                    .move_to_end_of_previous_line(*select, buffer.as_text_buffer());
            }
            Action::MoveToStartOfNextLine { select, .. } => {
                buffer_display_context
                    .selections
                    .move_to_start_of_next_line(*select, buffer.as_text_buffer());
            }
            Action::MoveToEndOfNextLine { select, .. } => {
                buffer_display_context
                    .selections
                    .move_to_end_of_next_line(*select, buffer.as_text_buffer());
            }
            Action::MovePageUp { count, select } => {
                let page_size = buffer_display_context
                    .display_map
                    .snapshot()
                    .visible_rows
                    .saturating_sub(4)
                    .max(1);
                buffer_display_context.selections.move_up(
                    *select,
                    page_size * *count,
                    buffer.as_text_buffer(),
                );
            }
            Action::MovePageDown { count, select } => {
                let page_size = buffer_display_context
                    .display_map
                    .snapshot()
                    .visible_rows
                    .saturating_sub(4)
                    .max(1);
                buffer_display_context.selections.move_down(
                    *select,
                    page_size * *count,
                    buffer.as_text_buffer(),
                );
            }
            Action::ScrollHalfPageUp { count } => {
                let half_page_size = (buffer_display_context
                    .display_map
                    .snapshot()
                    .visible_rows
                    .saturating_sub(4)
                    .max(2)
                    / 2)
                .max(1);
                buffer_display_context.selections.move_up(
                    false,
                    half_page_size * *count,
                    buffer.as_text_buffer(),
                );
            }
            Action::ScrollHalfPageDown { count } => {
                let half_page_size = (buffer_display_context
                    .display_map
                    .snapshot()
                    .visible_rows
                    .saturating_sub(4)
                    .max(2)
                    / 2)
                .max(1);
                buffer_display_context.selections.move_down(
                    false,
                    half_page_size * *count,
                    buffer.as_text_buffer(),
                );
            }
            Action::MoveToScreenTop { select, .. } => {
                let display_snapshot = buffer_display_context.display_map.snapshot();
                let target_point = display_snapshot
                    .display_point_to_point(DisplayPoint::new(display_snapshot.scroll_y, 0));
                buffer_display_context.selections.move_to_line(
                    *select,
                    target_point.row,
                    buffer.as_text_buffer(),
                );
            }
            Action::MoveToScreenMiddle { select, .. } => {
                let display_snapshot = buffer_display_context.display_map.snapshot();
                let middle_display_row =
                    display_snapshot.scroll_y + display_snapshot.visible_rows / 2;
                let target_point = display_snapshot
                    .display_point_to_point(DisplayPoint::new(middle_display_row, 0));
                buffer_display_context.selections.move_to_line(
                    *select,
                    target_point.row,
                    buffer.as_text_buffer(),
                );
            }
            Action::MoveToScreenBottom { select, .. } => {
                let display_snapshot = buffer_display_context.display_map.snapshot();
                let bottom_display_row =
                    display_snapshot.scroll_y + display_snapshot.visible_rows.saturating_sub(1);
                let target_point = display_snapshot
                    .display_point_to_point(DisplayPoint::new(bottom_display_row, 0));
                buffer_display_context.selections.move_to_line(
                    *select,
                    target_point.row,
                    buffer.as_text_buffer(),
                );
            }
            Action::InsertText(text) => {
                self.delete_text(buffer, &mut buffer_display_context.selections, 0);
                self.insert_text(buffer, &mut buffer_display_context.selections, text);
            }
            Action::DeleteChar { count } | Action::Delete { count } => {
                let text = if buffer_display_context
                    .selections
                    .has_selection(buffer.as_text_buffer())
                {
                    buffer_display_context
                        .selections
                        .text(buffer.as_text_buffer())
                } else {
                    let primary = buffer_display_context.selections.first().unwrap();
                    let head_offset = buffer.as_text_buffer().offset_for_anchor(&primary.head());
                    let end_offset = buffer
                        .as_text_buffer()
                        .clip_offset(head_offset + *count as usize, Bias::Right);
                    buffer
                        .as_text_buffer()
                        .as_rope()
                        .chunks_in_range(head_offset..end_offset)
                        .collect()
                };
                services.clipboard.set_text(&text);

                if self.delete_text(buffer, &mut buffer_display_context.selections, 0) {
                    // Deleted selection
                } else {
                    for _ in 0..*count {
                        self.delete_text(buffer, &mut buffer_display_context.selections, 1);
                    }
                }
            }
            Action::DeleteCharBefore { count } => {
                let text = if buffer_display_context
                    .selections
                    .has_selection(buffer.as_text_buffer())
                {
                    buffer_display_context
                        .selections
                        .text(buffer.as_text_buffer())
                } else {
                    let primary = buffer_display_context.selections.first().unwrap();
                    let head_offset = buffer.as_text_buffer().offset_for_anchor(&primary.head());
                    let start_offset = if head_offset >= *count as usize {
                        head_offset - *count as usize
                    } else {
                        0
                    };
                    buffer
                        .as_text_buffer()
                        .as_rope()
                        .chunks_in_range(start_offset..head_offset)
                        .collect()
                };
                services.clipboard.set_text(&text);

                if self.delete_text(buffer, &mut buffer_display_context.selections, 0) {
                    // Deleted selection
                } else {
                    for _ in 0..*count {
                        buffer_display_context.selections.move_left(
                            false,
                            1,
                            buffer.as_text_buffer(),
                        );
                        self.delete_text(buffer, &mut buffer_display_context.selections, 1);
                    }
                }
            }
            Action::DeleteLines {
                start_line,
                end_line,
            } => {
                let start_row = start_line.saturating_sub(1);
                let end_row = end_line.saturating_sub(1);

                let max_row = buffer.as_text_buffer().row_count().saturating_sub(1);
                let start_row = std::cmp::min(start_row, max_row);
                let end_row = std::cmp::min(end_row, max_row);
                let start_row = std::cmp::min(start_row, end_row);

                let start_offset = Point::new(start_row, 0).to_offset(buffer.as_text_buffer());
                let end_offset = if end_row + 1 < buffer.as_text_buffer().row_count() {
                    Point::new(end_row + 1, 0).to_offset(buffer.as_text_buffer())
                } else {
                    Point::new(end_row, buffer.as_text_buffer().line_len(end_row))
                        .to_offset(buffer.as_text_buffer())
                };

                let text: String = buffer
                    .as_text_buffer()
                    .as_rope()
                    .chunks_in_range(start_offset..end_offset)
                    .collect();
                services.clipboard.set_lines(text);

                let mut tx = buffer.transaction(vim_buffer::EditOrigin::User);
                tx.delete(
                    None,
                    vim_buffer::TextRange {
                        start: vim_buffer::ByteOffset(start_offset),
                        end: vim_buffer::ByteOffset(end_offset),
                    },
                );
                let _ = tx.commit(Some(buffer_display_context.selections.clone()));
            }
            Action::YankLines {
                start_line,
                end_line,
            } => {
                let start_row = start_line.saturating_sub(1);
                let end_row = end_line.saturating_sub(1);

                let max_row = buffer.as_text_buffer().row_count().saturating_sub(1);
                let start_row = std::cmp::min(start_row, max_row);
                let end_row = std::cmp::min(end_row, max_row);
                let start_row = std::cmp::min(start_row, end_row);

                let start_offset = Point::new(start_row, 0).to_offset(buffer.as_text_buffer());
                let end_offset = if end_row + 1 < buffer.as_text_buffer().row_count() {
                    Point::new(end_row + 1, 0).to_offset(buffer.as_text_buffer())
                } else {
                    Point::new(end_row, buffer.as_text_buffer().line_len(end_row))
                        .to_offset(buffer.as_text_buffer())
                };

                let text: String = buffer
                    .as_text_buffer()
                    .as_rope()
                    .chunks_in_range(start_offset..end_offset)
                    .collect();
                services.clipboard.set_lines(text);
            }
            Action::DeleteLine { count } | Action::ChangeLine { count } => {
                let selections = buffer_display_context.selections.selections.clone();
                let point = buffer_display_context.selections.point;
                let anchor = buffer_display_context.selections.anchor.clone();

                buffer_display_context
                    .selections
                    .move_to_start_of_line(false, buffer.as_text_buffer());
                if *count > 1 {
                    buffer_display_context.selections.move_down(
                        true,
                        count.saturating_sub(1),
                        buffer.as_text_buffer(),
                    );
                }
                buffer_display_context
                    .selections
                    .move_to_end_of_line(true, buffer.as_text_buffer());

                let mut text = buffer_display_context
                    .selections
                    .text(buffer.as_text_buffer());
                if !text.ends_with('\n') {
                    text.push('\n');
                }
                services.clipboard.set_lines(text);

                buffer_display_context.selections.selections = selections;
                buffer_display_context.selections.point = point;
                buffer_display_context.selections.anchor = anchor;

                self.delete_current_line(buffer, &mut buffer_display_context.selections, *count);
            }
            Action::JoinLines { count } => {
                let count = *count;
                let lines_to_join = if count <= 1 { 2 } else { count };
                let newlines_to_remove = lines_to_join - 1;

                let current_row = buffer_display_context
                    .selections
                    .first()
                    .unwrap()
                    .head()
                    .to_point(buffer.as_text_buffer())
                    .row;
                let total_rows = buffer.as_text_buffer().row_count();
                let actual_removes = std::cmp::min(
                    newlines_to_remove as usize,
                    (total_rows.saturating_sub(1) - current_row) as usize,
                );

                let mut target_col = None;

                for _ in 0..actual_removes {
                    let current_line_len = buffer.as_text_buffer().line_len(current_row);
                    if target_col.is_none() {
                        target_col = Some(current_line_len);
                    }

                    let end_of_current = Point {
                        row: current_row,
                        column: current_line_len,
                    }
                    .to_offset(buffer.as_text_buffer());
                    let next_line_text = Self::row_text(buffer, current_row + 1);
                    let leading_whitespace_len = next_line_text
                        .chars()
                        .take_while(|c| c.is_whitespace())
                        .map(|c| c.len_utf8())
                        .sum::<usize>();

                    let delete_start = end_of_current;
                    let delete_end = end_of_current + 1 + leading_whitespace_len;

                    let current_line_text = Self::row_text(buffer, current_row);
                    let ends_with_space = current_line_text.as_str().ends_with(char::is_whitespace);
                    let next_first_non_space = next_line_text.as_str().trim_start().chars().next();

                    let replacement = if ends_with_space || next_first_non_space.is_none() {
                        ""
                    } else {
                        " "
                    };

                    let mut tx = buffer.transaction(vim_buffer::EditOrigin::User);
                    tx.replace(
                        None,
                        vim_buffer::TextRange {
                            start: vim_buffer::ByteOffset(delete_start),
                            end: vim_buffer::ByteOffset(delete_end),
                        },
                        replacement,
                    );
                    let _ = tx.commit(Some(buffer_display_context.selections.clone()));
                }

                if let Some(col) = target_col {
                    let target_point = Point {
                        row: current_row,
                        column: col,
                    };
                    let target_anchor = buffer
                        .as_text_buffer()
                        .anchor_at(target_point.to_offset(buffer.as_text_buffer()), Bias::Left);
                    buffer_display_context
                        .selections
                        .clear(buffer.as_text_buffer());
                    let first = buffer_display_context.selections.first().unwrap().clone();
                    let next = Selection {
                        id: first.id,
                        start: target_anchor.clone(),
                        end: target_anchor,
                        reversed: false,
                        goal: SelectionGoal::None,
                    };
                    buffer_display_context.selections.point = target_point;
                    buffer_display_context
                        .selections
                        .update(buffer.as_text_buffer(), &next);
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
                    | Action::MoveToNextCharacter { select, .. } => *select = true,
                    _ => {}
                }

                let selections = buffer_display_context.selections.selections.clone();
                let point = buffer_display_context.selections.point;
                let anchor = buffer_display_context.selections.anchor.clone();

                for _ in 0..*count {
                    self.apply_action(
                        mode,
                        &motion,
                        buffer,
                        buffer_context,
                        buffer_display_context,
                        services,
                    );
                }

                let text = buffer_display_context
                    .selections
                    .text(buffer.as_text_buffer());
                services.clipboard.set_text(text);

                buffer_display_context.selections.selections = selections;
                buffer_display_context.selections.point = point;
                buffer_display_context.selections.anchor = anchor;

                if is_textobject {
                    let inclusive = matches!(
                        motion,
                        Action::MoveWithinCharacter { .. } | Action::MoveAroundCharacter { .. }
                    );
                    for _idx in 0..*count {
                        self.apply_action(
                            mode,
                            &motion,
                            buffer,
                            buffer_context,
                            buffer_display_context,
                            services,
                        );
                        self.delete_text_object(
                            buffer,
                            &mut buffer_display_context.selections,
                            inclusive,
                        );
                    }
                } else {
                    for _ in 0..*count {
                        self.apply_action(
                            mode,
                            &motion,
                            buffer,
                            buffer_context,
                            buffer_display_context,
                            services,
                        );
                        self.delete_text(buffer, &mut buffer_display_context.selections, 0);
                    }
                }
            }
            Action::Change { count } => {
                let text = buffer_display_context
                    .selections
                    .text(buffer.as_text_buffer());
                if !text.is_empty() {
                    services.clipboard.set_text(text);
                }
                self.delete_text(buffer, &mut buffer_display_context.selections, 0);
            }
            Action::InsertNewLine { count } => {
                let text = buffer_display_context
                    .selections
                    .text(buffer.as_text_buffer());
                if !text.is_empty() {
                    services.clipboard.set_text(text);
                }
                self.delete_text(buffer, &mut buffer_display_context.selections, 0);
                for _ in 0..*count {
                    self.insert_text(
                        buffer,
                        &mut buffer_display_context.selections,
                        &self.new_line(buffer),
                    );
                }
            }
            Action::InsertNewLineMotion { count, motion } => {
                let mut motion = (**motion).clone();
                for _ in 0..*count {
                    self.apply_action(
                        mode,
                        &motion,
                        buffer,
                        buffer_context,
                        buffer_display_context,
                        services,
                    );
                    self.insert_text(
                        buffer,
                        &mut buffer_display_context.selections,
                        &self.new_line(buffer),
                    );
                    motion = Action::NoOp;
                }
                buffer_display_context
                    .selections
                    .move_left(false, 1, buffer.as_text_buffer());
            }
            Action::InsertTab => {
                for _ in 0..4 {
                    self.insert_text(buffer, &mut buffer_display_context.selections, " ");
                }
            }
            Action::YankMotion { count, motion } => {
                self.yank_motion(
                    mode,
                    buffer,
                    buffer_context,
                    buffer_display_context,
                    services,
                    *count,
                    motion,
                );
            }
            Action::YankLine { count } => {
                self.yank_current_line(
                    buffer,
                    &mut buffer_display_context.selections,
                    *count,
                    services,
                );
            }
            Action::Put { count } => {
                self.paste(
                    buffer,
                    &mut buffer_display_context.selections,
                    *count,
                    services,
                );
            }
            Action::Undo { count } => {
                for _ in 0..*count {
                    // let _ = buffer.undo();
                    if let Some(outcome) = buffer.undo().ok()? {
                        if let Some(selections) = outcome.selections {
                            buffer_display_context.selections = selections;
                        }
                    }
                }
            }
            Action::Redo { count } => {
                for _ in 0..*count {
                    // let _ = buffer.redo();
                    if let Some(outcome) = buffer.redo().ok()? {
                        if let Some(selections) = outcome.selections {
                            buffer_display_context.selections = selections;
                        }
                    }
                }
            }
            Action::NoOp | Action::Quit => {
                return None;
            }
            _ => {}
        }

        self.sync(mode, buffer, buffer_context, buffer_display_context);

        let mut recursive_mode = None;
        if next_action != Action::NoOp {
            recursive_mode = self.apply_action(
                mode,
                &next_action,
                buffer,
                buffer_context,
                buffer_display_context,
                services,
            );
        }
        recursive_mode.or(next_mode)
    }

    fn yank_motion(
        &self,
        mode: Mode,
        buffer: &mut Buffer,
        buffer_context: &mut BufferState,
        buffer_display_context: &mut WindowState,
        services: &mut crate::app::services::Services,
        count: u32,
        motion: &Action,
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
            | Action::MoveToNextCharacter { select, .. } => *select = true,
            _ => {}
        }

        let selections = buffer_display_context.selections.selections.clone();
        let point = buffer_display_context.selections.point;
        let anchor = buffer_display_context.selections.anchor.clone();

        for _ in 0..count {
            self.apply_action(
                mode,
                &motion,
                buffer,
                buffer_context,
                buffer_display_context,
                services,
            );
        }
        let text = buffer_display_context
            .selections
            .text(buffer.as_text_buffer());
        services.clipboard.set_text(text);

        buffer_display_context.selections.selections = selections;
        buffer_display_context.selections.point = point;
        buffer_display_context.selections.anchor = anchor;
    }

    fn yank_current_line(
        &self,
        buffer: &Buffer,
        selections: &mut SelectionSet,
        count: u32,
        services: &mut crate::app::services::Services,
    ) {
        let original_selections = selections.selections.clone();
        let point = selections.point;
        let anchor = selections.anchor.clone();

        selections.move_to_start_of_line(false, buffer.as_text_buffer());
        if count > 1 {
            selections.move_down(true, count.saturating_sub(1), buffer.as_text_buffer());
        }
        selections.move_to_end_of_line(true, buffer.as_text_buffer());

        let mut text = selections.text(buffer.as_text_buffer());
        if !text.ends_with('\n') {
            text.push('\n');
        }
        services.clipboard.set_lines(text);

        selections.selections = original_selections;
        selections.point = point;
        selections.anchor = anchor;
    }

    fn paste(
        &self,
        buffer: &mut Buffer,
        selections: &mut SelectionSet,
        count: u32,
        services: &mut crate::app::services::Services,
    ) {
        if services.clipboard.is_empty() || count == 0 {
            return;
        }
        let text = services.clipboard.text().to_string();
        let kind = services.clipboard.kind();

        match kind {
            vim_clipboard::ClipboardKind::Character | vim_clipboard::ClipboardKind::Block => {
                selections.move_right(false, 1, buffer.as_text_buffer());
                for _ in 0..count {
                    self.insert_text(buffer, selections, &text);
                }
            }
            vim_clipboard::ClipboardKind::Line => {
                let cursor_row = selections
                    .first()
                    .unwrap()
                    .head()
                    .to_point(buffer.as_text_buffer())
                    .row;
                let has_next_line = cursor_row + 1 < buffer.as_text_buffer().row_count();
                if has_next_line {
                    selections.move_to_start_of_next_line(false, buffer.as_text_buffer());
                } else {
                    selections.move_to_end_of_line(false, buffer.as_text_buffer());
                    self.insert_text(buffer, selections, &self.new_line(buffer));
                }
                for _ in 0..count {
                    self.insert_text(buffer, selections, &text);
                }
            }
        }
    }

    fn insert_text(&self, buffer: &mut Buffer, selections: &mut SelectionSet, text: &str) {
        let mut edits = Vec::new();
        let cursors = selections.selections.clone();
        for cursor in cursors.iter() {
            let start = buffer.as_text_buffer().offset_for_anchor(&cursor.head());
            edits.push((start, text.to_string()));
        }

        let mut tx = buffer.transaction(vim_buffer::EditOrigin::User);
        for &(start, ref text_val) in &edits {
            tx.insert(None, vim_buffer::ByteOffset(start), text_val.clone());
        }
        let _ = tx.commit(Some(selections.clone()));

        for cursor in cursors.iter() {
            let start = buffer.as_text_buffer().offset_for_anchor(&cursor.head());
            let new_offset = buffer
                .as_text_buffer()
                .clip_offset(start + text.len(), Bias::Left);
            let new_head = buffer.as_text_buffer().anchor_at(new_offset, Bias::Left);
            selections.update(
                buffer.as_text_buffer(),
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

    fn delete_text(
        &self,
        buffer: &mut Buffer,
        selections: &mut SelectionSet,
        count: usize,
    ) -> bool {
        let mut edits = Vec::new();
        let cursors = selections.selections.clone();
        for cursor in cursors.iter() {
            let (start, mut end) = {
                let (cs, ce) = if cursor.head().cmp(&cursor.tail(), buffer.as_text_buffer())
                    == Ordering::Less
                {
                    (
                        cursor.head().bias_left(buffer.as_text_buffer()),
                        cursor.tail().bias_right(buffer.as_text_buffer()),
                    )
                } else {
                    (
                        cursor.tail().bias_left(buffer.as_text_buffer()),
                        cursor.head().bias_right(buffer.as_text_buffer()),
                    )
                };

                let start = buffer.as_text_buffer().offset_for_anchor(&cs);
                let mut end = buffer.as_text_buffer().offset_for_anchor(&ce);
                if start != end {
                    end = buffer.as_text_buffer().clip_offset(end + 1, Bias::Right);
                }
                (start, end)
            };

            if count != 0 {
                end = buffer
                    .as_text_buffer()
                    .clip_offset(end + count, Bias::Right);
            }

            if start != end {
                edits.push(vim_buffer::TextRange {
                    start: vim_buffer::ByteOffset(start),
                    end: vim_buffer::ByteOffset(end),
                });
            }
        }

        if !edits.is_empty() {
            let mut tx = buffer.transaction(vim_buffer::EditOrigin::User);
            for range in edits {
                tx.delete(None, range);
            }
            let _ = tx.commit(Some(selections.clone()));
            true
        } else {
            false
        }
    }

    fn delete_text_object(
        &self,
        buffer: &mut Buffer,
        selections: &mut SelectionSet,
        inclusive: bool,
    ) -> bool {
        let mut edits = Vec::new();
        let cursors = selections.selections.clone();
        for cursor in cursors.iter() {
            let (start, end) = {
                let (cs, ce) = if cursor.head().cmp(&cursor.tail(), buffer.as_text_buffer())
                    == Ordering::Less
                {
                    (
                        cursor.head().bias_left(buffer.as_text_buffer()),
                        cursor.tail().bias_right(buffer.as_text_buffer()),
                    )
                } else {
                    (
                        cursor.tail().bias_left(buffer.as_text_buffer()),
                        cursor.head().bias_right(buffer.as_text_buffer()),
                    )
                };

                let start = buffer.as_text_buffer().offset_for_anchor(&cs);
                let mut end = buffer.as_text_buffer().offset_for_anchor(&ce);
                if inclusive && start != end {
                    end = buffer.as_text_buffer().clip_offset(end + 1, Bias::Right);
                }
                (start, end)
            };

            if start != end {
                edits.push(vim_buffer::TextRange {
                    start: vim_buffer::ByteOffset(start),
                    end: vim_buffer::ByteOffset(end),
                });
            }
        }

        if !edits.is_empty() {
            let mut tx = buffer.transaction(vim_buffer::EditOrigin::User);
            for range in edits {
                tx.delete(None, range);
            }
            let _ = tx.commit(Some(selections.clone()));
            true
        } else {
            false
        }
    }

    pub fn delete_current_line(
        &self,
        buffer: &mut Buffer,
        selections: &mut SelectionSet,
        count: u32,
    ) {
        if self.delete_text(buffer, selections, 0) {
            return;
        }
        let mut edits = Vec::new();
        for _ in 0..count {
            let cursors = selections.selections.clone();
            for cursor in cursors.iter() {
                let mut point = cursor.head().to_point(buffer.as_text_buffer());
                let (start, end) = {
                    point.column = 0;
                    let start = buffer
                        .as_text_buffer()
                        .offset_for_anchor(&buffer.as_text_buffer().anchor_at(&point, Bias::Left));
                    if point.row < buffer.as_text_buffer().row_count() {
                        point.row += 1;
                    } else {
                        point.column = buffer.as_text_buffer().line_len(point.row);
                    }
                    let end = buffer.as_text_buffer().clip_offset(
                        buffer.as_text_buffer().offset_for_anchor(
                            &buffer.as_text_buffer().anchor_at(&point, Bias::Right),
                        ),
                        Bias::Right,
                    );
                    (start, end)
                };
                if start != end {
                    edits.push(vim_buffer::TextRange {
                        start: vim_buffer::ByteOffset(start),
                        end: vim_buffer::ByteOffset(end),
                    });
                }
            }
        }

        if !edits.is_empty() {
            let mut tx = buffer.transaction(vim_buffer::EditOrigin::User);
            for range in edits {
                tx.delete(None, range);
            }
            let _ = tx.commit(Some(selections.clone()));
        }
    }

    fn row_text(buffer: &Buffer, row: u32) -> String {
        let text_buf = buffer.as_text_buffer();
        let start = Point::new(row, 0).to_offset(text_buf);
        let end = Point::new(row, text_buf.line_len(row)).to_offset(text_buf);
        text_buf.as_rope().chunks_in_range(start..end).collect()
    }

    fn new_line(&self, _buffer: &Buffer) -> String {
        "\n".to_string()
    }
}
