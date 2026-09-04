//! Visual / Visual-Line / Visual-Block mode: entry, exit, and the handful
//! of commands that are genuinely Visual-specific (`o`/`O`, `gv`, block-wise
//! `I`/`A`). Every motion and mutating operator is *not* reimplemented here
//! -- `dispatch` forwards those straight to `kernel::command::normal`'s
//! existing functions (the incoming `Action` already carries `select: true`
//! from the resolver for motions, and operators already compose into the
//! same `Action::DeleteMotion`/`ChangeMotion`/`YankMotion`/... shapes
//! Normal-mode operators use, per `RESCUE.md` Rule 5: reuse before
//! rewriting). This file only owns the Visual-specific state transitions
//! `normal`'s dispatch has no reason to know about.

use text::{Bias, Point, Selection, SelectionGoal, ToOffset, ToPoint};
use vim_input::Action;

use crate::kernel::{
    Editor,
    command::CommandContext,
    ids::WindowId,
    mode::{Mode, VisualKind},
    outcome::{Outcome, RedrawInvalidation},
};

pub fn dispatch(editor: &mut Editor, ctx: CommandContext, action: Action) -> Outcome {
    match action {
        Action::SetToVisual => return enter(editor, ctx.window, VisualKind::Char),
        Action::SetToVisualLine => return enter(editor, ctx.window, VisualKind::Line),
        Action::SetToVisualBlock => return enter(editor, ctx.window, VisualKind::Block),
        Action::SetToNormal | Action::Clear => return exit(editor, ctx.window),
        Action::SwapSelectionEnds { corner } => {
            return swap_selection_ends(editor, ctx.window, corner);
        }
        Action::SetToInsert
        | Action::SetToAppend
        | Action::SetToInsertStartOfLineNonSpace
        | Action::SetToAppendEndOfLine => {
            if editor.window(ctx.window).and_then(|w| w.visual_kind()) == Some(VisualKind::Block) {
                return block_insert_or_append(
                    editor,
                    ctx.window,
                    matches!(action, Action::SetToAppend | Action::SetToAppendEndOfLine),
                );
            } else {
                return exit(editor, ctx.window);
            }
        }
        _ => {}
    }

    // Every other action (motions, which already carry `select: true` from
    // the resolver, and operators, which already compose into the same
    // `*Motion` shapes 7.4's `operators.rs` handles) forwards straight to
    // `normal::dispatch`. Record the Visual state beforehand so it can be
    // captured into `gv` history and cleared the moment the action leaves
    // Visual mode (an operator applying, or a mode-changing command like
    // `SetToInsert`) -- `kernel::Mode` is the only source of truth for
    // "are we in Visual", so `Window::visual_kind` must never outlive it.
    let kind_before = editor.window(ctx.window).and_then(|w| w.visual_kind());
    let selection_before = editor
        .window(ctx.window)
        .map(|w| w.selections().primary().clone());
    // Mutating operators (composed by the resolver into these `*Motion`
    // shapes, per this module's own doc comment) always leave Visual mode
    // once applied, matching real Vim -- except `ChangeMotion`, which
    // already leaves it by entering `Mode::Insert` itself.
    let exits_visual_on_its_own = matches!(action, Action::ChangeMotion { .. });
    let is_visual_exiting_operator = matches!(
        action,
        Action::DeleteMotion { .. }
            | Action::ChangeMotion { .. }
            | Action::YankMotion { .. }
            | Action::UpperCaseMotion { .. }
            | Action::LowerCaseMotion { .. }
            | Action::ToggleCaseMotion { .. }
            | Action::IndentMotion { .. }
            | Action::OutdentMotion { .. }
    );

    // Un-sync/un-shift the primary selection before executing the motion
    if editor.window(ctx.window).and_then(|w| w.visual_kind()).is_some() {
        let (win, buffer) = editor.window_and_buffer_mut(ctx.window);
        let text_buf = buffer.as_text_buffer();
        let anchor = win
            .selections()
            .anchor
            .clone()
            .unwrap_or_else(|| win.selections().primary().clone());
        let anchor_offset = anchor.tail().to_offset(text_buf);
        let head_offset = win.selections().point.to_offset(text_buf);

        let (start_anchor, end_anchor, reversed) = if anchor_offset <= head_offset {
            (
                anchor.tail().clone(),
                text_buf.anchor_before(head_offset),
                false,
            )
        } else {
            (
                text_buf.anchor_before(head_offset),
                anchor.tail().clone(),
                true,
            )
        };

        let primary_id = win.selections().primary().id;
        let primary = Selection {
            id: primary_id,
            start: start_anchor,
            end: end_anchor,
            reversed,
            goal: SelectionGoal::None,
        };
        let saved_anchor = win.selections().anchor.clone();
        let saved_point = win.selections().point;
        *win.selections_mut() =
            vim_buffer::SelectionSet::from_selections(win.selections().primary_id(), vec![primary])
                .unwrap();
        win.selections_mut().anchor = saved_anchor;
        win.selections_mut().point = saved_point;
    }
    let selection_before = editor
        .window(ctx.window)
        .map(|w| w.selections().primary().clone());

    let is_set_to_command = matches!(action, Action::SetToCommand);
    let mut outcome = super::normal::dispatch(editor, ctx, action);

    if is_visual_exiting_operator && !exits_visual_on_its_own && editor.mode().is_visual() {
        editor.set_mode(Mode::Normal);
        outcome.mode_changed = true;
    }

    // Sync the selections to update the cursors for Visual-Line/Block/Char modes
    if editor.mode().is_visual() {
        let (win, buffer) = editor.window_and_buffer_mut(ctx.window);
        // Update the logical point cursor location from the newly moved primary selection
        win.selections_mut().point = win
            .selections()
            .primary()
            .head()
            .to_point(buffer.as_text_buffer());
        sync_cursors(win, buffer.as_text_buffer(), win.visual_kind().unwrap());
    }

    let mode_after = editor.mode();
    if !mode_after.is_visual()
        && let (Some(kind), Some(selection)) = (kind_before, selection_before)
    {
        if let Some(win) = editor.windows_mut().get_mut(ctx.window) {
            win.set_last_visual(kind, selection.clone());
            win.set_visual_kind(None);
        }
        if is_set_to_command {
            let (anchor_pt, cursor_pt) = if let Some(win) = editor.window(ctx.window) {
                let text_buf = editor.buffer(ctx.buffer).unwrap().as_text_buffer();
                let anchor_pt = win.selections().anchor.as_ref().map(|a| a.tail().to_point(text_buf)).unwrap_or_else(|| win.selections().point);
                (anchor_pt, win.selections().point)
            } else {
                (text::Point::new(0, 0), text::Point::new(0, 0))
            };
            if let Some(buf) = editor.buffers_mut().get_mut(ctx.buffer) {
                use text::ToOffset;
                let text_buf = buf.as_text_buffer();
                let upper_row = anchor_pt.row.min(cursor_pt.row);
                let lower_row = anchor_pt.row.max(cursor_pt.row);
                let upper_off = text::Point { row: upper_row, column: 0 }.to_offset(text_buf);
                let lower_off = text::Point { row: lower_row, column: text_buf.line_len(lower_row) }.to_offset(text_buf);
                let start_anchor = text_buf.anchor_before(upper_off);
                let end_anchor = text_buf.anchor_before(lower_off);
                let _ = buf.set_mark_anchor('<', start_anchor);
                let _ = buf.set_mark_anchor('>', end_anchor);
            }
        } else if let Some(buf) = editor.buffers_mut().get_mut(ctx.buffer) {
            use text::ToOffset;
            let (start_anchor, end_anchor) = {
                let text_buf = buf.as_text_buffer();
                let start_off = selection.start.to_offset(text_buf);
                let end_off = selection.end.to_offset(text_buf);
                if start_off <= end_off {
                    (selection.start.clone(), selection.end.clone())
                } else {
                    (selection.end.clone(), selection.start.clone())
                }
            };
            let _ = buf.set_mark_anchor('<', start_anchor);
            let _ = buf.set_mark_anchor('>', end_anchor);
        }
    }
    outcome
}

/// Handles `Action::SetToVisual`/`SetToVisualLine`/`SetToVisualBlock`.
/// Entering a Visual kind that's already active toggles back to Normal
/// (matching real Vim's `v`/`V`/`Ctrl-v` toggle-off); entering a *different*
/// kind while already in Visual switches `VisualKind` in place without
/// collapsing the selection.
pub fn enter(editor: &mut Editor, window: WindowId, kind: VisualKind) -> Outcome {
    if let Mode::Visual(current) = editor.mode()
        && current == kind
    {
        return exit(editor, window);
    }
    editor.set_mode(Mode::Visual(kind));
    if let Some(_) = editor.window(window) {
        let (win, buffer) = editor.window_and_buffer_mut(window);
        win.set_visual_kind(Some(kind));

        // Save the current primary cursor location as anchor - no need to advance the cursor!
        let primary = win.selections().primary().clone();
        win.selections_mut().anchor = Some(primary.clone());
        win.selections_mut().point = primary.head().to_point(buffer.as_text_buffer());

        // Reconstruct/sync selections based on anchor and primary cursor location on entry
        sync_cursors(win, buffer.as_text_buffer(), kind);
    }
    Outcome {
        mode_changed: true,
        invalidation: RedrawInvalidation::CurrentWindow,
        ..Outcome::default()
    }
}

/// Leaves Visual mode: records the exited selection/kind as `gv` history,
/// collapses the selection to its head, and returns to `Mode::Normal`.
pub fn exit(editor: &mut Editor, window: WindowId) -> Outcome {
    let mut visual_info = None;
    if let Some(win) = editor.window(window) {
        if let Some(kind) = win.visual_kind() {
            visual_info = Some((kind, win.selections().primary().clone(), win.buffer_id()));
        }
    }

    if let Some((kind, primary, buffer_id)) = visual_info {
        if let Some(win) = editor.windows_mut().get_mut(window) {
            win.set_last_visual(kind, primary.clone());
        }
        if let Some(buf) = editor.buffers_mut().get_mut(buffer_id) {
            use text::ToOffset;
            let (start_anchor, end_anchor) = {
                let text_buf = buf.as_text_buffer();
                let start_off = primary.start.to_offset(text_buf);
                let end_off = primary.end.to_offset(text_buf);
                if start_off <= end_off {
                    (primary.start.clone(), primary.end.clone())
                } else {
                    (primary.end.clone(), primary.start.clone())
                }
            };
            let _ = buf.set_mark_anchor('<', start_anchor);
            let _ = buf.set_mark_anchor('>', end_anchor);
        }
    }

    if let Some(_) = editor.window(window) {
        let (win, buffer) = editor.window_and_buffer_mut(window);
        win.set_visual_kind(None);
        win.selections_mut().end_line();
        win.selections_mut().end_block();
        win.selections_mut().anchor = None;

        let text_buf = buffer.as_text_buffer();
        let primary = win.selections().primary();
        let end_offset = primary.end.to_offset(text_buf);
        let original_head = if primary.reversed {
            primary.start.clone()
        } else {
            let raw_head = end_offset.saturating_sub(1);
            text_buf.anchor_after(raw_head)
        };

        let collapsed = Selection {
            id: primary.id,
            start: original_head.clone(),
            end: original_head,
            reversed: false,
            goal: SelectionGoal::None,
        };
        let _ = win.selections_mut().replace_primary(collapsed);
    }
    editor.set_mode(Mode::Normal);
    Outcome {
        mode_changed: true,
        invalidation: RedrawInvalidation::CurrentWindow,
        ..Outcome::default()
    }
}

/// Handles `Action::SwapSelectionEnds` (`o`/`O`). Plain `o` flips which end
/// of the selection is the head/anchor in place. Block-wise `O`
/// (`corner: true`) instead swaps the selection's left/right *columns*
/// while keeping each end's row fixed -- the block's diagonal-corner swap
/// (`:help v_b_O`), distinct from a plain head/tail flip.
pub fn swap_selection_ends(editor: &mut Editor, window: WindowId, corner: bool) -> Outcome {
    let Some(buffer_id) = editor.window(window).map(|w| w.buffer_id()) else {
        return Outcome::default();
    };
    let Some(primary) = editor
        .window(window)
        .map(|w| w.selections().primary().clone())
    else {
        return Outcome::default();
    };

    let updated = if corner {
        let Some(buffer) = editor.buffer(buffer_id) else {
            return Outcome::default();
        };
        let text_buffer = buffer.as_text_buffer();
        let start_point = primary.start.to_point(text_buffer);
        let end_point = primary.end.to_point(text_buffer);
        let swapped_start = Point::new(start_point.row, end_point.column);
        let swapped_end = Point::new(end_point.row, start_point.column);
        let start_anchor = text_buffer.anchor_before(swapped_start.to_offset(text_buffer));
        let end_anchor = text_buffer.anchor_before(swapped_end.to_offset(text_buffer));
        Selection {
            id: primary.id,
            start: start_anchor,
            end: end_anchor,
            reversed: !primary.reversed,
            goal: SelectionGoal::None,
        }
    } else {
        Selection {
            reversed: !primary.reversed,
            ..primary
        }
    };

    let Some(win) = editor.windows_mut().get_mut(window) else {
        return Outcome::default();
    };
    let _ = win.selections_mut().replace_primary(updated);
    Outcome {
        invalidation: RedrawInvalidation::CurrentWindow,
        ..Outcome::default()
    }
}

/// Handles block-wise `I`/`A`: moves the cursor to the block's left
/// (`I`) or right (`A`) column on its top row and enters `Mode::Insert`,
/// with no edit yet -- matching real Vim up to the point where it starts
/// replaying typed text on every block line on `Esc`. That replay is
/// explicitly deferred (see `IMPLEMENT.md`'s "Other modes" milestone, item
/// 10): this lands the single-line-effective behavior only.
fn block_insert_or_append(editor: &mut Editor, window: WindowId, append: bool) -> Outcome {
    let (win, buffer) = editor.window_and_buffer_mut(window);
    let text_buffer = buffer.as_text_buffer();
    let primary = win.selections().primary().clone();
    let primary_sel_id = win.selections().primary_id();
    let primary_id = primary_sel_id.get() as usize;

    let (row_start, row_end, col_start, col_end) =
        if let Some(anchor_sel) = win.selections().anchor.clone() {
            let anchor_pt = anchor_sel.tail().to_point(text_buffer);
            let cursor_pt = win.selections().point;
            let r_start = anchor_pt.row.min(cursor_pt.row);
            let r_end = anchor_pt.row.max(cursor_pt.row);
            let c_start = anchor_pt.column.min(cursor_pt.column);
            let c_end = anchor_pt.column.max(cursor_pt.column) + 1;
            (r_start, r_end, c_start, c_end)
        } else {
            let start_point = primary.start.to_point(text_buffer);
            let end_point = primary.end.to_point(text_buffer);
            let r_start = start_point.row.min(end_point.row);
            let r_end = start_point.row.max(end_point.row);
            let c_start = start_point.column.min(end_point.column);
            let c_end = start_point.column.max(end_point.column) + 1;
            (r_start, r_end, c_start, c_end)
        };
    let target_col = if append { col_end } else { col_start };

    if let Some(kind) = win.visual_kind() {
        win.set_last_visual(kind, primary);
    }
    win.set_visual_kind(None);
    win.selections_mut().anchor = None;

    let mut new_selections = Vec::new();
    let mut next_id = win.selections().id.max(primary_id + 1);

    for row in row_start..=row_end {
        let line_len = text_buffer.line_len(row);
        let offset = Point::new(row, target_col.min(line_len)).to_offset(text_buffer);
        let anchor = text_buffer.anchor_before(offset);
        let id = if row == row_start {
            primary_id
        } else {
            let id = next_id;
            next_id += 1;
            id
        };
        new_selections.push(Selection {
            id,
            start: anchor.clone(),
            end: anchor,
            reversed: false,
            goal: SelectionGoal::None,
        });
    }

    win.selections_mut().id = next_id;
    *win.selections_mut() =
        vim_buffer::SelectionSet::from_selections(primary_sel_id, new_selections).unwrap();

    editor.set_mode(Mode::Insert);
    Outcome {
        mode_changed: true,
        invalidation: RedrawInvalidation::CurrentWindow,
        ..Outcome::default()
    }
}

/// Handles `Action::ReselectLastVisual` (`gv`): restores the most recently
/// exited Visual selection's kind and range. A no-op if no Visual
/// selection has been exited yet in this window.
pub fn reselect_last_visual(editor: &mut Editor, window: WindowId) -> Outcome {
    let Some((kind, selection)) = editor.window(window).and_then(|w| w.last_visual()) else {
        return Outcome::default();
    };
    editor.set_mode(Mode::Visual(kind));
    let Some(win) = editor.windows_mut().get_mut(window) else {
        return Outcome::default();
    };
    win.set_visual_kind(Some(kind));
    let _ = win.selections_mut().replace_primary(selection);
    Outcome {
        mode_changed: true,
        invalidation: RedrawInvalidation::CurrentWindow,
        ..Outcome::default()
    }
}

pub fn sync_cursors(
    win: &mut crate::kernel::window::Window,
    buffer: &text::Buffer,
    kind: VisualKind,
) {
    let text_buf = buffer;
    let Some(anchor_sel) = win.selections().anchor.clone() else {
        return;
    };
    let Some(current) = win.selections().first().cloned() else {
        return;
    };

    let anchor_pt = anchor_sel.tail().to_point(text_buf);
    let cursor_pt = win.selections().point;

    match kind {
        VisualKind::Char => {
            let anchor_offset = anchor_sel.tail().to_offset(text_buf);
            let head_offset = cursor_pt.to_offset(text_buf);

            let (low, high) = if anchor_offset <= head_offset {
                (anchor_offset, head_offset)
            } else {
                (head_offset, anchor_offset)
            };

            let next_offset = {
                let mut chunks = text_buf.as_rope().chunks_in_range(high..text_buf.len());
                if let Some(first_chunk) = chunks.next() {
                    if let Some(ch) = first_chunk.chars().next() {
                        high + ch.len_utf8()
                    } else {
                        high + 1
                    }
                } else {
                    high + 1
                }
            };
            let inclusive_high = next_offset.min(text_buf.len());
            let start_anchor = text_buf.anchor_before(low);
            let end_anchor = text_buf.anchor_before(inclusive_high);

            let reversed = head_offset < anchor_offset;
            let primary_id = current.id;
            let _ = win.selections_mut().replace_primary(Selection {
                id: primary_id,
                start: start_anchor,
                end: end_anchor,
                reversed,
                goal: SelectionGoal::None,
            });
        }
        VisualKind::Line => {
            let upper_row = anchor_pt.row.min(cursor_pt.row);
            let lower_row = anchor_pt.row.max(cursor_pt.row);

            let upper = Point {
                row: upper_row,
                column: 0,
            };
            let lower = Point {
                row: lower_row,
                column: text_buf.line_len(lower_row),
            };

            let start_anchor = text_buf.anchor_before(upper.to_offset(text_buf));
            let end_anchor = text_buf.anchor_before(lower.to_offset(text_buf));

            let reversed = cursor_pt.row < anchor_pt.row;
            let primary_id = current.id;
            let _ = win.selections_mut().replace_primary(Selection {
                id: primary_id,
                start: start_anchor,
                end: end_anchor,
                reversed,
                goal: SelectionGoal::None,
            });
        }
        VisualKind::Block => {
            let row_start = anchor_pt.row.min(cursor_pt.row);
            let row_end = anchor_pt.row.max(cursor_pt.row);
            let col_start = anchor_pt.column.min(cursor_pt.column);

            let floor_char_boundary = |text: &str, offset: usize| -> usize {
                let mut offset = offset.min(text.len());
                while offset > 0 && !text.is_char_boundary(offset) {
                    offset -= 1;
                }
                offset
            };

            let get_aligned_cols = |row: u32| -> (u32, u32) {
                let line_len = text_buf.line_len(row);
                if line_len == 0 {
                    return (0, 0);
                }
                let start_of_line_offset = Point::new(row, 0).to_offset(text_buf);
                let end_of_line_offset = Point::new(row, line_len).to_offset(text_buf);
                let line_text: String = text_buf
                    .as_rope()
                    .chunks_in_range(start_of_line_offset..end_of_line_offset)
                    .collect();

                let s_col_val = col_start.min(line_len);
                let s_col_aligned = floor_char_boundary(&line_text, s_col_val as usize) as u32;

                let max_col_val = anchor_pt.column.max(cursor_pt.column).min(line_len);
                let char_start_idx = floor_char_boundary(&line_text, max_col_val as usize);
                let char_end_idx = if char_start_idx < line_text.len() {
                    let ch = line_text[char_start_idx..].chars().next().unwrap();
                    char_start_idx + ch.len_utf8()
                } else {
                    char_start_idx
                };
                let e_col_aligned = char_end_idx as u32;

                (s_col_aligned, e_col_aligned)
            };

            let reversed = cursor_pt.column < anchor_pt.column;
            let first_id = current.id;

            win.selections_mut().selections.retain(|sel| {
                if sel.id == first_id {
                    return true;
                }
                let row = sel.head().to_point(text_buf).row;
                row >= row_start && row <= row_end
            });

            for row in row_start..=row_end {
                if row == cursor_pt.row {
                    continue;
                }

                let existing_idx = win
                    .selections()
                    .selections
                    .iter()
                    .position(|s| s.id != first_id && s.head().to_point(text_buf).row == row);

                let (s_col, e_col) = get_aligned_cols(row);

                let start_pt = Point { row, column: s_col };
                let end_pt = Point { row, column: e_col };
                let start_anchor = text_buf.anchor_before(start_pt.to_offset(text_buf));
                let end_anchor = text_buf.anchor_before(end_pt.to_offset(text_buf));

                if let Some(idx) = existing_idx {
                    let id = win.selections().selections[idx].id;
                    win.selections_mut().selections[idx] = Selection {
                        id,
                        start: start_anchor,
                        end: end_anchor,
                        reversed,
                        goal: SelectionGoal::None,
                    };
                } else {
                    let id = win.selections().id;
                    win.selections_mut().id += 1;
                    win.selections_mut().selections.push(Selection {
                        id,
                        start: start_anchor,
                        end: end_anchor,
                        reversed,
                        goal: SelectionGoal::None,
                    });
                }
            }

            let (s_col, e_col) = get_aligned_cols(cursor_pt.row);
            let start_pt = Point {
                row: cursor_pt.row,
                column: s_col,
            };
            let end_pt = Point {
                row: cursor_pt.row,
                column: e_col,
            };
            let start_anchor = text_buf.anchor_before(start_pt.to_offset(text_buf));
            let end_anchor = text_buf.anchor_before(end_pt.to_offset(text_buf));
            win.selections_mut().selections[0] = Selection {
                id: first_id,
                start: start_anchor,
                end: end_anchor,
                reversed,
                goal: SelectionGoal::None,
            };
        }
    }
}
