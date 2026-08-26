use crate::app::App;
use text::ToPoint;
use vim_script::host::RangeStateProvider;

use super::buffer_handler::BufferHandler;
use super::command::{Command, CommandOutcome};
use super::commandline_handler::CommandlineHandler;
use super::editor_handler::EditorHandler;
use super::lifecycle_handler::LifecycleHandler;
use super::range::RangeCommandHandler;
use super::shared_operations::SharedOperations;
use super::task_dispatcher::TaskDispatcher;
use super::window_handler::WindowHandler;

const DEBUG_VIM_INPUT: bool = false;

/// Formats the standard `[Mode] Action: ...` status message shared by
/// resolved editor actions and resolved range commands.
pub(super) fn describe_action(mode: vim_input::Mode, action: &vim_input::Action) -> String {
    format!("[{mode:?}] Action: {action:?}")
}

pub struct Dispatcher;

impl Dispatcher {
    pub fn dispatch(app: &mut App, command: Command) -> CommandOutcome {
        // Commands can be submitted by the runtime or directly by an
        // embedding/test caller. Keep the ID context aligned with the focused
        // buffer before constructing the command context.
        app.sync_kernel_context();
        let Some(context) = app.model.kernel().command_context_for(&command) else {
            app.model.status = Some("No current editor context".to_string());
            return CommandOutcome::redraw();
        };
        if !matches!(&command, Command::PendingInput(_)) {
            app.model.kernel_mut().clear_pending_command();
        }
        log::trace!(
            "dispatching {:?} command in tab {} window {} buffer {}",
            context.kind,
            context.current.tab.get(),
            context.current.window.get(),
            context.current.buffer.get()
        );

        match command {
            Command::PendingInput(pending) => {
                app.model.status = Some(format!("Pending sequence: {}", pending.display));
                app.model.kernel_mut().set_pending_command(pending);
                CommandOutcome::redraw()
            }
            Command::ExecuteScript(_) | Command::CommandLine(_) => CommandOutcome::redraw(),
            Command::SearchForward { pattern } => {
                let active_window = app.ui.focused_window_id();
                app.model.search_pattern = Some(pattern.clone());
                app.model.search_regex =
                    vim_regex::Regex::compile(&pattern, vim_regex::CompileOptions::default()).ok();
                app.model.search_range = None;
                app.model.substitute_text = None;
                let _ = crate::app::windows::WindowOps::edit_window(
                    &mut app.ui,
                    &mut app.model,
                    active_window,
                    |buffer, _context, window_state| {
                        window_state.selections.move_to_next_match(
                            &pattern,
                            true,
                            buffer.as_text_buffer(),
                        );
                    },
                );
                CommandOutcome::redraw()
            }
            Command::SearchBackward { pattern } => {
                let active_window = app.ui.focused_window_id();
                app.model.search_pattern = Some(pattern.clone());
                app.model.search_regex =
                    vim_regex::Regex::compile(&pattern, vim_regex::CompileOptions::default()).ok();
                app.model.search_range = None;
                app.model.substitute_text = None;
                let _ = crate::app::windows::WindowOps::edit_window(
                    &mut app.ui,
                    &mut app.model,
                    active_window,
                    |buffer, _context, window_state| {
                        window_state.selections.move_to_previous_match(
                            &pattern,
                            true,
                            buffer.as_text_buffer(),
                        );
                    },
                );
                CommandOutcome::redraw()
            }
            Command::Substitute {
                pattern,
                substitute_text,
                flags,
                range,
            } => super::substitute_handler::SubstituteHandler::start(
                app,
                pattern,
                substitute_text,
                flags,
                range,
            ),
            Command::PromptChoice { handler, choice } => match handler {
                super::substitute_handler::PromptHandler::Substitute => {
                    super::substitute_handler::SubstituteHandler::respond(app, choice)
                }
            },
            Command::InvalidInput => {
                app.model.status = Some("Invalid sequence".to_string());
                CommandOutcome::redraw()
            }
            Command::Save { path, force } => {
                let active_window = app.ui.focused_window_id();
                if let Some(buffer_id) =
                    crate::app::windows::WindowOps::window_buffer(&app.ui, active_window)
                {
                    if let Ok(buffer) = app.model.get_buffer(buffer_id) {
                        if buffer.options().readonly && !force {
                            app.model.status = Some(format!(
                                "Save failed: ReadOnly (buffer {})",
                                buffer_id.get()
                            ));
                            return CommandOutcome::redraw();
                        }
                        let path_buf = match path {
                            Some(p) => p,
                            None => match buffer.path() {
                                Some(p) => p.to_path_buf(),
                                None => {
                                    app.model.status = Some(format!(
                                        "Save failed: No file name (buffer {})",
                                        buffer_id.get()
                                    ));
                                    return CommandOutcome::redraw();
                                }
                            },
                        };
                        let snapshot = buffer.snapshot();
                        let options = buffer.options().clone();
                        let revision = app
                            .model
                            .buffer_state(buffer_id)
                            .map(|s| s.revision)
                            .unwrap_or(0);
                        let sequence = app
                            .services
                            .files
                            .begin_save(buffer_id, snapshot.changedtick());

                        let owner = crate::app::services::TaskOwner {
                            buffer_id: Some(buffer_id),
                            window_id: Some(active_window),
                            revision,
                        };

                        let task_id = app.services.spawn_cancellable_task(
                            "files",
                            sequence,
                            owner,
                            crate::app::services::TaskType::Files,
                            move |token| {
                                Some(files::save_file_cancellable(
                                    snapshot,
                                    path_buf,
                                    options,
                                    move || token.is_cancelled(),
                                )?)
                            },
                        );
                        if let Some(tid) = task_id {
                            app.services.files.set_pending_task(buffer_id, tid);
                            app.model.status = Some("Saving file in background...".to_string());
                        }
                    }
                }
                CommandOutcome::redraw()
            }
            Command::Quit { force } => {
                let active_window = app.ui.focused_window_id();
                LifecycleHandler::quit(&mut app.ui, &mut app.model, active_window, force)
            }
            Command::QuitAll { force } => LifecycleHandler::quit_all(&mut app.model, force),
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
            Command::SplitNew { vertical } => {
                let active_window = app.ui.focused_window_id();
                app.command_queue.push_back(Command::Edit {
                    path: None,
                    // The current buffer remains open in the original split, so
                    // creating the new buffer must not fail when it is modified.
                    force: true,
                });
                SharedOperations::split_window(active_window, !vertical)
            }
            Command::TabNew { path } => {
                let buffer = match path {
                    Some(path) => app.model.open_path(path),
                    None => app.model.create(""),
                };
                match app.new_tab(buffer) {
                    Ok(_) => CommandOutcome::redraw(),
                    Err(error) => {
                        app.model.status = Some(error);
                        CommandOutcome::redraw()
                    }
                }
            }
            Command::TabNext { count } => {
                if let Err(error) = app.next_tab(count) {
                    app.model.status = Some(error);
                }
                CommandOutcome::redraw()
            }
            Command::TabPrevious { count } => {
                if let Err(error) = app.previous_tab(count) {
                    app.model.status = Some(error);
                }
                CommandOutcome::redraw()
            }
            Command::TabClose => {
                if let Err(error) = app.close_active_tab() {
                    app.model.status = Some(error);
                }
                CommandOutcome::redraw()
            }
            Command::BufferNext { count } => {
                let active = app.ui.focused_window_id();
                SharedOperations::switch_buffer(&mut app.ui, &mut app.model, active, true, count)
            }
            Command::BufferPrevious { count } => {
                let active = app.ui.focused_window_id();
                SharedOperations::switch_buffer(&mut app.ui, &mut app.model, active, false, count)
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
            Command::Task(result) => {
                TaskDispatcher::dispatch(&mut app.ui, &mut app.model, &mut app.services, result)
            }
            Command::ClearSearchHighlight => {
                LifecycleHandler::clear_search_highlight(&mut app.model)
            }
            Command::Colorscheme { name } => LifecycleHandler::colorscheme(
                &mut app.ui,
                &mut app.model,
                &mut app.colorscheme,
                &mut app.highlighter,
                name.as_deref(),
            ),
            Command::Set { arguments } => {
                let active_window = app.ui.focused_window_id();
                let buffer_id =
                    crate::app::windows::WindowOps::window_buffer(&app.ui, active_window);
                match app
                    .config
                    .execute_set_command(&arguments, buffer_id, Some(active_window))
                {
                    Ok(Some(msg)) => {
                        app.model.status = Some(msg);
                    }
                    Ok(None) => {}
                    Err(err) => {
                        app.model.status = Some(format!("Error: {}", err));
                    }
                }
                if let Some(val) = app.config.get("inspect", buffer_id, Some(active_window)) {
                    if let Some(s) = val.as_string() {
                        app.inspect_what = match s {
                            "treesitter" => crate::app::InspectKind::TreeSitter,
                            "textmate" => crate::app::InspectKind::Textmate,
                            "indexer" => crate::app::InspectKind::Indexer,
                            _ => crate::app::InspectKind::None,
                        };
                    }
                }
                CommandOutcome::redraw()
            }
            Command::Syntax { enable } => {
                app.syntax_highlight = enable;
                app.model.invalidate_all_highlights();
                CommandOutcome::redraw()
            }
            Command::Treesitter { enable } => {
                app.treesitter_enabled = enable;
                CommandOutcome::redraw()
            }
            Command::Indexer { enable } => {
                app.indexer_enabled = enable;
                CommandOutcome::redraw()
            }
            Command::Inspect { enable } => {
                app.inspect = enable;
                CommandOutcome::redraw()
            }
            Command::Echo { message } => {
                app.model.status = Some(message.clone());
                app.message = message.clone();
                app.messages.push(message);
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

                if DEBUG_VIM_INPUT {
                    let mut message = describe_action(app.controller.mode(), &action);
                    if let Some(register) = register {
                        message.push_str(&format!(" (reg: '{register}')"));
                    }
                    app.model.status = Some(message);
                } else {
                    app.model.status = None;
                }

                match &action {
                    vim_input::Action::NextTab { count } => {
                        if let Err(error) = app.next_tab(*count as usize) {
                            app.model.status = Some(error);
                        }
                        return CommandOutcome::redraw();
                    }
                    vim_input::Action::PreviousTab { count } => {
                        if let Err(error) = app.previous_tab(*count as usize) {
                            app.model.status = Some(error);
                        }
                        return CommandOutcome::redraw();
                    }
                    vim_input::Action::BeginMacro { register } => {
                        if let Err(error) = app
                            .model
                            .kernel_mut()
                            .begin_macro_recording(register.clone())
                        {
                            app.model.status = Some(error.to_string());
                            return CommandOutcome::redraw();
                        }
                        app.services.macros.begin(register.clone());
                        app.controller.set_in_recording(true);
                        app.model.status = Some(format!("recording @{register}"));
                        return CommandOutcome::redraw();
                    }
                    vim_input::Action::EndMacro => {
                        let Some(register) = app.model.kernel_mut().end_macro_recording() else {
                            app.model.status = Some("macro recording is not active".to_string());
                            return CommandOutcome::redraw();
                        };
                        app.services.macros.end();
                        app.controller.set_in_recording(false);
                        app.model.status = Some(format!("macro @{register} recorded"));
                        return CommandOutcome::redraw();
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
                                return CommandOutcome::redraw();
                            }
                        };
                        let replay_actions = app.services.macros.replay(&register, count);
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
                            app.command_queue
                                .push_back(Command::ExecuteScript(script.clone()));
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
                    vim_input::Action::RepeatCharacterSearchForward { count, select } => {
                        if let Some(last) = app.model.last_character_search.as_ref() {
                            let action = match last {
                                vim_input::Action::MoveToNextCharacter { ch, till, .. }
                                | vim_input::Action::MoveToPreviousCharacter { ch, till, .. } => {
                                    vim_input::Action::MoveToNextCharacter {
                                        count: *count,
                                        ch: *ch,
                                        till: *till,
                                        select: *select,
                                    }
                                }
                                _ => return CommandOutcome::redraw(),
                            };
                            app.command_queue.push_back(Command::Editor {
                                action,
                                register: None,
                            });
                        }
                        return CommandOutcome::redraw();
                    }
                    vim_input::Action::RepeatCharacterSearchBackward { count, select } => {
                        if let Some(last) = app.model.last_character_search.as_ref() {
                            let action = match last {
                                vim_input::Action::MoveToNextCharacter { ch, till, .. }
                                | vim_input::Action::MoveToPreviousCharacter { ch, till, .. } => {
                                    vim_input::Action::MoveToPreviousCharacter {
                                        count: *count,
                                        ch: *ch,
                                        till: *till,
                                        select: *select,
                                    }
                                }
                                _ => return CommandOutcome::redraw(),
                            };
                            app.command_queue.push_back(Command::Editor {
                                action,
                                register: None,
                            });
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
                        if app.model.kernel().recording_target().is_some() {
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
                    &context,
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
                                app.services.recording_repeat = Some(vec![action.clone()]);
                            } else {
                                app.services.repeat_actions = Some(vec![action.clone()]);
                                app.services.recording_repeat = None;
                            }
                        } else if mode_before.is_insert() {
                            if let Some(ref mut rec) = app.services.recording_repeat {
                                rec.push(action.clone());
                            }
                        }
                    } else if mode_before.is_insert() {
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
                        &mut app.controller,
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
                                let end = text::Point::new(
                                    current_row,
                                    text_buffer.line_len(current_row),
                                )
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
                        return CommandOutcome::redraw();
                    }
                }

                outcome
            }
        }
    }
}
