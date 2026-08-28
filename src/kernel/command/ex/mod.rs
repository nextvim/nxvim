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
    events::EditorEvent,
    mode::Mode,
    options::{self, OptionScope, OptionValue, OptionValueKind},
    outcome::{Effect, Outcome, RedrawInvalidation},
    transaction,
};
use text::{Selection, SelectionGoal, ToOffset};
use vim_buffer::{ByteOffset, Edit, EditOrigin, PlannedEdit, TextRange};
use vim_input::Action;
use vim_script::SourceId;
use vim_script::ast::{Address, CommandRange, ExCommand};
use vim_script::ex_parser::ExLineParser;

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

pub fn parse(line: &str) -> Option<ExCommand> {
    ExLineParser::new(SourceId(0), line, 0)
        .parse()
        .ok()
        .map(|p| p.command)
}

/// Admissions check and executor for Ex commands submitted from the app/prompt.
pub fn admit(editor: &mut Editor, ctx: CommandContext, line: &str) -> Outcome {
    if let Some(command) = parse(line) {
        admit_command(editor, ctx, command)
    } else {
        Outcome::default()
    }
}

pub fn admit_command(editor: &mut Editor, ctx: CommandContext, command: ExCommand) -> Outcome {
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

            let (start_line, end_line) =
                match resolve_range(editor, ctx, &command.range, current_row, max_row) {
                    Some(r) => r,
                    None => return Outcome::default(),
                };

            execute_delete_lines(editor, ctx, start_line, end_line)
        }
        "q" | "quit" => Outcome {
            effects: vec![Effect::Quit],
            ..Outcome::default()
        },
        "w" | "write" => {
            let force = command.bang;
            let trimmed = command.arguments.trim();
            let res = if !trimmed.is_empty() {
                editor.buffers_mut().write_to(ctx.buffer, trimmed, force)
            } else {
                editor.buffers_mut().save(ctx.buffer, force)
            };

            match res {
                Ok(save_outcome) => Outcome {
                    effects: vec![Effect::FileSaved {
                        path: save_outcome.path,
                        bytes_written: save_outcome.bytes_written,
                    }],
                    ..Outcome::default()
                },
                Err(err) => Outcome {
                    effects: vec![Effect::FileSaveFailed {
                        message: err.to_string(),
                    }],
                    ..Outcome::default()
                },
            }
        }
        "set" | "se" => {
            let mut outcome = Outcome {
                invalidation: RedrawInvalidation::CurrentWindow,
                ..Outcome::default()
            };

            let args: Vec<&str> = command.arguments.split_whitespace().collect();
            if args.is_empty() {
                return outcome;
            }

            for arg in args {
                let (name, action) = parse_set_arg(arg);
                if let Some(spec) = options::lookup(&name) {
                    match action {
                        SetAction::Query => {
                            let val_str = get_option_string(editor, ctx, spec);
                            outcome.effects.push(Effect::OptionMessage {
                                message: format!("{}={}", spec.canonical_name, val_str),
                            });
                        }
                        SetAction::SetBool(val) => {
                            if spec.kind != OptionValueKind::Bool {
                                outcome.effects.push(Effect::OptionMessage {
                                    message: format!("E474: Invalid argument: {}", arg),
                                });
                                continue;
                            }
                            set_option_val(editor, ctx, spec, OptionValue::Bool(val), &mut outcome);
                        }
                        SetAction::Toggle => {
                            if spec.kind != OptionValueKind::Bool {
                                outcome.effects.push(Effect::OptionMessage {
                                    message: format!("E474: Invalid argument: {}", arg),
                                });
                                continue;
                            }
                            let current_val = get_option_bool(editor, ctx, spec);
                            set_option_val(
                                editor,
                                ctx,
                                spec,
                                OptionValue::Bool(!current_val),
                                &mut outcome,
                            );
                        }
                        SetAction::SetValue(val_str) => {
                            let val = match spec.kind {
                                OptionValueKind::Bool => match val_str.as_str() {
                                    "true" | "on" | "1" => Ok(OptionValue::Bool(true)),
                                    "false" | "off" | "0" => Ok(OptionValue::Bool(false)),
                                    _ => Err(()),
                                },
                                OptionValueKind::Number => {
                                    if let Ok(num) = val_str.parse::<i64>() {
                                        Ok(OptionValue::Number(num))
                                    } else {
                                        Err(())
                                    }
                                }
                                OptionValueKind::Str => Ok(OptionValue::Str(val_str)),
                            };
                            match val {
                                Ok(v) => {
                                    set_option_val(editor, ctx, spec, v, &mut outcome);
                                }
                                Err(_) => {
                                    outcome.effects.push(Effect::OptionMessage {
                                        message: format!("E474: Invalid argument: {}", arg),
                                    });
                                }
                            }
                        }
                    }
                } else {
                    outcome.effects.push(Effect::OptionMessage {
                        message: format!("E518: Unknown option: {}", name),
                    });
                }
            }
            outcome
        }
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
        let buffer = editor.buffer(buffer_id).expect("active buffer");
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

enum SetAction {
    Query,
    SetBool(bool),
    Toggle,
    SetValue(String),
}

fn parse_set_arg(arg: &str) -> (String, SetAction) {
    if arg.ends_with('?') {
        let name = &arg[..arg.len() - 1];
        return (name.to_string(), SetAction::Query);
    }
    if arg.ends_with('!') {
        let name = &arg[..arg.len() - 1];
        return (name.to_string(), SetAction::Toggle);
    }
    if let Some(idx) = arg.find('=') {
        let name = &arg[..idx];
        let val = &arg[idx + 1..];
        return (name.to_string(), SetAction::SetValue(val.to_string()));
    }
    if arg.starts_with("no") {
        let name = &arg[2..];
        if options::lookup(name).is_some() {
            return (name.to_string(), SetAction::SetBool(false));
        }
    }
    (arg.to_string(), SetAction::SetBool(true))
}

fn get_option_string(editor: &Editor, ctx: CommandContext, spec: options::OptionSpec) -> String {
    match spec.scope {
        OptionScope::Global => match spec.canonical_name {
            "ignorecase" => editor.global_options().ignorecase.to_string(),
            "hlsearch" => editor.global_options().hlsearch.to_string(),
            "incsearch" => editor.global_options().incsearch.to_string(),
            _ => String::new(),
        },
        OptionScope::Window => {
            if let Some(win) = editor.window(ctx.window) {
                match spec.canonical_name {
                    "wrap" => win.options().wrap.to_string(),
                    _ => String::new(),
                }
            } else {
                String::new()
            }
        }
        OptionScope::Buffer => {
            if let Some(buf) = editor.buffer(ctx.buffer) {
                match spec.canonical_name {
                    "expandtab" => buf.options().expandtab.to_string(),
                    "textwidth" => buf.options().textwidth.to_string(),
                    "shiftwidth" => buf.options().shiftwidth.to_string(),
                    "tabstop" => buf.options().tabstop.to_string(),
                    _ => String::new(),
                }
            } else {
                String::new()
            }
        }
    }
}

fn get_option_bool(editor: &Editor, ctx: CommandContext, spec: options::OptionSpec) -> bool {
    match spec.scope {
        OptionScope::Global => match spec.canonical_name {
            "ignorecase" => editor.global_options().ignorecase,
            "hlsearch" => editor.global_options().hlsearch,
            "incsearch" => editor.global_options().incsearch,
            _ => false,
        },
        OptionScope::Window => {
            if let Some(win) = editor.window(ctx.window) {
                match spec.canonical_name {
                    "wrap" => win.options().wrap,
                    _ => false,
                }
            } else {
                false
            }
        }
        OptionScope::Buffer => {
            if let Some(buf) = editor.buffer(ctx.buffer) {
                match spec.canonical_name {
                    "expandtab" => buf.options().expandtab,
                    _ => false,
                }
            } else {
                false
            }
        }
    }
}

fn set_option_val(
    editor: &mut Editor,
    ctx: CommandContext,
    spec: options::OptionSpec,
    val: OptionValue,
    outcome: &mut Outcome,
) {
    match spec.scope {
        OptionScope::Global => {
            let global = editor.global_options_mut();
            match spec.canonical_name {
                "ignorecase" => {
                    if let OptionValue::Bool(b) = val {
                        global.ignorecase = b;
                    }
                }
                "hlsearch" => {
                    if let OptionValue::Bool(b) = val {
                        global.hlsearch = b;
                    }
                }
                "incsearch" => {
                    if let OptionValue::Bool(b) = val {
                        global.incsearch = b;
                    }
                }
                _ => {}
            }
        }
        OptionScope::Window => {
            if let Some(win) = editor.windows_mut().get_mut(ctx.window) {
                let mut opts = win.options().clone();
                match spec.canonical_name {
                    "wrap" => {
                        if let OptionValue::Bool(b) = val {
                            opts.wrap = b;
                        }
                    }
                    _ => {}
                }
                win.set_options(opts);
            }
        }
        OptionScope::Buffer => {
            if let Some(buf) = editor.buffers_mut().get_mut(ctx.buffer) {
                let mut opts = buf.options().clone();
                match spec.canonical_name {
                    "expandtab" => {
                        if let OptionValue::Bool(b) = val {
                            opts.expandtab = b;
                        }
                    }
                    "textwidth" => {
                        if let OptionValue::Number(num) = val {
                            opts.textwidth = num.max(0) as u32;
                        }
                    }
                    "shiftwidth" => {
                        if let OptionValue::Number(num) = val {
                            opts.shiftwidth = num.max(0) as u32;
                        }
                    }
                    "tabstop" => {
                        if let OptionValue::Number(num) = val {
                            opts.tabstop = num.max(0) as u32;
                        }
                    }
                    _ => {}
                }
                let _ = buf.set_options(opts);
            }
        }
    }
    outcome.events.push(EditorEvent::OptionSet {
        name: spec.canonical_name,
    });
}
