use super::{CommandLineRequest, EditorContext};
use crate::app::App;
use crate::controller::lifecycle_handler::LifecycleHandler;
use crate::controller::range::RangeCommandHandler;
use crate::controller::shared_operations::SharedOperations;
use crate::controller::substitute_handler::SubstituteHandler;
use crate::controller::{Command, CommandOutcome};

/// Kernel entry point for an already parsed command-line request.
///
/// The script runtime remains the Ex parser/host, but identity validation and
/// request admission belong to the semantic kernel rather than the terminal
/// runtime loop.
pub struct ExDispatcher;

impl ExDispatcher {
    pub fn dispatch(
        current: Option<EditorContext>,
        request: &CommandLineRequest,
        execute: impl FnOnce(&CommandLineRequest) -> Result<(), String>,
    ) -> Result<(), String> {
        if current != Some(request.current) {
            return Err("Command-line context changed before execution".to_string());
        }
        execute(request)
    }

    /// Validate a command emitted by the script host against its originating
    /// context. Execution is performed by [`Self::execute_host_command`].
    pub fn dispatch_host_command(
        current: Option<EditorContext>,
        origin: Option<EditorContext>,
        command: Command,
    ) -> Result<Command, String> {
        if origin.is_some() && current != origin {
            return Err("Script command context changed before execution".to_string());
        }
        Ok(command)
    }

    /// Executes a command emitted by the script host without passing through
    /// the compatibility controller dispatcher.
    pub fn execute_host_command(
        app: &mut App,
        current: Option<EditorContext>,
        origin: Option<EditorContext>,
        command: Command,
    ) -> Result<CommandOutcome, String> {
        app.sync_kernel_context();
        let synchronized = app.current_context();
        if synchronized != current {
            return Err("Editor context changed before script command admission".to_string());
        }
        let command = Self::dispatch_host_command(synchronized, origin, command)?;
        let Some(context) = app.model.kernel().command_context_for(&command) else {
            return Err("No current editor context".to_string());
        };
        app.model.kernel_mut().clear_pending_command();
        log::trace!(
            "executing host {:?} command in tab {} window {} buffer {}",
            context.kind,
            context.current.tab.get(),
            context.current.window.get(),
            context.current.buffer.get()
        );

        let outcome = match command {
            Command::Quit { force } => {
                let window = app.ui.focused_window_id();
                LifecycleHandler::quit(&mut app.ui, &mut app.model, window, force)
            }
            Command::QuitAll { force } => LifecycleHandler::quit_all(&mut app.model, force),
            Command::Save { path, force } => Self::save(app, path, force),
            Command::Edit { path, force } => {
                let window = app.ui.focused_window_id();
                LifecycleHandler::edit(&mut app.ui, &mut app.model, window, path.as_deref(), force)
            }
            Command::WriteQuit { path, force } => {
                let window = app.ui.focused_window_id();
                LifecycleHandler::write_and_quit(
                    &mut app.ui,
                    &mut app.model,
                    window,
                    path.as_deref(),
                    force,
                )
            }
            Command::WriteQuitAll { force } => {
                let window = app.ui.focused_window_id();
                LifecycleHandler::write_and_quit_all(&mut app.ui, &mut app.model, window, force)
            }
            Command::BufferNext { count } => {
                let window = app.ui.focused_window_id();
                SharedOperations::switch_buffer(&mut app.ui, &mut app.model, window, true, count)
            }
            Command::BufferPrevious { count } => {
                let window = app.ui.focused_window_id();
                SharedOperations::switch_buffer(&mut app.ui, &mut app.model, window, false, count)
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
            Command::TabNew { path } => {
                let buffer = match path {
                    Some(path) => app.model.open_path(path),
                    None => app.model.create(""),
                };
                if let Err(error) = app.new_tab(buffer) {
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
            Command::SplitNew { vertical } => {
                let window = app.ui.focused_window_id();
                app.command_queue.push_back(Command::Edit {
                    path: None,
                    force: true,
                });
                SharedOperations::split_window(window, !vertical)
            }
            Command::Editor {
                action,
                register: _,
            } => {
                let window = app.ui.focused_window_id();
                match action {
                    vim_input::Action::SplitHorizontal { .. } => {
                        SharedOperations::split_window(window, true)
                    }
                    vim_input::Action::SplitVertical { .. } => {
                        SharedOperations::split_window(window, false)
                    }
                    other => {
                        return Err(format!("Unsupported script host editor action: {other:?}"));
                    }
                }
            }
            Command::ClearSearchHighlight => {
                LifecycleHandler::clear_search_highlight(&mut app.model)
            }
            Command::SearchForward { pattern } => Self::search(app, pattern, true),
            Command::SearchBackward { pattern } => Self::search(app, pattern, false),
            Command::Substitute {
                pattern,
                substitute_text,
                flags,
                range,
            } => SubstituteHandler::start(app, pattern, substitute_text, flags, range),
            Command::RangeOp {
                operation,
                bang,
                range,
                count,
                register,
            } => RangeCommandHandler::execute(app, operation, bang, range, count, register),
            Command::Colorscheme { name } => LifecycleHandler::colorscheme(
                &mut app.ui,
                &mut app.model,
                &mut app.colorscheme,
                &mut app.highlighter,
                name.as_deref(),
            ),
            Command::Set { arguments } => Self::set(app, arguments),
            Command::SetOption { name, value, scope } => {
                Self::set_option(app, context.current, name, value, scope)
            }
            Command::ReplaceBuffer {
                buffer,
                range,
                text,
            } => Self::replace_buffer(app, buffer, range, text)?,
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
            Command::OpenPrompt { message } => {
                let window = app.ui.focused_window_id();
                let prompt = crate::controller::Prompt::script(message, window);
                app.model.status = Some(format!("{} (y/n/q)", prompt.message));
                app.prompt = Some(prompt);
                CommandOutcome::redraw()
            }
            Command::Echo { message } => {
                app.model.status = Some(message.clone());
                app.message = message.clone();
                app.messages.push(message);
                CommandOutcome::redraw()
            }
            other => return Err(format!("Unsupported script host command: {other:?}")),
        };
        Ok(outcome)
    }

    fn save(app: &mut App, path: Option<std::path::PathBuf>, force: bool) -> CommandOutcome {
        let active_window = app.ui.focused_window_id();
        let Some(buffer_id) = crate::app::windows::WindowOps::window_buffer(&app.ui, active_window)
        else {
            return CommandOutcome::redraw();
        };
        let Ok(buffer) = app.model.get_buffer(buffer_id) else {
            return CommandOutcome::redraw();
        };
        if buffer.options().readonly && !force {
            app.model.status = Some(format!(
                "Save failed: ReadOnly (buffer {})",
                buffer_id.get()
            ));
            return CommandOutcome::redraw();
        }
        let path = match path.or_else(|| buffer.path().map(ToOwned::to_owned)) {
            Some(path) => path,
            None => {
                app.model.status = Some(format!(
                    "Save failed: No file name (buffer {})",
                    buffer_id.get()
                ));
                return CommandOutcome::redraw();
            }
        };
        let snapshot = buffer.snapshot();
        let options = buffer.options().clone();
        let revision = app
            .model
            .buffer_state(buffer_id)
            .map(|state| state.revision)
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
                    path,
                    options,
                    move || token.is_cancelled(),
                )?)
            },
        );
        if let Some(task_id) = task_id {
            app.services.files.set_pending_task(buffer_id, task_id);
            app.model.status = Some("Saving file in background...".to_string());
        }
        CommandOutcome::redraw()
    }

    fn search(app: &mut App, pattern: String, forward: bool) -> CommandOutcome {
        let window = app.ui.focused_window_id();
        app.model.search_pattern = Some(pattern.clone());
        app.model.search_regex =
            vim_regex::Regex::compile(&pattern, vim_regex::CompileOptions::default()).ok();
        app.model.search_range = None;
        app.model.substitute_text = None;
        let _ = crate::app::windows::WindowOps::edit_window(
            &mut app.ui,
            &mut app.model,
            window,
            |buffer, _context, window_state| {
                if forward {
                    window_state.selections.move_to_next_match(
                        &pattern,
                        true,
                        buffer.as_text_buffer(),
                    );
                } else {
                    window_state.selections.move_to_previous_match(
                        &pattern,
                        true,
                        buffer.as_text_buffer(),
                    );
                }
            },
        );
        CommandOutcome::redraw()
    }

    fn replace_buffer(
        app: &mut App,
        buffer: u64,
        range: vim_script::host::OwnedTextRange,
        text: String,
    ) -> Result<CommandOutcome, String> {
        let id = crate::kernel::BufferId::new(buffer)
            .ok_or_else(|| format!("Invalid buffer ID: {buffer}"))?;
        let buffer = app
            .model
            .get_buffer_mut(id)
            .map_err(|_| format!("Stale buffer ID: {}", id.get()))?;
        let len = buffer.as_text_buffer().len();
        let start = usize::try_from(range.start).unwrap_or(usize::MAX).min(len);
        let end = usize::try_from(range.end).unwrap_or(usize::MAX).min(len);
        let range =
            vim_buffer::TextRange::new(vim_buffer::ByteOffset(start), vim_buffer::ByteOffset(end))
                .ok_or_else(|| "Invalid replacement range".to_owned())?;
        let mutation = crate::kernel::transaction(
            buffer,
            vim_buffer::EditOrigin::VimScript,
            None,
            |transaction| transaction.replace(None, range, text.as_str()),
        )?;
        Ok(CommandOutcome::from_kernel(
            crate::kernel::CommandOutcome::mutation_committed(mutation),
        ))
    }

    fn set_option(
        app: &mut App,
        current: EditorContext,
        name: String,
        value: vim_script::runtime::Value,
        scope: vim_script::host::OptionRequestScope,
    ) -> CommandOutcome {
        let value = match value {
            vim_script::runtime::Value::Bool(value) => crate::app::config::ConfigValue::Bool(value),
            vim_script::runtime::Value::Integer(value) => {
                crate::app::config::ConfigValue::Number(value)
            }
            vim_script::runtime::Value::String(value) => {
                crate::app::config::ConfigValue::String(value.to_string())
            }
            _ => {
                app.model.status = Some(format!("Invalid value type for option: {name}"));
                return CommandOutcome::statusline();
            }
        };
        let (buffer, window) = match scope {
            vim_script::host::OptionRequestScope::Global => (None, None),
            vim_script::host::OptionRequestScope::Local
            | vim_script::host::OptionRequestScope::Unqualified => {
                (Some(current.buffer), Some(current.window))
            }
        };
        let result = {
            let mut config = app.config.write().expect("config store lock poisoned");
            let canonical_name = config
                .registry()
                .lookup(&name)
                .map(|spec| spec.name.to_owned())
                .unwrap_or_else(|| name.clone());
            config
                .set(&name, value.clone(), buffer, window)
                .map(|()| canonical_name)
        };
        match result {
            Ok(canonical_name) => {
                app.model
                    .kernel_mut()
                    .events_mut()
                    .push(crate::kernel::EditorEvent::OptionSet {
                        name: canonical_name.into(),
                        value: Some(match value {
                            crate::app::config::ConfigValue::Bool(value) => value.to_string(),
                            crate::app::config::ConfigValue::Number(value) => value.to_string(),
                            crate::app::config::ConfigValue::String(value) => value,
                        }),
                    });
                CommandOutcome::redraw()
            }
            Err(error) => {
                app.model.status = Some(format!("Error: {error}"));
                CommandOutcome::statusline()
            }
        }
    }

    fn set(app: &mut App, arguments: String) -> CommandOutcome {
        let window = app.ui.focused_window_id();
        let buffer = crate::app::windows::WindowOps::window_buffer(&app.ui, window);
        let result = app
            .config
            .write()
            .expect("config store lock poisoned")
            .execute_set_command(&arguments, buffer, Some(window));
        match result {
            Ok(Some(message)) => app.model.status = Some(message),
            Ok(None) => {}
            Err(error) => app.model.status = Some(format!("Error: {error}")),
        }
        let inspect = app.config.read().expect("config store lock poisoned").get(
            "inspect",
            buffer,
            Some(window),
        );
        if let Some(value) = inspect {
            if let Some(value) = value.as_string() {
                app.inspect_what = match value {
                    "treesitter" => crate::app::InspectKind::TreeSitter,
                    "textmate" => crate::app::InspectKind::Textmate,
                    "indexer" => crate::app::InspectKind::Indexer,
                    _ => crate::app::InspectKind::None,
                };
            }
        }
        CommandOutcome::redraw()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::{BufferId, CommandLineRequest, TabPageId, WindowId};

    fn context(buffer: u64) -> EditorContext {
        EditorContext {
            tab: TabPageId::new(1),
            window: WindowId::new(1),
            buffer: BufferId::new(buffer).unwrap(),
        }
    }

    #[test]
    fn rejects_stale_context_before_calling_host() {
        let request = CommandLineRequest::parse(context(1), ":quit").unwrap();
        let mut called = false;
        let result = ExDispatcher::dispatch(Some(context(2)), &request, |_| {
            called = true;
            Ok(())
        });
        assert!(result.is_err());
        assert!(!called);
    }

    #[test]
    fn admits_typed_request_with_matching_context() {
        let current = context(1);
        let request = CommandLineRequest::parse(current, ":quit").unwrap();
        let mut called = false;
        ExDispatcher::dispatch(Some(current), &request, |accepted| {
            called = true;
            assert_eq!(accepted.current, current);
            assert_eq!(accepted.text, ":quit");
            Ok(())
        })
        .unwrap();
        assert!(called);
    }

    #[test]
    fn rejects_host_command_from_stale_context() {
        let command = Command::Echo {
            message: "from script".to_string(),
        };
        let result =
            ExDispatcher::dispatch_host_command(Some(context(2)), Some(context(1)), command);
        assert!(result.is_err());
    }

    #[test]
    fn admits_host_command_without_origin_context() {
        let command = Command::Echo {
            message: "global script".to_string(),
        };
        assert!(ExDispatcher::dispatch_host_command(None, None, command).is_ok());
    }

    #[test]
    fn executes_host_echo_directly() {
        let mut app = App::new(
            vim_ui::Rect::new(0, 0, 80, 24),
            crate::app::args::Args::default(),
        );
        let current = app.current_context();
        let outcome = ExDispatcher::execute_host_command(
            &mut app,
            current,
            current,
            Command::Echo {
                message: "hello".to_string(),
            },
        )
        .unwrap();
        assert_ne!(outcome.redraw, crate::kernel::RedrawRequest::None);
        assert_eq!(app.model.status.as_deref(), Some("hello"));
        assert_eq!(app.messages.last().map(String::as_str), Some("hello"));
    }

    #[test]
    fn host_execution_rejects_stale_and_unsupported_commands() {
        let mut app = App::new(
            vim_ui::Rect::new(0, 0, 80, 24),
            crate::app::args::Args::default(),
        );
        let current = app.current_context();
        assert!(
            ExDispatcher::execute_host_command(
                &mut app,
                Some(context(u64::MAX)),
                current,
                Command::Echo {
                    message: "stale".to_string()
                },
            )
            .is_err()
        );
        assert!(
            ExDispatcher::execute_host_command(&mut app, current, current, Command::InvalidInput,)
                .is_err()
        );
    }
}
