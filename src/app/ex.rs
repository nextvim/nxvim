use crate::app::App;
use crate::app::command::{AppCommand, ApplicationRequest, ScriptRequest, SemanticRequest};
use crate::app::outcome::CommandOutcome;
use crate::kernel::{CommandLineRequest, EditorContext, ExAdmission};

/// Application admission adapter for parsed Ex and script-host requests.
pub struct ExDispatcher;

impl ExDispatcher {
    pub fn dispatch(
        current: Option<EditorContext>,
        request: &CommandLineRequest,
        execute: impl FnOnce(&CommandLineRequest) -> Result<(), String>,
    ) -> Result<(), String> {
        execute(ExAdmission::command_line(current, request)?)
    }

    pub fn dispatch_host_command(
        current: Option<EditorContext>,
        origin: Option<EditorContext>,
        command: AppCommand,
    ) -> Result<AppCommand, String> {
        ExAdmission::host_command(current, origin)?;
        Ok(command)
    }

    pub fn execute_host_command(
        app: &mut App,
        current: Option<EditorContext>,
        origin: Option<EditorContext>,
        command: AppCommand,
    ) -> Result<CommandOutcome, String> {
        app.sync_kernel_context();
        let synchronized = app.current_context();
        if synchronized != current {
            return Err("Editor context changed before script command admission".to_string());
        }
        let command = Self::dispatch_host_command(synchronized, origin, command)?;
        app.model.kernel_mut().clear_pending_command();

        match command {
            AppCommand::Lifecycle(request) => Ok(crate::app::lifecycle::dispatch(app, request)),
            AppCommand::Navigation(request) => Ok(crate::app::navigation::dispatch(app, request)),
            AppCommand::Application(ApplicationRequest::SetOption { name, value, scope }) => {
                let current =
                    synchronized.ok_or_else(|| "No current editor context".to_string())?;
                Ok(Self::set_option(app, current, name, value, scope))
            }
            AppCommand::Application(request) => Ok(crate::app::application::dispatch(app, request)),
            AppCommand::Semantic(SemanticRequest::ReplaceBuffer {
                buffer,
                range,
                text,
            }) => Self::replace_buffer(app, buffer, range, text),
            AppCommand::Semantic(request) => {
                let request = match crate::app::search::dispatch(app, request) {
                    Ok(outcome) => return Ok(outcome),
                    Err(request) => request,
                };
                crate::app::editor::dispatch(app, request).map_err(|request| {
                    format!(
                        "Unsupported script host semantic request: {}",
                        semantic_name(&request)
                    )
                })
            }
            AppCommand::Prompt(request) => Ok(crate::app::prompt::dispatch(app, request)),
            AppCommand::Script(ScriptRequest::Execute(script)) => {
                app.command_queue
                    .push_back(AppCommand::Script(ScriptRequest::Execute(script)));
                Ok(CommandOutcome::default())
            }
            AppCommand::Script(ScriptRequest::CommandLine(_))
            | AppCommand::Input(_)
            | AppCommand::Service(_) => Err("Unsupported script host command category".to_string()),
        }
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
}

fn semantic_name(request: &SemanticRequest) -> &'static str {
    match request {
        SemanticRequest::Editor { .. } => "editor",
        SemanticRequest::RangeOp { .. } => "range",
        SemanticRequest::ReplaceBuffer { .. } => "replace-buffer",
        SemanticRequest::SearchForward { .. } => "search-forward",
        SemanticRequest::SearchBackward { .. } => "search-backward",
        SemanticRequest::Substitute { .. } => "substitute",
    }
}
