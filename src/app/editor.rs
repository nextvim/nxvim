//! App-owned dispatch for editor semantic action and range commands.

use crate::app::App;
use crate::app::buffer_handler::BufferHandler;
use crate::app::command::ExCommand as Command;
use crate::app::command::{AppCommand, ScriptRequest, SemanticRequest};
use crate::app::commandline_handler::CommandlineHandler;
use crate::app::editor_handler::EditorHandler;
use crate::app::lifecycle_ops::LifecycleHandler;
use crate::app::outcome::CommandOutcome;
use crate::app::range_ops::{RangeCommandHandler, RangeOperation};
use crate::app::window_handler::WindowHandler;

pub fn dispatch(
    app: &mut App,
    command: SemanticRequest,
) -> Result<CommandOutcome, SemanticRequest> {
    match command {
        SemanticRequest::RangeOp {
            operation,
            bang,
            range,
            count,
            register,
        } => Ok(RangeCommandHandler::execute(
            app, operation, bang, range, count, register,
        )),
        SemanticRequest::ReplaceBuffer { .. } => {
            app.model.status = Some("Typed host mutation requires the script host boundary".into());
            Ok(CommandOutcome::statusline())
        }
        SemanticRequest::Editor { action, register } => {
            let active_window = app.ui.focused_window_id();

            app.model.status = None;

            match &action {
                vim_input::Action::NextTab { count } => {
                    if let Err(error) = app.next_tab(*count as usize) {
                        app.model.status = Some(error);
                    }
                    return Ok(CommandOutcome::redraw());
                }
                vim_input::Action::PreviousTab { count } => {
                    if let Err(error) = app.previous_tab(*count as usize) {
                        app.model.status = Some(error);
                    }
                    return Ok(CommandOutcome::redraw());
                }
                vim_input::Action::BeginMacro { register } => {
                    if let Err(error) = app
                        .model
                        .kernel_mut()
                        .begin_macro_recording(register.clone())
                    {
                        app.model.status = Some(error.to_string());
                        return Ok(CommandOutcome::statusline());
                    }
                    app.services.macros.begin(register.clone());
                    app.input.set_in_recording(true);
                    app.model.status = Some(format!("recording @{register}"));
                    return Ok(CommandOutcome::statusline());
                }
                vim_input::Action::EndMacro => {
                    let Some(register) = app.model.kernel_mut().end_macro_recording() else {
                        app.model.status = Some("macro recording is not active".to_string());
                        return Ok(CommandOutcome::statusline());
                    };
                    app.services.macros.end();
                    app.input.set_in_recording(false);
                    app.model.status = Some(format!("macro @{register} recorded"));
                    return Ok(CommandOutcome::statusline());
                }
                vim_input::Action::ReplayMacro { count, register } => {
                    let (register, count) = match app
                        .model
                        .kernel_mut()
                        .request_macro_replay(register, *count)
                    {
                        Ok(request) => request,
                        Err(error) => {
                            app.model.status = Some(error.to_string());
                            return Ok(CommandOutcome::statusline());
                        }
                    };
                    let replay_actions = app.services.macros.replay(&register, count);
                    for rec in replay_actions {
                        app.command_queue.push_back(AppCommand::Semantic(
                            SemanticRequest::Editor {
                                action: rec.action,
                                register: rec.register,
                            },
                        ));
                    }
                    return Ok(CommandOutcome::redraw());
                }
                vim_input::Action::Script { count, script } => {
                    for _ in 0..*count {
                        app.command_queue
                            .push_back(AppCommand::Script(ScriptRequest::Execute(script.clone())));
                    }
                    return Ok(CommandOutcome::redraw());
                }
                vim_input::Action::KeySequence { count, keys } => {
                    let seq = match vim_input::KeySequence::parse(keys) {
                        Ok(s) => s,
                        Err(err) => {
                            app.model.status = Some(format!("Key sequence error: {err}"));
                            return Ok(CommandOutcome::statusline());
                        }
                    };
                    for _ in 0..*count {
                        for item in &seq.items {
                            if let vim_input::KeyPattern::Exact(key) = item {
                                if let Some(cmd) = app.input.feed_key(*key) {
                                    app.command_queue.push_back(cmd);
                                }
                            }
                        }
                    }
                    return Ok(CommandOutcome::redraw());
                }
                vim_input::Action::Sequence { count, actions } => {
                    for _ in 0..*count {
                        for act in actions {
                            app.command_queue.push_back(AppCommand::Semantic(
                                SemanticRequest::Editor {
                                    action: (**act).clone(),
                                    register,
                                },
                            ));
                        }
                    }
                    return Ok(CommandOutcome::redraw());
                }

                vim_input::Action::Repeat { count } => {
                    if let Some(actions) = app.model.kernel().repeat_actions() {
                        for _ in 0..*count {
                            for act in actions {
                                app.command_queue.push_back(AppCommand::Semantic(
                                    SemanticRequest::Editor {
                                        action: act.clone(),
                                        register: None,
                                    },
                                ));
                            }
                        }
                    }
                    return Ok(CommandOutcome::redraw());
                }
                _ => {
                    if app.model.kernel().recording_target().is_some() {
                        app.services.macros.record(action.clone(), register);
                    }
                }
            }

            let mode_before = app.input.mode();

            let Some(command_context) = app
                .model
                .kernel()
                .command_context(crate::kernel::CommandKind::Edit)
            else {
                app.model.status = Some("No current editor context".to_string());
                return Ok(CommandOutcome::statusline());
            };

            let mut outcome = EditorHandler::execute(
                &mut app.ui,
                &mut app.model,
                &mut app.input,
                &mut app.services,
                active_window,
                &action,
                register,
                &command_context,
            );

            let mode_after = app.input.mode();
            let is_repeat = matches!(action, vim_input::Action::Repeat { .. });
            if !is_repeat {
                let is_modifying = matches!(
                    action,
                    vim_input::Action::Delete { .. }
                        | vim_input::Action::DeleteChar { .. }
                        | vim_input::Action::DeleteCharBefore { .. }
                        | vim_input::Action::DeleteLine { .. }
                        | vim_input::Action::DeleteLines { .. }
                        | vim_input::Action::DeleteMotion { .. }
                        | vim_input::Action::Change { .. }
                        | vim_input::Action::ChangeLine { .. }
                        | vim_input::Action::ChangeMotion { .. }
                        | vim_input::Action::Put { .. }
                        | vim_input::Action::PutLines { .. }
                        | vim_input::Action::JoinLines { .. }
                        | vim_input::Action::InsertText { .. }
                        | vim_input::Action::InsertNewLine { .. }
                        | vim_input::Action::InsertNewLineMotion { .. }
                        | vim_input::Action::InsertTab
                        | vim_input::Action::SetToOpenLineBelow { .. }
                        | vim_input::Action::SetToOpenLineAbove { .. }
                        | vim_input::Action::SetToInsert
                        | vim_input::Action::SetToReplace
                        | vim_input::Action::SetToVirtualReplace
                        | vim_input::Action::SetToAppend
                        | vim_input::Action::SetToAppendEndOfLine
                        | vim_input::Action::SetToInsertStartOfLineNonSpace
                );

                if is_modifying {
                    let is_insert_entering = matches!(
                        action,
                        vim_input::Action::SetToInsert
                            | vim_input::Action::SetToReplace
                            | vim_input::Action::SetToVirtualReplace
                            | vim_input::Action::SetToAppend
                            | vim_input::Action::SetToAppendEndOfLine
                            | vim_input::Action::SetToInsertStartOfLineNonSpace
                            | vim_input::Action::SetToOpenLineBelow { .. }
                            | vim_input::Action::SetToOpenLineAbove { .. }
                            | vim_input::Action::Change { .. }
                            | vim_input::Action::ChangeLine { .. }
                            | vim_input::Action::ChangeMotion { .. }
                    );

                    if mode_before == vim_input::Mode::Normal || mode_before.is_visual() {
                        if is_insert_entering {
                            app.model
                                .kernel_mut()
                                .begin_repeat_recording(action.clone());
                        } else {
                            app.model
                                .kernel_mut()
                                .set_repeat_actions(vec![action.clone()]);
                        }
                    } else if mode_before.is_insert() {
                        app.model
                            .kernel_mut()
                            .append_repeat_recording(action.clone());
                    }
                } else if mode_before.is_insert() {
                    app.model
                        .kernel_mut()
                        .append_repeat_recording(action.clone());
                }
            }

            if mode_after == vim_input::Mode::Normal {
                app.model.kernel_mut().finish_repeat_recording();
            }

            if BufferHandler::handles(&action) {
                outcome.merge(BufferHandler::execute(
                    &mut app.ui,
                    &mut app.model,
                    active_window,
                    &action,
                ));
            }
            if WindowHandler::handles(&action) {
                outcome.merge(WindowHandler::execute(active_window, &action));
            }
            if CommandlineHandler::handles(active_window, app.view_ids.commandline, &action) {
                outcome.merge(CommandlineHandler::execute(
                    &mut app.ui,
                    &mut app.model,
                    &mut app.input,
                    &mut app.command_queue,
                    app.view_ids,
                    active_window,
                    &action,
                    mode_before,
                ));
            }
            if LifecycleHandler::handles(&action) {
                outcome.merge(LifecycleHandler::execute(
                    &mut app.ui,
                    &mut app.model,
                    active_window,
                    &action,
                ));
            }

            let current_focused = app.ui.focused_window_id();
            if crate::app::windows::WindowOps::window_buffer(&app.ui, current_focused)
                == Some(app.model.commandline_buffer())
            {
                if let Some(window) = app
                    .ui
                    .window(active_window)
                    .and_then(vim_ui::Window::window_state)
                {
                    if let Ok(buffer) = app.model.get_buffer(app.model.commandline_buffer()) {
                        if let Some(selection) = window.selections.first() {
                            use text::ToOffset;
                            use text::ToPoint;
                            let text_buffer = buffer.as_text_buffer();
                            let current_row = selection.head().to_point(text_buffer).row;
                            let start = text::Point::new(current_row, 0).to_offset(text_buffer);
                            let end =
                                text::Point::new(current_row, text_buffer.line_len(current_row))
                                    .to_offset(text_buffer);
                            let raw_pattern: String =
                                text_buffer.as_rope().chunks_in_range(start..end).collect();

                            let command_line =
                                format!("{}{}", app.model.commandline_mode, raw_pattern);
                            let mut runtime = crate::script::ScriptRuntime::new();
                            if let Ok(cmd) = runtime.peek_command(&command_line) {
                                match cmd {
                                    Command::SearchForward { pattern }
                                    | Command::SearchBackward { pattern } => {
                                        if pattern.is_empty() {
                                            app.model.search_pattern = None;
                                            app.model.search_regex = None;
                                        } else {
                                            app.model.search_regex = vim_regex::Regex::compile(
                                                &pattern,
                                                vim_regex::CompileOptions::default(),
                                            )
                                            .ok();
                                            app.model.search_pattern = Some(pattern);
                                        }
                                        app.model.search_range = None;
                                        app.model.substitute_text = None;
                                    }
                                    Command::Substitute {
                                        pattern,
                                        substitute_text,
                                        range,
                                        ..
                                    } => {
                                        if pattern.is_empty() {
                                            app.model.search_pattern = None;
                                            app.model.search_regex = None;
                                        } else {
                                            app.model.search_regex = vim_regex::Regex::compile(
                                                &pattern,
                                                vim_regex::CompileOptions::default(),
                                            )
                                            .ok();
                                            app.model.search_pattern = Some(pattern);
                                        }
                                        app.model.search_range = range;
                                        app.model.substitute_text = Some(substitute_text);
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
            } else {
                // clearing required
                if app.model.substitute_text.is_some() {
                    app.model.search_pattern = None;
                    app.model.search_regex = None;
                    app.model.search_range = None;
                    app.model.substitute_text = None;
                    return Ok(CommandOutcome::redraw());
                }
            }

            Ok(outcome)
        }
        command => Err(command),
    }
}
