use crate::kernel::Editor;
use crate::kernel::ids::WindowId;
use crate::kernel::buffer::registers::{RegisterName, RegisterKind};
use crate::kernel::outcome::{Outcome, Effect};
use vim_buffer::{ByteOffset, Edit, EditOrigin, PlannedEdit, Motions};
use text::{Selection, SelectionGoal, ToOffset, ToPoint, Point};

pub fn write_register(
    editor: &mut Editor,
    is_delete: bool,
    text: String,
    kind: RegisterKind,
) -> Option<Effect> {
    let pending = editor.pending_register();
    if pending == Some('+') || pending == Some('*') {
        let primary = pending == Some('*');
        return Some(Effect::ClipboardWrite { text, primary });
    }

    let _reg_name = pending
        .and_then(RegisterName::from_char)
        .unwrap_or(RegisterName::Unnamed);

    let selected = pending.map(|c| {
        RegisterName::from_char(c).unwrap_or(RegisterName::Unnamed)
    });

    if is_delete {
        editor.registers_mut().record_delete(selected, text, kind);
    } else {
        editor.registers_mut().record_yank(selected, text, kind);
    }
    None
}

pub fn read_register(editor: &Editor) -> (String, RegisterKind) {
    let pending = editor.pending_register();
    if pending == Some('+') || pending == Some('*') {
        if let Some(text) = &editor.primed_clipboard_register {
            return (text.clone(), RegisterKind::Character);
        }
    }

    let reg_name = pending
        .and_then(RegisterName::from_char)
        .unwrap_or(RegisterName::Unnamed);

    if let Some(reg) = editor.registers().get(reg_name) {
        (reg.text.clone(), reg.kind)
    } else {
        (String::new(), RegisterKind::Character)
    }
}

pub fn put(editor: &mut Editor, window: WindowId, count: u32) -> Outcome {
    put_impl(editor, window, count, false, None)
}

pub fn put_before(editor: &mut Editor, window: WindowId, count: u32) -> Outcome {
    put_impl(editor, window, count, true, None)
}

pub fn put_lines(editor: &mut Editor, window: WindowId, line: u32, before: bool) -> Outcome {
    put_impl(editor, window, 1, before, Some(line))
}

fn put_impl(
    editor: &mut Editor,
    window: WindowId,
    count: u32,
    is_before: bool,
    ex_line: Option<u32>,
) -> Outcome {
    let (text, kind) = read_register(editor);
    if text.is_empty() {
        return Outcome::default();
    }

    let repeated = text.repeat(count.max(1) as usize);

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

    let text_buffer = editor.buffer(buffer_id).expect("live buffer").as_text_buffer();
    let cursor_offset = primary.head().to_offset(text_buffer);

    let linewise = kind == RegisterKind::Line;

    let insert_offset = if linewise {
        let current_row = if let Some(l) = ex_line {
            l.saturating_sub(1).min(text_buffer.row_count().saturating_sub(1))
        } else {
            primary.head().to_point(text_buffer).row
        };

        if is_before {
            Point::new(current_row, 0).to_offset(text_buffer)
        } else {
            if current_row + 1 < text_buffer.row_count() {
                Point::new(current_row + 1, 0).to_offset(text_buffer)
            } else {
                text_buffer.len()
            }
        }
    } else {
        if is_before {
            cursor_offset
        } else {
            if text_buffer.len() == 0 {
                0
            } else {
                let point = cursor_offset.to_point(text_buffer);
                let line_len = text_buffer.line_len(point.row);
                if point.column < line_len {
                    text_buffer.as_rope().ceil_char_boundary(cursor_offset + 1)
                } else {
                    cursor_offset
                }
            }
        }
    };

    let mut insert_text = repeated;
    if linewise && !insert_text.ends_with('\n') {
        insert_text.push('\n');
    }

    let selections_before = editor.window(window).unwrap().selections().clone();
    let mutation = {
        let buffer = editor.buffers_mut().get_mut(buffer_id).expect("live buffer");
        crate::kernel::transaction::apply(
            buffer,
            crate::kernel::transaction::EditDescription {
                origin: EditOrigin::User,
                edits: vec![PlannedEdit {
                    selection: None,
                    edit: Edit::insert(ByteOffset(insert_offset), insert_text),
                }],
                selections: Some(selections_before),
                join_previous: false,
            },
        )
        .expect("pasting is always well-formed")
    };

    // Recalculate landing selection
    let text_buffer = editor.buffer(buffer_id).expect("live buffer").as_text_buffer();
    let landing = if linewise {
        let row = insert_offset
            .to_point(text_buffer)
            .row
            .min(text_buffer.row_count().saturating_sub(1));
        let row_start = Point::new(row, 0).to_offset(text_buffer);
        let anchor = text_buffer.anchor_before(row_start);
        let seed = Selection {
            id: primary.id,
            start: anchor.clone(),
            end: anchor,
            reversed: false,
            goal: SelectionGoal::None,
        };
        seed.move_to_start_of_line_non_space(false, text_buffer)
    } else {
        let anchor = text_buffer.anchor_before(insert_offset);
        Selection {
            id: primary.id,
            start: anchor.clone(),
            end: anchor,
            reversed: false,
            goal: SelectionGoal::None,
        }
    };

    let final_selections = {
        let win = editor.windows_mut().get_mut(window).expect("live window");
        win.selections_mut()
            .replace_primary(landing)
            .expect("primary id is unchanged by paste");
        win.selections().clone()
    };

    if let Some(tx_id) = mutation.transaction {
        let buffer = editor.buffers_mut().get_mut(buffer_id).expect("live buffer");
        buffer.record_selections(tx_id, final_selections);
    }

    Outcome::from_mutation(&mutation)
}
