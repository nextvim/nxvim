use text::{Point, ToOffset};
use vim_input::{Action, Mode};
use vim_regex::Regex;
use vim_ui::{Ui, WindowId};

use crate::app::command::{AppCommand, ScriptRequest};
use crate::app::input::InputAdapter;
use crate::app::ui::ViewIds;
use crate::app::windows::WindowOps;
use crate::model::EditorModel;

use crate::app::outcome::CommandOutcome;
use crate::app::ui::ViewEffect;

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
            && matches!(
                action,
                Action::MoveUp { .. } | Action::MoveDown { .. } | Action::DeleteCharBefore { .. }
            ))
    }

    pub fn execute(
        ui: &mut Ui,
        model: &mut EditorModel,
        input: &mut InputAdapter,
        command_queue: &mut std::collections::VecDeque<crate::app::command::AppCommand>,
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
                if mode_before == Mode::Normal
                    && matches!(action, Action::SetToCommandSearchForward)
                {
                    let _ = WindowOps::edit_window(
                        ui,
                        model,
                        active_window,
                        |buffer, _context, window_state| {
                            selection_text = window_state.selections.text(buffer.as_text_buffer());
                        },
                    );
                }

                model.commandline_mode = mode_char;
                model.history_index = None;
                model.history_temp.clear();
                if mode_char == '/' || mode_char == '?' {
                    model.search_range = None;
                    model.substitute_text = None;
                    if !selection_text.is_empty() {
                        let pattern = format!("\\<{selection_text}\\>");
                        model.search_pattern = Some(pattern);
                        if let Some(ref pattern) = model.search_pattern {
                            model.search_regex =
                                Regex::compile(pattern, vim_regex::CompileOptions::default()).ok();
                        }
                    } else {
                        model.search_pattern = None;
                        model.search_regex = None;
                    }
                }
                let _ = model.kernel_mut().transition_mode(Mode::Insert);
                input.set_mode(model.kernel().mode());
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
                        let content = if !selection_text.is_empty() {
                            format!("\\<{selection_text}\\>")
                        } else {
                            "".to_string()
                        };
                        let _ = crate::kernel::transaction(
                            buffer,
                            vim_buffer::EditOrigin::VimScript,
                            None,
                            |tx| tx.replace(None, range, content.as_str()),
                        );

                        window_state.selections.selections.clear();
                        window_state
                            .selections
                            .add(buffer.as_text_buffer(), content.len());
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
                model.search_pattern = None;
                model.search_regex = None;
                model.search_range = None;
                model.substitute_text = None;
                CommandOutcome::with_effect(ViewEffect::Focus(Self::editor_focus(ui, view_ids)))
            }
            Action::DeleteCharBefore { .. }
                if active_window == view_ids.commandline
                    && Self::get_commandline_text(model).is_some_and(|text| text.is_empty()) =>
            {
                let _ = model.kernel_mut().transition_mode(Mode::Normal);
                input.set_mode(model.kernel().mode());
                model.search_pattern = None;
                model.search_regex = None;
                model.search_range = None;
                model.substitute_text = None;
                CommandOutcome::with_effect(ViewEffect::Focus(Self::editor_focus(ui, view_ids)))
            }
            Action::InsertNewLine { .. }
                if active_window == view_ids.commandline
                    && WindowOps::window_buffer(ui, active_window)
                        == Some(model.commandline_buffer()) =>
            {
                let _ = model.kernel_mut().transition_mode(Mode::Normal);
                input.set_mode(model.kernel().mode());
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
                        model.search_range = None;
                        model.substitute_text = None;
                    } else if model.commandline_mode == '/' || model.commandline_mode == '?' {
                        model.search_regex =
                            Regex::compile(&command, vim_regex::CompileOptions::default()).ok();
                        model.search_pattern = Some(command.clone());
                        model.search_range = None;
                        model.substitute_text = None;
                    }

                    let command_to_execute = if command.starts_with(':') {
                        command
                    } else {
                        format!("{}{}", model.commandline_mode, command)
                    };

                    let target_window = Self::editor_focus(ui, view_ids);
                    let target_context = model.kernel().current().and_then(|current| {
                        WindowOps::window_buffer(ui, target_window).map(|buffer| {
                            crate::kernel::EditorContext {
                                tab: current.tab,
                                window: target_window,
                                buffer,
                            }
                        })
                    });
                    match target_context
                        .ok_or_else(|| "No editor context for command-line request".to_string())
                        .and_then(|current| {
                            crate::kernel::CommandLineRequest::parse(current, command_to_execute)
                        }) {
                        Ok(request) => command_queue
                            .push_back(AppCommand::Script(ScriptRequest::CommandLine(request))),
                        Err(err) => model.status = Some(err),
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
                            model.search_regex =
                                Regex::compile(&text, vim_regex::CompileOptions::default()).ok();
                            model.search_range = None;
                            model.substitute_text = None;
                        }
                    }
                }
                CommandOutcome::window_redraw(
                    view_ids.commandline,
                    crate::kernel::RedrawInvalidationKind::TextRows,
                )
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
                            model.search_regex =
                                Regex::compile(&text, vim_regex::CompileOptions::default()).ok();
                            model.search_range = None;
                            model.substitute_text = None;
                        }
                    } else {
                        model.history_index = None;
                        let text = model.history_temp.clone();
                        Self::set_commandline_text(ui, model, view_ids.commandline, &text);
                        if model.commandline_mode == '/' || model.commandline_mode == '?' {
                            model.search_range = None;
                            model.substitute_text = None;
                            if text.is_empty() {
                                model.search_pattern = None;
                                model.search_regex = None;
                            } else {
                                model.search_pattern = Some(text.clone());
                                model.search_regex =
                                    Regex::compile(&text, vim_regex::CompileOptions::default())
                                        .ok();
                            }
                        }
                    }
                }
                CommandOutcome::window_redraw(
                    view_ids.commandline,
                    crate::kernel::RedrawInvalidationKind::TextRows,
                )
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

    fn current_command(_ui: &Ui, model: &EditorModel, _commandline_id: WindowId) -> Option<String> {
        let buffer_id = model.commandline_buffer();
        let buffer = model.get_buffer(buffer_id).ok()?;
        let text_buffer = buffer.as_text_buffer();
        let rope = text_buffer.as_rope();
        let text: String = rope.chunks_in_range(0..rope.len()).collect();
        Some(text.replace('\n', ""))
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
                let _ = crate::kernel::transaction(
                    buffer,
                    vim_buffer::EditOrigin::VimScript,
                    None,
                    |tx| tx.replace(None, range, text),
                );

                let text_len = buffer.as_text_buffer().len();
                window_state.selections.selections.clear();
                window_state
                    .selections
                    .add(buffer.as_text_buffer(), text_len);
            },
        );
    }
}
