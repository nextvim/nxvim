//! Insert mode (and its `Replace`/`VirtualReplace` variants): entry, text
//! insertion/overtyping, and exit.
//!
//! Insert is a nested loop entered by a Normal command (`docs/VIM.md`
//! Architectural Lessons); `enter`/`exit` only flip `kernel::Mode` and report
//! that a redraw/mode change happened. Text mutation goes through
//! `kernel::transaction`, never a direct buffer edit. `Mode::VirtualReplace`
//! is scoped down to "behaves exactly like `Mode::Replace`" for now -- its
//! true difference (overtyping through tabs/virtual columns) is deferred,
//! per this milestone's own scope note.

use text::{Anchor, Point, Selection, SelectionGoal, ToOffset, ToPoint};
use vim_buffer::{BufferText, EditOrigin, Motions};
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
        Action::InsertText(text) => {
            if editor.mode().is_replace() {
                overtype_text(editor, ctx.window, &text)
            } else {
                insert_text(editor, ctx.window, &text)
            }
        }
        Action::InsertNewLine { count } => {
            let text = "\n".repeat(count as usize);
            insert_text(editor, ctx.window, &text)
        }
        Action::DeleteCharBefore { count } => {
            if editor.mode().is_replace() {
                replace_backspace(editor, ctx.window)
            } else {
                super::normal::operators::delete_char_before(editor, ctx.window, count)
            }
        }
        Action::InsertRegister => {
            let (text, _kind) =
                crate::kernel::command::normal::registers_ops::read_register(editor);
            if text.is_empty() {
                Outcome::default()
            } else {
                insert_text(editor, ctx.window, &text)
            }
        }
        Action::MoveLeft { count, select } => {
            super::normal::motions::move_left(editor, ctx.window, count, select)
        }
        Action::MoveRight { count, select } => {
            super::normal::motions::move_right(editor, ctx.window, count, select)
        }
        Action::MoveUp { count, select } => {
            super::normal::motions::move_up(editor, ctx.window, count, select)
        }
        Action::MoveDown { count, select } => {
            super::normal::motions::move_down(editor, ctx.window, count, select)
        }
        Action::MovePageUp { count, select } => {
            super::normal::motions::move_page_up(editor, ctx.window, count, select)
        }
        Action::MovePageDown { count, select } => {
            super::normal::motions::move_page_down(editor, ctx.window, count, select)
        }
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

pub fn enter_append(editor: &mut Editor, window: WindowId) -> Outcome {
    let (win, buffer) = editor.window_and_buffer_mut(window);
    let buf_text = buffer.as_text_buffer();
    for cursor in win.selections().selections().to_vec() {
        let point = cursor.head().to_point(buf_text);
        if point.column < buf_text.line_len(point.row) {
            let next = cursor.move_right_once(false, buf_text);
            win.selections_mut().update(buf_text, &next);
        }
    }
    enter(editor)
}

pub fn enter_append_eol(editor: &mut Editor, window: WindowId) -> Outcome {
    let (win, buffer) = editor.window_and_buffer_mut(window);
    win.selections_mut()
        .move_to_end_of_line(false, buffer.as_text_buffer());
    enter(editor)
}

pub fn enter_insert_start_non_space(editor: &mut Editor, window: WindowId) -> Outcome {
    let (win, buffer) = editor.window_and_buffer_mut(window);
    win.selections_mut()
        .move_to_start_of_line_non_space(false, buffer.as_text_buffer());
    enter(editor)
}

pub fn enter_open_line(
    editor: &mut Editor,
    window: WindowId,
    count: u32,
    above: bool,
) -> Outcome {
    let buffer_id = editor
        .window(window)
        .expect("dispatch only runs against a live window")
        .buffer_id();

    let text_to_insert = "\n".repeat(count.max(1) as usize);

    let cursors = editor
        .window(window)
        .unwrap()
        .selections()
        .selections()
        .to_vec();

    let mut edits = Vec::new();
    let mut insertion_offsets = Vec::new();

    for cursor in &cursors {
        let buffer = editor.buffer(buffer_id).expect("live buffer");
        let buf_text = buffer.as_text_buffer();
        let point = cursor.head().to_point(buf_text);
        let insertion_point = if above {
            Point {
                row: point.row,
                column: 0,
            }
        } else {
            Point {
                row: point.row,
                column: buf_text.line_len(point.row),
            }
        };
        let offset = insertion_point.to_offset(buf_text);
        insertion_offsets.push((cursor.id, offset));
        edits.push(vim_buffer::PlannedEdit {
            selection: Some(vim_buffer::SelectionId::new(cursor.id)),
            edit: vim_buffer::Edit::insert(vim_buffer::ByteOffset(offset), text_to_insert.clone()),
        });
    }

    let selections_before = editor.window(window).unwrap().selections().clone();
    let mutation = {
        let buffer = editor
            .buffers_mut()
            .get_mut(buffer_id)
            .expect("live buffer");
        transaction::apply(
            buffer,
            EditDescription {
                origin: EditOrigin::InsertMode,
                edits,
                selections: Some(selections_before),
                join_previous: false,
            },
        )
        .expect("open line edits are well-formed")
    };

    let (win, buf) = editor.window_and_buffer_mut(window);
    let buf_text = buf.as_text_buffer();

    for (id, offset) in insertion_offsets {
        let target_offset = if above { offset } else { offset + 1 };
        let target_anchor = buf_text.anchor_at(target_offset, text::Bias::Left);
        if let Some(selected) = win.selections_mut().selections.iter_mut().find(|s| s.id == id) {
            *selected = Selection {
                id,
                start: target_anchor.clone(),
                end: target_anchor,
                reversed: false,
                goal: SelectionGoal::None,
            };
        }
    }
    let final_selections = win.selections().clone();

    if let Some(tx_id) = mutation.transaction {
        let buffer = editor
            .buffers_mut()
            .get_mut(buffer_id)
            .expect("live buffer");
        buffer.record_selections(tx_id, final_selections);
    }

    let enter_outcome = enter(editor);
    let mut mutation_outcome = Outcome::from_mutation(&mutation);
    mutation_outcome.mode_changed = enter_outcome.mode_changed;
    mutation_outcome
}

/// Handles `Action::SetToReplace`/`Action::SetToVirtualReplace` (`R`/`gR`).
/// Resets the acting window's overtype history so a stale entry from a
/// previous Replace session can never be popped by this one's `Backspace`.
pub fn enter_replace(editor: &mut Editor, window: WindowId, virtual_replace: bool) -> Outcome {
    editor.set_mode(if virtual_replace {
        Mode::VirtualReplace
    } else {
        Mode::Replace
    });
    if let Some(win) = editor.windows_mut().get_mut(window) {
        win.clear_replace_overtype();
    }
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

    let cursors = editor
        .window(window)
        .unwrap()
        .selections()
        .selections()
        .to_vec();

    let mut pre_edit_ranges = Vec::new();
    let mut edits = Vec::new();
    for cursor in &cursors {
        let buffer = editor.buffer(buffer_id).expect("live buffer");
        let start = buffer.as_text_buffer().offset_for_anchor(&cursor.tail());
        let end = buffer.as_text_buffer().offset_for_anchor(&cursor.head());
        let (range_start, range_end) = if start <= end {
            (start, end)
        } else {
            (end, start)
        };
        pre_edit_ranges.push((cursor.id, range_start, range_end));
    }
    pre_edit_ranges.sort_by_key(|(_, start, _)| *start);

    for &(id, range_start, range_end) in &pre_edit_ranges {
        edits.push(vim_buffer::PlannedEdit {
            selection: Some(vim_buffer::SelectionId::new(id)),
            edit: vim_buffer::Edit::replace(
                vim_buffer::TextRange {
                    start: vim_buffer::ByteOffset(range_start),
                    end: vim_buffer::ByteOffset(range_end),
                },
                text.to_string(),
            ),
        });
    }

    let selections_before = editor.window(window).unwrap().selections().clone();
    let mutation = {
        let buffer = editor
            .buffers_mut()
            .get_mut(buffer_id)
            .expect("live buffer");
        transaction::apply(
            buffer,
            EditDescription {
                origin: EditOrigin::InsertMode,
                edits,
                selections: Some(selections_before),
                join_previous: false,
            },
        )
        .expect("inserting at the cursors is always a well-formed edit")
    };

    let new_anchors: Vec<(usize, Anchor)> = {
        let buffer = editor.buffer(buffer_id).unwrap();
        let text_buffer = buffer.as_text_buffer();
        let mut cumulative_delta = 0isize;
        let mut anchors = Vec::new();
        for (id, range_start, range_end) in pre_edit_ranges {
            let new_offset = (range_start as isize + cumulative_delta) as usize + text.len();
            cumulative_delta += text.len() as isize - (range_end - range_start) as isize;
            let new_anchor = text_buffer.anchor_before(new_offset);
            anchors.push((id, new_anchor));
        }
        anchors
    };

    let win = editor.windows_mut().get_mut(window).expect("live window");
    let selections_set = win.selections_mut();
    for (id, new_anchor) in new_anchors {
        if let Some(selected) = selections_set.selections.iter_mut().find(|s| s.id == id) {
            *selected = Selection {
                id,
                start: new_anchor.clone(),
                end: new_anchor,
                reversed: false,
                goal: SelectionGoal::None,
            };
        }
    }
    let final_selections = win.selections().clone();

    if let Some(tx_id) = mutation.transaction {
        let buffer = editor
            .buffers_mut()
            .get_mut(buffer_id)
            .expect("live buffer");
        buffer.record_selections(tx_id, final_selections);
    }

    Outcome::from_mutation(&mutation)
}

/// Handles `Action::InsertText` while in `Mode::Replace`/`VirtualReplace`:
/// each typed character overtypes the character under the cursor (deleting
/// it as part of the same `transaction::apply` call, so it undoes as one
/// step) instead of pushing text right, and records the overtyped character
/// so `Backspace` can restore it. At end-of-line this degrades to plain
/// insertion, exactly like `:help i_Backspace` describes for Replace mode.
fn overtype_text(editor: &mut Editor, window: WindowId, text: &str) -> Outcome {
    let buffer_id = editor
        .window(window)
        .expect("dispatch only runs against a live window")
        .buffer_id();
    let head = editor.window(window).unwrap().selections().primary().head();
    let start_offset = {
        let buffer = editor
            .buffer(buffer_id)
            .expect("window always names a live buffer");
        buffer.as_text_buffer().offset_for_anchor(&head)
    };

    let (edits, overtyped) = {
        let buffer = editor.buffer(buffer_id).expect("live buffer");
        let text_buffer = buffer.as_text_buffer();
        let mut point = start_offset.to_point(text_buffer);
        let mut edits = Vec::new();
        let mut overtyped = Vec::new();
        let mut append_suffix = String::new();
        let mut append_start: Option<usize> = None;

        for ch in text.chars() {
            let line_len = text_buffer.line_len(point.row);
            if append_start.is_none() && point.column < line_len {
                let row_text = text_buffer.row_text(point.row);
                let byte_col = point.column as usize;
                let existing_char = row_text[byte_col..]
                    .chars()
                    .next()
                    .expect("column within line length always has a character");
                let char_len = existing_char.len_utf8() as u32;
                let offset = point.to_offset(text_buffer);
                edits.push(vim_buffer::PlannedEdit {
                    selection: None,
                    edit: vim_buffer::Edit::replace(
                        vim_buffer::TextRange {
                            start: vim_buffer::ByteOffset(offset),
                            end: vim_buffer::ByteOffset(offset + existing_char.len_utf8()),
                        },
                        ch.to_string(),
                    ),
                });
                overtyped.push(Some(existing_char));
                point.column += char_len;
            } else {
                if append_start.is_none() {
                    append_start = Some(point.to_offset(text_buffer));
                }
                append_suffix.push(ch);
                overtyped.push(None);
            }
        }
        if let Some(offset) = append_start {
            edits.push(vim_buffer::PlannedEdit {
                selection: None,
                edit: vim_buffer::Edit::insert(vim_buffer::ByteOffset(offset), append_suffix),
            });
        }
        (edits, overtyped)
    };

    if edits.is_empty() {
        return Outcome::default();
    }

    let selections_before = editor.window(window).unwrap().selections().clone();
    let mutation = {
        let buffer = editor
            .buffers_mut()
            .get_mut(buffer_id)
            .expect("live buffer");
        transaction::apply(
            buffer,
            EditDescription {
                origin: EditOrigin::InsertMode,
                edits,
                selections: Some(selections_before),
                join_previous: false,
            },
        )
        .expect("overtype edits are always well-formed")
    };

    // The typed text's total byte length is unaffected by how wide the
    // characters it overtyped were -- see this function's `overtype_text`
    // sibling `insert_text`'s identical `offset + text.len()` derivation.
    let new_offset = start_offset + text.len();
    let new_anchor = {
        let buffer = editor.buffer(buffer_id).unwrap();
        buffer.as_text_buffer().anchor_after(new_offset)
    };
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
        .expect("primary id is unchanged by an overtype");
    for record in overtyped {
        win.push_replace_overtype(record);
    }
    let final_selections = win.selections().clone();

    if let Some(tx_id) = mutation.transaction {
        let buffer = editor
            .buffers_mut()
            .get_mut(buffer_id)
            .expect("live buffer");
        buffer.record_selections(tx_id, final_selections);
    }

    Outcome::from_mutation(&mutation)
}

/// Handles `Action::DeleteCharBefore` (`Backspace`) in `Mode::Replace`/
/// `VirtualReplace`: restores the character this Replace session overtyped
/// at that position, or removes it if it was appended past end-of-line --
/// matching `:help i_Backspace`'s Replace-mode rule. A no-op once nothing
/// from this Replace session is left to back up over (real Vim never
/// backspaces past where Replace mode started).
fn replace_backspace(editor: &mut Editor, window: WindowId) -> Outcome {
    let Some(entry) = editor
        .windows_mut()
        .get_mut(window)
        .and_then(|win| win.pop_replace_overtype())
    else {
        return Outcome::default();
    };

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
    let text_buffer = buffer.as_text_buffer();
    let cursor_point = primary.head().to_point(text_buffer);
    let cursor_offset = cursor_point.to_offset(text_buffer);
    let prev_point = if cursor_point.column > 0 {
        let row_text = text_buffer.row_text(cursor_point.row);
        let prev_len = row_text[..cursor_point.column as usize]
            .chars()
            .next_back()
            .map(char::len_utf8)
            .unwrap_or(1) as u32;
        Point::new(cursor_point.row, cursor_point.column - prev_len)
    } else {
        cursor_point
    };
    let prev_offset = prev_point.to_offset(text_buffer);

    if prev_offset == cursor_offset {
        // Already at the start of the line; nothing to overtype-restore.
        return Outcome::default();
    }

    let selections_before = editor.window(window).unwrap().selections().clone();
    let edit = match entry {
        Some(original) => vim_buffer::Edit::replace(
            vim_buffer::TextRange {
                start: vim_buffer::ByteOffset(prev_offset),
                end: vim_buffer::ByteOffset(cursor_offset),
            },
            original.to_string(),
        ),
        None => vim_buffer::Edit::delete(vim_buffer::TextRange {
            start: vim_buffer::ByteOffset(prev_offset),
            end: vim_buffer::ByteOffset(cursor_offset),
        }),
    };
    let mutation = {
        let buffer = editor
            .buffers_mut()
            .get_mut(buffer_id)
            .expect("live buffer");
        transaction::apply(
            buffer,
            EditDescription {
                origin: EditOrigin::InsertMode,
                edits: vec![vim_buffer::PlannedEdit {
                    selection: None,
                    edit,
                }],
                selections: Some(selections_before),
                join_previous: false,
            },
        )
        .expect("restoring/removing one overtyped character is always well-formed")
    };

    let anchor = {
        let buffer = editor.buffer(buffer_id).unwrap();
        buffer.as_text_buffer().anchor_before(prev_offset)
    };
    let win = editor.windows_mut().get_mut(window).expect("live window");
    win.selections_mut()
        .replace_primary(Selection {
            id: primary.id,
            start: anchor,
            end: anchor,
            reversed: false,
            goal: SelectionGoal::None,
        })
        .expect("primary id is unchanged by a replace-mode backspace");
    let final_selections = win.selections().clone();

    if let Some(tx_id) = mutation.transaction {
        let buffer = editor
            .buffers_mut()
            .get_mut(buffer_id)
            .expect("live buffer");
        buffer.record_selections(tx_id, final_selections);
    }

    Outcome::from_mutation(&mutation)
}
