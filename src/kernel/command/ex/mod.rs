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
    ExLineParser::new(SourceId(0), line.trim(), 0)
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

fn resolve_canonical_command_name(name: &str) -> String {
    for spec in crate::script::commands::COMMAND_SPECS {
        if spec.name == name {
            return spec.name.to_string();
        }
    }
    for spec in crate::script::commands::COMMAND_SPECS {
        for &(alias, _) in spec.aliases {
            if alias == name {
                return spec.name.to_string();
            }
        }
    }
    let mut matches = Vec::new();
    for spec in crate::script::commands::COMMAND_SPECS {
        if name.len() >= spec.minimum_abbreviation && spec.name.starts_with(name) {
            matches.push(spec.name);
        } else {
            for &(alias, min_abbr) in spec.aliases {
                if name.len() >= min_abbr && alias.starts_with(name) {
                    matches.push(spec.name);
                    break;
                }
            }
        }
    }
    if matches.len() == 1 {
        matches[0].to_string()
    } else {
        name.to_string()
    }
}

pub fn admit_command(editor: &mut Editor, ctx: CommandContext, mut command: ExCommand) -> Outcome {
    if let Some(_) = editor.window(ctx.window) {
        let (win, buffer) = editor.window_and_buffer_mut(ctx.window);
        if win.selections().selections.len() > 1 {
            win.selections_mut().clear(buffer.as_text_buffer());
        }
    }
    command.name = resolve_canonical_command_name(&command.name);
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
        "quit" => {
            let force = command.bang;
            if let Some(buf) = editor.buffer(ctx.buffer) {
                if buf.is_modified() && !force {
                    return Outcome {
                        effects: vec![Effect::OptionMessage {
                            message: "E37: No write since last change (add ! to override)".to_string(),
                        }],
                        ..Outcome::default()
                    };
                }
            }

            let active_tab = editor.tabs().active_id();
            let active_win_count = editor.tabs().active().layout().window_ids().len();
            let tab_count = editor.tabs().len();
            let active_buffers: Vec<_> = editor
                .buffers_mut()
                .list()
                .into_iter()
                .filter(|&id| {
                    editor
                        .buffer(id)
                        .map(|b| b.lifecycle() != vim_buffer::BufferLifecycle::Deleted)
                        .unwrap_or(false)
                })
                .collect();
            let buffer_count = active_buffers.len();

            if active_win_count > 1 {
                let mut outcome = super::normal::windows::close_window(editor, ctx);
                outcome.invalidation = RedrawInvalidation::All;
                outcome
            } else if tab_count > 1 {
                if let Ok(new_active) = editor.tabs_mut().close(active_tab) {
                    editor.set_current_tab(new_active);
                    Outcome {
                        invalidation: RedrawInvalidation::All,
                        ..Outcome::default()
                    }
                } else {
                    Outcome {
                        effects: vec![Effect::Quit],
                        ..Outcome::default()
                    }
                }
            } else if buffer_count > 1 {
                let replacement = active_buffers
                    .into_iter()
                    .find(|&x| x != ctx.buffer)
                    .unwrap_or_else(|| editor.buffers_mut().insert(""));

                editor.handle_buffer_deleted(ctx.buffer, replacement);
                let _ = editor.buffers_mut().set_current(replacement);
                editor.set_window_buffer(ctx.window, replacement);
                let _ = editor.buffers_mut().delete(ctx.buffer, force);

                Outcome {
                    invalidation: RedrawInvalidation::All,
                    ..Outcome::default()
                }
            } else {
                Outcome {
                    effects: vec![Effect::Quit],
                    ..Outcome::default()
                }
            }
        }
        "qall" | "quitall" => {
            let force = command.bang;
            if !force {
                for id in editor.buffers_mut().list() {
                    if let Some(buf) = editor.buffer(id) {
                        if buf.is_modified() {
                            return Outcome {
                                effects: vec![Effect::OptionMessage {
                                    message: format!(
                                        "E37: No write since last change for buffer {} (add ! to override)",
                                        id.get()
                                    ),
                                }],
                                ..Outcome::default()
                            };
                        }
                    }
                }
            }
            Outcome {
                effects: vec![Effect::Quit],
                ..Outcome::default()
            }
        }
        "wq" | "xit" | "exit" => {
            let force = command.bang;
            let trimmed = command.arguments.trim();
            let res = if !trimmed.is_empty() {
                editor.buffers_mut().write_to(ctx.buffer, trimmed, force)
            } else {
                editor.buffers_mut().save(ctx.buffer, force)
            };

            match res {
                Ok(_) => {
                    let active_tab = editor.tabs().active_id();
                    let active_win_count = editor.tabs().active().layout().window_ids().len();
                    let tab_count = editor.tabs().len();
                    let active_buffers: Vec<_> = editor
                        .buffers_mut()
                        .list()
                        .into_iter()
                        .filter(|&id| {
                            editor
                                .buffer(id)
                                .map(|b| b.lifecycle() != vim_buffer::BufferLifecycle::Deleted)
                                .unwrap_or(false)
                        })
                        .collect();
                    let buffer_count = active_buffers.len();

                    if active_win_count > 1 {
                        let mut outcome = super::normal::windows::close_window(editor, ctx);
                        outcome.invalidation = RedrawInvalidation::All;
                        outcome
                    } else if tab_count > 1 {
                        if let Ok(new_active) = editor.tabs_mut().close(active_tab) {
                            editor.set_current_tab(new_active);
                            Outcome {
                                invalidation: RedrawInvalidation::All,
                                ..Outcome::default()
                            }
                        } else {
                            Outcome {
                                effects: vec![Effect::Quit],
                                ..Outcome::default()
                            }
                        }
                    } else if buffer_count > 1 {
                        let replacement = active_buffers
                            .into_iter()
                            .find(|&x| x != ctx.buffer)
                            .unwrap_or_else(|| editor.buffers_mut().insert(""));

                        editor.handle_buffer_deleted(ctx.buffer, replacement);
                        let _ = editor.buffers_mut().set_current(replacement);
                        editor.set_window_buffer(ctx.window, replacement);
                        let _ = editor.buffers_mut().delete(ctx.buffer, force);

                        Outcome {
                            invalidation: RedrawInvalidation::All,
                            ..Outcome::default()
                        }
                    } else {
                        Outcome {
                            effects: vec![Effect::Quit],
                            ..Outcome::default()
                        }
                    }
                }
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
        "" => {
            // Range-only command: jump to the specified line(s)
            // e.g. :10 jumps to line 10, :+5 jumps 5 lines down, :-3 jumps 3 lines up,
            // :10,+10 jumps to line 10
            if command.range.is_none() {
                return Outcome::default();
            }

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

            let (start_line, _end_line) =
                match resolve_range(editor, ctx, &command.range, current_row, max_row) {
                    Some(r) => r,
                    None => return Outcome::default(),
                };

            // Jump to the start of the range (1-based line number -> 0-based row)
            let target_row = start_line.saturating_sub(1).min(max_row);
            set_cursor_to_row(editor, ctx.window, ctx.buffer, target_row);
            Outcome {
                invalidation: RedrawInvalidation::CurrentWindow,
                ..Outcome::default()
            }
        }
        "copy" => {
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
            let target_line = match resolve_target_address(
                editor,
                ctx,
                &command.arguments,
                current_row,
                max_row,
            ) {
                Some(t) => t,
                None => return Outcome::default(),
            };
            execute_copy_lines(editor, ctx, start_line, end_line, target_line)
        }
        "move" => {
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
            let target_line = match resolve_target_address(
                editor,
                ctx,
                &command.arguments,
                current_row,
                max_row,
            ) {
                Some(t) => t,
                None => return Outcome::default(),
            };
            execute_move_lines(editor, ctx, start_line, end_line, target_line)
        }
        "yank" => {
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
            let reg_char = command.arguments.trim().chars().next();
            if let Some(c) = reg_char {
                editor.pending_register = Some(c);
            }
            let outcome = execute_yank_lines(editor, ctx, start_line, end_line);
            editor.pending_register = None;
            outcome
        }
        "put" => {
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
            let (start_line, _) =
                match resolve_range(editor, ctx, &command.range, current_row, max_row) {
                    Some(r) => r,
                    None => (current_row + 1, current_row + 1),
                };

            let reg_char = command.arguments.trim().chars().next();
            if let Some(c) = reg_char {
                editor.pending_register = Some(c);
            }

            let before = command.bang;
            let outcome =
                super::normal::registers_ops::put_lines(editor, ctx.window, start_line, before);
            editor.pending_register = None;
            outcome
        }
        "join" => {
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
                let count = command.arguments.trim().parse::<u32>().unwrap_or(2).max(2);
                let start = current_row + 1;
                let end = (start + count - 1).min(max_row + 1);
                (start, end)
            } else {
                let (s, mut e) =
                    match resolve_range(editor, ctx, &command.range, current_row, max_row) {
                        Some(r) => r,
                        None => return Outcome::default(),
                    };
                if s == e && e < max_row + 1 {
                    e = s + 1;
                }
                (s, e)
            };

            let keep_space = !command.bang;
            execute_join_lines(editor, ctx, start_line, end_line, keep_space)
        }
        "read" => {
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
            let target_line =
                match resolve_range(editor, ctx, &command.range, current_row, max_row) {
                    Some((s, _)) => s,
                    None => current_row + 1,
                };

            let arg = command.arguments.trim();
            if arg.is_empty() {
                return Outcome {
                    effects: vec![Effect::OptionMessage {
                        message: "E32: No file name".to_string(),
                    }],
                    ..Outcome::default()
                };
            }

            let path = std::path::PathBuf::from(arg);
            let content = match std::fs::read_to_string(&path) {
                Ok(c) => c,
                Err(err) => {
                    return Outcome {
                        effects: vec![Effect::OptionMessage {
                            message: format!("E484: Can't open file {}: {}", arg, err),
                        }],
                        ..Outcome::default()
                    };
                }
            };

            execute_insert_text_at_line(editor, ctx, target_line, &content)
        }
        "file" => {
            let arg = command.arguments.trim();
            if arg.is_empty() {
                let msg = if let Some(buf) = editor.buffer(ctx.buffer) {
                    let path_str = buf
                        .path()
                        .map(|p| format!("\"{}\"", p.display()))
                        .unwrap_or_else(|| "\"No Name\"".to_string());
                    let mod_str = if buf.is_modified() {
                        " [Modified]"
                    } else {
                        ""
                    };
                    let lines = buf.as_text_buffer().row_count();
                    format!("{} {}line{}", path_str, mod_str, lines)
                } else {
                    "No buffer".to_string()
                };
                Outcome {
                    effects: vec![Effect::OptionMessage { message: msg }],
                    ..Outcome::default()
                }
            } else {
                let path = std::path::PathBuf::from(arg);
                if let Some(buf) = editor.buffers_mut().get_mut(ctx.buffer) {
                    let meta = vim_buffer::FileMetadata {
                        path: Some(path.clone()),
                        ..buf.file_metadata().clone()
                    };
                    buf.set_file_metadata(meta);
                }
                Outcome {
                    effects: vec![Effect::OptionMessage {
                        message: format!("\"{}\"", path.display()),
                    }],
                    invalidation: RedrawInvalidation::CurrentWindow,
                    ..Outcome::default()
                }
            }
        }
        "tabnew" => {
            let trimmed = command.arguments.trim();
            let buf_id = if !trimmed.is_empty() {
                let path = std::path::PathBuf::from(trimmed);
                let opened = editor
                    .buffers_mut()
                    .load(&path)
                    .or_else(|_| editor.buffers_mut().create_named(&path, ""));
                match opened {
                    Ok((id, _)) => id,
                    Err(err) => {
                        return Outcome {
                            effects: vec![Effect::OptionMessage {
                                message: format!("E297: Cannot open file: {}", err),
                            }],
                            ..Outcome::default()
                        };
                    }
                }
            } else {
                editor.buffers_mut().insert("")
            };

            let buffer = editor.buffer(buf_id).expect("live buffer");
            let win = crate::kernel::window::Window::new(buf_id, buffer);
            let win_id = editor.windows_mut().insert(win);
            let tab = crate::kernel::window::tabpage::TabPage::new(win_id);
            let tab_id = editor.tabs_mut().insert(tab);
            editor.set_current_tab(tab_id);

            Outcome {
                invalidation: RedrawInvalidation::All,
                ..Outcome::default()
            }
        }
        "tabnext" => {
            let arg = command.arguments.trim();
            let tab_id = if let Ok(num) = arg.parse::<usize>() {
                let ordered = editor.tabs().ordered();
                if num > 0 && num <= ordered.len() {
                    ordered[num - 1]
                } else {
                    editor.tabs_mut().next_tab(1)
                }
            } else {
                let count = arg.parse::<usize>().unwrap_or(1);
                editor.tabs_mut().next_tab(count)
            };
            editor.set_current_tab(tab_id);
            Outcome {
                invalidation: RedrawInvalidation::All,
                ..Outcome::default()
            }
        }
        "tabprevious" => {
            let count = command.arguments.trim().parse::<usize>().unwrap_or(1);
            let tab_id = editor.tabs_mut().previous_tab(count);
            editor.set_current_tab(tab_id);
            Outcome {
                invalidation: RedrawInvalidation::All,
                ..Outcome::default()
            }
        }
        "tabclose" => {
            let active_tab = editor.tabs().active_id();
            match editor.tabs_mut().close(active_tab) {
                Ok(new_active) => {
                    editor.set_current_tab(new_active);
                    Outcome {
                        invalidation: RedrawInvalidation::All,
                        ..Outcome::default()
                    }
                }
                Err(err) => Outcome {
                    effects: vec![Effect::OptionMessage {
                        message: format!("E784: {}", err),
                    }],
                    ..Outcome::default()
                },
            }
        }
        "pwd" => {
            let cwd = std::env::current_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| ".".to_string());
            Outcome {
                effects: vec![Effect::OptionMessage { message: cwd }],
                ..Outcome::default()
            }
        }
        "cd" | "chdir" | "lcd" | "tcd" => {
            let arg = command.arguments.trim();
            let path = if arg.is_empty() || arg == "~" {
                std::env::var("HOME").unwrap_or_else(|_| "/".to_string())
            } else {
                arg.to_string()
            };

            match std::env::set_current_dir(&path) {
                Ok(_) => {
                    let cwd = std::env::current_dir()
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or(path);
                    Outcome {
                        effects: vec![Effect::OptionMessage { message: cwd }],
                        ..Outcome::default()
                    }
                }
                Err(err) => Outcome {
                    effects: vec![Effect::OptionMessage {
                        message: format!("E344: Can't find directory \"{}\" in cdpath: {}", path, err),
                    }],
                    ..Outcome::default()
                },
            }
        }
        "nohlsearch" => {
            editor.peeked_search_range = None;
            Outcome {
                invalidation: RedrawInvalidation::All,
                ..Outcome::default()
            }
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
            win.scroll_to_line(target_row);
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
            win.scroll_to_line(target_row);
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
                if (*ch == '<' || *ch == '>')
                    && let Some((_, selection)) =
                        editor.window(ctx.window).and_then(|w| w.last_visual())
                {
                    use text::ToOffset;
                    let text_buf = buf.as_text_buffer();
                    let start_off = selection.start.to_offset(text_buf);
                    let end_off = selection.end.to_offset(text_buf);
                    let target_off = if *ch == '<' {
                        start_off.min(end_off)
                    } else {
                        start_off.max(end_off)
                    };
                    let pt = text_buf.offset_to_point(if *ch == '>' && target_off > 0 && text_buf.offset_to_point(target_off).column == 0 {
                        target_off - 1
                    } else {
                        target_off
                    });
                    return Some(pt.row + 1);
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

pub(crate) fn resolve_range(
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
        Some(end_addr) => {
            resolve_address(editor, ctx, end_addr, start_line.saturating_sub(1), max_row)?
        }
        None => start_line,
    };
    let (min_line, max_line) = if start_line <= end_line {
        (start_line, end_line)
    } else {
        (end_line, start_line)
    };
    Some((min_line, max_line))
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

    let final_selections = mutation.selections.clone();
    if let Some(selections) = &final_selections {
        if let Some(win) = editor.windows_mut().get_mut(window_id) {
            *win.selections_mut() = selections.clone();
        }
    }
    if let (Some(tx_id), Some(selections)) = (mutation.transaction, &final_selections) {
        let buffer = editor
            .buffers_mut()
            .get_mut(buffer_id)
            .expect("live buffer");
        buffer.record_selections(tx_id, selections.clone());
    }

    Outcome::from_mutation(&mutation)
}

fn resolve_target_address(
    editor: &Editor,
    ctx: CommandContext,
    arg: &str,
    current_row: u32,
    max_row: u32,
) -> Option<u32> {
    let trimmed = arg.trim();
    if trimmed.is_empty() {
        return Some(current_row + 1);
    }
    if trimmed == "0" {
        return Some(0);
    }
    if trimmed == "." {
        return Some(current_row + 1);
    }
    if trimmed == "$" {
        return Some(max_row + 1);
    }
    if let Ok(num) = trimmed.parse::<u32>() {
        return Some(num);
    }
    if trimmed.starts_with('\'') && trimmed.len() == 2 {
        let ch = trimmed.chars().nth(1)?;
        return resolve_address(editor, ctx, &Address::Mark(ch), current_row, max_row);
    }
    if let Ok(parsed) = ExLineParser::new(SourceId(0), trimmed, 0).parse() {
        if let Some(r) = parsed.command.range {
            return resolve_address(editor, ctx, &r.start, current_row, max_row);
        }
    }
    None
}

fn execute_copy_lines(
    editor: &mut Editor,
    ctx: CommandContext,
    start_line: u32,
    end_line: u32,
    target_line: u32,
) -> Outcome {
    let buffer_id = ctx.buffer;
    let window_id = ctx.window;
    let (start_row, end_row, max_row, text_to_insert, insert_offset) = {
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

        let lines: Vec<String> = (start_row..=end_row)
            .map(|r| text_buffer.row_text(r).to_string())
            .collect();

        let text_to_insert = if target_line == 0 {
            lines.join("\n") + "\n"
        } else {
            "\n".to_string() + &lines.join("\n")
        };

        let insert_offset = if target_line == 0 {
            0
        } else {
            let target_row = (target_line - 1).min(max_row);
            text::Point::new(target_row, text_buffer.line_len(target_row)).to_offset(text_buffer)
        };

        (start_row, end_row, max_row, text_to_insert, insert_offset)
    };

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
                    edit: Edit::insert(ByteOffset(insert_offset), text_to_insert),
                }],
                selections: Some(selections_before),
                join_previous: false,
            },
        )
        .expect("copying range-derived lines is well-formed")
    };

    let count = end_row - start_row + 1;
    let target_row = if target_line == 0 {
        count.saturating_sub(1)
    } else {
        (target_line - 1).min(max_row) + count
    };
    set_cursor_to_row_first_non_blank(editor, window_id, buffer_id, target_row);

    Outcome::from_mutation(&mutation)
}

fn execute_move_lines(
    editor: &mut Editor,
    ctx: CommandContext,
    start_line: u32,
    end_line: u32,
    target_line: u32,
) -> Outcome {
    let buffer_id = ctx.buffer;
    let window_id = ctx.window;
    let (_start_row, _end_row, _max_row, new_span_text, start_offset, end_offset, final_target_row) = {
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

        let target_row = if target_line == 0 {
            0
        } else {
            (target_line - 1).min(max_row)
        };

        if target_line >= start_line && target_line <= end_line {
            return Outcome {
                effects: vec![Effect::OptionMessage {
                    message: "E134: Move lines into themselves".to_string(),
                }],
                ..Outcome::default()
            };
        }

        let start_span = if target_line == 0 {
            0
        } else {
            start_row.min(target_row)
        };
        let end_span = end_row.max(target_row);

        let mut lines: Vec<String> = (start_span..=end_span)
            .map(|r| text_buffer.row_text(r).to_string())
            .collect();

        let rel_start = (start_row - start_span) as usize;
        let rel_end = (end_row - start_span) as usize;
        let moved: Vec<String> = lines.drain(rel_start..=rel_end).collect();

        let count = moved.len();
        let insert_idx = if target_line == 0 {
            0
        } else if target_line < start_line {
            (target_row - start_span + 1) as usize
        } else {
            (target_row - start_span + 1) as usize - count
        };

        for (i, line) in moved.into_iter().enumerate() {
            lines.insert(insert_idx + i, line);
        }

        let new_span_text = lines.join("\n") + if end_span + 1 < row_count { "\n" } else { "" };

        let start_offset = text::Point::new(start_span, 0).to_offset(text_buffer);
        let end_offset = if end_span + 1 < row_count {
            text::Point::new(end_span + 1, 0).to_offset(text_buffer)
        } else {
            text::Point::new(end_span, text_buffer.line_len(end_span)).to_offset(text_buffer)
        };

        let final_target_row = if target_line == 0 {
            count.saturating_sub(1) as u32
        } else if target_line < start_line {
            target_row + count as u32
        } else {
            target_row
        };

        (
            start_row,
            end_row,
            max_row,
            new_span_text,
            start_offset,
            end_offset,
            final_target_row,
        )
    };

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
                    edit: Edit::replace(
                        TextRange {
                            start: ByteOffset(start_offset),
                            end: ByteOffset(end_offset),
                        },
                        new_span_text,
                    ),
                }],
                selections: Some(selections_before),
                join_previous: false,
            },
        )
        .expect("moving lines is well-formed")
    };

    set_cursor_to_row_first_non_blank(editor, window_id, buffer_id, final_target_row);
    Outcome::from_mutation(&mutation)
}

fn execute_yank_lines(
    editor: &mut Editor,
    ctx: CommandContext,
    start_line: u32,
    end_line: u32,
) -> Outcome {
    let buffer = match editor.buffer(ctx.buffer) {
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

    let yanked_text = (start_row..=end_row)
        .map(|r| text_buffer.row_text(r))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";

    let effect = super::normal::registers_ops::write_register(
        editor,
        false,
        yanked_text,
        crate::kernel::buffer::registers::RegisterKind::Line,
    );

    let count = end_row - start_row + 1;
    let msg = format!("{} line{} yanked", count, if count == 1 { "" } else { "s" });
    let mut outcome = Outcome::default();
    if let Some(eff) = effect {
        outcome.effects.push(eff);
    }
    outcome.effects.push(Effect::OptionMessage { message: msg });
    outcome
}

fn execute_join_lines(
    editor: &mut Editor,
    ctx: CommandContext,
    start_line: u32,
    end_line: u32,
    keep_space: bool,
) -> Outcome {
    let buffer_id = ctx.buffer;
    let window_id = ctx.window;
    let buffer = match editor.buffer(buffer_id) {
        Some(b) => b,
        None => return Outcome::default(),
    };
    let text_buffer = buffer.as_text_buffer();
    let row_count = text_buffer.row_count();
    if row_count <= 1 {
        return Outcome::default();
    }
    let max_row = row_count.saturating_sub(1);
    let start_row = start_line.saturating_sub(1).min(max_row);
    let end_row = end_line.saturating_sub(1).min(max_row).max(start_row);
    if start_row == end_row {
        return Outcome::default();
    }

    let mut result = String::new();
    for r in start_row..=end_row {
        let line = text_buffer.row_text(r);
        if r == start_row {
            result.push_str(&line);
        } else if !keep_space {
            result.push_str(&line);
        } else {
            let trimmed = line.trim_start();
            if !result.is_empty() && !trimmed.is_empty() {
                if result.ends_with(' ') || result.ends_with('\t') {
                    result.push_str(trimmed);
                } else {
                    let last_char = result.chars().last().unwrap();
                    if last_char == '.' || last_char == '!' || last_char == '?' {
                        result.push_str("  ");
                    } else {
                        result.push(' ');
                    }
                    result.push_str(trimmed);
                }
            } else {
                if !result.ends_with(' ') && !result.ends_with('\t') && !trimmed.is_empty() {
                    result.push(' ');
                }
                result.push_str(trimmed);
            }
        }
    }

    if end_row + 1 < row_count {
        result.push('\n');
    }

    let start_offset = text::Point::new(start_row, 0).to_offset(text_buffer);
    let end_offset = if end_row + 1 < row_count {
        text::Point::new(end_row + 1, 0).to_offset(text_buffer)
    } else {
        text::Point::new(end_row, text_buffer.line_len(end_row)).to_offset(text_buffer)
    };

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
                    edit: Edit::replace(
                        TextRange {
                            start: ByteOffset(start_offset),
                            end: ByteOffset(end_offset),
                        },
                        result,
                    ),
                }],
                selections: Some(selections_before),
                join_previous: false,
            },
        )
        .expect("joining range-derived lines is well-formed")
    };

    set_cursor_to_row_first_non_blank(editor, window_id, buffer_id, start_row);
    Outcome::from_mutation(&mutation)
}

fn execute_insert_text_at_line(
    editor: &mut Editor,
    ctx: CommandContext,
    target_line: u32,
    text_to_insert: &str,
) -> Outcome {
    let buffer_id = ctx.buffer;
    let window_id = ctx.window;
    let (max_row, text_with_nl, insert_offset) = {
        let buffer = match editor.buffer(buffer_id) {
            Some(b) => b,
            None => return Outcome::default(),
        };
        let text_buffer = buffer.as_text_buffer();
        let row_count = text_buffer.row_count();
        let max_row = row_count.saturating_sub(1);

        let formatted = if text_to_insert.ends_with('\n') {
            text_to_insert.to_string()
        } else {
            text_to_insert.to_string() + "\n"
        };

        let (text_with_nl, insert_offset) = if target_line == 0 {
            (formatted, 0)
        } else {
            let target_row = (target_line - 1).min(max_row);
            let offset =
                text::Point::new(target_row, text_buffer.line_len(target_row)).to_offset(text_buffer);
            (
                "\n".to_string() + formatted.trim_end_matches('\n'),
                offset,
            )
        };

        (max_row, text_with_nl, insert_offset)
    };

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
                    edit: Edit::insert(ByteOffset(insert_offset), text_with_nl),
                }],
                selections: Some(selections_before),
                join_previous: false,
            },
        )
        .expect("inserting text at target line is well-formed")
    };

    let new_row = if target_line == 0 {
        0
    } else {
        (target_line - 1).min(max_row) + 1
    };
    set_cursor_to_row_first_non_blank(editor, window_id, buffer_id, new_row);

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

#[cfg(test)]
mod tests {
    use super::*;

    fn buf_text(editor: &Editor, buf_id: vim_buffer::BufferId) -> String {
        let buf = editor.buffer(buf_id).unwrap();
        let tb = buf.as_text_buffer();
        (0..tb.row_count())
            .map(|r| tb.row_text(r))
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn test_ex_copy_and_move() {
        let mut editor = Editor::new("line1\nline2\nline3\nline4");
        let ctx = editor.current_context();

        // :1,2copy 4 -> copies line 1..2 to after line 4
        admit(&mut editor, ctx, "1,2copy 4");
        assert_eq!(
            buf_text(&editor, ctx.buffer),
            "line1\nline2\nline3\nline4\nline1\nline2"
        );

        // :1,2move 6 -> moves line 1..2 to after line 6
        admit(&mut editor, ctx, "1,2move 6");
        assert_eq!(
            buf_text(&editor, ctx.buffer),
            "line3\nline4\nline1\nline2\nline1\nline2"
        );
    }

    #[test]
    fn test_ex_yank_and_put() {
        let mut editor = Editor::new("apple\nbanana\ncherry");
        let ctx = editor.current_context();

        // :2yank a -> yank "banana\n" into register a
        admit(&mut editor, ctx, "2yank a");
        let (reg_text, _) = editor
            .registers()
            .get(crate::kernel::buffer::registers::RegisterName::Named('a'))
            .map(|r| (r.text.clone(), r.kind))
            .unwrap();
        assert_eq!(reg_text, "banana\n");

        // :1put a -> put register a after line 1
        admit(&mut editor, ctx, "1put a");
        assert_eq!(
            buf_text(&editor, ctx.buffer),
            "apple\nbanana\nbanana\ncherry"
        );
    }

    #[test]
    fn test_ex_join() {
        let mut editor = Editor::new("hello\nworld\nfoo");
        let ctx = editor.current_context();

        // :1,2join -> joins line 1 and 2 with space
        admit(&mut editor, ctx, "1,2join");
        assert_eq!(buf_text(&editor, ctx.buffer), "hello world\nfoo");
    }

    #[test]
    fn test_ex_file_and_pwd() {
        let mut editor = Editor::new("sample content");
        let ctx = editor.current_context();

        // :file new_name.txt
        let outcome = admit(&mut editor, ctx, "file new_name.txt");
        assert!(!outcome.effects.is_empty());
        let buf = editor.buffer(ctx.buffer).unwrap();
        assert_eq!(buf.path().unwrap().to_str().unwrap(), "new_name.txt");

        // :pwd
        let outcome = admit(&mut editor, ctx, "pwd");
        assert!(!outcome.effects.is_empty());
    }

    #[test]
    fn test_ex_tab_commands() {
        let mut editor = Editor::new("tab1 content");
        let ctx = editor.current_context();

        assert_eq!(editor.tabs().len(), 1);

        // :tabnew
        admit(&mut editor, ctx, "tabnew");
        assert_eq!(editor.tabs().len(), 2);

        // :tabprevious
        let ctx2 = editor.current_context();
        admit(&mut editor, ctx2, "tabprevious");

        // :tabclose
        let ctx3 = editor.current_context();
        admit(&mut editor, ctx3, "tabclose");
        assert_eq!(editor.tabs().len(), 1);
    }

    #[test]
    fn test_ex_nohlsearch() {
        let mut editor = Editor::new("search test");
        let ctx = editor.current_context();
        let outcome = admit(&mut editor, ctx, "nohlsearch");
        assert_eq!(outcome.invalidation, RedrawInvalidation::All);
    }

    #[test]
    fn test_ex_quit_behavior() {
        let mut editor = Editor::new("line1\nline2");
        let ctx = editor.current_context();

        // Mutate buffer to make it modified
        editor.execute(Action::DeleteChar { count: 1 });
        assert!(editor.buffer(ctx.buffer).unwrap().is_modified());

        // :q without ! when modified should fail with E37
        let outcome = admit(&mut editor, ctx, "quit");
        assert_ne!(outcome.effects, vec![Effect::Quit]);
        assert!(!outcome.effects.is_empty());
        if let Effect::OptionMessage { message } = &outcome.effects[0] {
            assert!(message.contains("E37"));
        } else {
            panic!("Expected E37 option message effect");
        }

        // :q! with modified buffer should quit (when 1 window, 1 tab, 1 buf)
        let outcome = admit(&mut editor, ctx, "quit!");
        assert_eq!(outcome.effects, vec![Effect::Quit]);

        // Test :q with multiple windows (splits)
        let mut editor = Editor::new("split test");
        let ctx = editor.current_context();
        super::super::normal::windows::split_horizontal(&mut editor, ctx);
        assert_eq!(editor.tabs().active().layout().window_ids().len(), 2);

        let ctx_split = editor.current_context();
        let outcome = admit(&mut editor, ctx_split, "quit");
        assert_ne!(outcome.effects, vec![Effect::Quit]);
        assert_eq!(editor.tabs().active().layout().window_ids().len(), 1);

        // Test :q with multiple buffers
        let mut editor = Editor::new("buf1");
        let buf2 = editor.buffers_mut().insert("buf2");
        let ctx = editor.current_context();
        let _ = editor.buffers_mut().set_current(buf2);
        editor.set_window_buffer(ctx.window, buf2);
        editor.set_current_window(ctx.window);
        let ctx_buf2 = editor.current_context();

        let outcome = admit(&mut editor, ctx_buf2, "quit");
        assert_ne!(outcome.effects, vec![Effect::Quit]);
        let active_count = editor
            .buffers_mut()
            .list()
            .into_iter()
            .filter(|&id| {
                editor
                    .buffer(id)
                    .map(|b| b.lifecycle() != vim_buffer::BufferLifecycle::Deleted)
                    .unwrap_or(false)
            })
            .count();
        assert_eq!(active_count, 1);
    }
}
