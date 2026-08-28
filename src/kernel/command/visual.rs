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

use text::{Point, Selection, SelectionGoal, ToOffset, ToPoint};
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
        Action::SetToInsert | Action::SetToAppend
            if editor.window(ctx.window).and_then(|w| w.visual_kind())
                == Some(VisualKind::Block) =>
        {
            return block_insert_or_append(
                editor,
                ctx.window,
                matches!(action, Action::SetToAppend),
            );
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

    let mut outcome = super::normal::dispatch(editor, ctx, action);

    if is_visual_exiting_operator && !exits_visual_on_its_own && editor.mode().is_visual() {
        editor.set_mode(Mode::Normal);
        outcome.mode_changed = true;
    }

    if !editor.mode().is_visual()
        && let (Some(kind), Some(selection)) = (kind_before, selection_before)
        && let Some(win) = editor.windows_mut().get_mut(ctx.window)
    {
        win.set_last_visual(kind, selection);
        win.set_visual_kind(None);
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
    if let Some(win) = editor.windows_mut().get_mut(window) {
        win.set_visual_kind(Some(kind));
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
            let _ = buf.set_mark_anchor('<', primary.start.clone());
            let _ = buf.set_mark_anchor('>', primary.end.clone());
        }
    }

    if let Some(win) = editor.windows_mut().get_mut(window) {
        win.set_visual_kind(None);

        let primary = win.selections().primary();
        let head = primary.head();
        let id = primary.id;
        let collapsed = Selection {
            id,
            start: head.clone(),
            end: head,
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
    let Some(buffer_id) = editor.window(window).map(|w| w.buffer_id()) else {
        return Outcome::default();
    };
    let Some(primary) = editor
        .window(window)
        .map(|w| w.selections().primary().clone())
    else {
        return Outcome::default();
    };
    let Some(buffer) = editor.buffer(buffer_id) else {
        return Outcome::default();
    };
    let text_buffer = buffer.as_text_buffer();
    let start_point = primary.start.to_point(text_buffer);
    let end_point = primary.end.to_point(text_buffer);
    let row = start_point.row.min(end_point.row);
    let col_start = start_point.column.min(end_point.column);
    let col_end = start_point.column.max(end_point.column) + 1;
    let target_col = if append { col_end } else { col_start };
    let line_len = text_buffer.line_len(row);
    let offset = Point::new(row, target_col.min(line_len)).to_offset(text_buffer);
    let anchor = text_buffer.anchor_before(offset);

    let Some(win) = editor.windows_mut().get_mut(window) else {
        return Outcome::default();
    };
    let primary_id = win.selections().primary().id;
    if let Some(kind) = win.visual_kind() {
        win.set_last_visual(kind, primary);
    }
    win.set_visual_kind(None);
    let collapsed = Selection {
        id: primary_id,
        start: anchor,
        end: anchor,
        reversed: false,
        goal: SelectionGoal::None,
    };
    let _ = win.selections_mut().replace_primary(collapsed);

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
