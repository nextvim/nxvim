use crate::model::BufferState;
use display_map::DisplayPoint;
use std::cmp::Ordering;
use std::ops::Range;
use sum_tree::Bias;
use text::{Point, Selection, SelectionGoal, ToOffset, ToPoint};
use vim_buffer::{Buffer, Motions, SelectionSet};
use vim_input::{Action, Mode};
use vim_ui::WindowState;

/// The kind of case transformation to apply to a span of text.
#[derive(Clone, Copy, PartialEq, Eq)]
enum CaseChange {
    /// Swap the case of every letter (used by `~`).
    Toggle,
    /// Force every letter to uppercase (used by `gU`).
    Upper,
    /// Force every letter to lowercase (used by `gu`).
    Lower,
}

trait CaseChangeExt {
    fn change_case(&self, change: CaseChange) -> String;
}

impl CaseChangeExt for str {
    fn change_case(&self, change: CaseChange) -> String {
        match change {
            CaseChange::Toggle => self
                .chars()
                .map(|c| {
                    if c.is_lowercase() {
                        c.to_uppercase().to_string()
                    } else if c.is_uppercase() {
                        c.to_lowercase().to_string()
                    } else {
                        c.to_string()
                    }
                })
                .collect(),
            CaseChange::Upper => self.to_uppercase(),
            CaseChange::Lower => self.to_lowercase(),
        }
    }
}

/// Moves each cursor to the start of the syntax node returned by `target` for its position,
/// repeating `count` times. Stops early once no cursor can move any further.
fn move_to_syntax_target(
    selections: &mut SelectionSet,
    buffer: &text::Buffer,
    select: bool,
    count: u32,
    syntax_tree: &vim_treesitter::SyntaxTree,
    target: impl Fn(&vim_treesitter::SyntaxTree, usize) -> Option<vim_treesitter::SyntaxNode>,
) {
    for _ in 0..count {
        let cursors = selections.selections.clone();
        let mut moved = false;
        for cursor in cursors.iter() {
            let byte = buffer.offset_for_anchor(&cursor.head());
            if let Some(node) = target(syntax_tree, byte) {
                let new_head = buffer.anchor_at(node.byte_range.start, Bias::Left);
                let next = Selection {
                    id: cursor.id,
                    start: new_head,
                    end: if select { cursor.tail() } else { new_head },
                    reversed: true,
                    goal: SelectionGoal::None,
                };
                selections.point = new_head.to_point(buffer);
                selections.update(buffer, &next);
                moved = true;
            }
        }
        if !moved {
            break;
        }
    }
}

/// The number of rows scanned on each side of a cursor's row on the first
/// attempt to resolve a delimiter match via the structural-scanner
/// fallback. Doubled on each subsequent attempt until a match is found or
/// the whole buffer has been scanned once.
const SCANNER_FALLBACK_INITIAL_ROW_RADIUS: u32 = 64;

/// Runs `query` against growing windows of rows around `byte`'s row,
/// starting at [`SCANNER_FALLBACK_INITIAL_ROW_RADIUS`] rows on each side and
/// doubling the radius on each attempt until `query` finds a match or the
/// window covers the whole buffer. This avoids scanning (and allocating a
/// full `StructuralScanner` for) the entire buffer just to resolve a single,
/// usually nearby, delimiter match.
///
/// This is a heuristic: a string or comment spanning more rows than the
/// window being scanned can leave the scan not knowing it started "inside"
/// one, which could occasionally miscount nesting right at the window's
/// edges. Callers that need perfect accuracy should prefer a real syntax
/// tree instead.
fn scan_expanding(
    buffer: &text::Buffer,
    byte: usize,
    block_only: bool,
) -> Option<vim_scanner::MatchedDelimiter> {
    let row_count = buffer.row_count();
    let cursor_row = byte.to_point(buffer).row;
    let mut radius = SCANNER_FALLBACK_INITIAL_ROW_RADIUS;

    loop {
        let start_row = cursor_row.saturating_sub(radius);
        let end_row = cursor_row.saturating_add(radius);
        let covers_whole_buffer = start_row == 0 && end_row >= row_count;

        if let Some(m) = vim_scanner::StructuralScanner::scan_rows_for_enclosing(
            buffer, start_row, end_row, byte, block_only,
        ) {
            return Some(m);
        }

        if covers_whole_buffer {
            return None;
        }
        radius = radius.saturating_mul(2);
    }
}

/// Looks for a delimiter pair enclosing `byte`, using the dependency-free
/// [`vim_scanner::StructuralScanner`] instead of a tree-sitter syntax tree.
/// Used as a fallback for `i{`/`a{`-style motions when no syntax tree is
/// available for the buffer. Returns the byte offsets of the opening and
/// closing delimiter characters if `ch` matches the kind of the innermost
/// enclosing pair (backtick strings and tags aren't supported, since the
/// scanner has no notion of either).
fn scanner_delimiter_match(buffer: &text::Buffer, byte: usize, ch: char) -> Option<(usize, usize)> {
    let expected_kind = match ch {
        '{' | '}' => vim_scanner::DelimiterKind::Brace,
        '(' | ')' => vim_scanner::DelimiterKind::Paren,
        '[' | ']' => vim_scanner::DelimiterKind::Bracket,
        '"' => vim_scanner::DelimiterKind::DoubleQuote,
        '\'' => vim_scanner::DelimiterKind::SingleQuote,
        '`' => vim_scanner::DelimiterKind::BackTick,
        _ => return None,
    };
    let m = scan_expanding(buffer, byte, false)?;
    (m.kind == expected_kind).then_some((m.start, m.end))
}

/// Removes any fold overlapping the half-open byte range `start..end` (with a
/// one-byte tolerance on either edge), so that a pending buffer edit over that
/// range can never leave a fold pointing at text that no longer exists (which
/// would otherwise panic once the fold is rendered against the edited buffer).
fn remove_overlapping_folds(
    folds: &mut Vec<display_map::Fold>,
    buffer: &text::Buffer,
    start: usize,
    end: usize,
) {
    folds.retain(|fold| {
        let fold_start = fold.start.to_offset(buffer);
        let fold_end = fold.end.to_offset(buffer);
        !(end > fold_start.saturating_sub(1) && start < fold_end + 1)
    });
}

/// Like [`move_to_syntax_target`], but moves each cursor to the end of the matched syntax node
/// instead of its start.
fn move_to_syntax_target_end(
    selections: &mut SelectionSet,
    buffer: &text::Buffer,
    select: bool,
    count: u32,
    syntax_tree: &vim_treesitter::SyntaxTree,
    target: impl Fn(&vim_treesitter::SyntaxTree, usize) -> Option<vim_treesitter::SyntaxNode>,
) {
    for _ in 0..count {
        let cursors = selections.selections.clone();
        let mut moved = false;
        for cursor in cursors.iter() {
            let byte = buffer.offset_for_anchor(&cursor.head());
            if let Some(node) = target(syntax_tree, byte) {
                let new_head = buffer.anchor_at(node.byte_range.end.saturating_sub(1), Bias::Right);
                let next = Selection {
                    id: cursor.id,
                    start: new_head,
                    end: if select { cursor.tail() } else { new_head },
                    reversed: true,
                    goal: SelectionGoal::None,
                };
                selections.point = new_head.to_point(buffer);
                selections.update(buffer, &next);
                moved = true;
            }
        }
        if !moved {
            break;
        }
    }
}

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

        let viewport = buffer_display_context.viewport;
        buffer_display_context.update(
            buffer.snapshot().as_inner().clone(),
            viewport.width,
            viewport.height,
            viewport.has_border,
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
        buffer_display_context
            .selections
            .collapse_overlapping_cursors(buffer.as_text_buffer());
        if mode.is_visual() {
            let primary = buffer_display_context.selections.primary();
            let head = primary.head();
            let tail = primary.tail();
            let text_buf = buffer.as_text_buffer();
            let (top, end) =
                if text_buf.offset_for_anchor(&head) <= text_buf.offset_for_anchor(&tail) {
                    (head, tail)
                } else {
                    (tail, head)
                };
            _ = buffer.set_mark_anchor('<', top);
            _ = buffer.set_mark_anchor('>', end);
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
        if mode.is_visual() {
            match action {
                Action::DeleteCharBefore { .. }
                | Action::Delete { .. }
                | Action::DeleteMotion { .. } => next_action = Action::SetToInsert,
                _ => {}
            }
        }
        // These actions immediately drops mode back to Normal
        if mode.is_visual() {
            match action {
                Action::Yank { .. }
                | Action::YankLine { .. }
                | Action::YankMotion { .. }
                | Action::ChangeCase { .. }
                | Action::UpperCaseLine { .. }
                | Action::LowerCaseLine { .. }
                | Action::UpperCaseMotion { .. }
                | Action::LowerCaseMotion { .. } => next_action = Action::SetToNormal,
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
                let text_buffer = buffer.as_text_buffer();
                if !buffer_display_context.selections.has_selection(text_buffer) {
                    let cursors = buffer_display_context.selections.selections.clone();
                    for cursor in cursors.iter() {
                        let start_sel = cursor.move_to_word(false, text_buffer);
                        let end_sel = cursor.move_to_word_end(false, text_buffer);
                        let next = Selection {
                            id: cursor.id,
                            start: start_sel.head(),
                            end: end_sel.head(),
                            reversed: false,
                            goal: SelectionGoal::None,
                        };
                        buffer_display_context
                            .selections
                            .update(text_buffer, &next);
                    }
                } else {
                    let cursor = buffer_display_context.selections.primary().clone();
                    let selected_text = cursor.text(text_buffer);
                    if let Some(mut next_match) = cursor.clone().move_to_next_match_within(
                        selected_text.as_str(),
                        text_buffer,
                        text_buffer.row_count(),
                    ) {
                        for _ in 0..selected_text.len().saturating_sub(1) {
                            next_match = next_match.move_right_once(true, text_buffer);
                        }

                        let next_cursor = Selection {
                            id: cursor.id,
                            start: next_match.head(),
                            end: next_match.tail(),
                            reversed: false,
                            goal: SelectionGoal::None,
                        };
                        if buffer_display_context.selections.has_similar_cursor(&next_cursor, text_buffer) {
                            return None;
                        }

                        let sel = buffer_display_context.selections.add(text_buffer, 0);
                        buffer_display_context.selections.update(
                            text_buffer,
                            &Selection {
                                id: sel.id,
                                start: cursor.head(),
                                end: cursor.tail(),
                                reversed: false,
                                goal: SelectionGoal::None,
                            },
                        );
                        buffer_display_context.selections.update(text_buffer, &next_cursor);
                    }
                }
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
                    } else if *ch == 's' {
                        let text_buffer = buffer.as_text_buffer();
                        let prev_p = cursor
                            .move_to_previous_sentence(false, text_buffer)
                            .head()
                            .to_point(text_buffer);
                        let next_p = cursor
                            .move_to_next_sentence(false, text_buffer)
                            .head()
                            .to_point(text_buffer);

                        let start_offset = prev_p.to_offset(text_buffer);
                        let next_offset = next_p.to_offset(text_buffer);

                        // `next_p` marks the start of the *next* sentence
                        // (or the end of the document if there isn't one).
                        // Trim the whitespace between the end of the
                        // current sentence and that boundary so the inner
                        // sentence doesn't include the trailing space (or a
                        // blank line separating paragraphs).
                        let between: String = text_buffer
                            .as_rope()
                            .chunks_in_range(start_offset..next_offset)
                            .collect();
                        let end_offset = (start_offset + between.trim_end().len())
                            .saturating_sub(1)
                            .max(start_offset);

                        let next = Selection {
                            id: cursor.id,
                            start: text_buffer.anchor_at(start_offset, Bias::Left),
                            end: text_buffer.anchor_at(end_offset, Bias::Right),
                            reversed: false,
                            goal: SelectionGoal::None,
                        };
                        buffer_display_context.selections.update(text_buffer, &next);
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
                    } else if let Ok(syntax_tree) = &buffer_context.treesitter {
                        let byte = buffer.as_text_buffer().offset_for_anchor(&cursor.head());
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
                                let start_anchor =
                                    buffer.as_text_buffer().anchor_at(start_offset, Bias::Left);
                                let end_anchor =
                                    buffer.as_text_buffer().anchor_at(end_offset, Bias::Right);
                                let next = Selection {
                                    id: cursor.id,
                                    start: start_anchor,
                                    end: end_anchor,
                                    reversed: false,
                                    goal: SelectionGoal::None,
                                };
                                buffer_display_context
                                    .selections
                                    .update(buffer.as_text_buffer(), &next);
                                updated = true;
                            }
                        }
                    } else if let Some((start, end)) = scanner_delimiter_match(
                        buffer.as_text_buffer(),
                        buffer.as_text_buffer().offset_for_anchor(&cursor.head()),
                        *ch,
                    ) {
                        let start_offset = start + 1;
                        let end_offset = end.saturating_sub(1);
                        let start_anchor =
                            buffer.as_text_buffer().anchor_at(start_offset, Bias::Left);
                        let end_anchor = buffer.as_text_buffer().anchor_at(end_offset, Bias::Right);
                        let next = Selection {
                            id: cursor.id,
                            start: start_anchor,
                            end: end_anchor,
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
                    } else if let Ok(syntax_tree) = &buffer_context.treesitter {
                        let byte = buffer.as_text_buffer().offset_for_anchor(&cursor.head());
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
                                let start_anchor =
                                    buffer.as_text_buffer().anchor_at(start_offset, Bias::Left);
                                let end_anchor =
                                    buffer.as_text_buffer().anchor_at(end_offset, Bias::Right);
                                let next = Selection {
                                    id: cursor.id,
                                    start: start_anchor,
                                    end: end_anchor,
                                    reversed: false,
                                    goal: SelectionGoal::None,
                                };
                                buffer_display_context
                                    .selections
                                    .update(buffer.as_text_buffer(), &next);
                                updated = true;
                            }
                        }
                    } else if let Some((start_offset, end_offset)) = scanner_delimiter_match(
                        buffer.as_text_buffer(),
                        buffer.as_text_buffer().offset_for_anchor(&cursor.head()),
                        *ch,
                    ) {
                        let start_anchor =
                            buffer.as_text_buffer().anchor_at(start_offset, Bias::Left);
                        let end_anchor = buffer.as_text_buffer().anchor_at(end_offset, Bias::Right);
                        let next = Selection {
                            id: cursor.id,
                            start: start_anchor,
                            end: end_anchor,
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

            Action::MoveToNextFunction { select, count } => {
                if *count > 0 {
                    if let Ok(syntax_tree) = &buffer_context.treesitter {
                        move_to_syntax_target(
                            &mut buffer_display_context.selections,
                            buffer.as_text_buffer(),
                            *select,
                            *count,
                            syntax_tree,
                            |tree, byte| tree.next_function_after_byte(byte),
                        );
                    }
                }
            }
            Action::MoveToPreviousFunction { select, count } => {
                if *count > 0 {
                    if let Ok(syntax_tree) = &buffer_context.treesitter {
                        move_to_syntax_target(
                            &mut buffer_display_context.selections,
                            buffer.as_text_buffer(),
                            *select,
                            *count,
                            syntax_tree,
                            |tree, byte| tree.previous_function_before_byte(byte),
                        );
                    }
                }
            }
            Action::MoveToNextBlock { select, count } => {
                if *count > 0 {
                    if let Ok(syntax_tree) = &buffer_context.treesitter {
                        move_to_syntax_target(
                            &mut buffer_display_context.selections,
                            buffer.as_text_buffer(),
                            *select,
                            *count,
                            syntax_tree,
                            |tree, byte| tree.next_block_after_byte(byte),
                        );
                    }
                }
            }
            Action::MoveToPreviousBlock { select, count } => {
                if *count > 0 {
                    if let Ok(syntax_tree) = &buffer_context.treesitter {
                        move_to_syntax_target(
                            &mut buffer_display_context.selections,
                            buffer.as_text_buffer(),
                            *select,
                            *count,
                            syntax_tree,
                            |tree, byte| tree.previous_block_before_byte(byte),
                        );
                    }
                }
            }
            Action::MoveToBlockStart { select, count } => {
                if *count > 0 {
                    if let Ok(syntax_tree) = &buffer_context.treesitter {
                        move_to_syntax_target(
                            &mut buffer_display_context.selections,
                            buffer.as_text_buffer(),
                            *select,
                            *count,
                            syntax_tree,
                            |tree, byte| tree.block_start_at_byte(byte),
                        );
                    }
                }
            }
            Action::MoveToBlockEnd { select, count } => {
                if *count > 0 {
                    if let Ok(syntax_tree) = &buffer_context.treesitter {
                        move_to_syntax_target_end(
                            &mut buffer_display_context.selections,
                            buffer.as_text_buffer(),
                            *select,
                            *count,
                            syntax_tree,
                            |tree, byte| tree.block_end_at_byte(byte),
                        );
                    }
                }
            }
            Action::MoveToNextClass { select, count } => {
                if *count > 0 {
                    if let Ok(syntax_tree) = &buffer_context.treesitter {
                        move_to_syntax_target(
                            &mut buffer_display_context.selections,
                            buffer.as_text_buffer(),
                            *select,
                            *count,
                            syntax_tree,
                            |tree, byte| tree.next_class_after_byte(byte),
                        );
                    }
                }
            }
            Action::MoveToPreviousClass { select, count } => {
                if *count > 0 {
                    if let Ok(syntax_tree) = &buffer_context.treesitter {
                        move_to_syntax_target(
                            &mut buffer_display_context.selections,
                            buffer.as_text_buffer(),
                            *select,
                            *count,
                            syntax_tree,
                            |tree, byte| tree.previous_class_before_byte(byte),
                        );
                    }
                }
            }
            Action::MoveToNextArgument { select, count } => {
                if *count > 0 {
                    if let Ok(syntax_tree) = &buffer_context.treesitter {
                        move_to_syntax_target(
                            &mut buffer_display_context.selections,
                            buffer.as_text_buffer(),
                            *select,
                            *count,
                            syntax_tree,
                            |tree, byte| tree.next_argument_after_byte(byte),
                        );
                    }
                }
            }
            Action::MoveToPreviousArgument { select, count } => {
                if *count > 0 {
                    if let Ok(syntax_tree) = &buffer_context.treesitter {
                        move_to_syntax_target(
                            &mut buffer_display_context.selections,
                            buffer.as_text_buffer(),
                            *select,
                            *count,
                            syntax_tree,
                            |tree, byte| tree.previous_argument_before_byte(byte),
                        );
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
                self.delete_text(
                    buffer,
                    &mut buffer_display_context.selections,
                    &mut buffer_display_context.folds,
                    0,
                );
                self.insert_text(buffer, &mut buffer_display_context.selections, text);
            }
            Action::DeleteChar { count } | Action::Delete { count } => {
                if *count > 1
                    && buffer_display_context
                        .selections
                        .has_selection(buffer.as_text_buffer())
                {
                    if mode == Mode::VisualLine {
                        buffer_display_context.selections.move_down(
                            true,
                            count.saturating_sub(1),
                            buffer.as_text_buffer(),
                        );
                    } else {
                        buffer_display_context.selections.move_right(
                            true,
                            count.saturating_sub(1),
                            buffer.as_text_buffer(),
                        );
                    }
                }

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
                        .clip_offset(head_offset.saturating_add(*count as usize), Bias::Right);
                    buffer
                        .as_text_buffer()
                        .as_rope()
                        .chunks_in_range(head_offset..end_offset)
                        .collect()
                };
                services.clipboard.set_text(&text);

                if self.delete_text(
                    buffer,
                    &mut buffer_display_context.selections,
                    &mut buffer_display_context.folds,
                    0,
                ) {
                    // Deleted selection
                } else {
                    for _ in 0..*count {
                        self.delete_text(
                            buffer,
                            &mut buffer_display_context.selections,
                            &mut buffer_display_context.folds,
                            1,
                        );
                    }
                }
            }
            Action::DeleteCharBefore { count } => {
                if *count > 1
                    && buffer_display_context
                        .selections
                        .has_selection(buffer.as_text_buffer())
                {
                    if mode == Mode::VisualLine {
                        buffer_display_context.selections.move_down(
                            true,
                            count.saturating_sub(1),
                            buffer.as_text_buffer(),
                        );
                    } else {
                        buffer_display_context.selections.move_right(
                            true,
                            count.saturating_sub(1),
                            buffer.as_text_buffer(),
                        );
                    }
                }

                let text = if buffer_display_context
                    .selections
                    .has_selection(buffer.as_text_buffer())
                {
                    buffer_display_context
                        .selections
                        .text(buffer.as_text_buffer())
                } else {
                    let primary = buffer_display_context.selections.first().unwrap();
                    let mut head_offset =
                        buffer.as_text_buffer().offset_for_anchor(&primary.head());
                    let mut start_offset = if head_offset >= *count as usize {
                        head_offset - *count as usize
                    } else {
                        0
                    };

                    start_offset = buffer
                        .as_text_buffer()
                        .clip_offset(start_offset, Bias::Left);
                    head_offset = buffer
                        .as_text_buffer()
                        .clip_offset(head_offset, Bias::Right);

                    buffer
                        .as_text_buffer()
                        .as_rope()
                        .chunks_in_range(start_offset..head_offset)
                        .collect()
                };
                services.clipboard.set_text(&text);

                if self.delete_text(
                    buffer,
                    &mut buffer_display_context.selections,
                    &mut buffer_display_context.folds,
                    0,
                ) {
                    // Deleted selection
                } else {
                    for _ in 0..*count {
                        buffer_display_context.selections.move_left(
                            false,
                            1,
                            buffer.as_text_buffer(),
                        );
                        self.delete_text(
                            buffer,
                            &mut buffer_display_context.selections,
                            &mut buffer_display_context.folds,
                            1,
                        );
                    }
                }
            }
            Action::ChangeCase { count } => {
                let mut edits = Vec::new();
                let mut new_selections = Vec::new();
                let cursors = buffer_display_context.selections.selections.clone();

                for cursor in cursors.iter() {
                    let (start_offset, end_offset) = if buffer_display_context
                        .selections
                        .has_selection(buffer.as_text_buffer())
                    {
                        let head = cursor.head();
                        let tail = cursor.tail();
                        let (cs, ce) = if head.cmp(&tail, buffer.as_text_buffer()) == Ordering::Less
                        {
                            (
                                head.bias_left(buffer.as_text_buffer()),
                                tail.bias_right(buffer.as_text_buffer()),
                            )
                        } else {
                            (
                                tail.bias_left(buffer.as_text_buffer()),
                                head.bias_right(buffer.as_text_buffer()),
                            )
                        };
                        let start = buffer.as_text_buffer().offset_for_anchor(&cs);
                        let mut end = buffer.as_text_buffer().offset_for_anchor(&ce);
                        if start != end {
                            end = buffer.as_text_buffer().clip_offset(end + 1, Bias::Right);
                        }
                        (start, end)
                    } else {
                        let head = cursor.head();
                        let start = buffer.as_text_buffer().offset_for_anchor(&head);
                        let end = buffer
                            .as_text_buffer()
                            .clip_offset(start + *count as usize, Bias::Right);
                        (start, end)
                    };

                    if start_offset != end_offset {
                        let text: String = buffer
                            .as_text_buffer()
                            .as_rope()
                            .chunks_in_range(start_offset..end_offset)
                            .collect();

                        let toggled = text.change_case(CaseChange::Toggle);
                        edits.push((start_offset, end_offset, toggled));
                    }
                }

                if !edits.is_empty() {
                    let mut tx = buffer.transaction(vim_buffer::EditOrigin::User);
                    for &(start, end, ref toggled) in &edits {
                        tx.replace(
                            None,
                            vim_buffer::TextRange::new(
                                vim_buffer::ByteOffset(start),
                                vim_buffer::ByteOffset(end),
                            )
                            .unwrap(),
                            toggled.as_str(),
                        );
                    }
                    let _ = tx.commit(Some(buffer_display_context.selections.clone()));

                    for (i, cursor) in cursors.iter().enumerate() {
                        if i >= edits.len() {
                            break;
                        }
                        let (start, _, ref toggled) = edits[i];
                        let text_buf = buffer.as_text_buffer();
                        let new_offset = text_buf.clip_offset(start + toggled.len(), Bias::Left);
                        let new_anchor = text_buf.anchor_at(new_offset, Bias::Left);
                        new_selections.push(Selection {
                            id: cursor.id,
                            start: new_anchor.clone(),
                            end: new_anchor,
                            reversed: false,
                            goal: SelectionGoal::None,
                        });
                    }

                    for sel in new_selections {
                        buffer_display_context
                            .selections
                            .update(buffer.as_text_buffer(), &sel);
                    }
                }
            }
            Action::UpperCaseLine { count } | Action::LowerCaseLine { count } => {
                let change = if matches!(action, Action::UpperCaseLine { .. }) {
                    CaseChange::Upper
                } else {
                    CaseChange::Lower
                };
                self.change_case_lines(
                    buffer,
                    &mut buffer_display_context.selections,
                    *count,
                    change,
                );
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
            Action::PutLines { line, before } => {
                if services.clipboard.is_empty() {
                    return None;
                }

                let max_row = buffer.as_text_buffer().row_count().saturating_sub(1);
                let target_row = std::cmp::min(line.saturating_sub(1), max_row);

                if *before && target_row == 0 {
                    // Putting before the first line has no "previous row" to
                    // anchor `paste` on, so insert directly at the start.
                    let mut insert_text = services.clipboard.text();
                    if !insert_text.ends_with('\n') {
                        insert_text.push('\n');
                    }
                    let mut tx = buffer.transaction(vim_buffer::EditOrigin::User);
                    tx.insert(None, vim_buffer::ByteOffset(0), insert_text);
                    let _ = tx.commit(Some(buffer_display_context.selections.clone()));
                } else {
                    // `paste` always inserts a linewise register after the
                    // cursor's current row, so anchor the cursor on the row
                    // before the insertion point and reuse it rather than
                    // duplicating its end-of-buffer handling.
                    let anchor_row = if *before { target_row - 1 } else { target_row };
                    let anchor_offset =
                        Point::new(anchor_row, 0).to_offset(buffer.as_text_buffer());
                    // `SelectionSet::clear` only collapses existing selections
                    // down to one (it doesn't empty the list), so `add` right
                    // after it would leave two cursors instead of moving the
                    // one we have. Empty the list outright first.
                    buffer_display_context.selections.selections.clear();
                    buffer_display_context
                        .selections
                        .add(buffer.as_text_buffer(), anchor_offset);
                    self.paste(buffer, &mut buffer_display_context.selections, 1, services);
                }
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

                self.delete_current_line(
                    buffer,
                    &mut buffer_display_context.selections,
                    &mut buffer_display_context.folds,
                    *count,
                );
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
            Action::ChangeMotion { count, motion }
            | Action::DeleteMotion { count, motion }
            | Action::UpperCaseMotion { count, motion }
            | Action::LowerCaseMotion { count, motion } => {
                // `gu`/`gU` change the case of the motion's span in place instead of yanking
                // and deleting it, so they don't need a clipboard capture or selection restore.
                let case_change = match action {
                    Action::UpperCaseMotion { .. } => Some(CaseChange::Upper),
                    Action::LowerCaseMotion { .. } => Some(CaseChange::Lower),
                    _ => None,
                };

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

                if case_change.is_none() {
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
                }

                if is_textobject {
                    let inclusive = matches!(
                        motion,
                        Action::MoveWithinCharacter { .. } | Action::MoveAroundCharacter { .. }
                    );
                    for _ in 0..*count {
                        self.apply_action(
                            mode,
                            &motion,
                            buffer,
                            buffer_context,
                            buffer_display_context,
                            services,
                        );
                        if let Some(change) = case_change {
                            self.change_case_text_object(
                                buffer,
                                &mut buffer_display_context.selections,
                                inclusive,
                                change,
                            );
                        } else {
                            self.delete_text_object(
                                buffer,
                                &mut buffer_display_context.selections,
                                &mut buffer_display_context.folds,
                                inclusive,
                            );
                        }
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
                        if let Some(change) = case_change {
                            self.change_case_text(
                                buffer,
                                &mut buffer_display_context.selections,
                                change,
                            );
                        } else {
                            self.delete_text(
                                buffer,
                                &mut buffer_display_context.selections,
                                &mut buffer_display_context.folds,
                                0,
                            );
                        }
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
                self.delete_text(
                    buffer,
                    &mut buffer_display_context.selections,
                    &mut buffer_display_context.folds,
                    0,
                );
            }
            Action::InsertNewLine { count } => {
                let text = buffer_display_context
                    .selections
                    .text(buffer.as_text_buffer());
                if !text.is_empty() {
                    services.clipboard.set_text(text);
                }
                self.delete_text(
                    buffer,
                    &mut buffer_display_context.selections,
                    &mut buffer_display_context.folds,
                    0,
                );
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
            Action::Fold { count } => {
                if *count > 0 {
                    if let Ok(syntax_tree) = &buffer_context.treesitter {
                        self.fold(buffer, buffer_display_context, syntax_tree);
                    } else {
                        self.fold_with_scanner(buffer, buffer_display_context);
                    }
                }
            }
            Action::Unfold { count } => {
                if *count > 0 {
                    self.unfold(buffer, buffer_display_context);
                }
            }
            Action::NoOp | Action::Quit => {
                return None;
            }
            _ => {}
        }

        self.snap_selections_to_folds(buffer, buffer_display_context, action);
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

    /// Folds the syntax block enclosing each selection's cursor, collapsing the
    /// cursor to the start of that block. Nested/duplicate folds are skipped.
    fn fold(
        &self,
        buffer: &Buffer,
        buffer_display_context: &mut WindowState,
        syntax_tree: &vim_treesitter::SyntaxTree,
    ) {
        let text_buffer = buffer.as_text_buffer();
        self.fold_enclosing(buffer, buffer_display_context, |head_offset| {
            let block = syntax_tree.enclosing_block_at_byte(head_offset)?;
            let mut start_offset = block.byte_range.start;
            let mut end_offset = block.byte_range.end;

            let first_char = text_buffer
                .text_for_range(start_offset..start_offset + 1)
                .next()
                .and_then(|s| s.chars().next());
            let last_char = if end_offset > 0 {
                text_buffer
                    .text_for_range(end_offset - 1..end_offset)
                    .next()
                    .and_then(|s| s.chars().next())
            } else {
                None
            };

            if let (Some(fc), Some(lc)) = (first_char, last_char) {
                if (fc == '{' && lc == '}') || (fc == '[' && lc == ']') || (fc == '(' && lc == ')')
                {
                    start_offset += 1;
                    end_offset -= 1;
                }
            }

            Some((block.byte_range.clone(), start_offset..end_offset))
        });
    }

    /// Like [`Editor::fold`], but used when no tree-sitter grammar is
    /// available for the buffer: finds the enclosing block with the
    /// dependency-free [`vim_scanner::StructuralScanner`] instead, at the
    /// cost of only understanding braces/parens/brackets (no language-aware
    /// notion of "block", and no support for tags or backtick strings).
    fn fold_with_scanner(&self, buffer: &Buffer, buffer_display_context: &mut WindowState) {
        let text_buffer = buffer.as_text_buffer();
        self.fold_enclosing(buffer, buffer_display_context, |head_offset| {
            let block = scan_expanding(text_buffer, head_offset, true)?;
            Some((block.outer_range(), block.inner_range()))
        });
    }

    /// Shared implementation behind [`Editor::fold`] and
    /// [`Editor::fold_with_scanner`]: for each selection's cursor, asks
    /// `find_block` for the `(outer, inner)` byte ranges of the block
    /// enclosing it (used respectively to dedupe nested/duplicate folds and
    /// as the folded range itself), folds that inner range, and collapses
    /// the cursor to the block's start.
    fn fold_enclosing(
        &self,
        buffer: &Buffer,
        buffer_display_context: &mut WindowState,
        find_block: impl Fn(usize) -> Option<(Range<usize>, Range<usize>)>,
    ) {
        let text_buffer = buffer.as_text_buffer();
        let mut seen_ranges = std::collections::HashSet::new();
        let mut updated_selections = Vec::new();
        for selection in buffer_display_context.selections.selections.iter() {
            let head_offset = text_buffer.offset_for_anchor(&selection.head());
            let Some((outer, inner)) = find_block(head_offset) else {
                continue;
            };

            if seen_ranges.insert(outer.clone()) {
                let fold = display_map::Fold {
                    start: inner.start.to_point(text_buffer),
                    end: inner.end.to_point(text_buffer),
                };
                if !buffer_display_context.folds.contains(&fold) {
                    buffer_display_context.folds.push(fold);
                }
                let target_anchor = text_buffer.anchor_at(outer.start, Bias::Left);
                updated_selections.push(Selection {
                    id: selection.id,
                    start: target_anchor,
                    end: target_anchor,
                    reversed: false,
                    goal: SelectionGoal::None,
                });
            }
        }
        for new_sel in updated_selections {
            buffer_display_context
                .selections
                .update(text_buffer, &new_sel);
        }
        if let Some(first) = buffer_display_context.selections.first() {
            buffer_display_context.selections.point = first.head().to_point(text_buffer);
        }
    }

    /// Removes any fold whose range contains, or starts on the same row as, a
    /// selection's cursor.
    fn unfold(&self, buffer: &Buffer, buffer_display_context: &mut WindowState) {
        let text_buffer = buffer.as_text_buffer();
        let mut to_remove = Vec::new();
        for selection in buffer_display_context.selections.selections.iter() {
            let head_point = selection.head().to_point(text_buffer);
            for (idx, fold) in buffer_display_context.folds.iter().enumerate() {
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
            buffer_display_context.folds.remove(idx);
        }
    }

    /// After a motion runs, snaps any cursor that landed inside a fold to that
    /// fold's start (moving backward) or end (moving forward), so folded text
    /// can never be a cursor resting place.
    fn snap_selections_to_folds(
        &self,
        buffer: &Buffer,
        buffer_display_context: &mut WindowState,
        action: &Action,
    ) {
        if buffer_display_context.folds.is_empty() {
            return;
        }
        let text_buffer = buffer.as_text_buffer();

        let moving_right = matches!(
            action,
            Action::MoveRight { .. }
                | Action::MoveDown { .. }
                | Action::MoveToWord { .. }
                | Action::MoveToWordEnd { .. }
                | Action::MoveToBigWord { .. }
                | Action::MoveToEndOfLine { .. }
                | Action::MoveToEndOfDocument { .. }
                | Action::MoveToEndOfNextLine { .. }
        );

        let is_move_right = matches!(action, Action::MoveRight { .. });

        let mut updated_selections = Vec::new();
        for selection in &buffer_display_context.selections.selections {
            let head = selection.head().to_point(text_buffer);
            let mut new_head = head;
            for fold in &buffer_display_context.folds {
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
                let anchor_pos = selection.tail().to_point(text_buffer);
                let new_anchor = if anchor_pos == head {
                    new_head
                } else {
                    anchor_pos
                };

                updated_selections.push(Selection {
                    id: selection.id,
                    start: text_buffer.anchor_at(new_anchor, Bias::Left),
                    end: text_buffer.anchor_at(new_head, Bias::Left),
                    reversed: new_head < new_anchor,
                    goal: selection.goal,
                });
            }
        }

        for new_sel in updated_selections {
            buffer_display_context
                .selections
                .update(text_buffer, &new_sel);
        }

        if let Some(first) = buffer_display_context.selections.first() {
            buffer_display_context.selections.point = first.head().to_point(text_buffer);
        }
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
        let (text, kind) = services.clipboard.read();

        match kind {
            vim_clipboard::ClipboardKind::Character | vim_clipboard::ClipboardKind::Block => {
                // selections.move_right(false, 1, buffer.as_text_buffer());
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
        folds: &mut Vec<display_map::Fold>,
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
                    end = buffer
                        .as_text_buffer()
                        .clip_offset(end.saturating_add(1), Bias::Right);
                }

                (start, end)
            };

            if count != 0 {
                end = buffer
                    .as_text_buffer()
                    .clip_offset(end.saturating_add(count), Bias::Right);
            }

            if start != end {
                edits.push(vim_buffer::TextRange {
                    start: vim_buffer::ByteOffset(start),
                    end: vim_buffer::ByteOffset(end),
                });
            }
        }

        if !edits.is_empty() {
            for range in &edits {
                remove_overlapping_folds(
                    folds,
                    buffer.as_text_buffer(),
                    range.start.0,
                    range.end.0,
                );
            }
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
        folds: &mut Vec<display_map::Fold>,
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
                    end = buffer
                        .as_text_buffer()
                        .clip_offset(end.saturating_add(1), Bias::Right);
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
            for range in &edits {
                remove_overlapping_folds(
                    folds,
                    buffer.as_text_buffer(),
                    range.start.0,
                    range.end.0,
                );
            }
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

    /// Changes the case of the text spanned by each cursor's selection, mirroring the range
    /// computed by [`Editor::delete_text`] (always inclusive of one extra character when the
    /// selection is non-empty). Used for motion-based case changes like `guw`/`gUw`.
    fn change_case_text(
        &self,
        buffer: &mut Buffer,
        selections: &mut SelectionSet,
        change: CaseChange,
    ) -> bool {
        self.change_case_ranges(buffer, selections, true, change)
    }

    /// Changes the case of the text spanned by each cursor's selection, mirroring the range
    /// computed by [`Editor::delete_text_object`] (only inclusive of the trailing character when
    /// `inclusive` is set). Used for motion-based case changes over text objects, like `giw`.
    fn change_case_text_object(
        &self,
        buffer: &mut Buffer,
        selections: &mut SelectionSet,
        inclusive: bool,
        change: CaseChange,
    ) -> bool {
        self.change_case_ranges(buffer, selections, inclusive, change)
    }

    fn change_case_ranges(
        &self,
        buffer: &mut Buffer,
        selections: &mut SelectionSet,
        inclusive: bool,
        change: CaseChange,
    ) -> bool {
        let mut edits: Vec<(usize, usize, String)> = Vec::new();
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
                let text: String = buffer
                    .as_text_buffer()
                    .as_rope()
                    .chunks_in_range(start..end)
                    .collect();
                edits.push((start, end, text.change_case(change)));
            }
        }

        if edits.is_empty() {
            return false;
        }

        let mut tx = buffer.transaction(vim_buffer::EditOrigin::User);
        for (start, end, changed) in &edits {
            tx.replace(
                None,
                vim_buffer::TextRange::new(
                    vim_buffer::ByteOffset(*start),
                    vim_buffer::ByteOffset(*end),
                )
                .unwrap(),
                changed.as_str(),
            );
        }
        let _ = tx.commit(Some(selections.clone()));

        let mut new_selections = Vec::new();
        for (cursor, (start, _, _)) in cursors.iter().zip(edits.iter()) {
            let anchor = buffer.as_text_buffer().anchor_at(*start, Bias::Left);
            new_selections.push(Selection {
                id: cursor.id,
                start: anchor.clone(),
                end: anchor,
                reversed: false,
                goal: SelectionGoal::None,
            });
        }
        for sel in new_selections {
            selections.update(buffer.as_text_buffer(), &sel);
        }

        true
    }

    /// Changes the case of `count` whole lines starting at each cursor's current line, mirroring
    /// the row range computed by [`Editor::delete_current_line`]. Used for `guu`/`gUU`.
    fn change_case_lines(
        &self,
        buffer: &mut Buffer,
        selections: &mut SelectionSet,
        count: u32,
        change: CaseChange,
    ) {
        let mut edits: Vec<(usize, usize, String)> = Vec::new();
        let cursors = selections.selections.clone();
        for cursor in cursors.iter() {
            let point = cursor.head().to_point(buffer.as_text_buffer());
            let start_row = point.row;
            let end_row = (start_row + count).min(buffer.as_text_buffer().row_count());

            let start = Point::new(start_row, 0).to_offset(buffer.as_text_buffer());
            let end_point = Point::new(end_row, 0);
            let end = buffer
                .as_text_buffer()
                .clip_point(end_point, Bias::Right)
                .to_offset(buffer.as_text_buffer());

            if start != end {
                let text: String = buffer
                    .as_text_buffer()
                    .as_rope()
                    .chunks_in_range(start..end)
                    .collect();
                edits.push((start, end, text.change_case(change)));
            }
        }

        if edits.is_empty() {
            return;
        }

        let mut tx = buffer.transaction(vim_buffer::EditOrigin::User);
        for (start, end, changed) in &edits {
            tx.replace(
                None,
                vim_buffer::TextRange::new(
                    vim_buffer::ByteOffset(*start),
                    vim_buffer::ByteOffset(*end),
                )
                .unwrap(),
                changed.as_str(),
            );
        }
        let _ = tx.commit(Some(selections.clone()));

        // Replacing a range's text does not guarantee old anchors within that range keep their
        // original relative offset, so recompute fresh anchors at the start of each edited range
        // before moving to the first non-blank column.
        let mut new_selections = Vec::new();
        for (cursor, (start, _, _)) in cursors.iter().zip(edits.iter()) {
            let anchor = buffer.as_text_buffer().anchor_at(*start, Bias::Left);
            new_selections.push(Selection {
                id: cursor.id,
                start: anchor.clone(),
                end: anchor,
                reversed: false,
                goal: SelectionGoal::None,
            });
        }
        for sel in new_selections {
            selections.update(buffer.as_text_buffer(), &sel);
        }

        selections.move_to_start_of_line_non_space(false, buffer.as_text_buffer());
    }

    pub fn delete_current_line(
        &self,
        buffer: &mut Buffer,
        selections: &mut SelectionSet,
        folds: &mut Vec<display_map::Fold>,
        count: u32,
    ) {
        if count > 1 && selections.has_selection(buffer.as_text_buffer()) {
            selections.move_down(true, count.saturating_sub(1), buffer.as_text_buffer());
        }
        if self.delete_text(buffer, selections, folds, 0) {
            return;
        }
        let mut edits = Vec::new();
        let cursors = selections.selections.clone();
        for cursor in cursors.iter() {
            let point = cursor.head().to_point(buffer.as_text_buffer());
            let start_row = point.row;
            let end_row = (start_row + count).min(buffer.as_text_buffer().row_count());

            let start = Point::new(start_row, 0).to_offset(buffer.as_text_buffer());
            let end_point = Point::new(end_row, 0);
            let end = buffer
                .as_text_buffer()
                .clip_point(end_point, Bias::Right)
                .to_offset(buffer.as_text_buffer());

            if start != end {
                edits.push(vim_buffer::TextRange {
                    start: vim_buffer::ByteOffset(start),
                    end: vim_buffer::ByteOffset(end),
                });
            }
        }

        if !edits.is_empty() {
            for range in &edits {
                remove_overlapping_folds(
                    folds,
                    buffer.as_text_buffer(),
                    range.start.0,
                    range.end.0,
                );
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::services::Services;
    use crate::model::BufferState;
    use clock::ReplicaId;
    use vim_buffer::BufferId;
    use vim_ui::Viewport;

    #[test]
    fn test_select_similar() {
        let mut buffer = Buffer::new(BufferId::new(1).unwrap(), ReplicaId::LOCAL, "");
        let mut buffer_context = BufferState::unloaded();
        let mut window_state = WindowState::new(&buffer, Viewport::default());
        let mut services = Services::new();
        let editor = Editor::new();

        editor
            .execute(
                Mode::Normal,
                &Action::InsertText("hello hello hello".into()),
                &mut buffer,
                &mut buffer_context,
                &mut window_state,
                &mut services,
            )
            .unwrap();

        editor
            .execute(
                Mode::Normal,
                &Action::MoveLeft {
                    select: false,
                    count: 11,
                },
                &mut buffer,
                &mut buffer_context,
                &mut window_state,
                &mut services,
            )
            .unwrap();

        editor
            .execute(
                Mode::Normal,
                &Action::SelectSimilar,
                &mut buffer,
                &mut buffer_context,
                &mut window_state,
                &mut services,
            )
            .unwrap();

        assert_eq!(
            window_state.selections.primary().start.to_point(buffer.as_text_buffer()).column,
            6
        );
        assert_eq!(
            window_state.selections.primary().end.to_point(buffer.as_text_buffer()).column,
            10
        );

        editor
            .execute(
                Mode::Normal,
                &Action::SelectSimilar,
                &mut buffer,
                &mut buffer_context,
                &mut window_state,
                &mut services,
            )
            .unwrap();

        assert_eq!(window_state.selections.selections.len(), 2);
    }

    #[test]
    fn deleting_text_under_a_fold_removes_the_fold_instead_of_leaving_it_dangling() {
        let mut buffer = Buffer::new(BufferId::new(1).unwrap(), ReplicaId::LOCAL, "abcdef");
        let mut buffer_context = BufferState::unloaded();
        let mut window_state = WindowState::new(&buffer, Viewport::default());
        let mut services = Services::new();

        // A fold covering "abc", overlapping the single character `Delete`
        // is about to remove at the cursor (offset 0, the default selection).
        window_state.folds.push(display_map::Fold {
            start: Point::new(0, 0),
            end: Point::new(0, 3),
        });

        let editor = Editor::new();
        editor
            .execute(
                Mode::Normal,
                &Action::Delete { count: 1 },
                &mut buffer,
                &mut buffer_context,
                &mut window_state,
                &mut services,
            )
            .expect("action applies without panicking");

        assert!(window_state.folds.is_empty());
        assert_eq!(buffer.as_text_buffer().text(), "bcdef");
    }

    #[test]
    fn debug_put_lines_isolated() {
        let mut buffer = Buffer::new(
            BufferId::new(1).unwrap(),
            ReplicaId::LOCAL,
            "one\ntwo\nthree",
        );
        let mut buffer_context = BufferState::unloaded();
        let mut window_state = WindowState::new(&buffer, Viewport::default());
        let mut services = Services::new();
        services.clipboard.set_lines("one\n");

        let editor = Editor::new();
        editor
            .execute(
                Mode::Normal,
                &Action::PutLines {
                    line: 3,
                    before: false,
                },
                &mut buffer,
                &mut buffer_context,
                &mut window_state,
                &mut services,
            )
            .expect("action applies without panicking");

        eprintln!("RESULT: {:?}", buffer.as_text_buffer().text());
    }

    #[test]
    fn deleting_a_folds_entire_line_does_not_panic_on_the_next_update() {
        // Without `remove_overlapping_folds`, this fold would keep pointing at
        // row 1 after `dd` removes every row, and `WindowState::update`'s call
        // into `DisplayMap::fold` would panic trying to resolve a now
        // out-of-bounds `Point` against the shrunk buffer.
        let mut buffer = Buffer::new(BufferId::new(1).unwrap(), ReplicaId::LOCAL, "one\ntwo");
        let mut buffer_context = BufferState::unloaded();
        let mut window_state = WindowState::new(&buffer, Viewport::default());
        let mut services = Services::new();

        window_state.folds.push(display_map::Fold {
            start: Point::new(1, 0),
            end: Point::new(1, 3),
        });

        let editor = Editor::new();
        editor
            .execute(
                Mode::Normal,
                &Action::DeleteLine { count: 2 },
                &mut buffer,
                &mut buffer_context,
                &mut window_state,
                &mut services,
            )
            .expect("action applies without panicking");

        assert!(window_state.folds.is_empty());
        assert_eq!(buffer.as_text_buffer().text(), "");
    }

    #[test]
    fn move_within_character_falls_back_to_the_structural_scanner_without_a_syntax_tree() {
        let mut buffer = Buffer::new(BufferId::new(1).unwrap(), ReplicaId::LOCAL, "a { hello } b");
        let mut buffer_context = BufferState::unloaded();
        assert!(buffer_context.treesitter.is_err());
        let mut window_state = WindowState::new(&buffer, Viewport::default());
        let mut services = Services::new();

        // Place the cursor inside the braces, on 'h' of "hello".
        window_state.selections.clear(buffer.as_text_buffer());
        window_state.selections.add(buffer.as_text_buffer(), 4);

        let editor = Editor::new();
        editor
            .execute(
                Mode::Normal,
                &Action::MoveWithinCharacter { count: 1, ch: '{' },
                &mut buffer,
                &mut buffer_context,
                &mut window_state,
                &mut services,
            )
            .expect("action applies without panicking");

        assert_eq!(
            window_state.selections.text(buffer.as_text_buffer()),
            " hello "
        );
    }

    #[test]
    fn move_within_character_selects_inner_sentence_excluding_trailing_space() {
        let mut buffer = Buffer::new(
            BufferId::new(1).unwrap(),
            ReplicaId::LOCAL,
            "One. Two. Three.",
        );
        let mut buffer_context = BufferState::unloaded();
        let mut window_state = WindowState::new(&buffer, Viewport::default());
        let mut services = Services::new();

        // Place the cursor on the 'w' of "Two".
        window_state.selections.selections.clear();
        window_state.selections.add(buffer.as_text_buffer(), 6);

        let editor = Editor::new();
        editor
            .execute(
                Mode::Normal,
                &Action::MoveWithinCharacter { count: 1, ch: 's' },
                &mut buffer,
                &mut buffer_context,
                &mut window_state,
                &mut services,
            )
            .expect("action applies without panicking");

        // The inner sentence stops at the period, excluding the space that
        // separates it from "Three.".
        assert_eq!(
            window_state.selections.text(buffer.as_text_buffer()),
            "Two."
        );
    }

    #[test]
    fn move_within_character_selects_inner_sentence_across_a_blank_line() {
        let mut buffer = Buffer::new(
            BufferId::new(1).unwrap(),
            ReplicaId::LOCAL,
            "Line one. Line two.\n\nLine three.",
        );
        let mut buffer_context = BufferState::unloaded();
        let mut window_state = WindowState::new(&buffer, Viewport::default());
        let mut services = Services::new();

        // Place the cursor on the 't' of "two", the last sentence on row 0.
        window_state.selections.selections.clear();
        window_state.selections.add(buffer.as_text_buffer(), 15);

        let editor = Editor::new();
        editor
            .execute(
                Mode::Normal,
                &Action::MoveWithinCharacter { count: 1, ch: 's' },
                &mut buffer,
                &mut buffer_context,
                &mut window_state,
                &mut services,
            )
            .expect("action applies without panicking");

        // The inner sentence stops at the period, excluding the newline and
        // the following blank line.
        assert_eq!(
            window_state.selections.text(buffer.as_text_buffer()),
            "Line two."
        );
    }

    #[test]
    fn fold_falls_back_to_the_structural_scanner_without_a_syntax_tree() {
        let mut buffer = Buffer::new(BufferId::new(1).unwrap(), ReplicaId::LOCAL, "a { hello } b");
        let mut buffer_context = BufferState::unloaded();
        assert!(buffer_context.treesitter.is_err());
        let mut window_state = WindowState::new(&buffer, Viewport::default());
        let mut services = Services::new();

        window_state.selections.clear(buffer.as_text_buffer());
        window_state.selections.add(buffer.as_text_buffer(), 4);

        let editor = Editor::new();
        editor
            .execute(
                Mode::Normal,
                &Action::Fold { count: 1 },
                &mut buffer,
                &mut buffer_context,
                &mut window_state,
                &mut services,
            )
            .expect("action applies without panicking");

        assert_eq!(window_state.folds.len(), 1);
        assert_eq!(window_state.folds[0].start, Point::new(0, 3));
        assert_eq!(window_state.folds[0].end, Point::new(0, 10));
    }

    #[test]
    fn move_within_character_scanner_fallback_resolves_each_cursor_independently() {
        // Two disjoint paren pairs; one cursor inside each. The scanner-backed
        // fallback scan is built once for the whole action and then reused
        // for every cursor below, so this guards against a regression where
        // that sharing accidentally collapses every cursor onto the same
        // match instead of resolving each independently.
        let mut buffer = Buffer::new(BufferId::new(1).unwrap(), ReplicaId::LOCAL, "(a)(b)");
        let mut buffer_context = BufferState::unloaded();
        assert!(buffer_context.treesitter.is_err());
        let mut window_state = WindowState::new(&buffer, Viewport::default());
        let mut services = Services::new();

        window_state.selections.selections.clear();
        window_state.selections.add(buffer.as_text_buffer(), 1); // 'a'
        window_state.selections.add(buffer.as_text_buffer(), 4); // 'b'

        let editor = Editor::new();
        editor
            .execute(
                Mode::Normal,
                &Action::MoveWithinCharacter { count: 1, ch: '(' },
                &mut buffer,
                &mut buffer_context,
                &mut window_state,
                &mut services,
            )
            .expect("action applies without panicking");

        let text_buffer = buffer.as_text_buffer();
        let mut selected: Vec<String> = window_state
            .selections
            .selections
            .iter()
            .map(|s| s.text(text_buffer))
            .collect();
        selected.sort();
        assert_eq!(selected, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn scanner_fallback_expands_across_many_rows_to_find_a_distant_match() {
        // The opening brace is on row 0, the closing brace 200 rows later,
        // and the cursor sits on row 100: with the fallback's initial and
        // first-doubled row radius (64, then 128), neither attempt's window
        // reaches row 0, so this only succeeds if the scan keeps expanding
        // until the window covers the whole buffer.
        let mut lines = vec!["{".to_string()];
        for i in 0..199 {
            lines.push(format!("line{i}"));
        }
        lines.push("}".to_string());
        let text = lines.join("\n");
        let open_pos = text.find('{').unwrap();
        let close_pos = text.rfind('}').unwrap();
        let expected_inner = text[open_pos + 1..close_pos].to_string();

        let cursor_row = 100u32;
        let mut buffer = Buffer::new(BufferId::new(1).unwrap(), ReplicaId::LOCAL, text);
        let mut buffer_context = BufferState::unloaded();
        assert!(buffer_context.treesitter.is_err());
        let mut window_state = WindowState::new(&buffer, Viewport::default());
        let mut services = Services::new();

        let cursor_offset = Point::new(cursor_row, 0).to_offset(buffer.as_text_buffer());
        window_state.selections.selections.clear();
        window_state
            .selections
            .add(buffer.as_text_buffer(), cursor_offset);

        let editor = Editor::new();
        editor
            .execute(
                Mode::Normal,
                &Action::MoveWithinCharacter { count: 1, ch: '{' },
                &mut buffer,
                &mut buffer_context,
                &mut window_state,
                &mut services,
            )
            .expect("action applies without panicking");

        assert_eq!(
            window_state.selections.text(buffer.as_text_buffer()),
            expected_inner
        );
    }

    #[test]
    fn test_search_movements() {
        let text = "hello\nworld\nrust nextvim\nnext level\n";
        let mut buffer = Buffer::new(BufferId::new(1).unwrap(), ReplicaId::LOCAL, text);
        let mut buffer_context = BufferState::unloaded();
        let mut window_state = WindowState::new(&buffer, Viewport::default());
        let mut services = Services::new();

        window_state.selections.selections.clear();
        window_state.selections.add(buffer.as_text_buffer(), 0);
        window_state.selections.search = "next".to_string();
        window_state.selections.regex = vim_buffer::compile("next").map(std::sync::Arc::new);

        let editor = Editor::new();

        editor
            .execute(
                Mode::Normal,
                &Action::SearchForward { count: 1 },
                &mut buffer,
                &mut buffer_context,
                &mut window_state,
                &mut services,
            )
            .unwrap();

        let cursor_head = window_state.selections.primary().head();
        let point = cursor_head.to_point(buffer.as_text_buffer());
        assert_eq!(point.row, 2);
        assert_eq!(point.column, 5);

        editor
            .execute(
                Mode::Normal,
                &Action::SearchForward { count: 1 },
                &mut buffer,
                &mut buffer_context,
                &mut window_state,
                &mut services,
            )
            .unwrap();

        let point = window_state
            .selections
            .primary()
            .head()
            .to_point(buffer.as_text_buffer());
        assert_eq!(point.row, 3);
        assert_eq!(point.column, 0);

        editor
            .execute(
                Mode::Normal,
                &Action::SearchBackward { count: 1 },
                &mut buffer,
                &mut buffer_context,
                &mut window_state,
                &mut services,
            )
            .unwrap();

        let point = window_state
            .selections
            .primary()
            .head()
            .to_point(buffer.as_text_buffer());
        assert_eq!(point.row, 2);
        assert_eq!(point.column, 5);
    }

    #[test]
    fn test_change_case_normal_and_visual() {
        // Test Normal mode toggle case
        let text = "Hello WORLD";
        let mut buffer = Buffer::new(BufferId::new(1).unwrap(), ReplicaId::LOCAL, text);
        let mut buffer_context = BufferState::unloaded();
        let mut window_state = WindowState::new(&buffer, Viewport::default());
        let mut services = Services::new();

        // Put cursor at index 1 ('e')
        window_state.selections.selections.clear();
        window_state.selections.add(buffer.as_text_buffer(), 1);

        let editor = Editor::new();
        // Toggle 3 chars: "ell" -> "ELL"
        let next_mode = editor
            .execute(
                Mode::Normal,
                &Action::ChangeCase { count: 3 },
                &mut buffer,
                &mut buffer_context,
                &mut window_state,
                &mut services,
            )
            .unwrap();

        // Check text after toggle
        let buffer_text: String = buffer
            .as_text_buffer()
            .as_rope()
            .chunks_in_range(0..buffer.as_text_buffer().len())
            .collect();
        assert_eq!(buffer_text, "HELLo WORLD");

        // Cursor should move right by 3 chars (from offset 1 to offset 4)
        let point = window_state
            .selections
            .primary()
            .head()
            .to_point(buffer.as_text_buffer());
        assert_eq!(point.column, 4);

        // Test Visual mode toggle case
        let text2 = "Rust NextVim";
        let mut buffer2 = Buffer::new(BufferId::new(1).unwrap(), ReplicaId::LOCAL, text2);
        let mut window_state2 = WindowState::new(&buffer2, Viewport::default());

        // Set visual selection on "NextVim" (offset 5 to 12)
        window_state2.selections.selections.clear();
        window_state2.selections.add(buffer2.as_text_buffer(), 5);
        window_state2
            .selections
            .move_right(true, 7, buffer2.as_text_buffer());

        let next_mode2 = editor
            .execute(
                Mode::Visual,
                &Action::ChangeCase { count: 1 },
                &mut buffer2,
                &mut buffer_context,
                &mut window_state2,
                &mut services,
            )
            .unwrap();

        // "NextVim" -> "nEXTvIM"
        let buffer_text2: String = buffer2
            .as_text_buffer()
            .as_rope()
            .chunks_in_range(0..buffer2.as_text_buffer().len())
            .collect();
        assert_eq!(buffer_text2, "Rust nEXTvIM");
    }

    #[test]
    fn test_guu_and_guu_change_case_of_whole_lines() {
        let text = "Hello World\nSecond Line\nThird Line\n";
        let mut buffer = Buffer::new(BufferId::new(1).unwrap(), ReplicaId::LOCAL, text);
        let mut buffer_context = BufferState::unloaded();
        let mut window_state = WindowState::new(&buffer, Viewport::default());
        let mut services = Services::new();

        // Put cursor in the middle of the first line.
        window_state.selections.selections.clear();
        window_state.selections.add(buffer.as_text_buffer(), 3);

        let editor = Editor::new();

        // guu lowercases the current line and moves the cursor to the first non-blank.
        editor
            .execute(
                Mode::Normal,
                &Action::LowerCaseLine { count: 1 },
                &mut buffer,
                &mut buffer_context,
                &mut window_state,
                &mut services,
            )
            .unwrap();

        let buffer_text: String = buffer
            .as_text_buffer()
            .as_rope()
            .chunks_in_range(0..buffer.as_text_buffer().len())
            .collect();
        assert_eq!(buffer_text, "hello world\nSecond Line\nThird Line\n");

        let point = window_state
            .selections
            .primary()
            .head()
            .to_point(buffer.as_text_buffer());
        assert_eq!(point.row, 0);
        assert_eq!(point.column, 0);

        // gUU (with a count of 2) uppercases the current and next line.
        editor
            .execute(
                Mode::Normal,
                &Action::UpperCaseLine { count: 2 },
                &mut buffer,
                &mut buffer_context,
                &mut window_state,
                &mut services,
            )
            .unwrap();

        let buffer_text: String = buffer
            .as_text_buffer()
            .as_rope()
            .chunks_in_range(0..buffer.as_text_buffer().len())
            .collect();
        assert_eq!(buffer_text, "HELLO WORLD\nSECOND LINE\nThird Line\n");
    }

    #[test]
    fn test_gu_motion_and_gu_motion_change_case_of_a_range() {
        let text = "Hello World Again";
        let mut buffer = Buffer::new(BufferId::new(1).unwrap(), ReplicaId::LOCAL, text);
        let mut buffer_context = BufferState::unloaded();
        let mut window_state = WindowState::new(&buffer, Viewport::default());
        let mut services = Services::new();

        // Put cursor at the start of "World".
        window_state.selections.selections.clear();
        window_state.selections.add(buffer.as_text_buffer(), 6);

        let editor = Editor::new();

        // guw lowercases from the cursor to the start of the next word.
        editor
            .execute(
                Mode::Normal,
                &Action::LowerCaseMotion {
                    count: 1,
                    motion: Box::new(Action::MoveToWord {
                        count: 1,
                        select: false,
                    }),
                },
                &mut buffer,
                &mut buffer_context,
                &mut window_state,
                &mut services,
            )
            .unwrap();

        let buffer_text: String = buffer
            .as_text_buffer()
            .as_rope()
            .chunks_in_range(0..buffer.as_text_buffer().len())
            .collect();
        assert_eq!(buffer_text, "Hello world Again");

        // Cursor should rest at the start of the changed range.
        let point = window_state
            .selections
            .primary()
            .head()
            .to_point(buffer.as_text_buffer());
        assert_eq!(point.column, 6);

        // gUw uppercases from the cursor to the start of the next word.
        editor
            .execute(
                Mode::Normal,
                &Action::UpperCaseMotion {
                    count: 1,
                    motion: Box::new(Action::MoveToWord {
                        count: 1,
                        select: false,
                    }),
                },
                &mut buffer,
                &mut buffer_context,
                &mut window_state,
                &mut services,
            )
            .unwrap();

        let buffer_text: String = buffer
            .as_text_buffer()
            .as_rope()
            .chunks_in_range(0..buffer.as_text_buffer().len())
            .collect();
        assert_eq!(buffer_text, "Hello WORLD Again");
    }
}
