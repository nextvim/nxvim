//! `h`/`j`/`k`/`l` and friends.
//!
//! Motions never mutate text, so they don't go through
//! `kernel::transaction` — they update the window's `SelectionSet` in place
//! against the current buffer's text.

use crate::kernel::{
    Editor,
    ids::WindowId,
    outcome::{Outcome, RedrawInvalidation},
};

fn moved(select: bool) -> Outcome {
    // A pure cursor move never mutates the buffer or changes mode; only the
    // window it happened in needs to be redrawn. `select` is threaded
    // through so this is ready for Visual mode once that command family
    // exists, even though nothing can set it to `true` yet.
    let _ = select;
    Outcome {
        invalidation: RedrawInvalidation::CurrentWindow,
        ..Outcome::default()
    }
}

pub fn move_left(editor: &mut Editor, window: WindowId, count: u32, select: bool) -> Outcome {
    let (win, buffer) = editor.window_and_buffer_mut(window);
    win.selections_mut()
        .move_left(select, count, buffer.as_text_buffer());
    moved(select)
}

pub fn move_right(editor: &mut Editor, window: WindowId, count: u32, select: bool) -> Outcome {
    let (win, buffer) = editor.window_and_buffer_mut(window);
    win.selections_mut()
        .move_right(select, count, buffer.as_text_buffer());
    moved(select)
}

pub fn move_up(editor: &mut Editor, window: WindowId, count: u32, select: bool) -> Outcome {
    let (win, buffer) = editor.window_and_buffer_mut(window);
    win.selections_mut()
        .move_up(select, count, buffer.as_text_buffer());
    moved(select)
}

pub fn move_down(editor: &mut Editor, window: WindowId, count: u32, select: bool) -> Outcome {
    let (win, buffer) = editor.window_and_buffer_mut(window);
    win.selections_mut()
        .move_down(select, count, buffer.as_text_buffer());
    moved(select)
}
