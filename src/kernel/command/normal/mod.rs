//! Normal-mode command family. One module per family of commands
//! (`RESCUE.md` Rule 3) — `motions`/`operators` today, `text_objects`/... as
//! later milestones add them.

pub mod motions;
pub mod operators;
pub mod windows;

use vim_buffer::MutationOutcome;
use vim_input::Action;

use crate::kernel::{
    Editor, command::CommandContext, ids::WindowId, outcome::Outcome, transaction,
};

pub fn dispatch(editor: &mut Editor, ctx: CommandContext, action: Action) -> Outcome {
    match action {
        Action::MoveLeft { count, select } => motions::move_left(editor, ctx.window, count, select),
        Action::MoveRight { count, select } => {
            motions::move_right(editor, ctx.window, count, select)
        }
        Action::MoveUp { count, select } => motions::move_up(editor, ctx.window, count, select),
        Action::MoveDown { count, select } => motions::move_down(editor, ctx.window, count, select),
        Action::SetToInsert => super::insert::enter(editor),
        Action::DeleteMotion { count, motion } => {
            operators::delete_motion(editor, ctx.window, count, &motion)
        }
        Action::Undo { count } => undo(editor, ctx.window, count),
        Action::Redo { count } => redo(editor, ctx.window, count),
        Action::SplitHorizontal { .. } => windows::split_horizontal(editor, ctx),
        Action::SplitVertical { .. } => windows::split_vertical(editor, ctx),
        Action::CloseWindow => windows::close_window(editor, ctx),
        Action::OnlyWindow => windows::only_window(editor, ctx),
        Action::FocusLeftWindow => windows::focus_window(editor, ctx, crate::kernel::window::tabpage::NavigationDirection::Left),
        Action::FocusRightWindow => windows::focus_window(editor, ctx, crate::kernel::window::tabpage::NavigationDirection::Right),
        Action::FocusUpWindow => windows::focus_window(editor, ctx, crate::kernel::window::tabpage::NavigationDirection::Up),
        Action::FocusDownWindow => windows::focus_window(editor, ctx, crate::kernel::window::tabpage::NavigationDirection::Down),
        Action::NextTab { count } => windows::next_tab(editor, count),
        Action::PreviousTab { count } => windows::previous_tab(editor, count),
        _ => Outcome::default(),
    }
}


fn undo(editor: &mut Editor, window: WindowId, count: u32) -> Outcome {
    replay_history(editor, window, count, transaction::undo)
}

fn redo(editor: &mut Editor, window: WindowId, count: u32) -> Outcome {
    replay_history(editor, window, count, transaction::redo)
}

/// Shared loop for `undo`/`redo`: both step `count` times through
/// `vim_buffer`'s history via `kernel::transaction`, stopping early if
/// there's nothing left to replay, and restore the cursor to the selection
/// recorded alongside whichever transaction was last replayed.
fn replay_history(
    editor: &mut Editor,
    window: WindowId,
    count: u32,
    step: fn(&mut vim_buffer::Buffer) -> Result<Option<MutationOutcome>, vim_buffer::BufferError>,
) -> Outcome {
    let buffer_id = editor
        .window(window)
        .expect("dispatch only runs against a live window")
        .buffer_id();

    let mut last = None;
    for _ in 0..count.max(1) {
        let buffer = editor
            .buffers_mut()
            .get_mut(buffer_id)
            .expect("live buffer");
        match step(buffer) {
            Ok(Some(mutation)) => last = Some(mutation),
            Ok(None) | Err(_) => break,
        }
    }

    let Some(mutation) = last else {
        return Outcome::default();
    };
    if let Some(selections) = mutation.selections.clone() {
        let win = editor.windows_mut().get_mut(window).expect("live window");
        *win.selections_mut() = selections;
    }
    Outcome::from_mutation(&mutation)
}
