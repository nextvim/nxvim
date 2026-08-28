//! Insert mode: entry, text insertion, and exit.
//!
//! Insert is a nested loop entered by a Normal command (`docs/VIM.md`
//! Architectural Lessons); `enter`/`exit` only flip `kernel::Mode` and report
//! that a redraw/mode change happened. Text mutation goes through
//! `kernel::transaction`, never a direct buffer edit.

use text::{Selection, SelectionGoal};
use vim_buffer::EditOrigin;
use vim_input::Action;

use crate::kernel::{
    Editor,
    command::CommandContext,
    ids::WindowId,
    mode::Mode,
    outcome::{Outcome, RedrawInvalidation},
    transaction::{self, EditDescription},
};

pub fn dispatch(editor: &mut Editor, ctx: CommandContext, action: Action) -> Outcome {
    match action {
        Action::InsertText(text) => insert_text(editor, ctx.window, &text),
        // `Esc` in Insert mode resolves to `Action::Clear`, not
        // `Action::SetToNormal` (see `vim_input::Keymap::vim_defaults`'s
        // `insert_actions` table) — `vim_input::Resolver` treats both as
        // "leave Insert" for its own key-decoding mode, so `kernel::Mode`
        // must too, or the two mode trackers desync: the resolver starts
        // decoding keys as Normal-mode commands while the kernel is still
        // dispatching them through `insert::dispatch`, silently dropping
        // every motion.
        Action::SetToNormal | Action::Clear => exit(editor),
        _ => Outcome::default(),
    }
}

pub fn enter(editor: &mut Editor) -> Outcome {
    editor.set_mode(Mode::Insert);
    Outcome {
        mode_changed: true,
        invalidation: RedrawInvalidation::CurrentWindow,
        ..Outcome::default()
    }
}

pub fn exit(editor: &mut Editor) -> Outcome {
    editor.set_mode(Mode::Normal);
    Outcome {
        mode_changed: true,
        invalidation: RedrawInvalidation::CurrentWindow,
        ..Outcome::default()
    }
}

fn insert_text(editor: &mut Editor, window: WindowId, text: &str) -> Outcome {
    let buffer_id = editor
        .window(window)
        .expect("dispatch only runs against a live window")
        .buffer_id();
    let head = editor.window(window).unwrap().selections().primary().head();
    let offset = {
        let buffer = editor
            .buffer(buffer_id)
            .expect("window always names a live buffer");
        buffer.as_text_buffer().offset_for_anchor(&head)
    };

    let buffer = editor
        .buffers_mut()
        .get_mut(buffer_id)
        .expect("live buffer");
    let mutation = transaction::apply(
        buffer,
        EditDescription {
            origin: EditOrigin::InsertMode,
            edits: vec![vim_buffer::PlannedEdit {
                selection: None,
                edit: vim_buffer::Edit::insert(vim_buffer::ByteOffset(offset), text.to_string()),
            }],
            selections: None,
        },
    )
    .expect("inserting at the cursor is always a well-formed edit");

    let new_offset = offset + text.len();
    let new_anchor = buffer.as_text_buffer().anchor_after(new_offset);
    let primary_id = editor.window(window).unwrap().selections().primary().id;
    let win = editor.windows_mut().get_mut(window).expect("live window");
    win.selections_mut()
        .replace_primary(Selection {
            id: primary_id,
            start: new_anchor,
            end: new_anchor,
            reversed: false,
            goal: SelectionGoal::None,
        })
        .expect("primary id is unchanged by an insert");

    Outcome::from_mutation(&mutation)
}
