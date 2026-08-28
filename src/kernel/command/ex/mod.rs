//! Ex command admission, parsing, and execution.
//!
//! Real Ex work never arrives as a per-keystroke `vim_input::Action` because
//! the resolver treats command-line text as host-owned and never decodes it
//! into actions. The `dispatch` function here handles the minimal action-level
//! commands that can occur while in Command mode (e.g. cancelling/returning
//! to Normal mode).

use crate::kernel::{
    Editor,
    command::CommandContext,
    mode::Mode,
    outcome::{Outcome, Effect, RedrawInvalidation},
    transaction,
};
use text::{Selection, SelectionGoal, ToOffset};
use vim_buffer::{ByteOffset, Edit, EditOrigin, PlannedEdit, TextRange};
use vim_input::Action;
use vim_script::SourceId;
use vim_script::ex_parser::ExLineParser;
use vim_script::ast::{Address, CommandRange};

pub fn dispatch(editor: &mut Editor, _ctx: CommandContext, action: Action) -> Outcome {
    match action {
        Action::SetToNormal | Action::Clear => exit(editor),
        _ => Outcome::default(),
    }
}

pub fn enter(editor: &mut Editor) -> Outcome {
    editor.set_mode(Mode::Command);
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

/// Admissions check and executor for Ex commands submitted from the app/prompt.
pub fn admit(editor: &mut Editor, ctx: CommandContext, line: &str) -> Outcome {
    let parsed = match ExLineParser::new(SourceId(0), line, 0).parse() {
        Ok(p) => p,
        Err(_) => return Outcome::default(),
    };

    let command = parsed.command;
    match command.name.as_str() {
        "d" | "delete" => {
            let current_row = if let Some(win) = editor.window(ctx.window) {
                let head = win.selections().primary().head();
                if let Some(buf) = editor.buffer(ctx.buffer) {
                    let pt: text::Point = buf.as_text_buffer().summary_for_anchor(&head);
                    pt.row
                } else {
                    0
                }
            } else {
                0
            };

            let row_count = if let Some(buf) = editor.buffer(ctx.buffer) {
                buf.as_text_buffer().row_count()
            } else {
                0
            };
            let max_row = row_count.saturating_sub(1);

            let (start_line, end_line) = match resolve_range(editor, ctx, &command.range, current_row, max_row) {
                Some(r) => r,
                None => return Outcome::default(),
            };

            execute_delete_lines(editor, ctx, start_line, end_line)
        }
        "q" | "quit" => Outcome {
            effects: vec![Effect::Quit],
            ..Outcome::default()
        },
        _ => Outcome::default(),
    }
}

fn resolve_address(
    editor: &Editor,
    ctx: CommandContext,
    address: &Address,
    current_row: u32,
    max_row: u32,
) -> Option<u32> {
    match address {
        Address::Current => Some(current_row + 1),
        Address::Last => Some(max_row + 1),
        Address::Line(n) => Some(*n as u32),
        Address::WholeFile => Some(1),
        Address::Offset { base, amount } => {
            let base_val = resolve_address(editor, ctx, base, current_row, max_row)?;
            let val = (base_val as i64 + amount).max(1);
            Some(val as u32)
        }
        Address::Mark(ch) => {
            if let Some(buf) = editor.buffer(ctx.buffer) {
                if let Some(offset) = buf.resolve_mark(*ch) {
                    let point = buf.as_text_buffer().offset_to_point(offset.0);
                    return Some(point.row + 1);
                }
            }
            None
        }
        Address::Search { .. } => None,
    }
}

fn resolve_range(
    editor: &Editor,
    ctx: CommandContext,
    range: &Option<CommandRange>,
    current_row: u32,
    max_row: u32,
) -> Option<(u32, u32)> {
    let range = match range {
        None => return Some((current_row + 1, current_row + 1)),
        Some(r) => r,
    };
    if matches!(range.start, Address::WholeFile) {
        return Some((1, max_row + 1));
    }
    let start_line = resolve_address(editor, ctx, &range.start, current_row, max_row)?;
    let end_line = match &range.end {
        Some(end_addr) => resolve_address(editor, ctx, end_addr, current_row, max_row)?,
        None => start_line,
    };
    Some((start_line, end_line))
}

fn execute_delete_lines(
    editor: &mut Editor,
    ctx: CommandContext,
    start_line: u32,
    end_line: u32,
) -> Outcome {
    let buffer_id = ctx.buffer;
    let window_id = ctx.window;

    let (start_offset, end_offset) = {
        let buffer = editor
            .buffer(buffer_id)
            .expect("active buffer");
        let text_buffer = buffer.as_text_buffer();

        let max = text_buffer.row_count().saturating_sub(1);
        let start_row = start_line.saturating_sub(1).min(max);
        let end_row = end_line.saturating_sub(1).min(max).max(start_row);
        let start = text::Point::new(start_row, 0).to_offset(text_buffer);
        let end = if end_row + 1 < text_buffer.row_count() {
            text::Point::new(end_row + 1, 0).to_offset(text_buffer)
        } else {
            text::Point::new(end_row, text_buffer.line_len(end_row)).to_offset(text_buffer)
        };
        (start, end)
    };

    if start_offset >= end_offset {
        return Outcome::default();
    }

    let buffer = editor
        .buffers_mut()
        .get_mut(buffer_id)
        .expect("live buffer");

    let mutation = transaction::apply(
        buffer,
        transaction::EditDescription {
            origin: EditOrigin::User,
            edits: vec![PlannedEdit {
                selection: None,
                edit: Edit::delete(TextRange {
                    start: ByteOffset(start_offset),
                    end: ByteOffset(end_offset),
                }),
            }],
            selections: None,
        },
    )
    .expect("deleting range-derived lines is always well-formed");

    let new_anchor = buffer.as_text_buffer().anchor_before(start_offset);
    if let Some(win) = editor.windows_mut().get_mut(window_id) {
        let primary_id = win.selections().primary().id;
        let _ = win.selections_mut().replace_primary(Selection {
            id: primary_id,
            start: new_anchor.clone(),
            end: new_anchor,
            reversed: false,
            goal: SelectionGoal::None,
        });
    }

    Outcome::from_mutation(&mutation)
}
