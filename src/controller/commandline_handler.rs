use text::{Point, ToOffset, ToPoint};
use vim_input::{Action, Mode};
use vim_regex::Regex;
use vim_ui::{Ui, WindowId};

use crate::app::ui::ViewIds;
use crate::app::windows::WindowOps;
use crate::controller::input::InputController;
use crate::model::EditorModel;
use crate::script::ScriptRuntime;

use super::command::{CommandOutcome, ViewEffect};

pub struct CommandlineHandler;

impl CommandlineHandler {
    pub fn handles(active_window: WindowId, commandline_window: WindowId, action: &Action) -> bool {
        matches!(
            action,
            Action::SetToCommand
                | Action::SetToCommandSearchForward
                | Action::SetToCommandSearchBackward
                | Action::Clear
                | Action::InsertNewLine { .. }
        ) || (active_window == commandline_window
            && matches!(action, Action::MoveUp { .. } | Action::MoveDown { .. }))
    }

    pub fn execute(
        ui: &mut Ui,
        model: &mut EditorModel,
        input: &mut InputController,
        script: &mut ScriptRuntime,
        view_ids: ViewIds,
        active_window: WindowId,
        action: &Action,
        mode_before: Mode,
    ) -> CommandOutcome {
        match action {
            Action::SetToCommand
            | Action::SetToCommandSearchForward
            | Action::SetToCommandSearchBackward => {
                let mode_char = match action {
                    Action::SetToCommand => ':',
                    Action::SetToCommandSearchForward => '/',
                    Action::SetToCommandSearchBackward => '?',
                    _ => unreachable!(),
                };

                let mut selection_text = String::new();
                if mode_before == Mode::Normal && matches!(action, Action::SetToCommandSearchForward) {
                    let _ = WindowOps::edit_window(
                        ui,
                        model,
                        active_window,
                        |buffer, _context, window_state| {
                            selection_text = window_state.selections.text(buffer.as_text_buffer());
                        }
                    );
                }

                model.commandline_mode = mode_char;
                model.history_index = None;
                model.history_temp.clear();
                if mode_char == '/' || mode_char == '?' {
                    if !selection_text.is_empty() {
                        let pattern = format!("\\<{selection_text}\\>");
                        model.search_pattern = Some(pattern);
                        if let Some(ref pattern) = model.search_pattern {
                            model.search_regex = Regex::compile(pattern, vim_regex::CompileOptions::default()).ok();
                        }
                    } else {
                        model.search_pattern = None;
                        model.search_regex = None;
                    }
                }
                input.set_mode(Mode::Insert);
                let _ = WindowOps::edit_window(
                    ui,
                    model,
                    view_ids.commandline,
                    |buffer, _context, window_state| {
                        let len = buffer.as_text_buffer().len();
                        let range = vim_buffer::TextRange::new(
                            vim_buffer::ByteOffset(0),
                            vim_buffer::ByteOffset(len),
                        )
                        .unwrap();
                        let mut tx = buffer.transaction(vim_buffer::EditOrigin::VimScript);
                        
                        let content = if !selection_text.is_empty() {
                            format!("\\<{selection_text}\\>")
                        } else {
                            "".to_string()
                        };
                        tx.replace(None, range, content.as_str());
                        let _ = tx.commit(None);

                        window_state.selections.selections.clear();
                        window_state.selections.add(buffer.as_text_buffer(), content.len());
                    },
                );
                let mut outcome =
                    CommandOutcome::with_effect(ViewEffect::Focus(view_ids.commandline));
                outcome
                    .view_effects
                    .push(ViewEffect::SetCommandLineMode(mode_char));
                outcome
            }
            Action::Clear if active_window == view_ids.commandline => {
                CommandOutcome::with_effect(ViewEffect::Focus(Self::editor_focus(ui, view_ids)))
            }
            Action::InsertNewLine { .. }
                if active_window == view_ids.commandline
                    && WindowOps::window_buffer(ui, active_window)
                        == Some(model.commandline_buffer()) =>
            {
                input.set_mode(Mode::Normal);
                if let Some(command) = Self::current_command(ui, model, active_window) {
                    if !command.is_empty() {
                        if model.commandline_mode == '/' || model.commandline_mode == '?' {
                            if model.search_history.last() != Some(&command) {
                                model.search_history.push(command.clone());
                            }
                        } else {
                            if model.command_history.last() != Some(&command) {
                                model.command_history.push(command.clone());
                            }
                        }
                    }
                    if command.starts_with('/') || command.starts_with('?') {
                        let pattern = command[1..].to_string();
                        model.search_regex =
                            Regex::compile(&pattern, vim_regex::CompileOptions::default()).ok();
                        model.search_pattern = Some(pattern);
                    } else if model.commandline_mode == '/' || model.commandline_mode == '?' {
                        model.search_regex =
                            Regex::compile(&command, vim_regex::CompileOptions::default()).ok();
                        model.search_pattern = Some(command.clone());
                    }
                    
                    let cmd_to_execute = if command.starts_with(':')
                        || command.starts_with('/')
                        || command.starts_with('?')
                    {
                        command.chars().skip(1).collect::<String>()
                    } else {
                        format!("{}{}", model.commandline_mode, command)
                    };

                    if let Err(error) = script.execute(&cmd_to_execute) {
                        model.status = Some(error);
                    }
                }
                CommandOutcome::with_effect(ViewEffect::Focus(Self::editor_focus(ui, view_ids)))
            }
            Action::MoveUp { .. } if active_window == view_ids.commandline => {
                let history = if model.commandline_mode == '/' || model.commandline_mode == '?' {
                    &model.search_history
                } else {
                    &model.command_history
                };
                if !history.is_empty() {
                    let next_idx = match model.history_index {
                        None => {
                            if let Some(text) = Self::get_commandline_text(model) {
                                model.history_temp = text;
                            }
                            Some(history.len() - 1)
                        }
                        Some(idx) => {
                            if idx > 0 {
                                Some(idx - 1)
                            } else {
                                Some(0)
                            }
                        }
                    };
                    if let Some(idx) = next_idx {
                        model.history_index = Some(idx);
                        let text = history[idx].clone();
                        Self::set_commandline_text(ui, model, view_ids.commandline, &text);
                        if model.commandline_mode == '/' || model.commandline_mode == '?' {
                            model.search_pattern = Some(text.clone());
                            model.search_regex = Regex::compile(&text, vim_regex::CompileOptions::default()).ok();
                        }
                    }
                }
                CommandOutcome::redraw()
            }
            Action::MoveDown { .. } if active_window == view_ids.commandline => {
                let history = if model.commandline_mode == '/' || model.commandline_mode == '?' {
                    &model.search_history
                } else {
                    &model.command_history
                };
                if let Some(idx) = model.history_index {
                    if idx + 1 < history.len() {
                        model.history_index = Some(idx + 1);
                        let text = history[idx + 1].clone();
                        Self::set_commandline_text(ui, model, view_ids.commandline, &text);
                        if model.commandline_mode == '/' || model.commandline_mode == '?' {
                            model.search_pattern = Some(text.clone());
                            model.search_regex = Regex::compile(&text, vim_regex::CompileOptions::default()).ok();
                        }
                    } else {
                        model.history_index = None;
                        let text = model.history_temp.clone();
                        Self::set_commandline_text(ui, model, view_ids.commandline, &text);
                        if model.commandline_mode == '/' || model.commandline_mode == '?' {
                            if text.is_empty() {
                                model.search_pattern = None;
                                model.search_regex = None;
                            } else {
                                model.search_pattern = Some(text.clone());
                                model.search_regex = Regex::compile(&text, vim_regex::CompileOptions::default()).ok();
                            }
                        }
                    }
                }
                CommandOutcome::redraw()
            }
            _ => CommandOutcome::default(),
        }
    }

    fn editor_focus(ui: &Ui, view_ids: ViewIds) -> WindowId {
        ui.focus_manager()
            .previous_id()
            .filter(|&id| {
                id != view_ids.commandline && ui.window(id).is_some_and(vim_ui::Window::has_content)
            })
            .unwrap_or(view_ids.main)
    }

    fn current_command(ui: &Ui, model: &EditorModel, commandline_id: WindowId) -> Option<String> {
        let buffer_id = model.commandline_buffer();
        let window = ui
            .window(commandline_id)
            .and_then(vim_ui::Window::window_state)?;
        let buffer = model.get_buffer(buffer_id).ok()?;
        let current_row = window
            .selections
            .first()?
            .head()
            .to_point(buffer.as_text_buffer())
            .row;
        let target_row = current_row.checked_sub(1)?;
        let text_buffer = buffer.as_text_buffer();
        let start = Point::new(target_row, 0).to_offset(text_buffer);
        let end = Point::new(target_row, text_buffer.line_len(target_row)).to_offset(text_buffer);
        Some(text_buffer.as_rope().chunks_in_range(start..end).collect())
    }

    pub fn get_commandline_text(model: &EditorModel) -> Option<String> {
        let buffer_id = model.commandline_buffer();
        let buffer = model.get_buffer(buffer_id).ok()?;
        let text_buffer = buffer.as_text_buffer();
        if text_buffer.row_count() > 0 {
            let start = Point::new(0, 0).to_offset(text_buffer);
            let end = Point::new(0, text_buffer.line_len(0)).to_offset(text_buffer);
            Some(text_buffer.as_rope().chunks_in_range(start..end).collect())
        } else {
            Some(String::new())
        }
    }

    fn set_commandline_text(
        ui: &mut Ui,
        model: &mut EditorModel,
        commandline_id: WindowId,
        text: &str,
    ) {
        let _ = WindowOps::edit_window(
            ui,
            model,
            commandline_id,
            |buffer, _context, window_state| {
                let len = buffer.as_text_buffer().len();
                let range = vim_buffer::TextRange::new(
                    vim_buffer::ByteOffset(0),
                    vim_buffer::ByteOffset(len),
                )
                .unwrap();
                let mut tx = buffer.transaction(vim_buffer::EditOrigin::VimScript);
                tx.replace(None, range, text);
                let _ = tx.commit(None);

                let text_len = buffer.as_text_buffer().len();
                window_state.selections.selections.clear();
                window_state.selections.add(buffer.as_text_buffer(), text_len);
            },
        );
    }
}
