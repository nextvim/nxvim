//! App-owned dispatch for editor semantic action and range commands.

use crate::app::App;

use crate::app::command::{AppCommand, ScriptRequest, SemanticRequest};
use crate::app::commandline;
use crate::app::lifecycle::LifecycleHandler;
use crate::app::outcome::AppCommandOutcome;
use crate::app::range_ops::RangeCommandHandler;

/// Thin application adapter for kernel-owned editor action execution.
///
/// The kernel owns action classification and semantics. This boundary only
/// lends concrete buffer/window state, selects the requested register, and
/// synchronizes application input state after a semantic mode transition.
pub(crate) fn execute_action(
    app: &mut App,
    active_window: vim_ui::WindowId,
    action: &vim_input::Action,
    register: Option<char>,
    command_context: &crate::kernel::CommandContext,
) -> AppCommandOutcome {
    if crate::app::windows::WindowOps::window_buffer(&app.ui, active_window).is_none() {
        return AppCommandOutcome::redraw();
    }

    app.model
        .kernel_mut()
        .record_character_search(action.clone());
    if let Some(register) = register.and_then(vim_clipboard::RegisterName::from_char) {
        app.services.clipboard.grab(register);
    }

    let current_mode = app.model.kernel().mode();
    let join_insert_transaction = app.model.kernel().join_insert_transaction();
    let search_pattern = app.model.kernel().search().pattern().map(str::to_owned);
    let mut execution = None;
    let _ = crate::app::windows::WindowOps::edit_window(
        &mut app.ui,
        &mut app.model,
        active_window,
        |buffer, buffer_state, window_state| {
            execution = Some(crate::kernel::editor::execute_action(
                buffer,
                buffer_state,
                window_state,
                &mut app.services.clipboard,
                action,
                command_context,
                current_mode,
                join_insert_transaction,
                search_pattern.as_deref(),
            ));
        },
    );
    app.services.clipboard.release();

    let Some(mut execution) = execution else {
        return AppCommandOutcome::redraw();
    };
    if let Some(mode) = execution.next_mode {
        let mode_outcome = app.model.kernel_mut().transition_mode(mode);
        app.input.set_mode(app.model.kernel().mode());
        execution.outcome.merge(mode_outcome);
    }
    let mutated = execution.outcome.effects.iter().any(|effect| {
        matches!(
            effect,
            crate::kernel::CommandEffect::BufferMutated { .. }
                | crate::kernel::CommandEffect::MutationCommitted(_)
        )
    });
    if mutated && app.model.kernel().mode().is_insert() {
        app.model.kernel_mut().note_insert_mutation();
    }
    if !execution.outcome.effects.is_empty() {
        log::trace!("kernel command produced {:?}", execution.outcome.effects);
        AppCommandOutcome::from_kernel(execution.outcome)
    } else {
        AppCommandOutcome::redraw()
    }
}

pub fn dispatch(
    app: &mut App,
    command: SemanticRequest,
) -> Result<AppCommandOutcome, SemanticRequest> {
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
            Ok(AppCommandOutcome::statusline())
        }
        SemanticRequest::Editor { action, register } => {
            let active_window = app.ui.focused_window_id();

            app.model.status = None;

            match &action {
                vim_input::Action::NextTab { count } => {
                    if let Err(error) = app.next_tab(*count as usize) {
                        app.model.status = Some(error);
                    }
                    return Ok(AppCommandOutcome::redraw());
                }
                vim_input::Action::PreviousTab { count } => {
                    if let Err(error) = app.previous_tab(*count as usize) {
                        app.model.status = Some(error);
                    }
                    return Ok(AppCommandOutcome::redraw());
                }
                vim_input::Action::BeginMacro { register } => {
                    if let Err(error) = app
                        .model
                        .kernel_mut()
                        .begin_macro_recording(register.clone())
                    {
                        app.model.status = Some(error.to_string());
                        return Ok(AppCommandOutcome::statusline());
                    }
                    app.services.macros.begin(register.clone());
                    app.input.set_in_recording(true);
                    app.model.status = Some(format!("recording @{register}"));
                    return Ok(AppCommandOutcome::statusline());
                }
                vim_input::Action::EndMacro => {
                    let Some(register) = app.model.kernel_mut().end_macro_recording() else {
                        app.model.status = Some("macro recording is not active".to_string());
                        return Ok(AppCommandOutcome::statusline());
                    };
                    app.services.macros.end();
                    app.input.set_in_recording(false);
                    app.model.status = Some(format!("macro @{register} recorded"));
                    return Ok(AppCommandOutcome::statusline());
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
                            return Ok(AppCommandOutcome::statusline());
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
                    return Ok(AppCommandOutcome::redraw());
                }
                vim_input::Action::Script { count, script } => {
                    for _ in 0..*count {
                        app.command_queue
                            .push_back(AppCommand::Script(ScriptRequest::Execute(script.clone())));
                    }
                    return Ok(AppCommandOutcome::redraw());
                }
                vim_input::Action::KeySequence { count, keys } => {
                    let seq = match vim_input::KeySequence::parse(keys) {
                        Ok(s) => s,
                        Err(err) => {
                            app.model.status = Some(format!("Key sequence error: {err}"));
                            return Ok(AppCommandOutcome::statusline());
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
                    return Ok(AppCommandOutcome::redraw());
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
                    return Ok(AppCommandOutcome::redraw());
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
                    return Ok(AppCommandOutcome::redraw());
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
                return Ok(AppCommandOutcome::statusline());
            };

            let mut outcome =
                execute_action(app, active_window, &action, register, &command_context);

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

            let window_effect = match action {
                vim_input::Action::SplitHorizontal { .. } => Some(
                    crate::app::lifecycle::LifecycleOperations::split_window(active_window, true),
                ),
                vim_input::Action::SplitVertical { .. } => Some(
                    crate::app::lifecycle::LifecycleOperations::split_window(active_window, false),
                ),
                vim_input::Action::FocusLeftWindow => {
                    Some(crate::app::lifecycle::LifecycleOperations::focus_window(
                        vim_ui::NavigationDirection::Left,
                    ))
                }
                vim_input::Action::FocusRightWindow => {
                    Some(crate::app::lifecycle::LifecycleOperations::focus_window(
                        vim_ui::NavigationDirection::Right,
                    ))
                }
                vim_input::Action::FocusUpWindow => {
                    Some(crate::app::lifecycle::LifecycleOperations::focus_window(
                        vim_ui::NavigationDirection::Up,
                    ))
                }
                vim_input::Action::FocusDownWindow => {
                    Some(crate::app::lifecycle::LifecycleOperations::focus_window(
                        vim_ui::NavigationDirection::Down,
                    ))
                }
                _ => None,
            };
            if let Some(window_effect) = window_effect {
                outcome.merge(window_effect);
            }
            if commandline::handles(active_window, app.view_ids.commandline, &action) {
                outcome.merge(commandline::execute(
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

                            let command_line = format!(
                                "{}{}",
                                app.model.kernel().command_line().prefix(),
                                raw_pattern
                            );
                            let mut runtime = crate::script::ScriptRuntime::new();
                            if let Ok(cmd) = runtime.peek_command(&command_line) {
                                match cmd {
                                    AppCommand::Semantic(SemanticRequest::SearchForward {
                                        pattern,
                                    })
                                    | AppCommand::Semantic(SemanticRequest::SearchBackward {
                                        pattern,
                                    }) => {
                                        if pattern.is_empty() {
                                            app.model.kernel_mut().search_mut().clear();
                                        } else {
                                            app.model
                                                .kernel_mut()
                                                .search_mut()
                                                .set_pattern(pattern);
                                        }
                                    }
                                    AppCommand::Semantic(SemanticRequest::Substitute {
                                        pattern,
                                        substitute_text,
                                        range,
                                        ..
                                    }) => {
                                        if pattern.is_empty() {
                                            app.model.kernel_mut().search_mut().clear();
                                        } else {
                                            app.model.kernel_mut().search_mut().set_substitution(
                                                pattern,
                                                range,
                                                substitute_text,
                                            );
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
            } else {
                // clearing required
                if app.model.kernel().search().substitute_text().is_some() {
                    app.model.kernel_mut().search_mut().clear();
                    return Ok(AppCommandOutcome::redraw());
                }
            }

            Ok(outcome)
        }
        command => Err(command),
    }
}
