use super::{CommandEffect, CommandOutcome, NormalCommand, WindowId};

use sum_tree::Bias;
use text::{Point, Selection, SelectionGoal, ToOffset, ToPoint};
use vim_buffer::{Buffer, Motions, SelectionSet};
use vim_input::Mode;
use vim_ui::WindowState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MotionKind {
    Characterwise { inclusive: bool },
    Linewise,
}

#[derive(Clone)]
struct ResolvedMotion {
    selections: SelectionSet,
    kind: MotionKind,
}

fn delimiter_kind(ch: char) -> Option<vim_scanner::DelimiterKind> {
    Some(match ch {
        '{' | '}' => vim_scanner::DelimiterKind::Brace,
        '[' | ']' => vim_scanner::DelimiterKind::Bracket,
        '(' | ')' => vim_scanner::DelimiterKind::Paren,
        '\'' => vim_scanner::DelimiterKind::SingleQuote,
        '"' => vim_scanner::DelimiterKind::DoubleQuote,
        '`' => vim_scanner::DelimiterKind::BackTick,
        _ => return None,
    })
}

fn enclosing_delimiter(buffer: &text::Buffer, byte: usize, ch: char) -> Option<(usize, usize)> {
    let kind = delimiter_kind(ch)?;
    let matched = vim_scanner::StructuralScanner::scan_rows_for_enclosing(
        buffer,
        0,
        buffer.row_count(),
        byte,
        false,
    )?;
    (matched.kind == kind).then_some((matched.start, matched.end))
}

fn motion_kind(motion: &vim_input::Action) -> MotionKind {
    if matches!(
        motion,
        vim_input::Action::MoveUp { .. } | vim_input::Action::MoveDown { .. }
    ) {
        return MotionKind::Linewise;
    }
    let inclusive = matches!(
        motion,
        vim_input::Action::MoveToWordEnd { .. }
            | vim_input::Action::MoveToPreviousWordEnd { .. }
            | vim_input::Action::MoveToBigWordEnd { .. }
            | vim_input::Action::MoveToPreviousBigWordEnd { .. }
            | vim_input::Action::MoveToEndOfLine { .. }
            | vim_input::Action::MoveToNextCharacter { .. }
            | vim_input::Action::MoveToPreviousCharacter { .. }
    );
    MotionKind::Characterwise { inclusive }
}

fn resolve_text_object_selection(
    action: &vim_input::Action,
    buffer: &text::Buffer,
    selections: &SelectionSet,
    syntax_tree: Option<&vim_treesitter::SyntaxTree>,
) -> Option<SelectionSet> {
    let mut resolved = selections.clone();
    if resolved.selections.is_empty() {
        resolved.add(buffer, 0);
    }
    let (around, count, ch) = match action {
        vim_input::Action::MoveWithinCharacter { count, ch } => (false, *count, *ch),
        vim_input::Action::MoveAroundCharacter { count, ch } => (true, *count, *ch),
        _ => return None,
    };
    if count == 0 {
        return None;
    }
    if ch == 'w' {
        let cursors = resolved.selections.clone();
        for cursor in cursors {
            let start = cursor.move_to_word(false, buffer).head();
            let end = if around {
                let next = cursor.move_to_next_word(false, buffer).head();
                let offset = buffer.offset_for_anchor(&next).saturating_sub(1);
                buffer.anchor_at(offset, Bias::Right)
            } else {
                cursor.move_to_word_end(false, buffer).head()
            };
            resolved.update(
                buffer,
                &Selection {
                    id: cursor.id,
                    start,
                    end,
                    reversed: false,
                    goal: SelectionGoal::None,
                },
            );
        }
        return Some(resolved);
    }
    let cursors = resolved.selections.clone();
    for cursor in cursors {
        let byte = buffer.offset_for_anchor(&cursor.head());
        let boundaries = if let Some(tree) = syntax_tree {
            tree.delimiter_boundaries_at_byte(byte)
                .map(|(start, end)| (start.byte_range.start, end.byte_range.end))
        } else {
            enclosing_delimiter(buffer, byte, ch)
        }?;
        let (start, end) = if around {
            boundaries
        } else {
            (
                boundaries.0.saturating_add(1),
                boundaries.1.saturating_sub(1),
            )
        };
        resolved.update(
            buffer,
            &Selection {
                id: cursor.id,
                start: buffer.anchor_at(start, Bias::Left),
                end: buffer.anchor_at(end, Bias::Right),
                reversed: false,
                goal: SelectionGoal::None,
            },
        );
    }
    Some(resolved)
}

fn resolve_operator_motion(
    operator_count: usize,
    motion: &vim_input::Action,
    buffer: &text::Buffer,
    selections: &SelectionSet,
    syntax_tree: Option<&vim_treesitter::SyntaxTree>,
) -> Option<ResolvedMotion> {
    if matches!(
        motion,
        vim_input::Action::MoveWithinCharacter { .. }
            | vim_input::Action::MoveAroundCharacter { .. }
    ) {
        return Some(ResolvedMotion {
            selections: resolve_text_object_selection(motion, buffer, selections, syntax_tree)?,
            kind: MotionKind::Characterwise { inclusive: false },
        });
    }
    let mut resolved = selections.clone();
    if resolved.selections.is_empty() {
        resolved.add(buffer, 0);
    }
    let kind = motion_kind(motion);
    if kind == MotionKind::Linewise {
        resolved.begin_line(buffer);
    }
    if !apply_buffer_motion(motion, Some(true), operator_count, &mut resolved, buffer) {
        return None;
    }
    if kind == MotionKind::Linewise {
        resolved.sync_line(buffer);
        resolved.end_line();
    }
    Some(ResolvedMotion {
        selections: resolved,
        kind,
    })
}

/// Executes the migrated basic-motion slice without entering the legacy action
/// match. The window state is supplied for the duration of this command only;
/// no reference escapes the call boundary.
pub(crate) fn execute_delete(
    count: usize,
    mode: Mode,
    buffer: &mut Buffer,
    selections: &mut SelectionSet,
    folds: &mut Vec<display_map::Fold>,
) -> Option<super::MutationOutcome> {
    if count > 1 && selections.has_selection(buffer.as_text_buffer()) {
        if mode == Mode::VisualLine {
            selections.move_down(
                true,
                count.saturating_sub(1).min(u32::MAX as usize) as u32,
                buffer.as_text_buffer(),
            );
        } else {
            selections.move_right(
                true,
                count.saturating_sub(1).min(u32::MAX as usize) as u32,
                buffer.as_text_buffer(),
            );
        }
    }

    let mut edits = Vec::new();
    for cursor in selections.selections.clone() {
        let (start_anchor, end_anchor) =
            if cursor.head().cmp(&cursor.tail(), buffer.as_text_buffer())
                == std::cmp::Ordering::Less
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
        let start = buffer.as_text_buffer().offset_for_anchor(&start_anchor);
        let mut end = buffer.as_text_buffer().offset_for_anchor(&end_anchor);
        if start != end {
            end = buffer
                .as_text_buffer()
                .clip_offset(end.saturating_add(1), text::Bias::Right);
        }
        if count != 0 {
            end = buffer
                .as_text_buffer()
                .clip_offset(end.saturating_add(count), text::Bias::Right);
        }
        if start != end {
            edits.push(vim_buffer::TextRange {
                start: vim_buffer::ByteOffset(start),
                end: vim_buffer::ByteOffset(end),
            });
        }
    }

    for range in &edits {
        crate::controller::editor::remove_overlapping_folds(
            folds,
            buffer.as_text_buffer(),
            range.start.0,
            range.end.0,
        );
    }
    if edits.is_empty() {
        return None;
    }
    let selection_snapshot = selections.clone();
    super::transaction(
        buffer,
        vim_buffer::EditOrigin::User,
        Some(selection_snapshot),
        |tx| {
            for range in edits {
                tx.delete(None, range);
            }
        },
    )
    .ok()
}

fn apply_buffer_motion(
    action: &vim_input::Action,
    force_select: Option<bool>,
    count_multiplier: usize,
    selections: &mut SelectionSet,
    buffer: &text::Buffer,
) -> bool {
    let select = force_select.unwrap_or_else(|| match action {
        vim_input::Action::MoveLeft { select, .. }
        | vim_input::Action::MoveRight { select, .. }
        | vim_input::Action::MoveUp { select, .. }
        | vim_input::Action::MoveDown { select, .. }
        | vim_input::Action::MoveToWord { select, .. }
        | vim_input::Action::MoveToPreviousWord { select, .. }
        | vim_input::Action::MoveToWordEnd { select, .. }
        | vim_input::Action::MoveToPreviousWordEnd { select, .. }
        | vim_input::Action::MoveToBigWord { select, .. }
        | vim_input::Action::MoveToPreviousBigWord { select, .. }
        | vim_input::Action::MoveToBigWordEnd { select, .. }
        | vim_input::Action::MoveToPreviousBigWordEnd { select, .. }
        | vim_input::Action::MoveToStartOfDocument { select, .. }
        | vim_input::Action::MoveToEndOfDocument { select, .. }
        | vim_input::Action::MoveToStartOfLine { select, .. }
        | vim_input::Action::MoveToStartOfLineNonSpace { select, .. }
        | vim_input::Action::MoveToEndOfLine { select, .. }
        | vim_input::Action::MoveToLine { select, .. }
        | vim_input::Action::MoveToLastNonWhitespace { select, .. }
        | vim_input::Action::MoveToStartOfPreviousLine { select, .. }
        | vim_input::Action::MoveToEndOfPreviousLine { select, .. }
        | vim_input::Action::MoveToStartOfNextLine { select, .. }
        | vim_input::Action::MoveToEndOfNextLine { select, .. }
        | vim_input::Action::MoveToPreviousParagraph { select, .. }
        | vim_input::Action::MoveToNextParagraph { select, .. }
        | vim_input::Action::MoveToPreviousSentence { select, .. }
        | vim_input::Action::MoveToNextSentence { select, .. }
        | vim_input::Action::MoveToNextCharacter { select, .. }
        | vim_input::Action::MoveToPreviousCharacter { select, .. } => *select,
        _ => false,
    });
    let scaled = |count: u32| {
        (count as usize)
            .max(1)
            .saturating_mul(count_multiplier.max(1))
            .min(u32::MAX as usize) as u32
    };

    match action {
        vim_input::Action::MoveLeft { count, .. } => {
            selections.move_left(select, scaled(*count), buffer)
        }
        vim_input::Action::MoveRight { count, .. } => {
            selections.move_right(select, scaled(*count), buffer)
        }
        vim_input::Action::MoveUp { count, .. } => {
            selections.move_up(select, scaled(*count), buffer)
        }
        vim_input::Action::MoveDown { count, .. } => {
            selections.move_down(select, scaled(*count), buffer)
        }
        vim_input::Action::MoveToWord { count, .. } => {
            selections.move_to_next_word(select, scaled(*count), buffer)
        }
        vim_input::Action::MoveToPreviousWord { count, .. } => {
            selections.move_to_previous_word(select, scaled(*count), buffer)
        }
        vim_input::Action::MoveToWordEnd { count, .. } => {
            selections.move_to_word_end(select, scaled(*count), buffer)
        }
        vim_input::Action::MoveToPreviousWordEnd { count, .. } => {
            selections.move_to_previous_word_end(select, scaled(*count), buffer)
        }
        vim_input::Action::MoveToBigWord { count, .. } => {
            selections.move_to_big_word(select, scaled(*count), buffer)
        }
        vim_input::Action::MoveToPreviousBigWord { count, .. } => {
            selections.move_to_previous_big_word(select, scaled(*count), buffer)
        }
        vim_input::Action::MoveToBigWordEnd { count, .. } => {
            selections.move_to_big_word_end(select, scaled(*count), buffer)
        }
        vim_input::Action::MoveToPreviousBigWordEnd { count, .. } => {
            selections.move_to_previous_big_word_end(select, scaled(*count), buffer)
        }
        vim_input::Action::MoveToStartOfDocument { .. } => {
            selections.move_to_start_of_document(select, buffer)
        }
        vim_input::Action::MoveToEndOfDocument { .. } => {
            selections.move_to_end_of_document(select, buffer)
        }
        vim_input::Action::MoveToStartOfLine { .. } => {
            selections.move_to_start_of_line(select, buffer)
        }
        vim_input::Action::MoveToStartOfLineNonSpace { .. } => {
            selections.move_to_start_of_line_non_space(select, buffer)
        }
        vim_input::Action::MoveToEndOfLine { .. } => selections.move_to_end_of_line(select, buffer),
        vim_input::Action::MoveToLine { line, .. } => {
            selections.move_to_line(select, *line, buffer)
        }
        vim_input::Action::MoveToLastNonWhitespace { count, .. } => {
            let cursors = selections.selections.clone();
            let last_row = buffer.row_count().saturating_sub(1);
            for cursor in cursors {
                let row = cursor
                    .head()
                    .to_point(buffer)
                    .row
                    .saturating_add(scaled(*count).saturating_sub(1))
                    .min(last_row);
                let start = Point::new(row, 0).to_offset(buffer);
                let end = Point::new(row, buffer.line_len(row)).to_offset(buffer);
                let row_text: String = buffer.as_rope().chunks_in_range(start..end).collect();
                let column = row_text
                    .char_indices()
                    .rev()
                    .find_map(|(offset, ch)| (!ch.is_whitespace()).then_some(offset as u32))
                    .unwrap_or(0);
                let target =
                    buffer.anchor_at(Point::new(row, column).to_offset(buffer), text::Bias::Left);
                selections.update(
                    buffer,
                    &Selection {
                        id: cursor.id,
                        start: if select {
                            cursor.tail()
                        } else {
                            target.clone()
                        },
                        end: target,
                        reversed: true,
                        goal: SelectionGoal::None,
                    },
                );
            }
        }
        vim_input::Action::MoveToStartOfPreviousLine { count, .. } => {
            for _ in 0..scaled(*count) {
                selections.move_to_start_of_previous_line(select, buffer);
            }
        }
        vim_input::Action::MoveToEndOfPreviousLine { count, .. } => {
            for _ in 0..scaled(*count) {
                selections.move_to_end_of_previous_line(select, buffer);
            }
        }
        vim_input::Action::MoveToStartOfNextLine { count, .. } => {
            for _ in 0..scaled(*count) {
                selections.move_to_start_of_next_line(select, buffer);
            }
        }
        vim_input::Action::MoveToEndOfNextLine { count, .. } => {
            for _ in 0..scaled(*count) {
                selections.move_to_end_of_next_line(select, buffer);
            }
        }
        vim_input::Action::MoveToPreviousParagraph { count, .. } => {
            selections.move_to_previous_paragraph(select, scaled(*count), buffer)
        }
        vim_input::Action::MoveToNextParagraph { count, .. } => {
            selections.move_to_next_paragraph(select, scaled(*count), buffer)
        }
        vim_input::Action::MoveToPreviousSentence { count, .. } => {
            selections.move_to_previous_sentence(select, scaled(*count), buffer)
        }
        vim_input::Action::MoveToNextSentence { count, .. } => {
            selections.move_to_next_sentence(select, scaled(*count), buffer)
        }
        vim_input::Action::MoveToNextCharacter {
            count, ch, till, ..
        } => selections.find_character(select, scaled(*count), *ch, true, *till, buffer),
        vim_input::Action::MoveToPreviousCharacter {
            count, ch, till, ..
        } => selections.find_character(select, scaled(*count), *ch, false, *till, buffer),
        vim_input::Action::SearchForward { count }
        | vim_input::Action::SearchBackward { count } => {
            let originals = selections.selections.clone();
            let pattern = selections.search.clone();
            for _ in 0..scaled(*count) {
                if matches!(action, vim_input::Action::SearchForward { .. }) {
                    selections.move_to_next_match(&pattern, true, buffer);
                } else {
                    selections.move_to_previous_match(&pattern, true, buffer);
                }
            }
            if select {
                let destinations = selections.selections.clone();
                for destination in destinations {
                    if let Some(original) = originals.iter().find(|item| item.id == destination.id)
                    {
                        selections.update(
                            buffer,
                            &Selection {
                                id: destination.id,
                                start: original.tail(),
                                end: destination.head(),
                                reversed: destination.head().cmp(&original.tail(), buffer)
                                    == std::cmp::Ordering::Less,
                                goal: SelectionGoal::None,
                            },
                        );
                    }
                }
            }
        }
        _ => return false,
    }
    true
}

/// Resolves and executes the first operator-motion family entirely at the
/// kernel transaction boundary. Unsupported motions return `None` so the
/// compatibility dispatcher can continue handling them while migration is in
/// progress.
pub(crate) fn execute_yank_motion(
    operator_count: usize,
    motion: &vim_input::Action,
    buffer: &text::Buffer,
    selections: &SelectionSet,
) -> Option<(String, MotionKind)> {
    execute_yank_motion_with_syntax(operator_count, motion, buffer, selections, None)
}

pub(crate) fn execute_yank_motion_with_syntax(
    operator_count: usize,
    motion: &vim_input::Action,
    buffer: &text::Buffer,
    selections: &SelectionSet,
    syntax_tree: Option<&vim_treesitter::SyntaxTree>,
) -> Option<(String, MotionKind)> {
    let resolved =
        resolve_operator_motion(operator_count, motion, buffer, selections, syntax_tree)?;
    Some((resolved.selections.text(buffer), resolved.kind))
}

pub(crate) fn execute_case_motion(
    operator_count: usize,
    motion: &vim_input::Action,
    change: super::CaseChange,
    buffer: &mut Buffer,
    selections: &mut SelectionSet,
    folds: &mut Vec<display_map::Fold>,
) -> Option<Option<super::MutationOutcome>> {
    execute_case_motion_with_syntax(
        operator_count,
        motion,
        change,
        buffer,
        selections,
        folds,
        None,
    )
}

pub(crate) fn execute_case_motion_with_syntax(
    operator_count: usize,
    motion: &vim_input::Action,
    change: super::CaseChange,
    buffer: &mut Buffer,
    selections: &mut SelectionSet,
    folds: &mut Vec<display_map::Fold>,
    syntax_tree: Option<&vim_treesitter::SyntaxTree>,
) -> Option<Option<super::MutationOutcome>> {
    let resolved = resolve_operator_motion(
        operator_count,
        motion,
        buffer.as_text_buffer(),
        selections,
        syntax_tree,
    )?;
    let inclusive = match resolved.kind {
        MotionKind::Linewise => true,
        MotionKind::Characterwise { inclusive } => inclusive,
    };
    let cursors = resolved.selections.selections.clone();
    let mut edits = Vec::new();
    for cursor in &cursors {
        let (start_anchor, end_anchor) =
            if cursor.head().cmp(&cursor.tail(), buffer.as_text_buffer())
                == std::cmp::Ordering::Less
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
        let start = buffer.as_text_buffer().offset_for_anchor(&start_anchor);
        let mut end = buffer.as_text_buffer().offset_for_anchor(&end_anchor);
        if inclusive && start != end {
            end = buffer
                .as_text_buffer()
                .clip_offset(end.saturating_add(1), text::Bias::Right);
        }
        if start == end {
            continue;
        }
        let source: String = buffer
            .as_text_buffer()
            .as_rope()
            .chunks_in_range(start..end)
            .collect();
        let replacement = match change {
            super::CaseChange::Upper => source.to_uppercase(),
            super::CaseChange::Lower => source.to_lowercase(),
        };
        edits.push((cursor.id, start, end, replacement));
    }
    if edits.is_empty() {
        return Some(None);
    }
    for &(_, start, end, _) in &edits {
        crate::controller::editor::remove_overlapping_folds(
            folds,
            buffer.as_text_buffer(),
            start,
            end,
        );
    }
    let selection_snapshot = resolved.selections.clone();
    let mutation = super::transaction(
        buffer,
        vim_buffer::EditOrigin::User,
        Some(selection_snapshot),
        |tx| {
            for &(_, start, end, ref replacement) in &edits {
                tx.replace(
                    None,
                    vim_buffer::TextRange::new(
                        vim_buffer::ByteOffset(start),
                        vim_buffer::ByteOffset(end),
                    )
                    .expect("resolved motion range must be ordered"),
                    replacement.as_str(),
                );
            }
        },
    )
    .ok();
    for &(id, start, _, _) in &edits {
        let anchor = buffer.as_text_buffer().anchor_at(start, text::Bias::Left);
        selections.update(
            buffer.as_text_buffer(),
            &Selection {
                id,
                start: anchor.clone(),
                end: anchor,
                reversed: false,
                goal: SelectionGoal::None,
            },
        );
    }
    Some(mutation)
}

fn delete_exact_selection(
    buffer: &mut Buffer,
    selections: &mut SelectionSet,
    folds: &mut Vec<display_map::Fold>,
) -> Option<super::MutationOutcome> {
    let mut ranges = Vec::new();
    for cursor in selections.selections.clone() {
        let a = buffer.as_text_buffer().offset_for_anchor(&cursor.tail());
        let b = buffer.as_text_buffer().offset_for_anchor(&cursor.head());
        let start = a.min(b);
        let end = buffer
            .as_text_buffer()
            .clip_offset(a.max(b).saturating_add(1), Bias::Right);
        if start != end {
            ranges.push(
                vim_buffer::TextRange::new(
                    vim_buffer::ByteOffset(start),
                    vim_buffer::ByteOffset(end),
                )
                .expect("text object range must be ordered"),
            );
        }
    }
    if ranges.is_empty() {
        return None;
    }
    for range in &ranges {
        crate::controller::editor::remove_overlapping_folds(
            folds,
            buffer.as_text_buffer(),
            range.start.0,
            range.end.0,
        );
    }
    let selection_snapshot = selections.clone();
    super::transaction(
        buffer,
        vim_buffer::EditOrigin::User,
        Some(selection_snapshot),
        |tx| {
            for range in ranges {
                tx.delete(None, range);
            }
        },
    )
    .ok()
}

pub(crate) fn execute_delete_motion(
    operator_count: usize,
    motion: &vim_input::Action,
    buffer: &mut Buffer,
    selections: &mut SelectionSet,
    folds: &mut Vec<display_map::Fold>,
) -> Option<(String, Option<super::MutationOutcome>)> {
    execute_delete_motion_with_syntax(operator_count, motion, buffer, selections, folds, None)
}

pub(crate) fn execute_delete_motion_with_syntax(
    operator_count: usize,
    motion: &vim_input::Action,
    buffer: &mut Buffer,
    selections: &mut SelectionSet,
    folds: &mut Vec<display_map::Fold>,
    syntax_tree: Option<&vim_treesitter::SyntaxTree>,
) -> Option<(String, Option<super::MutationOutcome>)> {
    let resolved = resolve_operator_motion(
        operator_count,
        motion,
        buffer.as_text_buffer(),
        selections,
        syntax_tree,
    )?;
    *selections = resolved.selections;
    let deleted_text = selections.text(buffer.as_text_buffer());
    let is_text_object = matches!(
        motion,
        vim_input::Action::MoveWithinCharacter { .. }
            | vim_input::Action::MoveAroundCharacter { .. }
    );
    let mutation = if is_text_object {
        delete_exact_selection(buffer, selections, folds)
    } else {
        execute_delete(0, Mode::Normal, buffer, selections, folds)
    };
    if mutation.is_none() {
        return Some((String::new(), None));
    }
    Some((deleted_text, mutation))
}

fn apply_viewport_motion(
    action: &vim_input::Action,
    mode: Mode,
    buffer: &text::Buffer,
    window: &mut WindowState,
) -> bool {
    match action {
        vim_input::Action::MovePageUp { count, select }
        | vim_input::Action::MovePageDown { count, select } => {
            let rows = window
                .display_map
                .snapshot()
                .visible_rows
                .saturating_sub(4)
                .max(1)
                .saturating_mul(*count);
            if matches!(action, vim_input::Action::MovePageUp { .. }) {
                window
                    .selections
                    .move_up(*select || mode.is_visual(), rows, buffer);
            } else {
                window
                    .selections
                    .move_down(*select || mode.is_visual(), rows, buffer);
            }
        }
        vim_input::Action::ScrollHalfPageUp { count }
        | vim_input::Action::ScrollHalfPageDown { count } => {
            let rows = (window
                .display_map
                .snapshot()
                .visible_rows
                .saturating_sub(4)
                .max(2)
                / 2)
            .max(1)
            .saturating_mul(*count);
            if matches!(action, vim_input::Action::ScrollHalfPageUp { .. }) {
                window.selections.move_up(false, rows, buffer);
            } else {
                window.selections.move_down(false, rows, buffer);
            }
        }
        vim_input::Action::CenterCursorLine
        | vim_input::Action::CursorLineTop
        | vim_input::Action::CursorLineBottom => {
            let Some(cursor) = window
                .selections
                .first()
                .map(|selection| selection.head().to_point(buffer))
            else {
                return true;
            };
            let display_cursor = window.display_map.snapshot().point_to_display_point(cursor);
            let viewport = window.viewport;
            window.display_map.scroll_to_cursor(
                display_cursor,
                viewport.height as i32,
                viewport.width as i32,
            );
            let snapshot = window.display_map.snapshot();
            let visible_rows = snapshot.visible_rows;
            if visible_rows > 0 {
                let desired = match action {
                    vim_input::Action::CenterCursorLine => {
                        display_cursor.row().saturating_sub(visible_rows / 2)
                    }
                    vim_input::Action::CursorLineTop => display_cursor.row(),
                    vim_input::Action::CursorLineBottom => display_cursor
                        .row()
                        .saturating_sub(visible_rows.saturating_sub(1)),
                    _ => unreachable!(),
                };
                window.display_map.scroll_y =
                    desired.min(snapshot.row_count().saturating_sub(visible_rows));
            }
        }
        vim_input::Action::MoveToScreenTop { select, .. }
        | vim_input::Action::MoveToScreenMiddle { select, .. }
        | vim_input::Action::MoveToScreenBottom { select, .. } => {
            let snapshot = window.display_map.snapshot();
            let display_row = match action {
                vim_input::Action::MoveToScreenTop { .. } => snapshot.scroll_y,
                vim_input::Action::MoveToScreenMiddle { .. } => {
                    snapshot.scroll_y + snapshot.visible_rows / 2
                }
                vim_input::Action::MoveToScreenBottom { .. } => {
                    snapshot.scroll_y + snapshot.visible_rows.saturating_sub(1)
                }
                _ => unreachable!(),
            };
            let point =
                snapshot.display_point_to_point(display_map::DisplayPoint::new(display_row, 0));
            window
                .selections
                .move_to_line(*select || mode.is_visual(), point.row, buffer);
        }
        _ => return false,
    }
    true
}

pub(crate) fn execute_syntax_text_object(
    action: &vim_input::Action,
    buffer: &text::Buffer,
    window: &mut WindowState,
    syntax_tree: &vim_treesitter::SyntaxTree,
) -> bool {
    let (around, count) = match action {
        vim_input::Action::MoveWithinCharacter { count, .. } => (false, *count),
        vim_input::Action::MoveAroundCharacter { count, .. } => (true, *count),
        _ => return false,
    };
    let ch = match action {
        vim_input::Action::MoveWithinCharacter { ch, .. }
        | vim_input::Action::MoveAroundCharacter { ch, .. } => *ch,
        _ => unreachable!(),
    };
    if count == 0 || !matches!(ch, '<' | '>' | 't' | '{' | '}' | '(' | ')' | '[' | ']') {
        return false;
    }
    let cursors = window.selections.selections.clone();
    let mut changed = false;
    for cursor in cursors {
        let byte = buffer.offset_for_anchor(&cursor.head());
        let Some((start_node, end_node)) = syntax_tree.delimiter_boundaries_at_byte(byte) else {
            continue;
        };
        let (start, end) = if around {
            (start_node.byte_range.start, end_node.byte_range.end)
        } else {
            (start_node.byte_range.end, end_node.byte_range.start)
        };
        if start > end {
            continue;
        }
        let start = buffer.anchor_at(start, Bias::Left);
        let end = buffer.anchor_at(end, Bias::Right);
        window.selections.update(
            buffer,
            &Selection {
                id: cursor.id,
                start,
                end,
                reversed: false,
                goal: SelectionGoal::None,
            },
        );
        changed = true;
    }
    changed
}

pub(crate) fn execute_fold(
    count: usize,
    buffer: &text::Buffer,
    window: &mut WindowState,
    syntax_tree: Option<&vim_treesitter::SyntaxTree>,
) -> bool {
    if count == 0 {
        return false;
    }
    let mut changed = false;
    let cursors = window.selections.selections.clone();
    let mut seen = std::collections::HashSet::new();
    for selection in cursors {
        let byte = buffer.offset_for_anchor(&selection.head());
        let ranges = syntax_tree
            .and_then(|tree| {
                tree.enclosing_block_at_byte(byte)
                    .map(|node| (node.byte_range.clone(), node.byte_range))
            })
            .or_else(|| {
                vim_scanner::StructuralScanner::scan_rows_for_enclosing(
                    buffer,
                    0,
                    buffer.row_count(),
                    byte,
                    true,
                )
                .map(|matched| (matched.outer_range(), matched.inner_range()))
            });
        let Some((outer, inner)) = ranges else {
            continue;
        };
        if seen.insert(outer.clone()) {
            let fold = display_map::Fold {
                start: inner.start.to_point(buffer),
                end: inner.end.to_point(buffer),
            };
            if !window.folds.contains(&fold) {
                window.folds.push(fold);
                changed = true;
            }
            let anchor = buffer.anchor_at(outer.start, Bias::Left);
            window.selections.update(
                buffer,
                &Selection {
                    id: selection.id,
                    start: anchor.clone(),
                    end: anchor,
                    reversed: false,
                    goal: SelectionGoal::None,
                },
            );
        }
    }
    changed
}

pub(crate) fn execute_unfold(buffer: &text::Buffer, window: &mut WindowState) -> bool {
    let mut remove = Vec::new();
    for selection in &window.selections.selections {
        let point = selection.head().to_point(buffer);
        for (index, fold) in window.folds.iter().enumerate() {
            if (point >= fold.start && point <= fold.end) || point.row == fold.start.row {
                remove.push(index);
            }
        }
    }
    remove.sort_unstable();
    remove.dedup();
    let changed = !remove.is_empty();
    for index in remove.into_iter().rev() {
        window.folds.remove(index);
    }
    changed
}

pub(crate) fn move_syntax_target(
    action: &vim_input::Action,
    buffer: &text::Buffer,
    window: &mut WindowState,
    tree: &vim_treesitter::SyntaxTree,
) -> bool {
    let (count, select, end_target) = match action {
        vim_input::Action::MoveToNextFunction { count, select }
        | vim_input::Action::MoveToPreviousFunction { count, select }
        | vim_input::Action::MoveToNextBlock { count, select }
        | vim_input::Action::MoveToPreviousBlock { count, select }
        | vim_input::Action::MoveToBlockStart { count, select }
        | vim_input::Action::MoveToBlockEnd { count, select }
        | vim_input::Action::MoveToNextClass { count, select }
        | vim_input::Action::MoveToPreviousClass { count, select }
        | vim_input::Action::MoveToNextArgument { count, select }
        | vim_input::Action::MoveToPreviousArgument { count, select } => (
            *count,
            *select,
            matches!(action, vim_input::Action::MoveToBlockEnd { .. }),
        ),
        _ => return false,
    };
    if count == 0 {
        return false;
    }
    let cursors = window.selections.selections.clone();
    for cursor in cursors {
        let mut current = cursor.clone();
        for _ in 0..count {
            let byte = buffer.offset_for_anchor(&current.head());
            let target = match action {
                vim_input::Action::MoveToNextFunction { .. } => tree.next_function_after_byte(byte),
                vim_input::Action::MoveToPreviousFunction { .. } => {
                    tree.previous_function_before_byte(byte)
                }
                vim_input::Action::MoveToNextBlock { .. } => tree.next_block_after_byte(byte),
                vim_input::Action::MoveToPreviousBlock { .. } => {
                    tree.previous_block_before_byte(byte)
                }
                vim_input::Action::MoveToBlockStart { .. } => tree.block_start_at_byte(byte),
                vim_input::Action::MoveToBlockEnd { .. } => tree.block_end_at_byte(byte),
                vim_input::Action::MoveToNextClass { .. } => tree.next_class_after_byte(byte),
                vim_input::Action::MoveToPreviousClass { .. } => {
                    tree.previous_class_before_byte(byte)
                }
                vim_input::Action::MoveToNextArgument { .. } => tree.next_argument_after_byte(byte),
                vim_input::Action::MoveToPreviousArgument { .. } => {
                    tree.previous_argument_before_byte(byte)
                }
                _ => None,
            };
            let Some(node) = target else { break };
            let offset = if end_target {
                node.byte_range.end.saturating_sub(1)
            } else {
                node.byte_range.start
            };
            let anchor = buffer.anchor_at(offset, Bias::Left);
            current = Selection {
                id: current.id,
                start: if select {
                    cursor.tail()
                } else {
                    anchor.clone()
                },
                end: anchor,
                reversed: false,
                goal: SelectionGoal::None,
            };
        }
        window.selections.update(buffer, &current);
    }
    true
}

pub(crate) fn execute_motion(
    command: &NormalCommand,
    mode: Mode,
    window_id: WindowId,
    buffer: &mut Buffer,
    window: &mut WindowState,
) -> Option<CommandOutcome> {
    let select = match command {
        NormalCommand::MoveLeft { select, .. }
        | NormalCommand::MoveRight { select, .. }
        | NormalCommand::MoveUp { select, .. }
        | NormalCommand::MoveDown { select, .. } => *select || mode.is_visual(),
        NormalCommand::BufferMotion { .. } => mode.is_visual(),
        NormalCommand::SearchMotion { .. } => false,
        NormalCommand::TextObject { .. } => mode.is_visual(),
        NormalCommand::ViewportMotion { action } => match action.as_ref() {
            vim_input::Action::MovePageUp { select, .. }
            | vim_input::Action::MovePageDown { select, .. }
            | vim_input::Action::MoveToScreenTop { select, .. }
            | vim_input::Action::MoveToScreenMiddle { select, .. }
            | vim_input::Action::MoveToScreenBottom { select, .. } => *select || mode.is_visual(),
            _ => false,
        },
        _ => return None,
    };
    if window.selections.selections.is_empty() {
        window.selections.add(buffer.as_text_buffer(), 0);
    }

    if let NormalCommand::TextObject { action } = command {
        match action.as_ref() {
            vim_input::Action::MoveWithinCharacter { ch, .. }
            | vim_input::Action::MoveAroundCharacter { ch, .. }
                if delimiter_kind(*ch).is_some() =>
            {
                let around = matches!(
                    action.as_ref(),
                    vim_input::Action::MoveAroundCharacter { .. }
                );
                let cursors = window.selections.selections.clone();
                for cursor in cursors {
                    let byte = buffer.as_text_buffer().offset_for_anchor(&cursor.head());
                    let Some((opening, closing)) =
                        enclosing_delimiter(buffer.as_text_buffer(), byte, *ch)
                    else {
                        continue;
                    };
                    let (start, end) = if around {
                        (opening, closing)
                    } else {
                        (opening.saturating_add(1), closing.saturating_sub(1))
                    };
                    let start = buffer.as_text_buffer().anchor_at(start, Bias::Left);
                    let end = buffer.as_text_buffer().anchor_at(end, Bias::Right);
                    window.selections.update(
                        buffer.as_text_buffer(),
                        &Selection {
                            id: cursor.id,
                            start,
                            end,
                            reversed: false,
                            goal: SelectionGoal::None,
                        },
                    );
                }
            }
            vim_input::Action::MoveWithinCharacter { count, ch } if *ch == 'w' => {
                let cursors = window.selections.selections.clone();
                for cursor in cursors {
                    let start = cursor.move_to_word(false, buffer.as_text_buffer()).head();
                    let end = cursor
                        .move_to_word_end(false, buffer.as_text_buffer())
                        .head();
                    window.selections.update(
                        buffer.as_text_buffer(),
                        &Selection {
                            id: cursor.id,
                            start,
                            end,
                            reversed: false,
                            goal: SelectionGoal::None,
                        },
                    );
                    let _ = count;
                }
            }
            vim_input::Action::MoveAroundCharacter { count, ch } if *ch == 'w' => {
                let cursors = window.selections.selections.clone();
                for cursor in cursors {
                    let start = cursor.move_to_word(false, buffer.as_text_buffer()).head();
                    let next = cursor
                        .move_to_next_word(false, buffer.as_text_buffer())
                        .head();
                    let next_offset = buffer.as_text_buffer().offset_for_anchor(&next);
                    let end = buffer
                        .as_text_buffer()
                        .clip_offset(next_offset.saturating_sub(1), Bias::Left);
                    window.selections.update(
                        buffer.as_text_buffer(),
                        &Selection {
                            id: cursor.id,
                            start,
                            end: buffer.as_text_buffer().anchor_at(end, Bias::Right),
                            reversed: false,
                            goal: SelectionGoal::None,
                        },
                    );
                    let _ = count;
                }
            }
            vim_input::Action::MoveWithinCharacter { count, ch } => {
                window.selections.move_within_character(
                    mode.is_visual(),
                    *count,
                    *ch,
                    buffer.as_text_buffer(),
                );
            }
            vim_input::Action::MoveAroundCharacter { count, ch } => {
                window.selections.move_around_character(
                    mode.is_visual(),
                    *count,
                    *ch,
                    buffer.as_text_buffer(),
                );
            }
            _ => return None,
        }
    } else if let NormalCommand::ViewportMotion { action } = command {
        if !apply_viewport_motion(action, mode, buffer.as_text_buffer(), window) {
            return None;
        }
    } else if let NormalCommand::SearchMotion { count, direction } = command {
        let pattern = window.selections.search.clone();
        for _ in 0..*count {
            match direction {
                super::SearchDirection::Forward => {
                    window
                        .selections
                        .move_to_next_match(&pattern, true, buffer.as_text_buffer())
                }
                super::SearchDirection::Backward => window.selections.move_to_previous_match(
                    &pattern,
                    true,
                    buffer.as_text_buffer(),
                ),
            }
        }
    } else if let NormalCommand::BufferMotion { action } = command {
        if !apply_buffer_motion(
            action,
            if mode.is_visual() { Some(true) } else { None },
            1,
            &mut window.selections,
            buffer.as_text_buffer(),
        ) {
            return None;
        }
    } else {
        match command {
            NormalCommand::MoveLeft { count, .. } => window.selections.move_left(
                select,
                (*count).min(u32::MAX as usize) as u32,
                buffer.as_text_buffer(),
            ),
            NormalCommand::MoveRight { count, .. } => window.selections.move_right(
                select,
                (*count).min(u32::MAX as usize) as u32,
                buffer.as_text_buffer(),
            ),
            NormalCommand::MoveUp { count, .. } => window.selections.move_up(
                select,
                (*count).min(u32::MAX as usize) as u32,
                buffer.as_text_buffer(),
            ),
            NormalCommand::MoveDown { count, .. } => window.selections.move_down(
                select,
                (*count).min(u32::MAX as usize) as u32,
                buffer.as_text_buffer(),
            ),
            _ => unreachable!("non-motion command passed to execute_motion"),
        }
    }

    let viewport = window.viewport;
    window.update(
        buffer.snapshot().as_inner().clone(),
        viewport.width,
        viewport.height,
        viewport.has_border,
    );

    Some(CommandOutcome::with_effect(
        CommandEffect::CursorMoved { window: window_id },
        super::RedrawRequest::View,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yank_motion_resolves_a_pure_buffer_range_without_mutating() {
        let buffer = Buffer::new(
            vim_buffer::BufferId::new(1).unwrap(),
            clock::ReplicaId::LOCAL,
            "one two",
        );
        let mut selections = SelectionSet::new();
        selections.add(buffer.as_text_buffer(), 0);
        let text = execute_yank_motion(
            1,
            &vim_input::Action::MoveRight {
                count: 1,
                select: false,
            },
            buffer.as_text_buffer(),
            &selections,
        );
        assert_eq!(
            text.as_ref().map(|(text, kind)| (text.as_str(), *kind)),
            Some(("on", MotionKind::Characterwise { inclusive: false }))
        );
        assert_eq!(buffer.as_text_buffer().as_rope().to_string(), "one two");
        assert!(!selections.has_selection(buffer.as_text_buffer()));
    }

    #[test]
    fn search_motion_uses_explicit_window_search_state_and_count() {
        let mut buffer = Buffer::new(
            vim_buffer::BufferId::new(1).unwrap(),
            clock::ReplicaId::LOCAL,
            "zero next one next two",
        );
        let mut window = WindowState::new(&buffer, vim_ui::Viewport::default());
        window.selections.search = "next".to_string();
        window.selections.regex = vim_buffer::compile("next").map(std::sync::Arc::new);

        let outcome = execute_motion(
            &NormalCommand::SearchMotion {
                count: 2,
                direction: super::super::SearchDirection::Forward,
            },
            Mode::Normal,
            WindowId::new(1),
            &mut buffer,
            &mut window,
        );
        assert!(outcome.is_some());
        let offset = buffer
            .as_text_buffer()
            .offset_for_anchor(&window.selections.first().unwrap().head());
        assert_eq!(offset, 14);

        execute_motion(
            &NormalCommand::SearchMotion {
                count: 1,
                direction: super::super::SearchDirection::Backward,
            },
            Mode::Normal,
            WindowId::new(1),
            &mut buffer,
            &mut window,
        );
        let offset = buffer
            .as_text_buffer()
            .offset_for_anchor(&window.selections.first().unwrap().head());
        assert_eq!(offset, 5);
    }

    #[test]
    fn text_object_motion_selects_inner_word_without_legacy_dispatch() {
        let mut buffer = Buffer::new(
            vim_buffer::BufferId::new(1).unwrap(),
            clock::ReplicaId::LOCAL,
            "alpha beta",
        );
        let mut window = WindowState::new(&buffer, vim_ui::Viewport::default());
        window.selections.selections.clear();
        window.selections.add(buffer.as_text_buffer(), 2);
        let outcome = execute_motion(
            &NormalCommand::TextObject {
                action: Box::new(vim_input::Action::MoveWithinCharacter { count: 1, ch: 'w' }),
            },
            Mode::Normal,
            WindowId::new(1),
            &mut buffer,
            &mut window,
        );
        assert!(outcome.is_some());
        assert_eq!(window.selections.text(buffer.as_text_buffer()), "alpha");
    }

    #[test]
    fn fold_and_unfold_use_kernel_window_state() {
        let buffer = Buffer::new(
            vim_buffer::BufferId::new(1).unwrap(),
            clock::ReplicaId::LOCAL,
            "fn main() {\n  value\n}",
        );
        let mut window = WindowState::new(&buffer, vim_ui::Viewport::default());
        window.selections.selections.clear();
        window.selections.add(buffer.as_text_buffer(), 14);
        assert!(execute_fold(1, buffer.as_text_buffer(), &mut window, None));
        assert_eq!(window.folds.len(), 1);
        assert!(execute_unfold(buffer.as_text_buffer(), &mut window));
        assert!(window.folds.is_empty());
    }

    #[test]
    fn delimiter_text_objects_resolve_inner_and_around_ranges() {
        let mut buffer = Buffer::new(
            vim_buffer::BufferId::new(1).unwrap(),
            clock::ReplicaId::LOCAL,
            "call(hello)",
        );
        let mut window = WindowState::new(&buffer, vim_ui::Viewport::default());
        window.selections.selections.clear();
        window.selections.add(buffer.as_text_buffer(), 7);

        execute_motion(
            &NormalCommand::TextObject {
                action: Box::new(vim_input::Action::MoveWithinCharacter { count: 1, ch: '(' }),
            },
            Mode::Normal,
            WindowId::new(1),
            &mut buffer,
            &mut window,
        );
        assert_eq!(window.selections.text(buffer.as_text_buffer()), "hello");

        execute_motion(
            &NormalCommand::TextObject {
                action: Box::new(vim_input::Action::MoveAroundCharacter { count: 1, ch: '(' }),
            },
            Mode::Normal,
            WindowId::new(1),
            &mut buffer,
            &mut window,
        );
        assert_eq!(window.selections.text(buffer.as_text_buffer()), "(hello)");
    }

    #[test]
    fn delete_delimiter_text_object_uses_exact_object_range() {
        let mut buffer = Buffer::new(
            vim_buffer::BufferId::new(1).unwrap(),
            clock::ReplicaId::LOCAL,
            "call(hello)tail",
        );
        let mut selections = SelectionSet::new();
        selections.add(buffer.as_text_buffer(), 7);
        let deleted = execute_delete_motion(
            1,
            &vim_input::Action::MoveWithinCharacter { count: 1, ch: '(' },
            &mut buffer,
            &mut selections,
            &mut Vec::new(),
        );
        assert_eq!(
            deleted.as_ref().map(|(text, _)| text.as_str()),
            Some("hello")
        );
        assert!(deleted.and_then(|(_, mutation)| mutation).is_some());
        assert_eq!(buffer.as_text_buffer().as_rope().to_string(), "call()tail");
    }

    #[test]
    fn search_motion_resolves_an_operator_range_from_explicit_search_state() {
        let buffer = Buffer::new(
            vim_buffer::BufferId::new(1).unwrap(),
            clock::ReplicaId::LOCAL,
            "zero next one next",
        );
        let mut selections = SelectionSet::new();
        selections.add(buffer.as_text_buffer(), 0);
        selections.search = "next".to_string();
        selections.regex = vim_buffer::compile("next").map(std::sync::Arc::new);
        let resolved = execute_yank_motion(
            1,
            &vim_input::Action::SearchForward { count: 1 },
            buffer.as_text_buffer(),
            &selections,
        );
        assert_eq!(
            resolved.map(|(text, kind)| (text, kind)),
            Some((
                "zero n".to_string(),
                MotionKind::Characterwise { inclusive: false }
            ))
        );
    }

    #[test]
    fn case_motion_mutates_only_the_resolved_pure_buffer_range() {
        let mut buffer = Buffer::new(
            vim_buffer::BufferId::new(1).unwrap(),
            clock::ReplicaId::LOCAL,
            "ONE TWO",
        );
        let mut selections = SelectionSet::new();
        selections.add(buffer.as_text_buffer(), 0);
        let changed = execute_case_motion(
            1,
            &vim_input::Action::MoveToWord {
                count: 1,
                select: false,
            },
            super::super::CaseChange::Lower,
            &mut buffer,
            &mut selections,
            &mut Vec::new(),
        );
        assert!(matches!(changed, Some(Some(_))));
        assert_eq!(buffer.as_text_buffer().as_rope().to_string(), "one TWO");
        assert!(!selections.has_selection(buffer.as_text_buffer()));
    }

    #[test]
    fn motion_kind_distinguishes_end_motions_and_vertical_ranges() {
        assert_eq!(
            motion_kind(&vim_input::Action::MoveToWord {
                count: 1,
                select: false,
            }),
            MotionKind::Characterwise { inclusive: false }
        );
        assert_eq!(
            motion_kind(&vim_input::Action::MoveToWordEnd {
                count: 1,
                select: false,
            }),
            MotionKind::Characterwise { inclusive: true }
        );
        assert_eq!(
            motion_kind(&vim_input::Action::MoveDown {
                count: 1,
                select: false,
            }),
            MotionKind::Linewise
        );
    }

    #[test]
    fn vertical_delete_motion_is_linewise() {
        let mut buffer = Buffer::new(
            vim_buffer::BufferId::new(1).unwrap(),
            clock::ReplicaId::LOCAL,
            "one\ntwo\nthree",
        );
        let mut selections = SelectionSet::new();
        selections.add(buffer.as_text_buffer(), 0);
        let deleted = execute_delete_motion(
            1,
            &vim_input::Action::MoveDown {
                count: 1,
                select: false,
            },
            &mut buffer,
            &mut selections,
            &mut Vec::new(),
        );
        assert_eq!(
            deleted.as_ref().map(|(text, _)| text.as_str()),
            Some("one\ntwo\n")
        );
        assert!(deleted.and_then(|(_, mutation)| mutation).is_some());
        assert_eq!(buffer.as_text_buffer().as_rope().to_string(), "three");
    }
}
