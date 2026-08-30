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
use vim_buffer::{BufferText, ByteOffset, Edit, EditOrigin, PlannedEdit, TextRange};
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
    editor.set_mode(Mode::Command(crate::kernel::mode::CommandKind::Ex));
    Outcome {
        mode_changed: true,
        invalidation: RedrawInvalidation::CurrentWindow,
        ..Outcome::default()
    }
}

pub fn enter_search(editor: &mut Editor, forward: bool) -> Outcome {
    let kind = if forward {
        crate::kernel::mode::CommandKind::SearchForward
    } else {
        crate::kernel::mode::CommandKind::SearchBackward
    };
    editor.set_mode(Mode::Command(kind));
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
        "delete" => {
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
        "quit" => Outcome {
            effects: vec![Effect::Quit],
            ..Outcome::default()
        },
        "wq" | "xit" | "exit" => {
            let force = command.bang;
            let trimmed = command.arguments.trim();
            let res = if !trimmed.is_empty() {
                editor.buffers_mut().write_to(ctx.buffer, trimmed, force)
            } else {
                editor.buffers_mut().save(ctx.buffer, force)
            };

            match res {
                Ok(_) => Outcome {
                    effects: vec![Effect::Quit],
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
        "split" => {
            let trimmed = command.arguments.trim();
            let mut outcome = super::normal::windows::split_horizontal(editor, ctx);
            if !trimmed.is_empty() {
                let path = std::path::PathBuf::from(trimmed);
                let opened = editor
                    .buffers_mut()
                    .load(&path)
                    .or_else(|_| editor.buffers_mut().create_named(&path, ""));
                match opened {
                    Ok((id, _)) => {
                        let active_win = editor.current_context().window;
                        let _ = editor.buffers_mut().set_current(id);
                        editor.set_window_buffer(active_win, id);
                        outcome.invalidation = RedrawInvalidation::All;
                    }
                    Err(err) => {
                        outcome.effects.push(Effect::OptionMessage {
                            message: format!("E297: Cannot open file: {}", err),
                        });
                    }
                }
            }
            outcome
        }
        "vsplit" => {
            let trimmed = command.arguments.trim();
            let mut outcome = super::normal::windows::split_vertical(editor, ctx);
            if !trimmed.is_empty() {
                let path = std::path::PathBuf::from(trimmed);
                let opened = editor
                    .buffers_mut()
                    .load(&path)
                    .or_else(|_| editor.buffers_mut().create_named(&path, ""));
                match opened {
                    Ok((id, _)) => {
                        let active_win = editor.current_context().window;
                        let _ = editor.buffers_mut().set_current(id);
                        editor.set_window_buffer(active_win, id);
                        outcome.invalidation = RedrawInvalidation::All;
                    }
                    Err(err) => {
                        outcome.effects.push(Effect::OptionMessage {
                            message: format!("E297: Cannot open file: {}", err),
                        });
                    }
                }
            }
            outcome
        }
        "only" => super::normal::windows::only_window(editor, ctx),
        "close" => super::normal::windows::close_window(editor, ctx),
        "copen" => {
            let mut outcome = super::normal::windows::split_horizontal(editor, ctx);
            let qf_buf = get_or_create_quickfix_buffer(editor);
            let items = editor.quickfix_list().to_vec();
            populate_quickfix_buffer(editor, qf_buf, &items);
            let active_win = editor.current_context().window;
            editor.set_window_buffer(active_win, qf_buf);
            if let Some(win) = editor.windows_mut().get_mut(active_win) {
                win.set_window_type(crate::kernel::window::WindowType::Quickfix);
            }
            outcome.invalidation = RedrawInvalidation::All;
            outcome
        }
        "lopen" => {
            let target_win = ctx.window;
            let mut outcome = super::normal::windows::split_horizontal(editor, ctx);
            let items = if let Some(win) = editor.window(target_win) {
                win.location_list().to_vec()
            } else {
                Vec::new()
            };
            let loc_buf = get_or_create_location_buffer(editor, target_win);
            populate_quickfix_buffer(editor, loc_buf, &items);
            let active_win = editor.current_context().window;
            editor.set_window_buffer(active_win, loc_buf);
            if let Some(win) = editor.windows_mut().get_mut(active_win) {
                win.set_window_type(crate::kernel::window::WindowType::LocationList);
            }
            outcome.invalidation = RedrawInvalidation::All;
            outcome
        }
        "cclose" => {
            let active_tab = editor.tabs().active();
            let win_ids = active_tab.layout().window_ids();
            let mut win_to_close = None;
            for w in win_ids {
                if let Some(win) = editor.window(w) {
                    if win.window_type() == crate::kernel::window::WindowType::Quickfix {
                        win_to_close = Some(w);
                        break;
                    }
                }
            }
            if let Some(w) = win_to_close {
                let close_ctx = CommandContext { window: w, ..ctx };
                super::normal::windows::close_window(editor, close_ctx)
            } else {
                Outcome::default()
            }
        }
        "lclose" => {
            let active_tab = editor.tabs().active();
            let win_ids = active_tab.layout().window_ids();
            let mut win_to_close = None;
            for w in win_ids {
                if let Some(win) = editor.window(w) {
                    if win.window_type() == crate::kernel::window::WindowType::LocationList {
                        win_to_close = Some(w);
                        break;
                    }
                }
            }
            if let Some(w) = win_to_close {
                let close_ctx = CommandContext { window: w, ..ctx };
                super::normal::windows::close_window(editor, close_ctx)
            } else {
                Outcome::default()
            }
        }
        "cnext" => {
            let len = editor.quickfix_list().len();
            if len == 0 {
                return Outcome::default();
            }
            let next_idx = (editor.quickfix_index() + 1) % len;
            editor.set_quickfix_index(next_idx);
            let item = editor.quickfix_list()[next_idx].clone();
            jump_to_quickfix_item(editor, ctx, &item)
        }
        "cprevious" => {
            let len = editor.quickfix_list().len();
            if len == 0 {
                return Outcome::default();
            }
            let prev_idx = if editor.quickfix_index() == 0 {
                len - 1
            } else {
                editor.quickfix_index() - 1
            };
            editor.set_quickfix_index(prev_idx);
            let item = editor.quickfix_list()[prev_idx].clone();
            jump_to_quickfix_item(editor, ctx, &item)
        }
        "cfirst" => {
            let len = editor.quickfix_list().len();
            if len == 0 {
                return Outcome::default();
            }
            editor.set_quickfix_index(0);
            let item = editor.quickfix_list()[0].clone();
            jump_to_quickfix_item(editor, ctx, &item)
        }
        "clast" => {
            let len = editor.quickfix_list().len();
            if len == 0 {
                return Outcome::default();
            }
            let idx = len - 1;
            editor.set_quickfix_index(idx);
            let item = editor.quickfix_list()[idx].clone();
            jump_to_quickfix_item(editor, ctx, &item)
        }
        "lnext" => {
            let (len, next_idx) = if let Some(win) = editor.window(ctx.window) {
                let len = win.location_list().len();
                if len == 0 {
                    (0, 0)
                } else {
                    let next = (win.location_list_index() + 1) % len;
                    (len, next)
                }
            } else {
                (0, 0)
            };
            if len == 0 {
                return Outcome::default();
            }
            let item = if let Some(win) = editor.windows_mut().get_mut(ctx.window) {
                win.set_location_list_index(next_idx);
                win.location_list()[next_idx].clone()
            } else {
                return Outcome::default();
            };
            jump_to_quickfix_item(editor, ctx, &item)
        }
        "lprevious" => {
            let (len, prev_idx) = if let Some(win) = editor.window(ctx.window) {
                let len = win.location_list().len();
                if len == 0 {
                    (0, 0)
                } else {
                    let prev = if win.location_list_index() == 0 {
                        len - 1
                    } else {
                        win.location_list_index() - 1
                    };
                    (len, prev)
                }
            } else {
                (0, 0)
            };
            if len == 0 {
                return Outcome::default();
            }
            let item = if let Some(win) = editor.windows_mut().get_mut(ctx.window) {
                win.set_location_list_index(prev_idx);
                win.location_list()[prev_idx].clone()
            } else {
                return Outcome::default();
            };
            jump_to_quickfix_item(editor, ctx, &item)
        }
        "lfirst" => {
            let (len, idx) = if let Some(win) = editor.window(ctx.window) {
                (win.location_list().len(), 0)
            } else {
                (0, 0)
            };
            if len == 0 {
                return Outcome::default();
            }
            let item = if let Some(win) = editor.windows_mut().get_mut(ctx.window) {
                win.set_location_list_index(idx);
                win.location_list()[idx].clone()
            } else {
                return Outcome::default();
            };
            jump_to_quickfix_item(editor, ctx, &item)
        }
        "llast" => {
            let (len, idx) = if let Some(win) = editor.window(ctx.window) {
                let len = win.location_list().len();
                let last = len.saturating_sub(1);
                (len, last)
            } else {
                (0, 0)
            };
            if len == 0 {
                return Outcome::default();
            }
            let item = if let Some(win) = editor.windows_mut().get_mut(ctx.window) {
                win.set_location_list_index(idx);
                win.location_list()[idx].clone()
            } else {
                return Outcome::default();
            };
            jump_to_quickfix_item(editor, ctx, &item)
        }
        "new" => {
            let mut outcome = super::normal::windows::split_horizontal(editor, ctx);
            let new_buf = editor.buffers_mut().insert("");
            let current_win = editor.current_context().window;
            editor.set_window_buffer(current_win, new_buf);
            outcome.invalidation = RedrawInvalidation::All;
            outcome
        }
        "vnew" => {
            let mut outcome = super::normal::windows::split_vertical(editor, ctx);
            let new_buf = editor.buffers_mut().insert("");
            let current_win = editor.current_context().window;
            editor.set_window_buffer(current_win, new_buf);
            outcome.invalidation = RedrawInvalidation::All;
            outcome
        }
        "enew" => {
            let new_buf = editor.buffers_mut().insert("");
            editor.set_window_buffer(ctx.window, new_buf);
            Outcome {
                invalidation: RedrawInvalidation::CurrentWindow,
                ..Outcome::default()
            }
        }
        "bnext" => {
            let list = editor.buffers_mut().list();
            if list.len() <= 1 {
                return Outcome::default();
            }
            if let Some(pos) = list.iter().position(|&id| id == ctx.buffer) {
                let next_idx = (pos + 1) % list.len();
                let next_buf = list[next_idx];
                let _ = editor.buffers_mut().set_current(next_buf);
                editor.set_window_buffer(ctx.window, next_buf);
                Outcome {
                    invalidation: RedrawInvalidation::CurrentWindow,
                    ..Outcome::default()
                }
            } else {
                Outcome::default()
            }
        }
        "bprevious" => {
            let list = editor.buffers_mut().list();
            if list.len() <= 1 {
                return Outcome::default();
            }
            if let Some(pos) = list.iter().position(|&id| id == ctx.buffer) {
                let prev_idx = if pos == 0 { list.len() - 1 } else { pos - 1 };
                let prev_buf = list[prev_idx];
                let _ = editor.buffers_mut().set_current(prev_buf);
                editor.set_window_buffer(ctx.window, prev_buf);
                Outcome {
                    invalidation: RedrawInvalidation::CurrentWindow,
                    ..Outcome::default()
                }
            } else {
                Outcome::default()
            }
        }
        "buffer" => {
            let arg = command.arguments.trim();
            if arg.is_empty() {
                return Outcome::default();
            }
            let target_buf = if let Ok(num) = arg.parse::<u64>() {
                vim_buffer::BufferId::new(num)
            } else {
                editor.buffers_mut().list().into_iter().find(|&id| {
                    if let Some(buf) = editor.buffer(id) {
                        if let Some(path) = buf.path() {
                            if path.to_string_lossy().contains(arg) {
                                return true;
                            }
                        }
                    }
                    false
                })
            };

            if let Some(id) = target_buf {
                if editor.buffer(id).is_some() {
                    let _ = editor.buffers_mut().set_current(id);
                    editor.set_window_buffer(ctx.window, id);
                    Outcome {
                        invalidation: RedrawInvalidation::CurrentWindow,
                        ..Outcome::default()
                    }
                } else {
                    Outcome {
                        effects: vec![Effect::OptionMessage {
                            message: format!("E86: Buffer {} does not exist", arg),
                        }],
                        ..Outcome::default()
                    }
                }
            } else {
                Outcome {
                    effects: vec![Effect::OptionMessage {
                        message: format!("E94: No matching buffer for {}", arg),
                    }],
                    ..Outcome::default()
                }
            }
        }
        "bdelete" => {
            let force = command.bang;
            let arg = command.arguments.trim();
            let target_id = if arg.is_empty() {
                Some(ctx.buffer)
            } else if let Ok(num) = arg.parse::<u64>() {
                vim_buffer::BufferId::new(num)
            } else {
                editor.buffers_mut().list().into_iter().find(|&id| {
                    if let Some(buf) = editor.buffer(id) {
                        if let Some(path) = buf.path() {
                            if path.to_string_lossy().contains(arg) {
                                return true;
                            }
                        }
                    }
                    false
                })
            };

            if let Some(id) = target_id {
                if let Some(buf) = editor.buffer(id) {
                    if buf.is_modified() && !force {
                        return Outcome {
                            effects: vec![Effect::OptionMessage {
                                message: format!(
                                    "E89: No write since last change for buffer {} (add ! to override)",
                                    id.get()
                                ),
                            }],
                            ..Outcome::default()
                        };
                    }
                }

                let list = editor.buffers_mut().list();
                let replacement = list
                    .iter()
                    .copied()
                    .find(|&x| x != id)
                    .unwrap_or_else(|| editor.buffers_mut().insert(""));

                editor.handle_buffer_deleted(id, replacement);

                match editor.buffers_mut().delete(id, force) {
                    Ok(_) => Outcome {
                        invalidation: RedrawInvalidation::All,
                        ..Outcome::default()
                    },
                    Err(err) => Outcome {
                        effects: vec![Effect::OptionMessage {
                            message: format!("E515: Buffer delete failed: {}", err),
                        }],
                        ..Outcome::default()
                    },
                }
            } else {
                Outcome {
                    effects: vec![Effect::OptionMessage {
                        message: format!("E94: No matching buffer for {}", arg),
                    }],
                    ..Outcome::default()
                }
            }
        }
        "edit" => {
            let trimmed = command.arguments.trim();
            let force = command.bang;
            if trimmed.is_empty() {
                match editor.buffers_mut().reload(ctx.buffer, force) {
                    Ok(_) => Outcome {
                        invalidation: RedrawInvalidation::CurrentWindow,
                        ..Outcome::default()
                    },
                    Err(err) => Outcome {
                        effects: vec![Effect::OptionMessage {
                            message: err.to_string(),
                        }],
                        ..Outcome::default()
                    },
                }
            } else {
                let path = std::path::PathBuf::from(trimmed);
                if let Some(buf) = editor.buffer(ctx.buffer) {
                    if buf.is_modified() && !force {
                        return Outcome {
                            effects: vec![Effect::OptionMessage {
                                message: "E37: No write since last change (add ! to override)"
                                    .to_string(),
                            }],
                            ..Outcome::default()
                        };
                    }
                }

                let opened = editor
                    .buffers_mut()
                    .load(&path)
                    .or_else(|_| editor.buffers_mut().create_named(&path, ""));

                match opened {
                    Ok((id, _)) => {
                        let _ = editor.buffers_mut().set_current(id);
                        editor.set_window_buffer(ctx.window, id);
                        Outcome {
                            invalidation: RedrawInvalidation::CurrentWindow,
                            ..Outcome::default()
                        }
                    }
                    Err(err) => Outcome {
                        effects: vec![Effect::OptionMessage {
                            message: format!("E297: Cannot open file: {}", err),
                        }],
                        ..Outcome::default()
                    },
                }
            }
        }
        "substitute" => {
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

            let args =
                match crate::kernel::command::substitute::parse_substitute(&command.arguments) {
                    Ok(a) => a,
                    Err(_) => return Outcome::default(),
                };
            crate::kernel::command::substitute::execute_substitute(
                editor, ctx, start_line, end_line, args,
            )
        }
        "write" => {
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
        "set" => {
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
        "global" | "vglobal" => {
            let is_v = command.name == "vglobal";
            let args = &command.arguments;
            let (pattern, cmd_str) = match parse_global_arguments(args) {
                Some(p) => p,
                None => return Outcome::default(),
            };

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

            let (start_line, end_line) = if command.range.is_none() {
                (1, row_count)
            } else {
                match resolve_range(editor, ctx, &command.range, current_row, max_row) {
                    Some(r) => r,
                    None => return Outcome::default(),
                }
            };

            let buffer = match editor.buffer(ctx.buffer) {
                Some(b) => b,
                None => return Outcome::default(),
            };

            let ignorecase = editor.global_options().ignorecase;
            let compile_opts = vim_regex::CompileOptions {
                editor: vim_regex::EditorOptions {
                    ignore_case: ignorecase,
                    smart_case: false,
                    ..vim_regex::EditorOptions::default()
                },
                ..vim_regex::CompileOptions::default()
            };
            let regex = match vim_regex::Regex::compile(&pattern, compile_opts) {
                Ok(r) => r,
                Err(_) => return Outcome::default(),
            };

            use vim_buffer::TextSearch;

            let mut target_anchors = Vec::new();
            let start_row = start_line.saturating_sub(1).min(max_row);
            let end_row = end_line.saturating_sub(1).min(max_row).max(start_row);

            for r in start_row..=end_row {
                let text = buffer.as_text_buffer().row_text(r);
                let is_match = !text.find_pattern(&regex).is_empty();
                if is_match != is_v {
                    let offset = text::Point::new(r, 0).to_offset(buffer.as_text_buffer());
                    let anchor = buffer.as_text_buffer().anchor_before(offset);
                    target_anchors.push(anchor);
                }
            }

            let mut outcome = Outcome::default();
            for anchor in target_anchors {
                let buffer = match editor.buffer(ctx.buffer) {
                    Some(b) => b,
                    None => break,
                };
                let point: text::Point = buffer.as_text_buffer().summary_for_anchor(&anchor);
                let current_row = point.row;
                if current_row >= buffer.as_text_buffer().row_count() {
                    continue;
                }

                set_cursor_to_row(editor, ctx.window, ctx.buffer, current_row);

                let nested_outcome = if cmd_str.is_empty() {
                    Outcome {
                        invalidation: RedrawInvalidation::CurrentWindow,
                        ..Outcome::default()
                    }
                } else {
                    admit(editor, ctx, &cmd_str)
                };

                outcome.mutated |= nested_outcome.mutated;
                outcome.mode_changed |= nested_outcome.mode_changed;
                outcome.effects.extend(nested_outcome.effects);
                outcome.events.extend(nested_outcome.events);
                outcome.invalidation = match (outcome.invalidation, nested_outcome.invalidation) {
                    (RedrawInvalidation::All, _) | (_, RedrawInvalidation::All) => {
                        RedrawInvalidation::All
                    }
                    (
                        RedrawInvalidation::Range { buffer, range },
                        RedrawInvalidation::Range {
                            range: other_range, ..
                        },
                    ) => {
                        use std::cmp::{max, min};
                        RedrawInvalidation::Range {
                            buffer,
                            range: TextRange {
                                start: min(range.start, other_range.start),
                                end: max(range.end, other_range.end),
                            },
                        }
                    }
                    (RedrawInvalidation::Range { buffer, range }, _)
                    | (_, RedrawInvalidation::Range { buffer, range }) => {
                        RedrawInvalidation::Range { buffer, range }
                    }
                    (RedrawInvalidation::CurrentWindow, _)
                    | (_, RedrawInvalidation::CurrentWindow) => RedrawInvalidation::CurrentWindow,
                    (RedrawInvalidation::None, RedrawInvalidation::None) => {
                        RedrawInvalidation::None
                    }
                };
            }
            outcome
        }
        "normal" => {
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

            let start_row = start_line.saturating_sub(1).min(max_row);
            let end_row = end_line.saturating_sub(1).min(max_row).max(start_row);

            let mut outcome = Outcome::default();
            for r in start_row..=end_row {
                set_cursor_to_row_first_non_blank(editor, ctx.window, ctx.buffer, r);
                let nested_outcome = execute_normal_keys(editor, &command.arguments);

                outcome.mutated |= nested_outcome.mutated;
                outcome.mode_changed |= nested_outcome.mode_changed;
                outcome.effects.extend(nested_outcome.effects);
                outcome.events.extend(nested_outcome.events);
                outcome.invalidation = match (outcome.invalidation, nested_outcome.invalidation) {
                    (RedrawInvalidation::All, _) | (_, RedrawInvalidation::All) => {
                        RedrawInvalidation::All
                    }
                    (
                        RedrawInvalidation::Range { buffer, range },
                        RedrawInvalidation::Range {
                            range: other_range, ..
                        },
                    ) => {
                        use std::cmp::{max, min};
                        RedrawInvalidation::Range {
                            buffer,
                            range: TextRange {
                                start: min(range.start, other_range.start),
                                end: max(range.end, other_range.end),
                            },
                        }
                    }
                    (RedrawInvalidation::Range { buffer, range }, _)
                    | (_, RedrawInvalidation::Range { buffer, range }) => {
                        RedrawInvalidation::Range { buffer, range }
                    }
                    (RedrawInvalidation::CurrentWindow, _)
                    | (_, RedrawInvalidation::CurrentWindow) => RedrawInvalidation::CurrentWindow,
                    (RedrawInvalidation::None, RedrawInvalidation::None) => {
                        RedrawInvalidation::None
                    }
                };
            }
            outcome
        }
        "sort" => {
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

            let (start_line, end_line) = if command.range.is_none() {
                (1, row_count)
            } else {
                match resolve_range(editor, ctx, &command.range, current_row, max_row) {
                    Some(r) => r,
                    None => return Outcome::default(),
                }
            };

            let opts = parse_sort_options(command.bang, &command.arguments);
            execute_sort_lines(editor, ctx, start_line, end_line, opts)
        }
        _ => Outcome::default(),
    }
}

fn parse_global_arguments(args: &str) -> Option<(String, String)> {
    let args = args.trim();
    if args.is_empty() {
        return None;
    }
    let mut chars = args.chars().peekable();
    let delimiter = chars.next()?;
    let mut pattern = String::new();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(&next_c) = chars.peek() {
                if next_c == delimiter {
                    pattern.push(delimiter);
                    chars.next();
                    continue;
                }
            }
            pattern.push(c);
        } else if c == delimiter {
            break;
        } else {
            pattern.push(c);
        }
    }
    let cmd = chars.collect::<String>();
    Some((pattern, cmd.trim().to_string()))
}

fn set_cursor_to_row(
    editor: &mut Editor,
    window_id: crate::kernel::ids::WindowId,
    buffer_id: crate::kernel::ids::BufferId,
    row: u32,
) {
    if let Some(buf) = editor.buffer(buffer_id) {
        let text_buffer = buf.as_text_buffer();
        let max_row = text_buffer.row_count().saturating_sub(1);
        let target_row = row.min(max_row);
        let offset = text::Point::new(target_row, 0).to_offset(text_buffer);
        let anchor = text_buffer.anchor_before(offset);
        if let Some(win) = editor.windows_mut().get_mut(window_id) {
            let primary_id = win.selections().primary().id;
            let _ = win.selections_mut().replace_primary(Selection {
                id: primary_id,
                start: anchor.clone(),
                end: anchor,
                reversed: false,
                goal: SelectionGoal::None,
            });
        }
    }
}

fn set_cursor_to_row_first_non_blank(
    editor: &mut Editor,
    window_id: crate::kernel::ids::WindowId,
    buffer_id: crate::kernel::ids::BufferId,
    row: u32,
) {
    if let Some(buf) = editor.buffer(buffer_id) {
        let text_buffer = buf.as_text_buffer();
        let max_row = text_buffer.row_count().saturating_sub(1);
        let target_row = row.min(max_row);
        let row_text = text_buffer.row_text(target_row);
        let col = row_text
            .char_indices()
            .find(|(_, c)| !c.is_whitespace())
            .map(|(i, _)| i as u32)
            .unwrap_or(0);
        let offset = text::Point::new(target_row, col).to_offset(text_buffer);
        let anchor = text_buffer.anchor_before(offset);
        if let Some(win) = editor.windows_mut().get_mut(window_id) {
            let primary_id = win.selections().primary().id;
            let _ = win.selections_mut().replace_primary(Selection {
                id: primary_id,
                start: anchor.clone(),
                end: anchor,
                reversed: false,
                goal: SelectionGoal::None,
            });
        }
    }
}

fn execute_normal_keys(editor: &mut Editor, keys_str: &str) -> Outcome {
    let keymap = vim_input::Keymap::vim_defaults();
    let mut resolver = vim_input::Resolver::new(vim_input::Mode::Normal);

    let seq = match vim_input::KeySequence::parse(keys_str) {
        Ok(s) => s,
        Err(_) => return Outcome::default(),
    };

    let mut outcome = Outcome::default();
    for item in seq.items {
        if let vim_input::KeyPattern::Exact(key) = item {
            let resolve_outcome = resolver.feed(key, &keymap);
            if let vim_input::ResolveOutcome::Resolved(resolved) = resolve_outcome {
                let action_outcome =
                    editor.execute_with_register(resolved.action, resolved.register);

                outcome.mutated |= action_outcome.mutated;
                outcome.mode_changed |= action_outcome.mode_changed;
                outcome.effects.extend(action_outcome.effects);
                outcome.events.extend(action_outcome.events);
                outcome.invalidation = match (outcome.invalidation, action_outcome.invalidation) {
                    (RedrawInvalidation::All, _) | (_, RedrawInvalidation::All) => {
                        RedrawInvalidation::All
                    }
                    (
                        RedrawInvalidation::Range { buffer, range },
                        RedrawInvalidation::Range {
                            range: other_range, ..
                        },
                    ) => {
                        use std::cmp::{max, min};
                        RedrawInvalidation::Range {
                            buffer,
                            range: TextRange {
                                start: min(range.start, other_range.start),
                                end: max(range.end, other_range.end),
                            },
                        }
                    }
                    (RedrawInvalidation::Range { buffer, range }, _)
                    | (_, RedrawInvalidation::Range { buffer, range }) => {
                        RedrawInvalidation::Range { buffer, range }
                    }
                    (RedrawInvalidation::CurrentWindow, _)
                    | (_, RedrawInvalidation::CurrentWindow) => RedrawInvalidation::CurrentWindow,
                    (RedrawInvalidation::None, RedrawInvalidation::None) => {
                        RedrawInvalidation::None
                    }
                };
            }
        }
    }
    outcome
}

struct SortOptions {
    reverse: bool,
    ignore_case: bool,
    numeric: bool,
    unique: bool,
}

fn parse_sort_options(bang: bool, args: &str) -> SortOptions {
    let mut opts = SortOptions {
        reverse: bang,
        ignore_case: false,
        numeric: false,
        unique: false,
    };
    for c in args.chars() {
        match c {
            'i' => opts.ignore_case = true,
            'n' => opts.numeric = true,
            'u' => opts.unique = true,
            '!' => opts.reverse = !opts.reverse,
            _ => {}
        }
    }
    opts
}

fn extract_number(s: &str) -> i64 {
    let mut num_str = String::new();
    let mut chars = s.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_digit(10) || c == '-' || c == '+' {
            num_str.push(chars.next().unwrap());
            break;
        }
        chars.next();
    }
    while let Some(&c) = chars.peek() {
        if c.is_digit(10) {
            num_str.push(chars.next().unwrap());
        } else {
            break;
        }
    }
    num_str.parse::<i64>().unwrap_or(0)
}

fn execute_sort_lines(
    editor: &mut Editor,
    ctx: CommandContext,
    start_line: u32,
    end_line: u32,
    opts: SortOptions,
) -> Outcome {
    let buffer_id = ctx.buffer;
    let window_id = ctx.window;

    let buffer = match editor.buffer(buffer_id) {
        Some(b) => b,
        None => return Outcome::default(),
    };
    let text_buffer = buffer.as_text_buffer();
    let row_count = text_buffer.row_count();
    if row_count == 0 {
        return Outcome::default();
    }
    let max_row = row_count.saturating_sub(1);
    let start_row = start_line.saturating_sub(1).min(max_row);
    let end_row = end_line.saturating_sub(1).min(max_row).max(start_row);

    let mut lines: Vec<String> = (start_row..=end_row)
        .map(|r| text_buffer.row_text(r).to_string())
        .collect();

    lines.sort_by(|a, b| {
        let (cmp_a, cmp_b) = if opts.ignore_case {
            (a.to_lowercase(), b.to_lowercase())
        } else {
            (a.clone(), b.clone())
        };

        let ordering = if opts.numeric {
            let val_a = extract_number(&cmp_a);
            let val_b = extract_number(&cmp_b);
            val_a.cmp(&val_b)
        } else {
            cmp_a.cmp(&cmp_b)
        };

        if opts.reverse {
            ordering.reverse()
        } else {
            ordering
        }
    });

    if opts.unique {
        if opts.ignore_case {
            let mut seen = std::collections::HashSet::new();
            lines.retain(|item| seen.insert(item.to_lowercase()));
        } else {
            lines.dedup();
        }
    }

    let (start_offset, end_offset) = {
        let start = text::Point::new(start_row, 0).to_offset(text_buffer);
        let end = if end_row + 1 < row_count {
            text::Point::new(end_row + 1, 0).to_offset(text_buffer)
        } else {
            text::Point::new(end_row, text_buffer.line_len(end_row)).to_offset(text_buffer)
        };
        (start, end)
    };

    let original_text: String = text_buffer
        .text_for_range(start_offset..end_offset)
        .collect();

    let mut replacement = lines.join("\n");
    if original_text.ends_with('\n') && !replacement.ends_with('\n') {
        replacement.push('\n');
    }

    let selections_before = editor.window(window_id).unwrap().selections().clone();
    let mutation = {
        let buffer_mut = editor
            .buffers_mut()
            .get_mut(buffer_id)
            .expect("live buffer");
        transaction::apply(
            buffer_mut,
            transaction::EditDescription {
                origin: EditOrigin::User,
                edits: vec![PlannedEdit {
                    selection: None,
                    edit: Edit::replace(
                        TextRange {
                            start: ByteOffset(start_offset),
                            end: ByteOffset(end_offset),
                        },
                        replacement,
                    ),
                }],
                selections: Some(selections_before),
                join_previous: false,
            },
        )
        .expect("sorting lines is always well-formed")
    };

    Outcome::from_mutation(&mutation)
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
        Address::Search { pattern, forward } => {
            let buffer = editor.buffer(ctx.buffer)?;
            let row_count = buffer.snapshot().row_count();
            if row_count == 0 {
                return None;
            }
            let ignorecase = editor.global_options().ignorecase;
            let compile_opts = vim_regex::CompileOptions {
                editor: vim_regex::EditorOptions {
                    ignore_case: ignorecase,
                    smart_case: false,
                    ..vim_regex::EditorOptions::default()
                },
                ..vim_regex::CompileOptions::default()
            };
            let pattern_to_use = if pattern.is_empty() {
                if let Some(reg) = editor
                    .registers()
                    .get(crate::kernel::buffer::registers::RegisterName::Search)
                {
                    reg.text.clone()
                } else {
                    return None;
                }
            } else {
                pattern.clone()
            };
            let regex = vim_regex::Regex::compile(&pattern_to_use, compile_opts).ok()?;

            use vim_buffer::TextSearch;

            let start = current_row;
            if *forward {
                for i in 1..=row_count {
                    let r = (start + i) % row_count;
                    let text = buffer.as_text_buffer().row_text(r);
                    if !text.find_pattern(&regex).is_empty() {
                        return Some(r + 1);
                    }
                }
            } else {
                for i in 1..=row_count {
                    let r = if start >= i {
                        start - i
                    } else {
                        row_count - (i - start)
                    };
                    let text = buffer.as_text_buffer().row_text(r);
                    if !text.find_pattern(&regex).is_empty() {
                        return Some(r + 1);
                    }
                }
            }
            None
        }
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

    let selections_before = editor.window(window_id).unwrap().selections().clone();
    let mutation = {
        let buffer = editor
            .buffers_mut()
            .get_mut(buffer_id)
            .expect("live buffer");
        transaction::apply(
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
                selections: Some(selections_before),
                join_previous: false,
            },
        )
        .expect("deleting range-derived lines is always well-formed")
    };

    let new_anchor = {
        let buffer = editor.buffer(buffer_id).unwrap();
        buffer.as_text_buffer().anchor_before(start_offset)
    };
    let mut final_selections = None;
    if let Some(win) = editor.windows_mut().get_mut(window_id) {
        let primary_id = win.selections().primary().id;
        let _ = win.selections_mut().replace_primary(Selection {
            id: primary_id,
            start: new_anchor.clone(),
            end: new_anchor,
            reversed: false,
            goal: SelectionGoal::None,
        });
        final_selections = Some(win.selections().clone());
    }
    if let (Some(tx_id), Some(selections)) = (mutation.transaction, final_selections) {
        let buffer = editor
            .buffers_mut()
            .get_mut(buffer_id)
            .expect("live buffer");
        buffer.record_selections(tx_id, selections);
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
            "laststatus" => editor.global_options().laststatus.to_string(),
            "ruler" => editor.global_options().ruler.to_string(),
            "showtabline" => editor.global_options().showtabline.to_string(),
            _ => String::new(),
        },
        OptionScope::Window => {
            if let Some(win) = editor.window(ctx.window) {
                match spec.canonical_name {
                    "wrap" => win.options().wrap.to_string(),
                    "number" => win.options().number.to_string(),
                    "relativenumber" => win.options().relativenumber.to_string(),
                    "signcolumn" => win.options().signcolumn.clone(),
                    "foldcolumn" => win.options().foldcolumn.to_string(),
                    "scrollbar" => win.options().scrollbar.to_string(),
                    "hscrollbar" => win.options().hscrollbar.to_string(),
                    "cursorline" => win.options().cursorline.to_string(),
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
            "ruler" => editor.global_options().ruler,
            _ => false,
        },
        OptionScope::Window => {
            if let Some(win) = editor.window(ctx.window) {
                match spec.canonical_name {
                    "wrap" => win.options().wrap,
                    "number" => win.options().number,
                    "relativenumber" => win.options().relativenumber,
                    "cursorline" => win.options().cursorline,
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
                "laststatus" => {
                    if let OptionValue::Number(n) = val {
                        global.laststatus = n;
                    }
                }
                "ruler" => {
                    if let OptionValue::Bool(b) = val {
                        global.ruler = b;
                    }
                }
                "showtabline" => {
                    if let OptionValue::Number(n) = val {
                        global.showtabline = n;
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
                    "number" => {
                        if let OptionValue::Bool(b) = val {
                            opts.number = b;
                        }
                    }
                    "relativenumber" => {
                        if let OptionValue::Bool(b) = val {
                            opts.relativenumber = b;
                        }
                    }
                    "signcolumn" => {
                        if let OptionValue::Str(s) = val {
                            opts.signcolumn = s;
                        }
                    }
                    "foldcolumn" => {
                        if let OptionValue::Number(num) = val {
                            opts.foldcolumn = num;
                        }
                    }
                    "scrollbar" => {
                        if let OptionValue::Bool(b) = val {
                            opts.scrollbar = b;
                        }
                    }
                    "hscrollbar" => {
                        if let OptionValue::Bool(b) = val {
                            opts.hscrollbar = b;
                        }
                    }
                    "cursorline" => {
                        if let OptionValue::Bool(b) = val {
                            opts.cursorline = b;
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

pub fn get_or_create_quickfix_buffer(editor: &mut Editor) -> vim_buffer::BufferId {
    if let Some(id) = editor.buffers_mut().list().into_iter().find(|&id| {
        if let Some(buf) = editor.buffer(id) {
            if let Some(path) = buf.path() {
                if path.to_string_lossy() == "*quickfix*" {
                    return true;
                }
            }
        }
        false
    }) {
        id
    } else {
        let (id, _) = editor
            .buffers_mut()
            .create_named(&std::path::PathBuf::from("*quickfix*"), "")
            .unwrap();
        id
    }
}

pub fn get_or_create_location_buffer(
    editor: &mut Editor,
    target_window_id: crate::kernel::ids::WindowId,
) -> vim_buffer::BufferId {
    let name = format!("*location-list-{}*", target_window_id.get());
    if let Some(id) = editor.buffers_mut().list().into_iter().find(|&id| {
        if let Some(buf) = editor.buffer(id) {
            if let Some(path) = buf.path() {
                if path.to_string_lossy() == name {
                    return true;
                }
            }
        }
        false
    }) {
        id
    } else {
        let (id, _) = editor
            .buffers_mut()
            .create_named(&std::path::PathBuf::from(&name), "")
            .unwrap();
        id
    }
}

pub fn populate_quickfix_buffer(
    editor: &mut Editor,
    buffer_id: vim_buffer::BufferId,
    items: &[crate::kernel::window::QuickfixItem],
) {
    let mut lines = Vec::new();
    for item in items {
        lines.push(format!(
            "{}:{}:{}: {}",
            item.filename,
            item.row + 1,
            item.col + 1,
            item.text
        ));
    }
    let text = lines.join("\n");
    let text_len = {
        let buf = editor.buffer(buffer_id).unwrap();
        buf.snapshot().len_bytes()
    };
    let buffer_mut = editor.buffers_mut().get_mut(buffer_id).unwrap();
    let _ = transaction::apply(
        buffer_mut,
        transaction::EditDescription {
            origin: vim_buffer::EditOrigin::User,
            edits: vec![vim_buffer::PlannedEdit {
                selection: None,
                edit: vim_buffer::Edit::replace(
                    vim_buffer::TextRange {
                        start: vim_buffer::ByteOffset(0),
                        end: vim_buffer::ByteOffset(text_len),
                    },
                    text,
                ),
            }],
            selections: None,
            join_previous: false,
        },
    );
}

pub fn jump_to_quickfix_item(
    editor: &mut Editor,
    ctx: CommandContext,
    item: &crate::kernel::window::QuickfixItem,
) -> Outcome {
    let buffer_id = if let Some(id) = item.buffer {
        if editor.buffer(id).is_some() {
            id
        } else {
            let path = std::path::PathBuf::from(&item.filename);
            if let Ok((new_id, _)) = editor
                .buffers_mut()
                .load(&path)
                .or_else(|_| editor.buffers_mut().create_named(&path, ""))
            {
                new_id
            } else {
                return Outcome::default();
            }
        }
    } else {
        let path = std::path::PathBuf::from(&item.filename);
        if let Ok((new_id, _)) = editor
            .buffers_mut()
            .load(&path)
            .or_else(|_| editor.buffers_mut().create_named(&path, ""))
        {
            new_id
        } else {
            return Outcome::default();
        }
    };

    let active_tab = editor.tabs().active();
    let win_ids = active_tab.layout().window_ids();
    let mut target_win = ctx.window;
    if let Some(win) = editor.window(ctx.window) {
        if win.window_type() != crate::kernel::window::WindowType::Normal {
            for w in win_ids {
                if let Some(ow) = editor.window(w) {
                    if ow.window_type() == crate::kernel::window::WindowType::Normal {
                        target_win = w;
                        break;
                    }
                }
            }
        }
    }

    let _ = editor.buffers_mut().set_current(buffer_id);
    editor.set_window_buffer(target_win, buffer_id);
    editor.set_current_window(target_win);

    if let Some(buf) = editor.buffer(buffer_id) {
        let point = text::Point::new(item.row, item.col);
        let anchor = buf
            .as_text_buffer()
            .anchor_before(point.to_offset(buf.as_text_buffer()));
        let sel = Selection {
            id: 0,
            start: anchor,
            end: anchor,
            reversed: false,
            goal: SelectionGoal::None,
        };
        if let Some(win) = editor.windows_mut().get_mut(target_win) {
            use vim_buffer::SelectionId;
            *win.selections_mut() =
                vim_buffer::SelectionSet::from_selections(SelectionId::new(0), vec![sel]).unwrap();
            win.scroll_to_line(item.row);
        }
    }

    Outcome {
        invalidation: RedrawInvalidation::All,
        ..Outcome::default()
    }
}
