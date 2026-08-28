//! Text objects (`iw`/`aw`, `i(`/`a(`, `i"`/`a"`, `it`/`at`, `is`/`as`,
//! `ip`/`ap`, ...). `RESCUE.md` Rule 3: one file per command family.
//!
//! Text objects never mutate text, so they never go through
//! `kernel::transaction` and never emit `EditorEvent::TextChanged` -- they
//! only ever change what the window's primary selection spans, exactly like
//! a motion. The actual boundary math lives in `vim_buffer`'s
//! `SelectionSet::text_object`/`Motions::text_object`, built on top of
//! 7.2's motion boundary math and `vim-scanner`'s structural/tag scanning;
//! this file only resolves the current buffer/selection and forwards.

use text::{Anchor, Selection};
use vim_buffer::{BufferId, Motions};

use crate::kernel::{
    Editor,
    ids::WindowId,
    outcome::{Outcome, RedrawInvalidation},
};

/// Resolves the text object named by `ch` (`w`, `(`, `"`, `t`, `s`, `p`, ...)
/// from `from`, for `around`'s `i`/`a` variant. A plain function -- not a
/// method on a wrapper type -- so 7.4's `operators.rs` can import it
/// directly the same way `operators::motion_target` already consumes
/// 7.2's motions.
pub fn object_range(
    editor: &Editor,
    buffer_id: BufferId,
    from: &Selection<Anchor>,
    ch: char,
    around: bool,
) -> Selection<Anchor> {
    let buffer = editor
        .buffer(buffer_id)
        .expect("caller ensures a live buffer");
    from.text_object(false, ch, around, buffer.as_text_buffer())
}

/// Handles `Action::MoveWithinCharacter`/`Action::MoveAroundCharacter`:
/// replaces the window's primary selection with the resolved text object.
pub fn select(editor: &mut Editor, window: WindowId, ch: char, around: bool) -> Outcome {
    let buffer_id = editor
        .window(window)
        .expect("dispatch only runs against a live window")
        .buffer_id();
    let primary = editor
        .window(window)
        .expect("dispatch only runs against a live window")
        .selections()
        .primary()
        .clone();

    let target = object_range(editor, buffer_id, &primary, ch, around);

    let win = editor.windows_mut().get_mut(window).expect("live window");
    win.selections_mut()
        .replace_primary(target)
        .expect("text_object preserves the selection's id");

    Outcome {
        invalidation: RedrawInvalidation::CurrentWindow,
        ..Outcome::default()
    }
}
