use crate::app::App;

use super::buffer_handler::BufferHandler;
use super::command::{Command, CommandOutcome};
use super::commandline_handler::CommandlineHandler;
use super::editor_handler::EditorHandler;
use super::lifecycle_handler::LifecycleHandler;
use super::range::RangeCommandHandler;
use super::shared_operations::SharedOperations;
use super::task_dispatcher::TaskDispatcher;
use super::window_handler::WindowHandler;

/// Formats the standard `[Mode] Action: ...` status message shared by
/// resolved editor actions and resolved range commands.
pub(super) fn describe_action(mode: vim_input::Mode, action: &vim_input::Action) -> String {
    format!("[{mode:?}] Action: {action:?}")
}

pub struct Dispatcher;

impl Dispatcher {
    pub fn dispatch(app: &mut App, command: Command) -> CommandOutcome {
        match command {
            Command::PendingInput(sequence) => {
                app.model.status = Some(format!("Pending sequence: {sequence}"));
                CommandOutcome::redraw()
            }
            Command::InvalidInput => {
                app.model.status = Some("Invalid sequence".to_string());
                CommandOutcome::redraw()
            }
            Command::Save { path, force } => {
                let active_window = app.ui.focused_window_id();
                SharedOperations::write(
                    &mut app.ui,
                    &mut app.model,
                    active_window,
                    path.as_deref(),
                    force,
                )
            }
            Command::Quit { force } => {
                let active_window = app.ui.focused_window_id();
                LifecycleHandler::quit(&mut app.ui, &mut app.model, active_window, force)
            }
            Command::Edit { path, force } => {
                let active_window = app.ui.focused_window_id();
                LifecycleHandler::edit(
                    &mut app.ui,
                    &mut app.model,
                    active_window,
                    path.as_deref(),
                    force,
                )
            }
            Command::WriteQuit { path, force } => {
                let active_window = app.ui.focused_window_id();
                LifecycleHandler::write_and_quit(
                    &mut app.ui,
                    &mut app.model,
                    active_window,
                    path.as_deref(),
                    force,
                )
            }
            Command::WriteQuitAll { force } => {
                let active_window = app.ui.focused_window_id();
                LifecycleHandler::write_and_quit_all(
                    &mut app.ui,
                    &mut app.model,
                    active_window,
                    force,
                )
            }
            Command::Task(result) => TaskDispatcher::dispatch(
                &mut app.ui,
                &mut app.model,
                &mut app.services.treesitter,
                result,
            ),
            Command::ClearSearchHighlight => {
                LifecycleHandler::clear_search_highlight(&mut app.model)
            }
            Command::Colorscheme { name } => {
                LifecycleHandler::colorscheme(
                    &mut app.ui,
                    &mut app.model,
                    &mut app.colorscheme,
                    &mut app.highlighter,
                    name.as_deref(),
                )
            }
            Command::Set { arguments } => {
                let active_window = app.ui.focused_window_id();
                let buffer_id = crate::app::windows::WindowOps::window_buffer(&app.ui, active_window);
                match app.config.execute_set_command(&arguments, buffer_id, Some(active_window)) {
                    Ok(Some(msg)) => {
                        app.model.status = Some(msg);
                    }
                    Ok(None) => {}
                    Err(err) => {
                        app.model.status = Some(format!("Error: {}", err));
                    }
                }
                CommandOutcome::redraw()
            }
            Command::RangeOp {
                operation,
                bang,
                range,
                count,
                register,
            } => RangeCommandHandler::execute(app, operation, bang, range, count, register),
            Command::Editor { action, register } => {
                let active_window = app.ui.focused_window_id();

                let mut message = describe_action(app.controller.mode(), &action);
                if let Some(register) = register {
                    message.push_str(&format!(" (reg: '{register}')"));
                }
                app.model.status = Some(message);

                match &action {
                    vim_input::Action::BeginMacro { register } => {
                        app.services.macros.begin(register.clone());
                        app.controller.set_in_recording(true);
                        app.model.status = Some(format!("recording @{register}"));
                        return CommandOutcome::redraw();
                    }
                    vim_input::Action::EndMacro => {
                        app.services.macros.end();
                        app.controller.set_in_recording(false);
                        app.model.status = Some("macro recorded".to_string());
                        return CommandOutcome::redraw();
                    }
                    vim_input::Action::ReplayMacro { count, register } => {
                        let replay_actions = app.services.macros.replay(register, *count);
                        for rec in replay_actions {
                            app.command_queue.push_back(Command::Editor {
                                action: rec.action,
                                register: rec.register,
                            });
                        }
                        return CommandOutcome::redraw();
                    }
                    vim_input::Action::Script { count, script } => {
                        for _ in 0..*count {
                            if let Err(err) = app.script.execute(script) {
                                app.model.status = Some(format!("Script error: {err}"));
                                break;
                            }
                        }
                        return CommandOutcome::redraw();
                    }
                    vim_input::Action::KeySequence { count, keys } => {
                        let seq = match vim_input::KeySequence::parse(keys) {
                            Ok(s) => s,
                            Err(err) => {
                                app.model.status = Some(format!("Key sequence error: {err}"));
                                return CommandOutcome::redraw();
                            }
                        };
                        for _ in 0..*count {
                            for item in &seq.items {
                                if let vim_input::KeyPattern::Exact(key) = item {
                                    if let Some(cmd) = app.controller.feed_key(*key) {
                                        app.command_queue.push_back(cmd);
                                    }
                                }
                            }
                        }
                        return CommandOutcome::redraw();
                    }
                    vim_input::Action::Sequence { count, actions } => {
                        for _ in 0..*count {
                            for act in actions {
                                app.command_queue.push_back(Command::Editor {
                                    action: (**act).clone(),
                                    register,
                                });
                            }
                        }
                        return CommandOutcome::redraw();
                    }
                    vim_input::Action::Repeat { count } => {
                        if let Some(ref actions) = app.services.repeat_actions {
                            for _ in 0..*count {
                                for act in actions {
                                    app.command_queue.push_back(Command::Editor {
                                        action: act.clone(),
                                        register: None,
                                    });
                                }
                            }
                        }
                        return CommandOutcome::redraw();
                    }
                    _ => {
                        if app.services.macros.is_recording() {
                            app.services.macros.record(action.clone(), register);
                        }
                    }
                }

                let mode_before = app.controller.mode();

                let mut outcome = EditorHandler::execute(
                    &mut app.ui,
                    &mut app.model,
                    &mut app.controller,
                    &mut app.services,
                    active_window,
                    &action,
                    register,
                );

                let mode_after = app.controller.mode();
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
                            | vim_input::Action::SetToAppend
                            | vim_input::Action::SetToAppendEndOfLine
                            | vim_input::Action::SetToInsertStartOfLineNonSpace
                    );

                    if is_modifying {
                        let is_insert_entering = matches!(
                            action,
                            vim_input::Action::SetToInsert
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
                                app.services.recording_repeat = Some(vec![action.clone()]);
                            } else {
                                app.services.repeat_actions = Some(vec![action.clone()]);
                                app.services.recording_repeat = None;
                            }
                        } else if mode_before == vim_input::Mode::Insert {
                            if let Some(ref mut rec) = app.services.recording_repeat {
                                rec.push(action.clone());
                            }
                        }
                    } else if mode_before == vim_input::Mode::Insert {
                        if let Some(ref mut rec) = app.services.recording_repeat {
                            rec.push(action.clone());
                        }
                    }
                }

                if mode_after == vim_input::Mode::Normal {
                    if let Some(rec) = app.services.recording_repeat.take() {
                        app.services.repeat_actions = Some(rec);
                    }
                }

                if BufferHandler::handles(&action) {
                    outcome.merge(BufferHandler::execute(
                        &mut app.ui,
                        &app.model,
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
                        &mut app.controller,
                        &mut app.script,
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
                outcome
            }
        }
    }
}
