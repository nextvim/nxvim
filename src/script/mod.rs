use std::sync::{Arc, Mutex, mpsc};
use std::{collections::HashMap, path::PathBuf};

use crate::app::legacy_command::Command;
use text::{BufferId, BufferSnapshot};

use vim_script::{
    compiler::Compiler,
    host::{
        Capability, CommandDefinition, CommandRequest, EditorRequest, EditorRequestOperation,
        EditorResponse, Host, HostContext, HostFuture, HostRequest, HostRuntime, OptionRequest,
        OptionRequestOperation, OptionRequestScope, TabRequestOperation, WindowRequestOperation,
    },
    lexer::Lexer,
    parser::Parser,
    resolver::{Resolver, ResolverConfig},
    runtime::{
        RuntimeError, RuntimeErrorKind, RuntimeResult, Scheduler, Value, Vm,
        builtins::BuiltinRegistry,
    },
    source::SourceMap,
};

pub mod commands;
pub mod functions;

use commands::registry::COMMAND_SPECS;

/// Owned application-to-script event boundary. The envelope is resolved before
/// callbacks run, so nested commands cannot invalidate borrowed editor state.
#[derive(Clone, Debug)]
pub struct AutocmdEventEnvelope {
    pub event: vim_script::integration::Event,
    pub context: HostContext,
}

impl AutocmdEventEnvelope {
    pub fn from_editor_event(
        editor_event: &crate::kernel::EditorEvent,
        model: &crate::model::EditorModel,
    ) -> Self {
        use crate::kernel::EditorEvent;

        let current = model.kernel().current();
        let mut context = HostContext::default();
        if let Some(current) = current {
            context.current_tab = Some(current.tab.get());
            context.current_window = Some(current.window.get());
            context.current_buffer = Some(current.buffer.get());
        }

        let (name, buffer, window, explicit_match) = match editor_event {
            EditorEvent::BufAdd { buffer } => ("BufAdd", Some(*buffer), None, None),
            EditorEvent::BufRead { buffer } => ("BufRead", Some(*buffer), None, None),
            EditorEvent::BufEnter { buffer, window } => {
                ("BufEnter", Some(*buffer), Some(*window), None)
            }
            EditorEvent::BufLeave { buffer, window } => {
                ("BufLeave", Some(*buffer), Some(*window), None)
            }
            EditorEvent::BufWrite { buffer } => ("BufWrite", Some(*buffer), None, None),
            EditorEvent::BufUnload { buffer } => ("BufUnload", Some(*buffer), None, None),
            EditorEvent::BufDelete { buffer } => ("BufDelete", Some(*buffer), None, None),
            EditorEvent::BufWipeout { buffer } => ("BufWipeout", Some(*buffer), None, None),
            EditorEvent::TextChanged { buffer, .. } => ("TextChanged", Some(*buffer), None, None),
            EditorEvent::CursorMoved { window } => ("CursorMoved", None, Some(*window), None),
            EditorEvent::InsertEnter { window } => ("InsertEnter", None, Some(*window), None),
            EditorEvent::InsertLeave { window } => ("InsertLeave", None, Some(*window), None),
            EditorEvent::OptionSet { name, .. } => {
                ("OptionSet", None, None, Some(name.as_str().to_owned()))
            }
            EditorEvent::UserCommandRegistered { name } => ("User", None, None, Some(name.clone())),
            EditorEvent::UserCommandRemoved { name } => ("User", None, None, Some(name.clone())),
            EditorEvent::VimEnter => ("VimEnter", None, None, None),
            EditorEvent::VimLeave => ("VimLeave", None, None, None),
        };

        if let Some(buffer) = buffer {
            context.current_buffer = Some(buffer.get());
        }
        if let Some(window) = window {
            context.current_window = Some(window.get());
        }

        let file = buffer.and_then(|buffer| {
            model
                .get_buffer(buffer)
                .ok()
                .and_then(|buffer| buffer.path())
                .map(|path| path.to_string_lossy().into_owned())
        });
        let pattern = explicit_match.or_else(|| file.clone()).unwrap_or_default();
        let mut payload = HashMap::new();
        if let EditorEvent::OptionSet {
            value: Some(value), ..
        } = editor_event
        {
            payload.insert(
                "option_new".to_owned(),
                Value::String(Arc::<str>::from(value.as_str())),
            );
        }
        if let Some(buffer) = buffer {
            payload.insert("abuf".to_owned(), Value::Integer(buffer.get() as i64));
        }
        payload.insert(
            "amatch".to_owned(),
            Value::String(Arc::<str>::from(pattern.as_str())),
        );
        payload.insert(
            "afile".to_owned(),
            Value::String(Arc::<str>::from(file.as_deref().unwrap_or(""))),
        );

        Self {
            event: vim_script::integration::Event {
                name: name.to_owned(),
                pattern: Some(pattern),
                payload,
            },
            context,
        }
    }
}

#[derive(Debug)]
pub struct EmittedCommand {
    pub command: Command,
    pub context: HostContext,
}

impl EmittedCommand {
    pub fn editor_context(&self) -> Option<crate::kernel::EditorContext> {
        Some(crate::kernel::EditorContext {
            tab: crate::kernel::TabPageId::new(self.context.current_tab?),
            window: crate::kernel::WindowId::new(self.context.current_window?),
            buffer: crate::kernel::BufferId::new(self.context.current_buffer?)?,
        })
    }
}

pub struct ScriptRuntime {
    scheduler: Scheduler,
    commands: mpsc::Receiver<EmittedCommand>,
    globals: HashMap<String, Value>,
    builtins: BuiltinRegistry,
    sources: SourceMap,
    state: Arc<Mutex<EditorState>>,
    pending_user_commands: Vec<String>,
    keymaps: vim_script::integration::SharedKeymapStore,
}

impl ScriptRuntime {
    pub fn new() -> Self {
        Self::with_options(Arc::new(std::sync::RwLock::new(
            crate::app::config::ConfigStore::new(),
        )))
    }

    pub fn with_options(options: Arc<std::sync::RwLock<crate::app::config::ConfigStore>>) -> Self {
        let (sender, commands) = mpsc::channel();
        let state = Arc::new(Mutex::new(EditorState::default()));
        let keymaps = Arc::new(std::sync::RwLock::new(
            vim_script::integration::KeymapStore::default(),
        ));
        let mut host = HostRuntime::with_keymaps(
            Arc::new(EditorHost {
                sender,
                state: state.clone(),
                options: options.clone(),
            }),
            keymaps.clone(),
        );
        host.capabilities.grant(Capability::Editor);
        host.capabilities.grant(Capability::BufferRead);
        host.capabilities.grant(Capability::BufferWrite);
        host.capabilities.grant(Capability::Window);
        host.capabilities.grant(Capability::UserInterface);
        host.capabilities.grant(Capability::Settings);

        functions::register(&mut host);

        let builtins = BuiltinRegistry::with_defaults();

        for spec in COMMAND_SPECS {
            host.register_command(CommandDefinition::from(spec));
        }

        let mut scheduler = Scheduler::default();
        scheduler.set_host(host);
        Self {
            scheduler,
            commands,
            globals: HashMap::new(),
            builtins,
            sources: SourceMap::default(),
            state,
            pending_user_commands: Vec::new(),
            keymaps,
        }
    }

    pub fn keymaps(&self) -> vim_script::integration::SharedKeymapStore {
        self.keymaps.clone()
    }

    pub fn execute(&mut self, source: &str) -> Result<Value, String> {
        self.execute_with_context(source, None)
    }

    pub fn execute_with_context(
        &mut self,
        source: &str,
        current: Option<crate::kernel::EditorContext>,
    ) -> Result<Value, String> {
        // `&` and `~` are standalone Ex substitute-repeat commands, but the
        // general expression lexer also assigns meaning to both tokens. Route
        // these exact command lines through the canonical command spelling so
        // they reach the Ex host rather than the expression parser.
        let normalized = matches!(source.trim(), "&" | "~").then(|| "substitute".to_string());
        let mut source = normalized.as_deref().unwrap_or(source);
        if let Some(stripped) = source.strip_prefix(':') {
            if stripped
                .trim_start()
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            {
                source = stripped;
            }
        }

        let source_id = self.sources.add("command_line", source);
        let lexed = Lexer::new(source_id, source).lex();
        self.check_diagnostics(&lexed.diagnostics)?;

        let parsed = Parser::new_with_source(&lexed.tokens, source).parse();
        self.check_diagnostics(&parsed.diagnostics)?;
        let program = parsed
            .program
            .ok_or_else(|| "script produced no program".to_owned())?;

        let host = self.scheduler.host().expect("script host is installed");
        let mut config = ResolverConfig::default();
        config.unqualified_is_global = true;
        config
            .builtins
            .extend(host.functions.names().map(str::to_owned));

        let resolved = Resolver::new(config).resolve(program);
        self.check_diagnostics(&resolved.diagnostics)?;
        let resolved_program = resolved
            .program
            .ok_or_else(|| "script resolution produced no program".to_owned())?;

        let compiled = Compiler::new(&resolved_program).compile();
        self.check_diagnostics(&compiled.diagnostics)?;
        let module = compiled
            .module
            .ok_or_else(|| "script compilation produced no module".to_owned())?;

        let mut vm = Vm::with_globals(module, self.globals.clone()).map_err(runtime_message)?;
        vm.builtins = self.builtins.clone();
        if let Some(current) = current {
            vm.host_context.current_tab = Some(current.tab.get());
            vm.host_context.current_window = Some(current.window.get());
            vm.host_context.current_buffer = Some(current.buffer.get());
        }

        let task_res = self.scheduler.spawn(vm).map_err(runtime_message);
        let value = match task_res {
            Ok(task) => {
                let run_res = self
                    .scheduler
                    .run_until_complete(task)
                    .map_err(runtime_message);
                if let Some(task_info) = self.scheduler.task(task) {
                    self.globals = task_info.vm.globals.clone();
                }
                run_res
            }
            Err(e) => Err(e),
        };

        let value = value?;
        if let Some(host) = self.scheduler.host_mut() {
            self.pending_user_commands
                .extend(host.take_registered_user_commands());
            self.pending_user_commands.extend(
                host.take_removed_user_commands()
                    .into_iter()
                    .map(|name| format!("-:{name}")),
            );
        }
        Ok(value)
    }

    pub fn take_user_command_events(&mut self) -> Vec<crate::kernel::EditorEvent> {
        self.pending_user_commands
            .drain(..)
            .map(|name| {
                if let Some(name) = name.strip_prefix("-:") {
                    crate::kernel::EditorEvent::UserCommandRemoved {
                        name: name.to_owned(),
                    }
                } else {
                    crate::kernel::EditorEvent::UserCommandRegistered { name }
                }
            })
            .collect()
    }

    pub fn peek_command(&self, source: &str) -> Result<Command, String> {
        let mut source = source;
        if let Some(stripped) = source.strip_prefix(':') {
            if stripped
                .trim_start()
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            {
                source = stripped;
            }
        }

        let parsed = vim_script::ex_parser::ExLineParser::new(vim_script::SourceId(0), source, 0)
            .parse()
            .map_err(|diagnostic| diagnostic.message.clone())?;

        let host = self.scheduler.host().expect("script host is installed");

        let mut request = vim_script::host::CommandRequest {
            command: parsed.command,
            context: vim_script::host::HostContext::default(),
        };

        let definition = host
            .commands
            .resolve(&request.command.name)
            .map_err(runtime_message)?;
        request.command.name = definition.name.clone();

        if request.command.bang && !definition.accepts_bang {
            return Err(format!("{} does not accept !", definition.name));
        }
        if request.command.range.is_some() && !definition.accepts_range {
            return Err(format!("{} does not accept a range", definition.name));
        }
        if definition.accepts_count || definition.accepts_register {
            let (count, register, remaining) = parse_count_and_register_helper(
                &request.command.arguments,
                definition.accepts_count,
                definition.accepts_register,
            );
            request.command.count = count;
            request.command.register = register;
            request.command.arguments = remaining;
        }

        commands::execute(request).map_err(runtime_message)
    }

    pub fn try_next_emitted_command(&self) -> Option<EmittedCommand> {
        self.commands.try_recv().ok()
    }

    /// Resolves the matching callback set in registration order. `++once`
    /// handlers are consumed by the script event bus during this snapshot.
    pub fn snapshot_autocmd_commands(
        &mut self,
        envelope: &AutocmdEventEnvelope,
    ) -> Vec<vim_script::host::CommandRequest> {
        self.scheduler
            .host_mut()
            .expect("script host is installed")
            .event_commands(&envelope.event, envelope.context.clone())
    }

    /// Converts a snapshotted callback into the same owned command envelope
    /// used by ordinary script-host requests. The runtime performs the final
    /// identity admission through `ExDispatcher`.
    pub fn execute_autocmd_snapshot(
        &self,
        request: vim_script::host::CommandRequest,
    ) -> Result<EmittedCommand, String> {
        let command = commands::execute(request.clone()).map_err(runtime_message)?;
        Ok(EmittedCommand {
            command,
            context: request.context,
        })
    }

    /// Compatibility accessor for callers that do not yet consume host
    /// context. Runtime code must use [`Self::try_next_emitted_command`].
    #[deprecated(note = "use try_next_emitted_command to preserve host context")]
    pub fn try_next_command(&self) -> Option<Command> {
        self.try_next_emitted_command()
            .map(|emitted| emitted.command)
    }

    pub fn update_state(
        &self,
        model: &crate::model::EditorModel,
        current_id: vim_buffer::BufferId,
    ) -> Result<(), String> {
        let active_ids = model.buffers().list();
        let current_buffer_id = text::BufferId::new(current_id.get())
            .map_err(|_| format!("invalid current buffer id: {}", current_id.get()))?;

        let mut lock = self
            .state
            .lock()
            .map_err(|_| "Editor state lock is poisoned".to_owned())?;
        lock.current_buffer_id = Some(current_buffer_id);
        // Remove buffers that are no longer listed/active
        lock.buffers.retain(|id, _| {
            active_ids
                .iter()
                .any(|&active_id| active_id.get() == id.to_proto())
        });
        lock.names.clear();

        for id in active_ids {
            if let Ok(buffer) = model.buffers().get(id) {
                let text_id = text::BufferId::new(id.get()).unwrap();
                let current_tick = buffer.changedtick().get();

                let needs_update = match lock.buffers.get(&text_id) {
                    Some((_, existing_tick)) => *existing_tick != current_tick,
                    None => true,
                };

                if needs_update {
                    let snapshot = buffer.as_text_buffer().snapshot().clone();
                    lock.buffers.insert(text_id, (snapshot, current_tick));
                }

                if let Some(path) = buffer.path() {
                    lock.names.insert(path.to_path_buf(), text_id);
                }
            }
        }

        Ok(())
    }

    pub fn read_state<F, T>(&self, f: F) -> Result<T, String>
    where
        F: FnOnce(&EditorState) -> T,
    {
        let lock = self
            .state
            .lock()
            .map_err(|_| "Editor state lock is poisoned".to_string())?;
        Ok(f(&*lock))
    }

    fn check_diagnostics(&self, diagnostics: &[vim_script::Diagnostic]) -> Result<(), String> {
        if diagnostics.is_empty() {
            return Ok(());
        }
        Err(diagnostics
            .iter()
            .map(|diagnostic| self.sources.render(diagnostic))
            .collect::<Vec<_>>()
            .join("\n"))
    }
}

impl Default for ScriptRuntime {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone)]
pub struct EditorState {
    pub buffers: HashMap<BufferId, (BufferSnapshot, u64)>,
    pub names: HashMap<PathBuf, BufferId>,
    pub current_buffer_id: Option<BufferId>,
}

impl std::fmt::Debug for EditorState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EditorState")
            .field("buffers_count", &self.buffers.len())
            .field("names", &self.names)
            .field("current_buffer_id", &self.current_buffer_id)
            .finish()
    }
}

impl Default for EditorState {
    fn default() -> Self {
        Self {
            buffers: HashMap::new(),
            names: HashMap::new(),
            current_buffer_id: None,
        }
    }
}

struct EditorHost {
    sender: mpsc::Sender<EmittedCommand>,
    state: Arc<Mutex<EditorState>>,
    options: Arc<std::sync::RwLock<crate::app::config::ConfigStore>>,
}

impl Host for EditorHost {
    fn call(&self, request: HostRequest) -> HostFuture {
        let sender = self.sender.clone();
        Box::pin(async move {
            match request.function.as_str() {
                "echo" | "message" | "echomsg" => {
                    expect_arity(&request, 1)?;
                    let message = request.arguments[0].to_string();
                    sender
                        .send(EmittedCommand {
                            command: Command::Echo { message },
                            context: request.context.clone(),
                        })
                        .map_err(|_| {
                            RuntimeError::coded(
                                "E_HOST",
                                RuntimeErrorKind::HostError,
                                "editor command queue is closed",
                            )
                        })?;
                    Ok(Value::Null)
                }

                name => Err(RuntimeError::coded(
                    "E117",
                    RuntimeErrorKind::NameError,
                    format!("unknown host function: {name}"),
                )),
            }
        })
    }

    fn call_sync(&self, request: HostRequest) -> Option<RuntimeResult<Value>> {
        functions::call_sync(&self.state, &request)
    }

    fn editor(&self, request: EditorRequest) -> HostFuture {
        let state = self.state.clone();
        let sender = self.sender.clone();
        Box::pin(async move {
            let response = match request.operation {
                EditorRequestOperation::CurrentContext => EditorResponse::Context(request.context),
                EditorRequestOperation::BufferText { buffer, range } => {
                    let id = text::BufferId::new(buffer).map_err(|_| {
                        RuntimeError::coded("E86", RuntimeErrorKind::HostError, "invalid buffer ID")
                    })?;
                    let state = state.lock().map_err(|_| {
                        RuntimeError::coded(
                            "E_HOST",
                            RuntimeErrorKind::HostError,
                            "editor state lock poisoned",
                        )
                    })?;
                    let (snapshot, _) = state.buffers.get(&id).ok_or_else(|| {
                        RuntimeError::coded(
                            "E86",
                            RuntimeErrorKind::HostError,
                            format!("stale buffer ID: {buffer}"),
                        )
                    })?;
                    let start = usize::try_from(range.start)
                        .unwrap_or(usize::MAX)
                        .min(snapshot.len());
                    let end = usize::try_from(range.end)
                        .unwrap_or(usize::MAX)
                        .min(snapshot.len());
                    if start > end {
                        return Err(RuntimeError::coded(
                            "E16",
                            RuntimeErrorKind::InvalidCommand,
                            "invalid buffer range",
                        ));
                    }
                    EditorResponse::Text(snapshot.text_for_range(start..end).collect())
                }
                EditorRequestOperation::ReplaceBuffer {
                    buffer,
                    range,
                    text,
                } => {
                    sender
                        .send(EmittedCommand {
                            command: Command::ReplaceBuffer {
                                buffer,
                                range,
                                text,
                            },
                            context: request.context,
                        })
                        .map_err(|_| {
                            RuntimeError::coded(
                                "E_HOST",
                                RuntimeErrorKind::HostError,
                                "editor command queue is closed",
                            )
                        })?;
                    return Ok(Value::Null);
                }
                EditorRequestOperation::Window(operation) => {
                    let action = match operation {
                        WindowRequestOperation::SplitHorizontal => {
                            vim_input::Action::SplitHorizontal { file_path: None }
                        }
                        WindowRequestOperation::SplitVertical => {
                            vim_input::Action::SplitVertical { file_path: None }
                        }
                    };
                    sender
                        .send(EmittedCommand {
                            command: Command::Editor {
                                action,
                                register: None,
                            },
                            context: request.context,
                        })
                        .map_err(|_| {
                            RuntimeError::coded(
                                "E_HOST",
                                RuntimeErrorKind::HostError,
                                "editor command queue is closed",
                            )
                        })?;
                    return Ok(Value::Null);
                }
                EditorRequestOperation::Tab(operation) => {
                    let command = match operation {
                        TabRequestOperation::Next { count } => Command::TabNext { count },
                        TabRequestOperation::Previous { count } => Command::TabPrevious { count },
                        TabRequestOperation::Close => Command::TabClose,
                    };
                    sender
                        .send(EmittedCommand {
                            command,
                            context: request.context,
                        })
                        .map_err(|_| {
                            RuntimeError::coded(
                                "E_HOST",
                                RuntimeErrorKind::HostError,
                                "editor command queue is closed",
                            )
                        })?;
                    return Ok(Value::Null);
                }
                EditorRequestOperation::RegisterEvent {
                    event,
                    pattern,
                    command,
                    once,
                    nested,
                } => {
                    let mut registration = format!("autocmd");
                    if once {
                        registration.push_str(" ++once");
                    }
                    if nested {
                        registration.push_str(" ++nested");
                    }
                    registration.push_str(&format!(" {event} {pattern} {command}"));
                    sender
                        .send(EmittedCommand {
                            command: Command::ExecuteScript(registration),
                            context: request.context,
                        })
                        .map_err(|_| {
                            RuntimeError::coded(
                                "E_HOST",
                                RuntimeErrorKind::HostError,
                                "editor command queue is closed",
                            )
                        })?;
                    return Ok(Value::Null);
                }
                EditorRequestOperation::Message { text } => {
                    sender
                        .send(EmittedCommand {
                            command: Command::Echo { message: text },
                            context: request.context,
                        })
                        .map_err(|_| {
                            RuntimeError::coded(
                                "E_HOST",
                                RuntimeErrorKind::HostError,
                                "editor command queue is closed",
                            )
                        })?;
                    return Ok(Value::Null);
                }
                EditorRequestOperation::Prompt { message } => {
                    sender
                        .send(EmittedCommand {
                            command: Command::OpenPrompt { message },
                            context: request.context,
                        })
                        .map_err(|_| {
                            RuntimeError::coded(
                                "E_HOST",
                                RuntimeErrorKind::HostError,
                                "editor command queue is closed",
                            )
                        })?;
                    return Ok(Value::Null);
                }
                EditorRequestOperation::Selection { .. }
                | EditorRequestOperation::Register { .. }
                | EditorRequestOperation::Mark { .. } => {
                    return Err(RuntimeError::coded(
                        "E_NOTIMPL",
                        RuntimeErrorKind::HostError,
                        "editor request is not connected yet",
                    ));
                }
            };
            Ok(editor_response_value(response))
        })
    }

    fn option(&self, request: OptionRequest) -> HostFuture {
        let options = self.options.clone();
        let sender = self.sender.clone();
        Box::pin(async move {
            let (buffer, window) = match request.scope {
                OptionRequestScope::Global => (None, None),
                OptionRequestScope::Local | OptionRequestScope::Unqualified => (
                    request
                        .context
                        .current_buffer
                        .and_then(vim_buffer::BufferId::new),
                    request.context.current_window.map(vim_ui::WindowId::new),
                ),
            };
            match request.operation {
                OptionRequestOperation::Get => {
                    let options = options.read().map_err(|_| {
                        RuntimeError::coded(
                            "E_HOST",
                            RuntimeErrorKind::HostError,
                            "option store lock poisoned",
                        )
                    })?;
                    let value = options.get(&request.name, buffer, window).ok_or_else(|| {
                        RuntimeError::coded(
                            "E_UNKNOWN_OPTION",
                            RuntimeErrorKind::NameError,
                            format!("unknown option '{}'", request.name),
                        )
                    })?;
                    Ok(match value {
                        crate::app::config::ConfigValue::Bool(value) => Value::Bool(value),
                        crate::app::config::ConfigValue::Number(value) => Value::Integer(value),
                        crate::app::config::ConfigValue::String(value) => {
                            Value::String(value.into())
                        }
                    })
                }
                OptionRequestOperation::Set(value) => {
                    let config_value = match &value {
                        Value::Bool(value) => crate::app::config::ConfigValue::Bool(*value),
                        Value::Integer(value) => crate::app::config::ConfigValue::Number(*value),
                        Value::String(value) => {
                            crate::app::config::ConfigValue::String(value.to_string())
                        }
                        _ => {
                            return Err(RuntimeError::coded(
                                "E_OPTION_TYPE",
                                RuntimeErrorKind::TypeError,
                                format!("invalid value type for option '{}'", request.name),
                            ));
                        }
                    };
                    options
                        .read()
                        .map_err(|_| {
                            RuntimeError::coded(
                                "E_HOST",
                                RuntimeErrorKind::HostError,
                                "option store lock poisoned",
                            )
                        })?
                        .validate_value(&request.name, &config_value)
                        .map_err(|error| {
                            RuntimeError::coded("E_OPTION", RuntimeErrorKind::InvalidCommand, error)
                        })?;
                    sender
                        .send(EmittedCommand {
                            command: Command::SetOption {
                                name: request.name,
                                value,
                                scope: request.scope,
                            },
                            context: request.context,
                        })
                        .map_err(|_| {
                            RuntimeError::coded(
                                "E_HOST",
                                RuntimeErrorKind::HostError,
                                "editor command queue is closed",
                            )
                        })?;
                    Ok(Value::Null)
                }
            }
        })
    }

    fn execute_command(&self, request: CommandRequest) -> HostFuture {
        let sender = self.sender.clone();
        Box::pin(async move {
            let context = request.context.clone();
            let command = commands::execute(request)?;
            sender
                .send(EmittedCommand { command, context })
                .map_err(|_| {
                    RuntimeError::coded(
                        "E_HOST",
                        RuntimeErrorKind::HostError,
                        "editor command queue is closed",
                    )
                })?;
            Ok(Value::Null)
        })
    }
}

fn editor_response_value(response: EditorResponse) -> Value {
    match response {
        EditorResponse::Context(context) => Value::List(vec![
            Value::Integer(context.current_tab.unwrap_or(0) as i64),
            Value::Integer(context.current_window.unwrap_or(0) as i64),
            Value::Integer(context.current_buffer.unwrap_or(0) as i64),
        ]),
        EditorResponse::Text(text) => Value::String(text.into()),
        EditorResponse::Range(range) => Value::List(vec![
            Value::Integer(range.start as i64),
            Value::Integer(range.end as i64),
        ]),
        EditorResponse::Register(value) => value,
        EditorResponse::Mark(offset) => {
            offset.map_or(Value::Null, |offset| Value::Integer(offset as i64))
        }
    }
}

fn expect_arity(request: &HostRequest, expected: usize) -> RuntimeResult<()> {
    if request.arguments.len() == expected {
        Ok(())
    } else {
        Err(RuntimeError::coded(
            "E119",
            RuntimeErrorKind::ArityError,
            format!(
                "{} expects {expected} argument(s), got {}",
                request.function,
                request.arguments.len()
            ),
        ))
    }
}

fn runtime_message(error: RuntimeError) -> String {
    match error.code {
        Some(code) => format!("{code}: {}", error.message),
        None => error.message,
    }
}

fn parse_count_and_register_helper(
    arguments: &str,
    accepts_count: bool,
    accepts_register: bool,
) -> (Option<u64>, Option<char>, String) {
    let mut count = None;
    let mut register = None;
    let mut remaining = String::new();

    let words: Vec<&str> = arguments.split_whitespace().collect();
    let mut idx = 0;

    if idx < words.len() && accepts_register {
        let word = words[idx];
        if word.len() == 1 {
            let ch = word.chars().next().unwrap();
            let is_number = ch.is_ascii_digit();
            if !is_number || !accepts_count {
                register = Some(ch);
                idx += 1;
            }
        }
    }

    if idx < words.len() && accepts_count {
        let word = words[idx];
        if let Ok(c) = word.parse::<u64>() {
            count = Some(c);
            idx += 1;
        }
    }

    if idx < words.len() {
        remaining = words[idx..].join(" ");
    }

    (count, register, remaining)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_peek_command() {
        let runtime = ScriptRuntime::new();
        let cmd = runtime.peek_command("write! output.txt").unwrap();
        assert!(matches!(
            cmd,
            Command::Save {
                path: Some(path),
                force: true,
            } if path == PathBuf::from("output.txt")
        ));
    }

    #[test]
    fn test_central_registry_specifications() {
        let mut host = HostRuntime::new(Arc::new(EditorHost {
            sender: mpsc::channel().0,
            state: Arc::new(Mutex::new(EditorState::default())),
            options: Arc::new(std::sync::RwLock::new(
                crate::app::config::ConfigStore::new(),
            )),
        }));
        for spec in COMMAND_SPECS {
            host.register_command(CommandDefinition::from(spec));
        }

        // Verify standard vs extension flag
        for spec in COMMAND_SPECS {
            if spec.is_extension {
                assert!(
                    spec.name == "save"
                        || spec.name == "nexttab"
                        || spec.name == "previoustab"
                        || spec.name == "bprev"
                );
            } else {
                assert!(
                    spec.name != "save"
                        && spec.name != "nexttab"
                        && spec.name != "previoustab"
                        && spec.name != "bprev"
                );
            }

            // Test abbreviations and aliases
            let def = host.commands.resolve(spec.name).unwrap();
            assert_eq!(def.name, spec.name);

            // Test alias resolution
            for (alias, min_abbr) in spec.aliases {
                let resolved = host.commands.resolve(alias).unwrap();
                assert_eq!(resolved.name, spec.name);

                // Test min abbreviation for alias
                if alias.len() > *min_abbr {
                    let abbr = &alias[..*min_abbr];
                    let resolved_abbr = host.commands.resolve(abbr).unwrap();
                    assert_eq!(resolved_abbr.name, spec.name);
                }
            }

            // Test min abbreviation of canonical name
            if spec.name.len() > spec.minimum_abbreviation {
                let abbr = &spec.name[..spec.minimum_abbreviation];
                let resolved = host.commands.resolve(abbr).unwrap();
                assert_eq!(resolved.name, spec.name);
            }
        }
    }

    #[test]
    fn emitted_commands_preserve_the_execution_context() {
        let current = crate::kernel::EditorContext {
            tab: crate::kernel::TabPageId::new(7),
            window: crate::kernel::WindowId::new(11),
            buffer: crate::kernel::BufferId::new(13).unwrap(),
        };
        let mut runtime = ScriptRuntime::new();
        runtime.execute_with_context(":q", Some(current)).unwrap();
        let emitted = runtime.try_next_emitted_command().unwrap();
        assert_eq!(emitted.editor_context(), Some(current));
        assert!(matches!(emitted.command, Command::Quit { force: false }));
    }

    #[test]
    fn quit_and_its_abbreviation_are_dispatched() {
        for source in ["q", "quit"] {
            let mut runtime = ScriptRuntime::new();
            runtime.execute(source).unwrap();
            assert!(matches!(
                runtime.try_next_command(),
                Some(Command::Quit { force: false })
            ));
        }
    }

    #[test]
    fn qall_and_its_abbreviation_are_dispatched_as_quit_all() {
        for source in ["qa", "qall", "qa!"] {
            let mut runtime = ScriptRuntime::new();
            runtime.execute(source).unwrap();
            let force = source.ends_with('!');
            assert!(matches!(
                runtime.try_next_command(),
                Some(Command::QuitAll { force: actual }) if actual == force
            ));
        }
    }

    #[test]
    fn navigation_commands_are_dispatched() {
        let mut runtime = ScriptRuntime::new();
        runtime.execute("bnext").unwrap();
        assert!(matches!(
            runtime.try_next_command(),
            Some(Command::BufferNext { count: 1 })
        ));

        let mut runtime = ScriptRuntime::new();
        runtime.execute("previoustab").unwrap();
        assert!(matches!(
            runtime.try_next_command(),
            Some(Command::TabPrevious { count: 1 })
        ));

        let mut runtime = ScriptRuntime::new();
        runtime.execute("tabnew notes.txt").unwrap();
        assert!(matches!(
            runtime.try_next_command(),
            Some(Command::TabNew { path: Some(path) }) if path == PathBuf::from("notes.txt")
        ));

        let mut runtime = ScriptRuntime::new();
        runtime.execute("tabclose").unwrap();
        assert!(matches!(
            runtime.try_next_command(),
            Some(Command::TabClose)
        ));
    }

    #[test]
    fn write_commands_preserve_path_and_force() {
        let mut runtime = ScriptRuntime::new();
        runtime.execute("write! output.txt").unwrap();
        assert!(matches!(
            runtime.try_next_command(),
            Some(Command::Save {
                path: Some(path),
                force: true,
            }) if path == PathBuf::from("output.txt")
        ));
    }

    #[test]
    fn unknown_commands_are_rejected_by_the_engine() {
        let mut runtime = ScriptRuntime::new();
        assert!(runtime.execute("missing").is_err());
        assert!(runtime.try_next_command().is_none());
    }

    #[test]
    fn delete_commands_with_range_and_register_are_dispatched() {
        let mut runtime = ScriptRuntime::new();
        runtime.execute(":1,2d a").unwrap();
        let cmd = runtime.try_next_command().unwrap();
        if let Command::RangeOp {
            operation,
            range,
            count,
            register,
            ..
        } = cmd
        {
            assert_eq!(operation, crate::app::range_ops::RangeOperation::Delete);
            assert!(range.is_some());
            assert_eq!(count, None);
            assert_eq!(register, Some('a'));
        } else {
            panic!("Expected Command::RangeOp, got {:?}", cmd);
        }
    }

    #[test]
    fn test_nohl_commands_are_dispatched() {
        for source in ["nohl", "nohlsearch", ":nohl", ":nohlsearch"] {
            let mut runtime = ScriptRuntime::new();
            runtime.execute(source).unwrap();
            assert!(matches!(
                runtime.try_next_command(),
                Some(Command::ClearSearchHighlight)
            ));
        }
    }

    #[test]
    fn test_x_command() {
        let mut runtime = ScriptRuntime::new();
        runtime.execute(":1,2x").unwrap();
        assert!(matches!(
            runtime.try_next_command(),
            Some(Command::WriteQuit {
                path: None,
                force: false
            })
        ));

        let mut runtime = ScriptRuntime::new();
        runtime.execute(":xa").unwrap();
        assert!(matches!(
            runtime.try_next_command(),
            Some(Command::WriteQuitAll { force: false })
        ));
    }

    #[test]
    fn new_and_vnew_commands_are_dispatched() {
        for (source, vertical) in [("new", false), ("vnew", true), ("vne", true)] {
            let mut runtime = ScriptRuntime::new();
            runtime.execute(source).unwrap();
            assert!(matches!(
                runtime.try_next_command(),
                Some(Command::SplitNew { vertical: actual }) if actual == vertical
            ));
        }
    }

    #[test]
    fn split_and_vsplit_commands_are_dispatched() {
        // Test :split with no file path
        let mut runtime = ScriptRuntime::new();
        runtime.execute("split").unwrap();
        assert!(matches!(
            runtime.try_next_command(),
            Some(Command::Editor {
                action: vim_input::Action::SplitHorizontal { file_path: None },
                register: None,
            })
        ));

        // Test :vsplit with file path
        let mut runtime = ScriptRuntime::new();
        runtime.execute("vsplit my_file.rs").unwrap();
        assert!(matches!(
            runtime.try_next_command(),
            Some(Command::Editor {
                action: vim_input::Action::SplitVertical { file_path: Some(path) },
                register: None,
            }) if path == "my_file.rs"
        ));

        // Test abbreviations :sp and :vs
        let mut runtime = ScriptRuntime::new();
        runtime.execute("sp").unwrap();
        assert!(matches!(
            runtime.try_next_command(),
            Some(Command::Editor {
                action: vim_input::Action::SplitHorizontal { file_path: None },
                register: None,
            })
        ));

        let mut runtime = ScriptRuntime::new();
        runtime.execute("vs").unwrap();
        assert!(matches!(
            runtime.try_next_command(),
            Some(Command::Editor {
                action: vim_input::Action::SplitVertical { file_path: None },
                register: None,
            })
        ));
    }

    #[test]
    fn test_colorscheme_command_is_dispatched() {
        let mut runtime = ScriptRuntime::new();
        runtime.execute("colorscheme").unwrap();
        assert!(matches!(
            runtime.try_next_command(),
            Some(Command::Colorscheme { name: None })
        ));

        let mut runtime = ScriptRuntime::new();
        runtime.execute("colo tokyonight").unwrap();
        assert!(matches!(
            runtime.try_next_command(),
            Some(Command::Colorscheme { name: Some(ref name) }) if name == "tokyonight"
        ));
    }

    #[test]
    fn test_echo_command_is_dispatched() {
        let mut runtime = ScriptRuntime::new();
        runtime.execute("message('hello world')").unwrap();
        assert!(matches!(
            runtime.try_next_command(),
            Some(Command::Echo { ref message }) if message == "hello world"
        ));

        let mut runtime = ScriptRuntime::new();
        runtime.execute("message('message')").unwrap();
        assert!(matches!(
            runtime.try_next_command(),
            Some(Command::Echo { ref message }) if message == "message"
        ));

        let mut runtime = ScriptRuntime::new();
        runtime.execute("echo 'hello world'").unwrap();
        assert!(matches!(
            runtime.try_next_command(),
            Some(Command::Echo { ref message }) if message == "hello world"
        ));

        let mut runtime = ScriptRuntime::new();
        runtime.execute("echo 123").unwrap();
        assert!(matches!(
            runtime.try_next_command(),
            Some(Command::Echo { ref message }) if message == "123"
        ));

        let mut runtime = ScriptRuntime::new();
        runtime.execute("echo 1+1").unwrap();
        assert!(matches!(
            runtime.try_next_command(),
            Some(Command::Echo { ref message }) if message == "2"
        ));

        let mut runtime = ScriptRuntime::new();
        runtime.execute("let x = 123").unwrap();
        runtime.execute("echo x").unwrap();
        assert!(matches!(
            runtime.try_next_command(),
            Some(Command::Echo { ref message }) if message == "123"
        ));
    }

    #[test]
    fn test_set_command_is_dispatched() {
        let mut runtime = ScriptRuntime::new();
        runtime.execute("set number").unwrap();
        assert!(matches!(
            runtime.try_next_command(),
            Some(Command::Set { ref arguments }) if arguments == "number"
        ));

        let mut runtime = ScriptRuntime::new();
        runtime.execute("se ts=4").unwrap();
        assert!(matches!(
            runtime.try_next_command(),
            Some(Command::Set { ref arguments }) if arguments == "ts=4"
        ));
    }

    #[test]
    fn test_syntax_command_is_dispatched() {
        let mut runtime = ScriptRuntime::new();
        runtime.execute("syntax on").unwrap();
        assert!(matches!(
            runtime.try_next_command(),
            Some(Command::Syntax { enable: true })
        ));

        let mut runtime = ScriptRuntime::new();
        runtime.execute("syn off").unwrap();
        assert!(matches!(
            runtime.try_next_command(),
            Some(Command::Syntax { enable: false })
        ));

        let mut runtime = ScriptRuntime::new();
        assert!(runtime.execute("syntax invalid").is_err());
    }

    #[test]
    fn test_treesitter_command_is_dispatched() {
        let mut runtime = ScriptRuntime::new();
        runtime.execute("treesitter on").unwrap();
        assert!(matches!(
            runtime.try_next_command(),
            Some(Command::Treesitter { enable: true })
        ));

        let mut runtime = ScriptRuntime::new();
        runtime.execute("tre off").unwrap();
        assert!(matches!(
            runtime.try_next_command(),
            Some(Command::Treesitter { enable: false })
        ));

        let mut runtime = ScriptRuntime::new();
        assert!(runtime.execute("treesitter invalid").is_err());
    }

    #[test]
    fn test_indexer_command_is_dispatched() {
        let mut runtime = ScriptRuntime::new();
        runtime.execute("indexer on").unwrap();
        assert!(matches!(
            runtime.try_next_command(),
            Some(Command::Indexer { enable: true })
        ));

        let mut runtime = ScriptRuntime::new();
        runtime.execute("ind off").unwrap();
        assert!(matches!(
            runtime.try_next_command(),
            Some(Command::Indexer { enable: false })
        ));

        let mut runtime = ScriptRuntime::new();
        assert!(runtime.execute("indexer invalid").is_err());
    }

    #[test]
    fn test_substitute_command_is_dispatched() {
        for source in [
            "substitute /foo/bar/",
            "s /foo/bar/",
            "&",
            "~",
            "smagic /foo/bar/",
            "snomagic /foo/bar/",
        ] {
            let mut runtime = ScriptRuntime::new();
            runtime.execute(source).unwrap();
            assert!(matches!(
                runtime.try_next_command(),
                Some(Command::Substitute { ref pattern, .. }) if pattern == "foo" || pattern.is_empty()
            ));
        }
    }

    #[test]
    fn option_assignment_emits_a_typed_host_command() {
        let mut runtime = ScriptRuntime::new();

        runtime.execute("let &g:nu = v:true").unwrap();
        assert!(matches!(
            runtime.try_next_emitted_command(),
            Some(EmittedCommand {
                command: Command::SetOption {
                    ref name,
                    value: Value::Bool(true),
                    scope: OptionRequestScope::Global,
                },
                ..
            }) if name == "nu"
        ));
    }

    #[test]
    fn test_search_commands_are_dispatched() {
        let mut runtime = ScriptRuntime::new();
        runtime.execute("/foo/").unwrap();
        assert!(matches!(
            runtime.try_next_command(),
            Some(Command::SearchForward { ref pattern }) if pattern == "foo/"
        ));

        let mut runtime = ScriptRuntime::new();
        runtime.execute("?bar?").unwrap();
        assert!(matches!(
            runtime.try_next_command(),
            Some(Command::SearchBackward { ref pattern }) if pattern == "bar?"
        ));
    }
}
