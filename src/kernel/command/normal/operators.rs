//! Operator + motion composition (`RESCUE.md` Rule 3: one file per command
//! family -- `d`/`c`/`y`/`g~`/`gu`/`gU`/`>`/`<` and dot-repeat all live here
//! together, including the case-transform and indent/outdent variants,
//! rather than splintered across sibling files, since they all share the
//! same motion-range resolution machinery below). Every mutating function
//! here goes through `kernel::transaction`, never a family-specific edit
//! path. `=`/`!` are deliberately unimplemented `NoOp`s -- see this
//! milestone's scope note in `IMPLEMENT.md`'s "Operators (Build Order 7.4)"
//! section: `=` needs a reindent engine and `!` needs to shell out, neither
//! of which exists yet.

use text::{Anchor, Buffer as TextBuffer, Point, Selection, SelectionGoal, ToOffset, ToPoint};
use vim_buffer::{
    Buffer, BufferId, BufferOptions, ByteOffset, Edit, EditOrigin, Motions, PlannedEdit, TextRange,
};
use vim_input::Action;

use crate::kernel::{
    Editor,
    buffer::registers::RegisterKind,
    ids::WindowId,
    mode::{Mode, VisualKind},
    outcome::{Outcome, RedrawInvalidation},
    transaction::{self, EditDescription},
};

fn is_jump_motion(motion: &Action) -> bool {
    matches!(
        motion,
        Action::MoveToStartOfDocument { .. }
            | Action::MoveToEndOfDocument { .. }
            | Action::MoveToLine { .. }
            | Action::MoveToMatchingDelimiter { .. }
            | Action::MoveToScreenTop { .. }
            | Action::MoveToScreenMiddle { .. }
            | Action::MoveToScreenBottom { .. }
            | Action::MarkJump { .. }
    )
}

fn record_operator_jump(editor: &mut Editor, window: WindowId, motion: &Action) {
    if is_jump_motion(motion) {
        super::marks_and_jumps::record_jump(editor, window);
    }
}

use super::text_objects;

/// The sentinel motion `vim_input::Resolver::resolve_sequence` synthesizes
/// for a bare Visual-mode operator key pressed with no following motion
/// (`compose_operator(action, Action::MoveRight { count: 0, select: true })`)
/// -- means "operate on the current selection", not "move right by 0".
fn is_visual_selection_sentinel(motion: &Action) -> bool {
    matches!(
        motion,
        Action::MoveRight {
            count: 0,
            select: true
        }
    )
}

/// Whether `action` is one `.` should be able to replay. Excludes `c*`
/// (change): faithfully repeating a change also means replaying the
/// Insert-mode session typed after it, which this kernel has no mechanism
/// to capture yet (see this milestone's scope note). Excludes `y*` (yank):
/// it never mutates, so it is not a "change" to repeat.
pub(super) fn is_repeatable_change(action: &Action) -> bool {
    matches!(
        action,
        Action::DeleteMotion { .. }
            | Action::DeleteLine { .. }
            | Action::IndentMotion { .. }
            | Action::OutdentMotion { .. }
            | Action::Indent { .. }
            | Action::Outdent { .. }
            | Action::UpperCaseMotion { .. }
            | Action::UpperCaseLine { .. }
            | Action::LowerCaseMotion { .. }
            | Action::LowerCaseLine { .. }
            | Action::ToggleCaseMotion { .. }
            | Action::ToggleCaseLine { .. }
    )
}

/// Handles `Action::Repeat` (`.`): re-runs `Editor::last_change` through
/// `dispatch` again. A no-op when nothing has been recorded yet.
pub fn repeat_last_change(editor: &mut Editor, window: WindowId, count: u32) -> Outcome {
    let Some(mut action) = editor.last_change() else {
        return Outcome::default();
    };
    // `.`'s own count overrides the recorded change's count -- but only
    // when it's greater than 1. `vim_input::Resolver::take_count`'s
    // `unwrap_or(1)` default means a bare `.` and an explicit `1.` are
    // indistinguishable by the time `Action::Repeat` reaches here, so
    // treating every count as an override would wrongly clobber e.g. a
    // recorded `3dw`'s count with a bare `.`'s implicit `1`.
    if count > 1 {
        action = action.with_count(count);
    }
    let ctx = editor.current_context();
    let _ = window;
    super::dispatch(editor, ctx, action)
}

/// How a motion's landing selection turns into a byte range for an
/// operator, per `:help exclusive`/`:help linewise`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MotionKind {
    Linewise,
    InclusiveCharwise,
    ExclusiveCharwise,
}

fn classify_motion(action: &Action) -> MotionKind {
    match action {
        Action::MoveUp { .. }
        | Action::MoveDown { .. }
        | Action::MoveToStartOfDocument { .. }
        | Action::MoveToEndOfDocument { .. }
        | Action::MoveToLine { .. }
        | Action::MoveToScreenTop { .. }
        | Action::MoveToScreenMiddle { .. }
        | Action::MoveToScreenBottom { .. } => MotionKind::Linewise,

        Action::MoveToNextCharacter { .. }
        | Action::MoveToPreviousCharacter { .. }
        | Action::RepeatCharacterSearchForward { .. }
        | Action::RepeatCharacterSearchBackward { .. }
        | Action::MoveToWordEnd { .. }
        | Action::MoveToPreviousWordEnd { .. }
        | Action::MoveToBigWordEnd { .. }
        | Action::MoveToPreviousBigWordEnd { .. }
        | Action::MoveToMatchingDelimiter { .. }
        | Action::MoveToEndOfLine { .. }
        | Action::MoveToLastNonWhitespace { .. } => MotionKind::InclusiveCharwise,

        _ => MotionKind::ExclusiveCharwise,
    }
}

/// Computes where `motion` would land, applied `repeats` times to a clone
/// of `from` -- never the window's real `SelectionSet`, so previewing a
/// motion for an operator never mutates cursor state before the edit is
/// known to happen. Returns `None` for motions no operator supports yet
/// (scroll/viewport-only actions, which never move the cursor in real Vim
/// either, so no operator can compose with them).
fn motion_target(
    editor: &Editor,
    window: WindowId,
    buffer_id: BufferId,
    from: &Selection<Anchor>,
    motion: &Action,
    repeats: u32,
) -> Option<Selection<Anchor>> {
    let buffer = editor.buffer(buffer_id)?;
    let text_buffer = buffer.as_text_buffer();
    let repeats = repeats.max(1);

    match motion {
        Action::MoveLeft { .. } => Some(repeat_motion(from, repeats, |s| {
            s.move_left_once(false, text_buffer)
        })),
        Action::MoveRight { .. } => Some(repeat_motion(from, repeats, |s| {
            s.move_right_once(false, text_buffer)
        })),
        // See `motions.rs`'s `move_to_word`/kernel-wide note: `Action::
        // MoveToWord` is Vim's forward `w`, but the matching-sounding
        // `Motions::move_to_word` doesn't advance from a word start --
        // `move_to_next_word` is the real forward-progressing motion.
        Action::MoveToWord { .. } => Some(repeat_motion(from, repeats, |s| {
            s.move_to_next_word(false, text_buffer)
        })),
        Action::MoveToPreviousWord { .. } => Some(repeat_motion(from, repeats, |s| {
            s.move_to_previous_word(false, text_buffer)
        })),
        Action::MoveToWordEnd { .. } => Some(repeat_motion(from, repeats, |s| {
            s.move_to_word_end(false, text_buffer)
        })),
        Action::MoveToPreviousWordEnd { .. } => Some(repeat_motion(from, repeats, |s| {
            s.move_to_previous_word_end(false, text_buffer)
        })),
        Action::MoveToBigWord { .. } => Some(repeat_motion(from, repeats, |s| {
            s.move_to_big_word(false, text_buffer)
        })),
        Action::MoveToPreviousBigWord { .. } => Some(repeat_motion(from, repeats, |s| {
            s.move_to_previous_big_word(false, text_buffer)
        })),
        Action::MoveToBigWordEnd { .. } => Some(repeat_motion(from, repeats, |s| {
            s.move_to_big_word_end(false, text_buffer)
        })),
        Action::MoveToPreviousBigWordEnd { .. } => Some(repeat_motion(from, repeats, |s| {
            s.move_to_previous_big_word_end(false, text_buffer)
        })),
        Action::MoveToStartOfDocument { .. } => {
            Some(from.move_to_start_of_document(false, text_buffer))
        }
        Action::MoveToEndOfDocument { .. } => {
            Some(from.move_to_end_of_document(false, text_buffer))
        }
        Action::MoveToLine { line, .. } => Some(from.move_to_line(false, *line, text_buffer)),
        Action::MoveToStartOfLine { .. } => Some(from.move_to_start_of_line(false, text_buffer)),
        Action::MoveToStartOfLineNonSpace { .. } => {
            Some(from.move_to_start_of_line_non_space(false, text_buffer))
        }
        Action::MoveToEndOfLine { .. } => Some(from.move_to_end_of_line(false, text_buffer)),
        Action::MoveToLastNonWhitespace { .. } => {
            Some(from.move_to_last_non_whitespace(false, repeats, text_buffer))
        }
        Action::MoveToStartOfPreviousLine { .. } => {
            Some(from.move_to_start_of_previous_line(false, text_buffer))
        }
        Action::MoveToEndOfPreviousLine { .. } => {
            Some(from.move_to_end_of_previous_line(false, text_buffer))
        }
        Action::MoveToStartOfNextLine { .. } => {
            Some(from.move_to_start_of_next_line(false, text_buffer))
        }
        Action::MoveToEndOfNextLine { .. } => {
            Some(from.move_to_end_of_next_line(false, text_buffer))
        }
        Action::MoveToPreviousParagraph { .. } => Some(repeat_motion(from, repeats, |s| {
            s.move_to_previous_paragraph(false, text_buffer)
        })),
        Action::MoveToNextParagraph { .. } => Some(repeat_motion(from, repeats, |s| {
            s.move_to_next_paragraph(false, text_buffer)
        })),
        Action::MoveToPreviousSentence { .. } => Some(repeat_motion(from, repeats, |s| {
            s.move_to_previous_sentence(false, text_buffer)
        })),
        Action::MoveToNextSentence { .. } => Some(repeat_motion(from, repeats, |s| {
            s.move_to_next_sentence(false, text_buffer)
        })),
        Action::MoveToMatchingDelimiter { .. } => {
            Some(from.move_to_matching_delimiter(false, text_buffer))
        }
        Action::MoveToColumn { count } => Some(from.move_to_column(false, *count, text_buffer)),
        Action::MoveToNextCharacter {
            count, ch, till, ..
        } => Some(from.find_character(false, *count, *ch, true, *till, text_buffer)),
        Action::MoveToPreviousCharacter {
            count, ch, till, ..
        } => Some(from.find_character(false, *count, *ch, false, *till, text_buffer)),
        Action::RepeatCharacterSearchForward { count, .. } => {
            let search = editor.last_char_search()?;
            Some(from.find_character(
                false,
                *count,
                search.ch,
                search.forward,
                search.till,
                text_buffer,
            ))
        }
        Action::RepeatCharacterSearchBackward { count, .. } => {
            let search = editor.last_char_search()?;
            Some(from.find_character(
                false,
                *count,
                search.ch,
                !search.forward,
                search.till,
                text_buffer,
            ))
        }
        Action::MoveWithinCharacter { ch, .. } => Some(text_objects::object_range(
            editor, buffer_id, from, *ch, false,
        )),
        Action::MoveAroundCharacter { ch, .. } => Some(text_objects::object_range(
            editor, buffer_id, from, *ch, true,
        )),
        Action::MoveUp { .. } => Some(vertical_target(from, text_buffer, repeats, true)),
        Action::MoveDown { .. } => Some(vertical_target(from, text_buffer, repeats, false)),
        Action::MoveToScreenTop { .. } => {
            let win = editor.window(window)?;
            Some(from.move_to_line(false, win.scroll_top() + 1, text_buffer))
        }
        Action::MoveToScreenMiddle { .. } => {
            let win = editor.window(window)?;
            let height = win.viewport_height().max(1);
            Some(from.move_to_line(false, win.scroll_top() + height / 2 + 1, text_buffer))
        }
        Action::MoveToScreenBottom { .. } => {
            let win = editor.window(window)?;
            let height = win.viewport_height().max(1);
            Some(from.move_to_line(
                false,
                win.scroll_top() + height.saturating_sub(1) + 1,
                text_buffer,
            ))
        }
        _ => None,
    }
}

fn repeat_motion(
    from: &Selection<Anchor>,
    repeats: u32,
    step: impl Fn(&Selection<Anchor>) -> Selection<Anchor>,
) -> Selection<Anchor> {
    let mut current = from.clone();
    for _ in 0..repeats {
        current = step(&current);
    }
    current
}

fn vertical_target(
    from: &Selection<Anchor>,
    buffer: &TextBuffer,
    repeats: u32,
    up: bool,
) -> Selection<Anchor> {
    let row = from.head().to_point(buffer).row;
    let target_row = if up {
        row.saturating_sub(repeats)
    } else {
        (row + repeats).min(buffer.row_count().saturating_sub(1))
    };
    from.move_to_line(false, target_row + 1, buffer)
}

/// A motion resolved into the byte range an operator should act on.
struct ResolvedRange {
    start: usize,
    end: usize,
    linewise: bool,
}

/// Factors the byte-range math every operator+motion pair needs (resolve
/// motion -> offsets -> classify -> snap) into one place, so `delete_motion`/
/// `change_motion`/`yank_motion`/case operators all share one interpretation
/// of Vim's exclusive/inclusive/linewise rules instead of re-deriving it.
fn resolve_motion_range(
    editor: &Editor,
    window: WindowId,
    buffer_id: BufferId,
    from: &Selection<Anchor>,
    motion: &Action,
    count: u32,
) -> Option<ResolvedRange> {
    if is_visual_selection_sentinel(motion) {
        return resolve_visual_span(editor, window, buffer_id, from);
    }

    // Vim multiplies an operator count by the motion's own count
    // ("2d3w" deletes 6 words); both are already resolved into `count` and
    // `motion.count()` by `vim_input::Resolver` by the time this runs.
    let repeats = count.max(1).saturating_mul(motion.count().max(1));
    let target = motion_target(editor, window, buffer_id, from, motion, repeats)?;

    let buffer = editor.buffer(buffer_id)?;
    let text_buffer = buffer.as_text_buffer();
    let from_offset = text_buffer.offset_for_anchor(&from.head());
    let target_offset = text_buffer.offset_for_anchor(&target.head());
    let (low, high) = (
        from_offset.min(target_offset),
        from_offset.max(target_offset),
    );

    let kind = classify_motion(motion);
    let (start, end) = match kind {
        MotionKind::Linewise => {
            let start_row = low.to_point(text_buffer).row;
            let end_row = high.to_point(text_buffer).row;
            whole_line_range(text_buffer, start_row, end_row)
        }
        MotionKind::InclusiveCharwise => {
            let higher = if target_offset >= from_offset {
                target.clone()
            } else {
                from.clone()
            };
            let extended = higher.move_right_once(false, text_buffer);
            let extended_offset = text_buffer.offset_for_anchor(&extended.head());
            (low, extended_offset.max(high))
        }
        MotionKind::ExclusiveCharwise => (low, high),
    };

    if start >= end {
        return None;
    }
    Some(ResolvedRange {
        start,
        end,
        linewise: matches!(kind, MotionKind::Linewise),
    })
}

/// Resolves the current Visual (char-wise or line-wise) selection into a
/// byte range for an operator, per `:help visual-operators` -- a Visual
/// char-wise selection is always inclusive of the character under the
/// cursor, unlike a plain exclusive/inclusive motion. Returns `None` for a
/// block-wise selection; callers resolve that case separately via
/// `resolve_visual_block_rows`.
fn resolve_visual_span(
    editor: &Editor,
    window: WindowId,
    buffer_id: BufferId,
    from: &Selection<Anchor>,
) -> Option<ResolvedRange> {
    let win = editor.window(window)?;
    if win.visual_kind() == Some(VisualKind::Block) {
        return None;
    }
    let buffer = editor.buffer(buffer_id)?;
    let text_buffer = buffer.as_text_buffer();
    let start_offset = text_buffer.offset_for_anchor(&from.start);
    let end_offset = text_buffer.offset_for_anchor(&from.end);
    let (low, high) = (start_offset.min(end_offset), start_offset.max(end_offset));

    if win.visual_kind() == Some(VisualKind::Line) {
        let start_row = low.to_point(text_buffer).row;
        let end_row = high.to_point(text_buffer).row;
        let (start, end) = whole_line_range(text_buffer, start_row, end_row);
        return Some(ResolvedRange {
            start,
            end,
            linewise: true,
        });
    }

    // Char-wise: extend the higher end by one character boundary so the
    // character under the cursor is included, matching real Vim's Visual
    // selection semantics (a zero-width selection still covers one char).
    let extended_high = if high < text_buffer.len() {
        let anchor = text_buffer.anchor_before(high);
        let point_selection = Selection {
            id: from.id,
            start: anchor,
            end: anchor,
            reversed: false,
            goal: SelectionGoal::None,
        };
        let moved = point_selection.move_right_once(false, text_buffer);
        text_buffer.offset_for_anchor(&moved.head())
    } else {
        high
    };

    if low >= extended_high {
        return None;
    }
    Some(ResolvedRange {
        start: low,
        end: extended_high,
        linewise: false,
    })
}

/// A per-row column sub-range for a block-wise (`Ctrl-v`) Visual selection --
/// one entry per row the block spans, each already clipped to that row's
/// own length (so lines of unequal length inside the block are handled
/// without a panic or an out-of-bounds edit).
struct BlockRow {
    row: u32,
    start: usize,
    end: usize,
}

/// Resolves a block-wise Visual selection's row/column rectangle into one
/// `BlockRow` per spanned row. Returns `None` when the window's current
/// selection is not block-wise.
fn resolve_visual_block_rows(
    editor: &Editor,
    window: WindowId,
    buffer_id: BufferId,
    from: &Selection<Anchor>,
) -> Option<Vec<BlockRow>> {
    let win = editor.window(window)?;
    if win.visual_kind() != Some(VisualKind::Block) {
        return None;
    }
    let buffer = editor.buffer(buffer_id)?;
    let text_buffer = buffer.as_text_buffer();
    let start_point = from.start.to_point(text_buffer);
    let end_point = from.end.to_point(text_buffer);
    let row_start = start_point.row.min(end_point.row);
    let row_end = start_point.row.max(end_point.row);
    let col_start = start_point.column.min(end_point.column);
    // Inclusive of the column under the cursor, matching Vim's block Visual.
    let col_end = start_point.column.max(end_point.column) + 1;

    let mut rows = Vec::new();
    for row in row_start..=row_end {
        let line_len = text_buffer.line_len(row);
        let s_col = col_start.min(line_len);
        let e_col = col_end.min(line_len);
        let start = Point::new(row, s_col).to_offset(text_buffer);
        let end = Point::new(row, e_col).to_offset(text_buffer);
        rows.push(BlockRow { row, start, end });
    }
    Some(rows)
}

/// Like `resolve_motion_range`, but for operators that are always linewise
/// regardless of the motion given (`>`/`<`'s indent/outdent) -- resolves
/// `motion`'s landing point and returns the `[start_row, end_row]` span it
/// covers, ignoring `classify_motion`'s exclusive/inclusive distinction
/// entirely.
fn resolve_linewise_rows(
    editor: &Editor,
    window: WindowId,
    buffer_id: BufferId,
    from: &Selection<Anchor>,
    motion: &Action,
    count: u32,
) -> Option<(u32, u32)> {
    if is_visual_selection_sentinel(motion) {
        let buffer = editor.buffer(buffer_id)?;
        let text_buffer = buffer.as_text_buffer();
        let r1 = from.start.to_point(text_buffer).row;
        let r2 = from.end.to_point(text_buffer).row;
        return Some((r1.min(r2), r1.max(r2)));
    }

    let repeats = count.max(1).saturating_mul(motion.count().max(1));
    let target = motion_target(editor, window, buffer_id, from, motion, repeats)?;

    let buffer = editor.buffer(buffer_id)?;
    let text_buffer = buffer.as_text_buffer();
    let from_offset = text_buffer.offset_for_anchor(&from.head());
    let target_offset = text_buffer.offset_for_anchor(&target.head());
    let (low, high) = (
        from_offset.min(target_offset),
        from_offset.max(target_offset),
    );
    Some((
        low.to_point(text_buffer).row,
        high.to_point(text_buffer).row,
    ))
}

/// Snaps `[start_row, end_row]` to whole-line byte boundaries, including the
/// trailing newline of `end_row` (or the buffer's end, on the last line).
fn whole_line_range(buffer: &TextBuffer, start_row: u32, end_row: u32) -> (usize, usize) {
    let start = Point::new(start_row, 0).to_offset(buffer);
    let end = if end_row + 1 < buffer.row_count() {
        Point::new(end_row + 1, 0).to_offset(buffer)
    } else {
        buffer.len()
    };
    (start, end)
}

/// The row span `count` whole lines starting at `from`'s cursor line covers
/// -- the doubled linewise forms' (`dd`/`cc`/`yy`/`>>`/`<<`/`g~~`) range.
fn line_span_from_cursor(buffer: &TextBuffer, from: &Selection<Anchor>, count: u32) -> (u32, u32) {
    let start_row = from.head().to_point(buffer).row;
    let end_row = (start_row + count.max(1) - 1).min(buffer.row_count().saturating_sub(1));
    (start_row, end_row)
}

/// The selection an operator leaves the cursor at after mutating `[0,
/// offset)`'s worth of the buffer no longer applies: linewise operators land
/// on the first non-blank of the affected row (clamped if the buffer
/// shrank past it); charwise operators land exactly at `offset`.
fn landing_selection(
    buffer: &Buffer,
    id: usize,
    offset: usize,
    linewise: bool,
) -> Selection<Anchor> {
    let text_buffer = buffer.as_text_buffer();
    if linewise {
        let row = offset
            .to_point(text_buffer)
            .row
            .min(text_buffer.row_count().saturating_sub(1));
        let row_start = Point::new(row, 0).to_offset(text_buffer);
        let anchor = text_buffer.anchor_before(row_start);
        let seed = Selection {
            id,
            start: anchor,
            end: anchor,
            reversed: false,
            goal: SelectionGoal::None,
        };
        seed.move_to_start_of_line_non_space(false, text_buffer)
    } else {
        let anchor = text_buffer.anchor_before(offset);
        Selection {
            id,
            start: anchor,
            end: anchor,
            reversed: false,
            goal: SelectionGoal::None,
        }
    }
}

/// Handles `Action::DeleteMotion { count, motion }`.
pub fn delete_motion(
    editor: &mut Editor,
    window: WindowId,
    count: u32,
    motion: &Action,
) -> Outcome {
    record_operator_jump(editor, window, motion);
    let buffer_id = editor
        .window(window)
        .expect("dispatch only runs against a live window")
        .buffer_id();
    let primary = editor
        .window(window)
        .unwrap()
        .selections()
        .primary()
        .clone();

    if is_visual_selection_sentinel(motion)
        && let Some(rows) = resolve_visual_block_rows(editor, window, buffer_id, &primary)
    {
        return apply_delete_block(editor, window, buffer_id, primary.id, rows);
    }

    let Some(range) = resolve_motion_range(editor, window, buffer_id, &primary, motion, count)
    else {
        return Outcome::default();
    };
    apply_delete(editor, window, buffer_id, primary.id, range)
}

/// Handles `Action::DeleteLine { count }` (`dd`).
pub fn delete_line(editor: &mut Editor, window: WindowId, count: u32) -> Outcome {
    let buffer_id = editor
        .window(window)
        .expect("dispatch only runs against a live window")
        .buffer_id();
    let primary = editor
        .window(window)
        .unwrap()
        .selections()
        .primary()
        .clone();
    let buffer = editor.buffer(buffer_id).expect("live buffer");
    let (start_row, end_row) = line_span_from_cursor(buffer.as_text_buffer(), &primary, count);
    let (start, end) = whole_line_range(buffer.as_text_buffer(), start_row, end_row);
    if start >= end {
        return Outcome::default();
    }
    apply_delete(
        editor,
        window,
        buffer_id,
        primary.id,
        ResolvedRange {
            start,
            end,
            linewise: true,
        },
    )
}

fn apply_delete(
    editor: &mut Editor,
    window: WindowId,
    buffer_id: BufferId,
    primary_id: usize,
    range: ResolvedRange,
) -> Outcome {
    let deleted_text: String = {
        let buffer = editor.buffer(buffer_id).expect("live buffer");
        buffer
            .snapshot()
            .chunks_for_range(TextRange {
                start: ByteOffset(range.start),
                end: ByteOffset(range.end),
            })
            .expect("range is valid")
            .collect()
    };

    let kind = if range.linewise {
        RegisterKind::Line
    } else {
        RegisterKind::Character
    };
    let effect = super::registers_ops::write_register(editor, true, deleted_text, kind);

    let buffer = editor
        .buffers_mut()
        .get_mut(buffer_id)
        .expect("live buffer");
    let mutation = transaction::apply(
        buffer,
        EditDescription {
            origin: EditOrigin::User,
            edits: vec![PlannedEdit {
                selection: None,
                edit: Edit::delete(TextRange {
                    start: ByteOffset(range.start),
                    end: ByteOffset(range.end),
                }),
            }],
            selections: None,
        },
    )
    .expect("deleting a motion-derived range is always well-formed");

    let landing = landing_selection(buffer, primary_id, range.start, range.linewise);
    let win = editor.windows_mut().get_mut(window).expect("live window");
    win.selections_mut()
        .replace_primary(landing)
        .expect("primary id is unchanged by a delete");

    let mut outcome = Outcome::from_mutation(&mutation);
    if let Some(eff) = effect {
        outcome.effects.push(eff);
    }
    outcome
}

/// Deletes every `BlockRow`'s column sub-range as one `transaction::apply`
/// call (one `EditDescription`, multiple `PlannedEdit`s), so a block-wise
/// delete spanning several lines undoes as a single step. The cursor lands
/// at the block's top-left corner, matching real Vim's blockwise `d`.
fn apply_delete_block(
    editor: &mut Editor,
    window: WindowId,
    buffer_id: BufferId,
    primary_id: usize,
    rows: Vec<BlockRow>,
) -> Outcome {
    let Some(top_row) = rows.first().map(|r| r.row) else {
        return Outcome::default();
    };
    let top_col = rows
        .iter()
        .map(|r| r.start)
        .min()
        .map(|start| {
            let buffer = editor.buffer(buffer_id).expect("live buffer");
            start.to_point(buffer.as_text_buffer()).column
        })
        .unwrap_or(0);

    let edits: Vec<PlannedEdit> = rows
        .iter()
        .filter(|r| r.start < r.end)
        .map(|r| PlannedEdit {
            selection: None,
            edit: Edit::delete(TextRange {
                start: ByteOffset(r.start),
                end: ByteOffset(r.end),
            }),
        })
        .collect();
    if edits.is_empty() {
        return Outcome::default();
    }

    let joined = {
        let buffer = editor.buffer(buffer_id).expect("live buffer");
        let snapshot = buffer.snapshot();
        let mut lines = Vec::new();
        for r in &rows {
            if r.start < r.end {
                let line_text: String = snapshot
                    .chunks_for_range(TextRange {
                        start: ByteOffset(r.start),
                        end: ByteOffset(r.end),
                    })
                    .expect("block row range is valid")
                    .collect();
                lines.push(line_text);
            } else {
                lines.push(String::new());
            }
        }
        lines.join("\n")
    };
    let effect = super::registers_ops::write_register(editor, true, joined, RegisterKind::Block);

    let buffer = editor
        .buffers_mut()
        .get_mut(buffer_id)
        .expect("live buffer");
    let mutation = transaction::apply(
        buffer,
        EditDescription {
            origin: EditOrigin::User,
            edits,
            selections: None,
        },
    )
    .expect("block-delete edits are always well-formed");

    let text_buffer = buffer.as_text_buffer();
    let line_len = text_buffer.line_len(top_row);
    let offset = Point::new(top_row, top_col.min(line_len)).to_offset(text_buffer);
    let anchor = text_buffer.anchor_before(offset);
    let landing = Selection {
        id: primary_id,
        start: anchor,
        end: anchor,
        reversed: false,
        goal: SelectionGoal::None,
    };
    let win = editor.windows_mut().get_mut(window).expect("live window");
    win.selections_mut()
        .replace_primary(landing)
        .expect("primary id is unchanged by a block delete");

    let mut outcome = Outcome::from_mutation(&mutation);
    if let Some(eff) = effect {
        outcome.effects.push(eff);
    }
    outcome
}

/// Handles `Action::ChangeMotion { count, motion }`.
pub fn change_motion(
    editor: &mut Editor,
    window: WindowId,
    count: u32,
    motion: &Action,
) -> Outcome {
    record_operator_jump(editor, window, motion);
    let buffer_id = editor
        .window(window)
        .expect("dispatch only runs against a live window")
        .buffer_id();
    let primary = editor
        .window(window)
        .unwrap()
        .selections()
        .primary()
        .clone();

    if is_visual_selection_sentinel(motion)
        && let Some(rows) = resolve_visual_block_rows(editor, window, buffer_id, &primary)
    {
        let mut outcome = apply_delete_block(editor, window, buffer_id, primary.id, rows);
        editor.set_mode(Mode::Insert);
        outcome.mode_changed = true;
        return outcome;
    }

    let Some(range) = resolve_motion_range(editor, window, buffer_id, &primary, motion, count)
    else {
        return Outcome::default();
    };
    apply_change(editor, window, buffer_id, primary.id, range)
}

/// Handles `Action::ChangeLine { count }` (`cc`).
pub fn change_line(editor: &mut Editor, window: WindowId, count: u32) -> Outcome {
    let buffer_id = editor
        .window(window)
        .expect("dispatch only runs against a live window")
        .buffer_id();
    let primary = editor
        .window(window)
        .unwrap()
        .selections()
        .primary()
        .clone();
    let buffer = editor.buffer(buffer_id).expect("live buffer");
    let (start_row, end_row) = line_span_from_cursor(buffer.as_text_buffer(), &primary, count);
    let (start, end) = whole_line_range(buffer.as_text_buffer(), start_row, end_row);
    if start >= end {
        return Outcome::default();
    }
    apply_change(
        editor,
        window,
        buffer_id,
        primary.id,
        ResolvedRange {
            start,
            end,
            linewise: true,
        },
    )
}

/// `change_motion`/`change_line` delete exactly like `delete_motion`/
/// `delete_line`, then flip `Mode` to `Insert` at the deletion point within
/// the same returned `Outcome`, rather than a second dispatch round-trip.
fn apply_change(
    editor: &mut Editor,
    window: WindowId,
    buffer_id: BufferId,
    primary_id: usize,
    range: ResolvedRange,
) -> Outcome {
    let mut outcome = apply_delete(editor, window, buffer_id, primary_id, range);
    editor.set_mode(Mode::Insert);
    outcome.mode_changed = true;
    outcome
}

/// Handles `Action::YankMotion { count, motion }`. Never mutates the
/// buffer; only moves the cursor to the start of the resolved range
/// (Vim's `y` cursor rule). Actual register capture is out of scope until
/// 7.6.
pub fn yank_motion(editor: &mut Editor, window: WindowId, count: u32, motion: &Action) -> Outcome {
    record_operator_jump(editor, window, motion);
    let buffer_id = editor
        .window(window)
        .expect("dispatch only runs against a live window")
        .buffer_id();
    let primary = editor
        .window(window)
        .unwrap()
        .selections()
        .primary()
        .clone();
    if is_visual_selection_sentinel(motion)
        && let Some(rows) = resolve_visual_block_rows(editor, window, buffer_id, &primary)
    {
        let Some(top) = rows.iter().map(|r| r.start).min() else {
            return Outcome::default();
        };
        let buffer = editor.buffer(buffer_id).expect("live buffer");
        let snapshot = buffer.snapshot();
        let mut lines = Vec::new();
        for r in &rows {
            if r.start < r.end {
                let line_text: String = snapshot
                    .chunks_for_range(TextRange {
                        start: ByteOffset(r.start),
                        end: ByteOffset(r.end),
                    })
                    .expect("block row range is valid")
                    .collect();
                lines.push(line_text);
            } else {
                lines.push(String::new());
            }
        }
        let joined = lines.join("\n");
        let effect =
            super::registers_ops::write_register(editor, false, joined, RegisterKind::Block);
        let mut outcome = move_cursor_to_offset(editor, window, buffer_id, primary.id, top);
        if let Some(eff) = effect {
            outcome.effects.push(eff);
        }
        return outcome;
    }

    let Some(range) = resolve_motion_range(editor, window, buffer_id, &primary, motion, count)
    else {
        return Outcome::default();
    };
    let buffer = editor.buffer(buffer_id).expect("live buffer");
    let yanked_text: String = buffer
        .snapshot()
        .chunks_for_range(TextRange {
            start: ByteOffset(range.start),
            end: ByteOffset(range.end),
        })
        .expect("range is valid")
        .collect();
    let kind = if range.linewise {
        RegisterKind::Line
    } else {
        RegisterKind::Character
    };
    let effect = super::registers_ops::write_register(editor, false, yanked_text, kind);
    let mut outcome = move_cursor_to_offset(editor, window, buffer_id, primary.id, range.start);
    if let Some(eff) = effect {
        outcome.effects.push(eff);
    }
    outcome
}

/// Handles `Action::YankLine { count }` (`yy`).
pub fn yank_line(editor: &mut Editor, window: WindowId, count: u32) -> Outcome {
    let buffer_id = editor
        .window(window)
        .expect("dispatch only runs against a live window")
        .buffer_id();
    let primary = editor
        .window(window)
        .unwrap()
        .selections()
        .primary()
        .clone();
    let buffer = editor.buffer(buffer_id).expect("live buffer");
    let (start_row, end_row) = line_span_from_cursor(buffer.as_text_buffer(), &primary, count);
    let (start, end) = whole_line_range(buffer.as_text_buffer(), start_row, end_row);
    if start >= end {
        return Outcome::default();
    }
    let yanked_text: String = buffer
        .snapshot()
        .chunks_for_range(TextRange {
            start: ByteOffset(start),
            end: ByteOffset(end),
        })
        .expect("range is valid")
        .collect();
    let effect =
        super::registers_ops::write_register(editor, false, yanked_text, RegisterKind::Line);
    let mut outcome = move_cursor_to_offset(editor, window, buffer_id, primary.id, start);
    if let Some(eff) = effect {
        outcome.effects.push(eff);
    }
    outcome
}

fn move_cursor_to_offset(
    editor: &mut Editor,
    window: WindowId,
    buffer_id: BufferId,
    primary_id: usize,
    offset: usize,
) -> Outcome {
    let buffer = editor.buffer(buffer_id).expect("live buffer");
    let text_buffer = buffer.as_text_buffer();
    let anchor = text_buffer.anchor_before(offset);
    let target = Selection {
        id: primary_id,
        start: anchor,
        end: anchor,
        reversed: false,
        goal: SelectionGoal::None,
    };
    let win = editor.windows_mut().get_mut(window).expect("live window");
    win.selections_mut()
        .replace_primary(target)
        .expect("primary id is unchanged by a yank");
    Outcome {
        invalidation: RedrawInvalidation::CurrentWindow,
        ..Outcome::default()
    }
}

// --- `gU`/`gu`/`g~` (upper/lower/toggle case) ---------------------------
//
// Shares the motion-range resolution machinery above rather than
// re-deriving it.

fn toggle_case(text: &str) -> String {
    text.chars()
        .map(|ch| {
            if ch.is_uppercase() {
                ch.to_lowercase().next().unwrap_or(ch)
            } else if ch.is_lowercase() {
                ch.to_uppercase().next().unwrap_or(ch)
            } else {
                ch
            }
        })
        .collect()
}

/// Replaces `range`'s text with `transform`'s result of it via one
/// `transaction::apply` call, cursor landing at the start of the range --
/// shared by `upper_case_motion`/`lower_case_motion`/`toggle_case_motion`
/// and their `_line` counterparts.
fn apply_case_transform(
    editor: &mut Editor,
    window: WindowId,
    buffer_id: BufferId,
    primary_id: usize,
    range: ResolvedRange,
    transform: impl Fn(&str) -> String,
) -> Outcome {
    let buffer = editor.buffer(buffer_id).expect("live buffer");
    let text_buffer = buffer.as_text_buffer();
    let source: String = text_buffer
        .as_rope()
        .chunks_in_range(range.start..range.end)
        .collect();
    let replacement = transform(&source);

    let buffer = editor
        .buffers_mut()
        .get_mut(buffer_id)
        .expect("live buffer");
    let mutation = transaction::apply(
        buffer,
        EditDescription {
            origin: EditOrigin::User,
            edits: vec![PlannedEdit {
                selection: None,
                edit: Edit::replace(
                    TextRange {
                        start: ByteOffset(range.start),
                        end: ByteOffset(range.end),
                    },
                    replacement,
                ),
            }],
            selections: None,
        },
    )
    .expect("a case-transform edit is always well-formed");

    let landing = landing_selection(buffer, primary_id, range.start, range.linewise);
    let win = editor.windows_mut().get_mut(window).expect("live window");
    win.selections_mut()
        .replace_primary(landing)
        .expect("primary id is unchanged by a case transform");

    Outcome::from_mutation(&mutation)
}

/// Block-wise variant of `apply_case_transform`: applies `transform` to
/// each `BlockRow`'s column sub-range independently as one
/// `transaction::apply` call, so a block-wise `g~`/`gu`/`gU` spanning
/// several lines undoes as a single step.
fn apply_case_transform_block(
    editor: &mut Editor,
    window: WindowId,
    buffer_id: BufferId,
    primary_id: usize,
    rows: Vec<BlockRow>,
    transform: impl Fn(&str) -> String,
) -> Outcome {
    let Some(top) = rows.iter().map(|r| r.start).min() else {
        return Outcome::default();
    };

    let buffer = editor.buffer(buffer_id).expect("live buffer");
    let text_buffer = buffer.as_text_buffer();
    let edits: Vec<PlannedEdit> = rows
        .iter()
        .filter(|r| r.start < r.end)
        .map(|r| {
            let source: String = text_buffer
                .as_rope()
                .chunks_in_range(r.start..r.end)
                .collect();
            PlannedEdit {
                selection: None,
                edit: Edit::replace(
                    TextRange {
                        start: ByteOffset(r.start),
                        end: ByteOffset(r.end),
                    },
                    transform(&source),
                ),
            }
        })
        .collect();
    if edits.is_empty() {
        return Outcome::default();
    }

    let buffer = editor
        .buffers_mut()
        .get_mut(buffer_id)
        .expect("live buffer");
    let mutation = transaction::apply(
        buffer,
        EditDescription {
            origin: EditOrigin::User,
            edits,
            selections: None,
        },
    )
    .expect("block case-transform edits are always well-formed");

    let landing = landing_selection(buffer, primary_id, top, false);
    let win = editor.windows_mut().get_mut(window).expect("live window");
    win.selections_mut()
        .replace_primary(landing)
        .expect("primary id is unchanged by a block case transform");

    Outcome::from_mutation(&mutation)
}

fn case_motion(
    editor: &mut Editor,
    window: WindowId,
    count: u32,
    motion: &Action,
    transform: impl Fn(&str) -> String,
) -> Outcome {
    record_operator_jump(editor, window, motion);
    let buffer_id = editor
        .window(window)
        .expect("dispatch only runs against a live window")
        .buffer_id();
    let primary = editor
        .window(window)
        .unwrap()
        .selections()
        .primary()
        .clone();

    if is_visual_selection_sentinel(motion)
        && let Some(rows) = resolve_visual_block_rows(editor, window, buffer_id, &primary)
    {
        return apply_case_transform_block(editor, window, buffer_id, primary.id, rows, transform);
    }

    let Some(range) = resolve_motion_range(editor, window, buffer_id, &primary, motion, count)
    else {
        return Outcome::default();
    };
    apply_case_transform(editor, window, buffer_id, primary.id, range, transform)
}

fn case_line(
    editor: &mut Editor,
    window: WindowId,
    count: u32,
    transform: impl Fn(&str) -> String,
) -> Outcome {
    let buffer_id = editor
        .window(window)
        .expect("dispatch only runs against a live window")
        .buffer_id();
    let primary = editor
        .window(window)
        .unwrap()
        .selections()
        .primary()
        .clone();
    let buffer = editor.buffer(buffer_id).expect("live buffer");
    let (start_row, end_row) = line_span_from_cursor(buffer.as_text_buffer(), &primary, count);
    let (start, end) = whole_line_range(buffer.as_text_buffer(), start_row, end_row);
    if start >= end {
        return Outcome::default();
    }
    apply_case_transform(
        editor,
        window,
        buffer_id,
        primary.id,
        ResolvedRange {
            start,
            end,
            linewise: true,
        },
        transform,
    )
}

/// Handles `Action::UpperCaseMotion { count, motion }` (`gU{motion}`).
pub fn upper_case_motion(
    editor: &mut Editor,
    window: WindowId,
    count: u32,
    motion: &Action,
) -> Outcome {
    case_motion(editor, window, count, motion, |s| s.to_uppercase())
}

/// Handles `Action::LowerCaseMotion { count, motion }` (`gu{motion}`).
pub fn lower_case_motion(
    editor: &mut Editor,
    window: WindowId,
    count: u32,
    motion: &Action,
) -> Outcome {
    case_motion(editor, window, count, motion, |s| s.to_lowercase())
}

/// Handles `Action::ToggleCaseMotion { count, motion }` (`g~{motion}`).
pub fn toggle_case_motion(
    editor: &mut Editor,
    window: WindowId,
    count: u32,
    motion: &Action,
) -> Outcome {
    case_motion(editor, window, count, motion, |s| toggle_case(s))
}

/// Handles `Action::UpperCaseLine { count }` (`gUU`).
pub fn upper_case_line(editor: &mut Editor, window: WindowId, count: u32) -> Outcome {
    case_line(editor, window, count, |s| s.to_uppercase())
}

/// Handles `Action::LowerCaseLine { count }` (`guu`).
pub fn lower_case_line(editor: &mut Editor, window: WindowId, count: u32) -> Outcome {
    case_line(editor, window, count, |s| s.to_lowercase())
}

/// Handles `Action::ToggleCaseLine { count }` (`g~~`/`g~g~`).
pub fn toggle_case_line(editor: &mut Editor, window: WindowId, count: u32) -> Outcome {
    case_line(editor, window, count, |s| toggle_case(s))
}

// --- `>`/`<` (indent/outdent) --------------------------------------------
//
// Shares the motion-range resolution machinery above rather than
// re-deriving it.

/// One `shiftwidth`'s worth of indentation text, tabs vs. spaces per
/// `expandtab`, falling back to `tabstop` when `shiftwidth` is `0`.
fn indent_unit(options: &BufferOptions) -> String {
    let width = if options.shiftwidth > 0 {
        options.shiftwidth
    } else {
        options.tabstop.max(1)
    };
    if options.expandtab {
        " ".repeat(width as usize)
    } else {
        let tabstop = options.tabstop.max(1);
        let tabs = width / tabstop;
        let spaces = width % tabstop;
        format!(
            "{}{}",
            "\t".repeat(tabs as usize),
            " ".repeat(spaces as usize)
        )
    }
}

/// How many leading bytes of `row_text` to remove for one `<`-worth of
/// outdent, counting a tab as advancing to the next `tabstop` boundary.
fn leading_whitespace_removal_len(row_text: &str, options: &BufferOptions) -> usize {
    let width = if options.shiftwidth > 0 {
        options.shiftwidth
    } else {
        options.tabstop.max(1)
    };
    let tabstop = options.tabstop.max(1);
    let mut columns = 0u32;
    let mut bytes = 0usize;
    for ch in row_text.chars() {
        if columns >= width {
            break;
        }
        match ch {
            ' ' => {
                columns += 1;
                bytes += 1;
            }
            '\t' => {
                columns += tabstop - (columns % tabstop);
                bytes += 1;
            }
            _ => break,
        }
    }
    bytes
}

/// Shared by `indent_motion`/`outdent_motion`/`indent`/`outdent`: applies
/// one `shiftwidth`'s worth of indent/outdent to every line in
/// `[start_row, end_row]` via one `transaction::apply` call. Indent/outdent
/// is always linewise in Vim regardless of the motion given, so callers
/// resolve a row span, never a byte range.
fn indent_rows(
    editor: &mut Editor,
    window: WindowId,
    buffer_id: BufferId,
    primary_id: usize,
    start_row: u32,
    end_row: u32,
    outdent: bool,
) -> Outcome {
    let buffer = editor.buffer(buffer_id).expect("live buffer");
    let options = buffer.options().clone();
    let text_buffer = buffer.as_text_buffer();
    let unit = indent_unit(&options);

    let mut edits = Vec::new();
    for row in start_row..=end_row {
        let row_start = Point::new(row, 0).to_offset(text_buffer);
        let row_end = Point::new(row, text_buffer.line_len(row)).to_offset(text_buffer);
        let row_text: String = text_buffer
            .as_rope()
            .chunks_in_range(row_start..row_end)
            .collect();
        if row_text.is_empty() {
            // Vim doesn't add/remove indentation on a fully blank line.
            continue;
        }
        if outdent {
            let removed = leading_whitespace_removal_len(&row_text, &options);
            if removed == 0 {
                continue;
            }
            edits.push(PlannedEdit {
                selection: None,
                edit: Edit::delete(TextRange {
                    start: ByteOffset(row_start),
                    end: ByteOffset(row_start + removed),
                }),
            });
        } else {
            edits.push(PlannedEdit {
                selection: None,
                edit: Edit::insert(ByteOffset(row_start), unit.clone()),
            });
        }
    }
    if edits.is_empty() {
        return Outcome::default();
    }

    let buffer = editor
        .buffers_mut()
        .get_mut(buffer_id)
        .expect("live buffer");
    let mutation = transaction::apply(
        buffer,
        EditDescription {
            origin: EditOrigin::User,
            edits,
            selections: None,
        },
    )
    .expect("indent/outdent edits are always well-formed");

    let row_start_offset = Point::new(start_row, 0).to_offset(buffer.as_text_buffer());
    let landing = landing_selection(buffer, primary_id, row_start_offset, true);
    let win = editor.windows_mut().get_mut(window).expect("live window");
    win.selections_mut()
        .replace_primary(landing)
        .expect("primary id is unchanged by indent/outdent");

    Outcome::from_mutation(&mutation)
}

/// Handles `Action::IndentMotion { count, motion }` (`>{motion}`).
pub fn indent_motion(
    editor: &mut Editor,
    window: WindowId,
    count: u32,
    motion: &Action,
) -> Outcome {
    record_operator_jump(editor, window, motion);
    let buffer_id = editor
        .window(window)
        .expect("dispatch only runs against a live window")
        .buffer_id();
    let primary = editor
        .window(window)
        .unwrap()
        .selections()
        .primary()
        .clone();
    let Some((start_row, end_row)) =
        resolve_linewise_rows(editor, window, buffer_id, &primary, motion, count)
    else {
        return Outcome::default();
    };
    indent_rows(
        editor, window, buffer_id, primary.id, start_row, end_row, false,
    )
}

/// Handles `Action::OutdentMotion { count, motion }` (`<{motion}`).
pub fn outdent_motion(
    editor: &mut Editor,
    window: WindowId,
    count: u32,
    motion: &Action,
) -> Outcome {
    record_operator_jump(editor, window, motion);
    let buffer_id = editor
        .window(window)
        .expect("dispatch only runs against a live window")
        .buffer_id();
    let primary = editor
        .window(window)
        .unwrap()
        .selections()
        .primary()
        .clone();
    let Some((start_row, end_row)) =
        resolve_linewise_rows(editor, window, buffer_id, &primary, motion, count)
    else {
        return Outcome::default();
    };
    indent_rows(
        editor, window, buffer_id, primary.id, start_row, end_row, true,
    )
}

/// Handles `Action::Indent { count }` (`>>`).
pub fn indent(editor: &mut Editor, window: WindowId, count: u32) -> Outcome {
    indent_lines_from_cursor(editor, window, count, false)
}

/// Handles `Action::Outdent { count }` (`<<`).
pub fn outdent(editor: &mut Editor, window: WindowId, count: u32) -> Outcome {
    indent_lines_from_cursor(editor, window, count, true)
}

fn indent_lines_from_cursor(
    editor: &mut Editor,
    window: WindowId,
    count: u32,
    outdent: bool,
) -> Outcome {
    let buffer_id = editor
        .window(window)
        .expect("dispatch only runs against a live window")
        .buffer_id();
    let primary = editor
        .window(window)
        .unwrap()
        .selections()
        .primary()
        .clone();
    let buffer = editor.buffer(buffer_id).expect("live buffer");
    let (start_row, end_row) = line_span_from_cursor(buffer.as_text_buffer(), &primary, count);
    indent_rows(
        editor, window, buffer_id, primary.id, start_row, end_row, outdent,
    )
}
