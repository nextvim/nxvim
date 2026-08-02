use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    task::{Context, Poll, Wake, Waker},
};

use vim_buffer::{
    Action, ActionOutcome, BufferId, BufferManager, BufferOptions, ByteOffset, Callback,
    CallbackContext, Edit, EditOrigin, FileFormat, Mutator, PlannedEdit, Point, TextRange,
    VimEvent,
};
use vim_script::{
    compiler::Compiler,
    host::{
        Arity, Capability, CapabilitySet, CommandDefinition, CommandRequest, Host, HostContext,
        HostFuture, HostRequest, HostRuntime, OptionRequest, OptionRequestOperation,
    },
    integration::{Event, EventAction},
    lexer::Lexer,
    parser::Parser,
    resolver::{Resolver, ResolverConfig},
    runtime::{RuntimeError, RuntimeErrorKind, Scheduler, Value, Vm},
    source::SourceMap,
};

use crate::EditorError;

#[derive(Clone, Debug)]
struct PendingEvent {
    event: VimEvent,
    buffer: BufferId,
    file: Option<String>,
    matched: Option<String>,
}

struct EventCollector {
    queue: Arc<Mutex<Vec<PendingEvent>>>,
    trace: Arc<Mutex<Vec<(VimEvent, BufferId)>>>,
}

impl Callback for EventCollector {
    fn call(&mut self, event: VimEvent, context: &CallbackContext<'_>) {
        let pending = PendingEvent {
            event,
            buffer: context.buffer,
            file: context.file.map(str::to_owned),
            matched: context.matched.map(str::to_owned),
        };
        if let Ok(mut trace) = self.trace.lock() {
            trace.push((event, context.buffer));
        }
        if let Ok(mut queue) = self.queue.lock() {
            queue.push(pending);
        }
    }
}

struct EditorState {
    buffers: BufferManager,
    mutator: Mutator,
    exit_code: Option<u8>,
}

#[derive(Clone)]
struct EditorHost {
    state: Arc<Mutex<EditorState>>,
}

pub struct HeadlessEditor {
    state: Arc<Mutex<EditorState>>,
    host: HostRuntime,
    sources: SourceMap,
    globals: HashMap<String, Value>,
    events: Arc<Mutex<Vec<PendingEvent>>>,
    event_trace: Arc<Mutex<Vec<(VimEvent, BufferId)>>>,
    autocmd_budget: usize,
}

impl HeadlessEditor {
    pub fn new() -> Result<Self, EditorError> {
        let events = Arc::new(Mutex::new(Vec::new()));
        let event_trace = Arc::new(Mutex::new(Vec::new()));
        let mut mutator = Mutator::default();
        mutator.callbacks_mut().register(EventCollector {
            queue: Arc::clone(&events),
            trace: Arc::clone(&event_trace),
        });
        let mut state = EditorState {
            buffers: BufferManager::new(),
            mutator,
            exit_code: None,
        };
        let created = state.mutator.execute(
            &mut state.buffers,
            Action::Create {
                initial_text: String::new(),
            },
        )?;
        let ActionOutcome::Manager(vim_buffer::ManagerOutcome::Added(buffer)) = created else {
            return Err(EditorError::State(
                "initial buffer creation returned an invalid outcome",
            ));
        };
        state
            .mutator
            .execute(&mut state.buffers, Action::SetCurrent { buffer })?;
        events
            .lock()
            .map_err(|_| EditorError::State("event queue lock is poisoned"))?
            .clear();
        event_trace
            .lock()
            .map_err(|_| EditorError::State("event trace lock is poisoned"))?
            .clear();

        let state = Arc::new(Mutex::new(state));
        let mut host = HostRuntime::new(Arc::new(EditorHost {
            state: Arc::clone(&state),
        }));
        host.capabilities = CapabilitySet::from([Capability::BufferRead, Capability::BufferWrite]);
        host.register_function(
            "bufnr",
            Arity::Range { min: 0, max: 1 },
            vec![Capability::BufferRead],
        );
        host.register_function("getline", Arity::Exact(1), vec![Capability::BufferRead]);
        host.register_function("setline", Arity::Exact(2), vec![Capability::BufferWrite]);
        register_commands(&mut host);

        Ok(Self {
            state,
            host,
            sources: SourceMap::default(),
            globals: HashMap::from([
                ("v:version".into(), Value::Integer(902)),
                ("v:true".into(), Value::Bool(true)),
                ("v:false".into(), Value::Bool(false)),
                ("v:null".into(), Value::Null),
            ]),
            events,
            event_trace,
            autocmd_budget: 1_000,
        })
    }

    pub fn current_buffer(&self) -> Result<BufferId, EditorError> {
        self.with_state(|state| {
            state
                .buffers
                .current()
                .ok_or(EditorError::State("headless editor has no current buffer"))
        })
    }

    pub fn buffer_text(&self, buffer: BufferId) -> Result<String, EditorError> {
        self.with_state(|state| Ok(state.buffers.get(buffer)?.snapshot().chunks().collect()))
    }

    pub fn current_text(&self) -> Result<String, EditorError> {
        self.buffer_text(self.current_buffer()?)
    }

    pub fn changedtick(&self, buffer: BufferId) -> Result<u64, EditorError> {
        self.with_state(|state| Ok(state.buffers.get(buffer)?.changedtick().get()))
    }

    pub fn listed_buffers(&self) -> Result<Vec<BufferId>, EditorError> {
        self.with_state(|state| Ok(state.buffers.listed()))
    }

    pub fn buffer_exists(&self, buffer: BufferId) -> Result<bool, EditorError> {
        self.with_state(|state| Ok(state.buffers.get(buffer).is_ok()))
    }

    pub fn buffer_loaded(&self, buffer: BufferId) -> Result<bool, EditorError> {
        self.with_state(|state| Ok(state.buffers.get(buffer)?.is_loaded()))
    }

    pub fn alternate_buffer(&self) -> Result<Option<BufferId>, EditorError> {
        self.with_state(|state| Ok(state.buffers.alternate()))
    }

    pub fn option_value(&self, name: &str) -> Result<Value, EditorError> {
        self.with_state(|state| {
            let buffer = state
                .buffers
                .current()
                .ok_or(EditorError::State("headless editor has no current buffer"))?;
            option_value(state.buffers.get(buffer)?.options(), name).map_err(EditorError::Runtime)
        })
    }

    pub fn exit_requested(&self) -> Result<bool, EditorError> {
        self.with_state(|state| Ok(state.exit_code.is_some()))
    }

    pub fn requested_exit_code(&self) -> Result<Option<u8>, EditorError> {
        self.with_state(|state| Ok(state.exit_code))
    }

    pub fn event_trace(&self) -> Result<Vec<(VimEvent, BufferId)>, EditorError> {
        self.event_trace
            .lock()
            .map(|trace| trace.clone())
            .map_err(|_| EditorError::State("event trace lock is poisoned"))
    }

    pub fn clear_event_trace(&self) -> Result<(), EditorError> {
        self.event_trace
            .lock()
            .map(|mut trace| trace.clear())
            .map_err(|_| EditorError::State("event trace lock is poisoned"))
    }

    pub fn undo(&self) -> Result<bool, EditorError> {
        self.with_state(|state| {
            let buffer = state
                .buffers
                .current()
                .ok_or(EditorError::State("headless editor has no current buffer"))?;
            let EditorState {
                buffers, mutator, ..
            } = &mut *state;
            let outcome = mutator.execute(buffers, Action::Undo { buffer, count: 1 })?;
            Ok(matches!(outcome, ActionOutcome::Mutation(Some(_))))
        })
    }

    pub fn eval(
        &mut self,
        source_name: impl Into<String>,
        source: &str,
    ) -> Result<Value, EditorError> {
        let source_name = source_name.into();
        let source_id = self.sources.add(source_name.clone(), source);
        let lexed = Lexer::new(source_id, source).lex();
        if !lexed.diagnostics.is_empty() {
            return Err(EditorError::Diagnostics {
                stage: "lexing",
                diagnostics: lexed.diagnostics,
            });
        }
        let parsed = Parser::new_with_source(&lexed.tokens, source).parse();
        if !parsed.diagnostics.is_empty() {
            return Err(EditorError::Diagnostics {
                stage: "parsing",
                diagnostics: parsed.diagnostics,
            });
        }

        let mut resolver_config = ResolverConfig::default();
        resolver_config
            .builtins
            .extend(self.host.functions.names().map(str::to_owned));
        let resolved = Resolver::new(resolver_config)
            .resolve(parsed.program.expect("parser always returns a program"));
        if !resolved.diagnostics.is_empty() {
            return Err(EditorError::Diagnostics {
                stage: "resolution",
                diagnostics: resolved.diagnostics,
            });
        }
        let compiled = Compiler::new(
            &resolved
                .program
                .expect("resolver always returns a resolved program"),
        )
        .compile();
        if !compiled.diagnostics.is_empty() {
            return Err(EditorError::Diagnostics {
                stage: "compilation",
                diagnostics: compiled.diagnostics,
            });
        }

        let buffer = self.current_buffer()?;
        self.globals.insert(
            "b:changedtick".into(),
            Value::Integer(self.changedtick(buffer)? as i64),
        );
        let mut vm = Vm::with_globals(
            compiled.module.expect("compiler always returns a module"),
            self.globals.clone(),
        )?;
        vm.host_context = HostContext {
            script_name: Some(source_name),
            current_buffer: Some(buffer.get()),
            ..HostContext::default()
        };

        let mut scheduler = Scheduler::default();
        scheduler.set_host(self.host.clone());
        let task = scheduler.spawn(vm)?;
        let result = scheduler.run_until_complete(task)?;
        let completed = scheduler
            .task(task)
            .ok_or(EditorError::State("completed Vimscript task disappeared"))?;
        self.globals = completed.vm.globals.clone();
        self.globals.insert(
            "b:changedtick".into(),
            Value::Integer(self.changedtick(buffer)? as i64),
        );
        if let Some(host) = scheduler.host().cloned() {
            self.host = host;
        }
        self.drain_autocmds()?;
        Ok(result)
    }

    fn drain_autocmds(&mut self) -> Result<(), EditorError> {
        let mut executed = 0usize;
        loop {
            let pending = {
                let mut queue = self
                    .events
                    .lock()
                    .map_err(|_| EditorError::State("event queue lock is poisoned"))?;
                if queue.is_empty() {
                    None
                } else {
                    Some(queue.remove(0))
                }
            };
            let Some(pending) = pending else {
                return Ok(());
            };
            let event = Event {
                name: vim_event_name(pending.event).into(),
                pattern: pending.matched.clone(),
                payload: HashMap::from([
                    ("abuf".into(), Value::Integer(pending.buffer.get() as i64)),
                    (
                        "afile".into(),
                        pending
                            .file
                            .as_deref()
                            .map_or(Value::Null, |file| Value::String(Arc::from(file))),
                    ),
                ]),
            };
            let handlers = self.host.events.handlers_for(&event);
            for handler in handlers {
                executed += 1;
                if executed > self.autocmd_budget {
                    return Err(EditorError::Runtime(RuntimeError::coded(
                        "E218",
                        RuntimeErrorKind::ResourceLimit,
                        "autocommand recursion limit exceeded",
                    )));
                }
                let EventAction::Command(command) = handler.action else {
                    continue;
                };
                let request = CommandRequest {
                    command,
                    context: HostContext {
                        current_buffer: Some(pending.buffer.get()),
                        ..HostContext::default()
                    },
                };
                let request = self.host.prepare_command(request)?;
                let future = self.host.dispatch_command(request)?;
                let queued_before = self
                    .events
                    .lock()
                    .map_err(|_| EditorError::State("event queue lock is poisoned"))?
                    .len();
                poll_ready(future)?;
                if !handler.nested {
                    self.events
                        .lock()
                        .map_err(|_| EditorError::State("event queue lock is poisoned"))?
                        .truncate(queued_before);
                }
            }
        }
    }

    pub fn global(&self, name: &str) -> Option<&Value> {
        self.globals.get(name)
    }

    pub fn render_diagnostics(&self, error: &EditorError) -> Vec<String> {
        match error {
            EditorError::Diagnostics { diagnostics, .. } => diagnostics
                .iter()
                .map(|diagnostic| self.sources.render(diagnostic))
                .collect(),
            _ => Vec::new(),
        }
    }

    fn with_state<T>(
        &self,
        operation: impl FnOnce(&mut EditorState) -> Result<T, EditorError>,
    ) -> Result<T, EditorError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| EditorError::State("headless editor state lock is poisoned"))?;
        operation(&mut state)
    }
}

impl Host for EditorHost {
    fn call(&self, request: HostRequest) -> HostFuture {
        let state = Arc::clone(&self.state);
        Box::pin(async move {
            let mut state = state
                .lock()
                .map_err(|_| host_error("editor state lock is poisoned"))?;
            match request.function.as_str() {
                "bufnr" => {
                    let id = resolve_buffer(&state, request.arguments.first())?;
                    Ok(Value::Integer(id.map_or(-1, |id| id.get() as i64)))
                }
                "getline" => {
                    let line = integer_argument(&request, 0)?;
                    let buffer = current_buffer(&state)?;
                    let snapshot = state.buffers.get(buffer).map_err(buffer_error)?.snapshot();
                    let range = line_range(&snapshot, line)?;
                    let text = snapshot
                        .chunks_for_range(range)
                        .map_err(buffer_error)?
                        .collect::<String>();
                    Ok(Value::String(Arc::from(text)))
                }
                "setline" => {
                    let line = integer_argument(&request, 0)?;
                    let replacement = string_argument(&request, 1)?;
                    let buffer = current_buffer(&state)?;
                    let snapshot = state.buffers.get(buffer).map_err(buffer_error)?.snapshot();
                    let range = line_range(&snapshot, line)?;
                    let EditorState {
                        buffers, mutator, ..
                    } = &mut *state;
                    mutator
                        .apply_edits(
                            buffers,
                            buffer,
                            EditOrigin::VimScript,
                            [PlannedEdit {
                                selection: None,
                                edit: Edit::replace(range, replacement),
                            }],
                            None,
                            false,
                        )
                        .map_err(buffer_error)?;
                    Ok(Value::Integer(0))
                }
                name => Err(RuntimeError::coded(
                    "E117",
                    RuntimeErrorKind::NameError,
                    format!("unknown host function: {name}"),
                )),
            }
        })
    }

    fn option(&self, request: OptionRequest) -> HostFuture {
        let state = Arc::clone(&self.state);
        Box::pin(async move {
            let mut state = state
                .lock()
                .map_err(|_| host_error("editor state lock is poisoned"))?;
            let buffer = request
                .context
                .current_buffer
                .and_then(BufferId::new)
                .or_else(|| state.buffers.current())
                .ok_or_else(|| host_error("editor has no current buffer"))?;
            match request.operation {
                OptionRequestOperation::Get => option_value(
                    state.buffers.get(buffer).map_err(buffer_error)?.options(),
                    &request.name,
                ),
                OptionRequestOperation::Set(value) => {
                    let old = state
                        .buffers
                        .get(buffer)
                        .map_err(buffer_error)?
                        .options()
                        .clone();
                    let options = set_option_value(old, &request.name, value)?;
                    let EditorState {
                        buffers, mutator, ..
                    } = &mut *state;
                    mutator
                        .execute(buffers, Action::SetOptions { buffer, options })
                        .map_err(buffer_error)?;
                    Ok(Value::Null)
                }
            }
        })
    }

    fn execute_command(&self, request: CommandRequest) -> HostFuture {
        let state = Arc::clone(&self.state);
        Box::pin(async move {
            let mut state = state
                .lock()
                .map_err(|_| host_error("editor state lock is poisoned"))?;
            execute_editor_command(&mut state, request)
        })
    }
}

struct NoopWake;

impl Wake for NoopWake {
    fn wake(self: Arc<Self>) {}
}

fn poll_ready(mut future: HostFuture) -> Result<Value, EditorError> {
    let waker = Waker::from(Arc::new(NoopWake));
    let mut context = Context::from_waker(&waker);
    match future.as_mut().poll(&mut context) {
        Poll::Ready(result) => result.map_err(EditorError::Runtime),
        Poll::Pending => Err(EditorError::State(
            "autocommand host command yielded unexpectedly",
        )),
    }
}

fn vim_event_name(event: VimEvent) -> &'static str {
    match event {
        VimEvent::BufAdd => "BufAdd",
        VimEvent::BufNew => "BufNew",
        VimEvent::BufReadPre => "BufReadPre",
        VimEvent::BufReadPost => "BufReadPost",
        VimEvent::BufEnter => "BufEnter",
        VimEvent::BufLeave => "BufLeave",
        VimEvent::BufHidden => "BufHidden",
        VimEvent::BufUnload => "BufUnload",
        VimEvent::BufDelete => "BufDelete",
        VimEvent::BufWipeout => "BufWipeout",
        VimEvent::BufWritePre => "BufWritePre",
        VimEvent::BufWritePost => "BufWritePost",
        VimEvent::TextChanged => "TextChanged",
        VimEvent::TextChangedI => "TextChangedI",
        VimEvent::OptionSet => "OptionSet",
    }
}

fn register_commands(host: &mut HostRuntime) {
    for (name, abbreviation, bang, capability) in [
        ("enew", 1, true, Capability::BufferWrite),
        ("edit", 1, true, Capability::FileSystemRead),
        ("buffer", 1, true, Capability::BufferRead),
        ("bdelete", 2, true, Capability::BufferWrite),
        ("bwipeout", 2, true, Capability::BufferWrite),
        ("bunload", 3, true, Capability::BufferWrite),
        ("write", 1, true, Capability::FileSystemWrite),
        ("undo", 1, false, Capability::BufferWrite),
        ("redo", 3, false, Capability::BufferWrite),
        ("quit", 1, true, Capability::Editor),
        ("cquit", 2, false, Capability::Editor),
        ("set", 2, false, Capability::Settings),
        ("setlocal", 4, false, Capability::Settings),
    ] {
        host.capabilities.grant(capability.clone());
        host.register_command(CommandDefinition {
            name: name.into(),
            minimum_abbreviation: abbreviation,
            accepts_bang: bang,
            accepts_range: false,
            accepts_count: matches!(name, "undo" | "redo"),
            accepts_register: false,
            required_capabilities: vec![capability],
        });
    }
}

fn execute_editor_command(
    state: &mut EditorState,
    request: CommandRequest,
) -> Result<Value, RuntimeError> {
    let command = request.command;
    let current = current_buffer(state)?;
    let EditorState {
        buffers,
        mutator,
        exit_code,
    } = state;
    match command.name.as_str() {
        "enew" => {
            let current_buffer = buffers.get(current).map_err(buffer_error)?;
            let pristine_unnamed = current_buffer.path().is_none()
                && !current_buffer.is_modified()
                && current_buffer.snapshot().is_empty();
            if pristine_unnamed {
                return Ok(Value::Null);
            }
            if current_buffer.is_modified() && !command.bang {
                return Err(buffer_error(vim_buffer::BufferError::ModifiedBuffer(
                    current,
                )));
            }
            if command.bang && current_buffer.path().is_none() {
                mutator
                    .execute(
                        buffers,
                        Action::Delete {
                            buffer: current,
                            force: true,
                        },
                    )
                    .map_err(buffer_error)?;
            } else {
                let created = mutator
                    .execute(
                        buffers,
                        Action::Create {
                            initial_text: String::new(),
                        },
                    )
                    .map_err(buffer_error)?;
                let ActionOutcome::Manager(vim_buffer::ManagerOutcome::Added(buffer)) = created
                else {
                    return Err(host_error("invalid create outcome"));
                };
                mutator
                    .execute(buffers, Action::SetCurrent { buffer })
                    .map_err(buffer_error)?;
            }
        }
        "edit" => {
            let path = command.arguments.trim();
            if path.is_empty() {
                return Err(RuntimeError::coded(
                    "E32",
                    RuntimeErrorKind::InvalidCommand,
                    "no file name",
                ));
            }
            let loaded = mutator
                .execute(buffers, Action::Load { path: path.into() })
                .map_err(buffer_error)?;
            let buffer = match loaded {
                ActionOutcome::Manager(
                    vim_buffer::ManagerOutcome::Loaded(id)
                    | vim_buffer::ManagerOutcome::Existing(id),
                ) => id,
                _ => return Err(host_error("invalid load outcome")),
            };
            mutator
                .execute(buffers, Action::SetCurrent { buffer })
                .map_err(buffer_error)?;
        }
        "buffer" => {
            let buffer = command_buffer(buffers, command.arguments.trim())?;
            mutator
                .execute(buffers, Action::SetCurrent { buffer })
                .map_err(buffer_error)?;
        }
        "bdelete" | "bwipeout" | "bunload" => {
            let buffer = if command.arguments.trim().is_empty() {
                current
            } else {
                command_buffer(buffers, command.arguments.trim())?
            };
            let action = match command.name.as_str() {
                "bdelete" => Action::Delete {
                    buffer,
                    force: command.bang,
                },
                "bwipeout" => Action::Wipe {
                    buffer,
                    force: command.bang,
                },
                _ => Action::Unload {
                    buffer,
                    force: command.bang,
                },
            };
            mutator.execute(buffers, action).map_err(buffer_error)?;
        }
        "write" => {
            let path =
                (!command.arguments.trim().is_empty()).then(|| command.arguments.trim().into());
            mutator
                .execute(
                    buffers,
                    Action::Save {
                        buffer: current,
                        path,
                        force: command.bang,
                    },
                )
                .map_err(buffer_error)?;
        }
        "undo" | "redo" => {
            let count = command.count.unwrap_or(1).try_into().unwrap_or(u32::MAX);
            let action = if command.name == "undo" {
                Action::Undo {
                    buffer: current,
                    count,
                }
            } else {
                Action::Redo {
                    buffer: current,
                    count,
                }
            };
            mutator.execute(buffers, action).map_err(buffer_error)?;
        }
        "set" | "setlocal" => {
            for token in command.arguments.split_whitespace() {
                let old = buffers
                    .get(current)
                    .map_err(buffer_error)?
                    .options()
                    .clone();
                let options = apply_set_arguments(old, token)?;
                mutator
                    .execute(
                        buffers,
                        Action::SetOptions {
                            buffer: current,
                            options,
                        },
                    )
                    .map_err(buffer_error)?;
            }
        }
        "quit" => {
            if buffers.get(current).map_err(buffer_error)?.is_modified() && !command.bang {
                return Err(buffer_error(vim_buffer::BufferError::ModifiedBuffer(
                    current,
                )));
            }
            *exit_code = Some(0);
        }
        "cquit" => {
            let code = if command.arguments.trim().is_empty() {
                1
            } else {
                command.arguments.trim().parse::<u8>().map_err(|_| {
                    RuntimeError::coded(
                        "E488",
                        RuntimeErrorKind::InvalidCommand,
                        "trailing characters",
                    )
                })?
            };
            *exit_code = Some(code);
        }
        _ => {
            return Err(RuntimeError::coded(
                "E492",
                RuntimeErrorKind::InvalidCommand,
                "unsupported command",
            ));
        }
    }
    Ok(Value::Null)
}

fn apply_set_arguments(
    mut options: BufferOptions,
    arguments: &str,
) -> Result<BufferOptions, RuntimeError> {
    for token in arguments.split_whitespace() {
        let query = token.strip_suffix('?').unwrap_or(token);
        if token.ends_with('?') {
            option_value(&options, query)?;
            continue;
        }
        if let Some((name, value)) = token.split_once('=') {
            match canonical_option(name)? {
                "fileformat" => {
                    options.fileformat = match value {
                        "unix" => FileFormat::Unix,
                        "dos" => FileFormat::Dos,
                        "mac" => FileFormat::Mac,
                        _ => return Err(option_argument_error(token)),
                    }
                }
                "fileencoding" => options.fileencoding = value.to_owned(),
                _ => return Err(option_argument_error(token)),
            }
            continue;
        }

        let (name, operation) = if let Some(name) = token.strip_prefix("no") {
            (name, BoolOperation::Set(false))
        } else if let Some(name) = token.strip_prefix("inv") {
            (name, BoolOperation::Invert)
        } else if let Some(name) = token.strip_suffix('!') {
            (name, BoolOperation::Invert)
        } else if let Some(name) = token.strip_suffix('&') {
            (name, BoolOperation::Reset)
        } else {
            (token, BoolOperation::Set(true))
        };
        let canonical = canonical_option(name)?;
        let default = BufferOptions::default();
        match canonical {
            "modifiable" => apply_bool(&mut options.modifiable, default.modifiable, operation),
            "readonly" => apply_bool(&mut options.readonly, default.readonly, operation),
            "binary" => apply_bool(&mut options.binary, default.binary, operation),
            "endofline" => apply_bool(&mut options.endofline, default.endofline, operation),
            "fixeol" => apply_bool(&mut options.fixeol, default.fixeol, operation),
            "fileformat" if matches!(operation, BoolOperation::Reset) => {
                options.fileformat = default.fileformat
            }
            "fileencoding" if matches!(operation, BoolOperation::Reset) => {
                options.fileencoding = default.fileencoding
            }
            "fileformat" | "fileencoding" => {
                option_value(&options, canonical)?;
            }
            _ => unreachable!("canonical_option returned an unknown option"),
        }
    }
    Ok(options)
}

#[derive(Clone, Copy)]
enum BoolOperation {
    Set(bool),
    Invert,
    Reset,
}

fn apply_bool(value: &mut bool, default: bool, operation: BoolOperation) {
    *value = match operation {
        BoolOperation::Set(value) => value,
        BoolOperation::Invert => !*value,
        BoolOperation::Reset => default,
    };
}

fn canonical_option(name: &str) -> Result<&'static str, RuntimeError> {
    match name {
        "modifiable" | "ma" => Ok("modifiable"),
        "readonly" | "ro" => Ok("readonly"),
        "binary" | "bin" => Ok("binary"),
        "endofline" | "eol" => Ok("endofline"),
        "fixeol" => Ok("fixeol"),
        "fileformat" | "ff" => Ok("fileformat"),
        "fileencoding" | "fenc" => Ok("fileencoding"),
        _ => Err(RuntimeError::coded(
            "E518",
            RuntimeErrorKind::InvalidCommand,
            format!("unknown option: {name}"),
        )),
    }
}

fn option_value(options: &BufferOptions, name: &str) -> Result<Value, RuntimeError> {
    Ok(match canonical_option(name)? {
        "modifiable" => Value::Bool(options.modifiable),
        "readonly" => Value::Bool(options.readonly),
        "binary" => Value::Bool(options.binary),
        "endofline" => Value::Bool(options.endofline),
        "fixeol" => Value::Bool(options.fixeol),
        "fileformat" => Value::String(Arc::from(match options.fileformat {
            FileFormat::Unix => "unix",
            FileFormat::Dos => "dos",
            FileFormat::Mac => "mac",
        })),
        "fileencoding" => Value::String(Arc::from(options.fileencoding.as_str())),
        _ => unreachable!("canonical_option returned an unknown option"),
    })
}

fn set_option_value(
    mut options: BufferOptions,
    name: &str,
    value: Value,
) -> Result<BufferOptions, RuntimeError> {
    let canonical = canonical_option(name)?;
    match (canonical, value) {
        ("modifiable", Value::Bool(value)) => options.modifiable = value,
        ("readonly", Value::Bool(value)) => options.readonly = value,
        ("binary", Value::Bool(value)) => options.binary = value,
        ("endofline", Value::Bool(value)) => options.endofline = value,
        ("fixeol", Value::Bool(value)) => options.fixeol = value,
        ("fileformat", Value::String(value)) => {
            options.fileformat = match value.as_ref() {
                "unix" => FileFormat::Unix,
                "dos" => FileFormat::Dos,
                "mac" => FileFormat::Mac,
                _ => return Err(option_argument_error(value.as_ref())),
            }
        }
        ("fileencoding", Value::String(value)) => options.fileencoding = value.to_string(),
        (name, value) => {
            return Err(RuntimeError::coded(
                "E474",
                RuntimeErrorKind::TypeError,
                format!("invalid value type {} for option {name}", value.type_name()),
            ));
        }
    }
    Ok(options)
}

fn option_argument_error(token: &str) -> RuntimeError {
    RuntimeError::coded(
        "E474",
        RuntimeErrorKind::InvalidCommand,
        format!("invalid argument: {token}"),
    )
}

fn command_buffer(buffers: &BufferManager, argument: &str) -> Result<BufferId, RuntimeError> {
    let id = match argument {
        "%" => buffers.current(),
        "#" => buffers.alternate(),
        value => value.parse::<u64>().ok().and_then(BufferId::new),
    }
    .ok_or_else(|| {
        RuntimeError::coded(
            "E86",
            RuntimeErrorKind::InvalidCommand,
            "buffer does not exist",
        )
    })?;
    buffers.get(id).map_err(buffer_error)?;
    Ok(id)
}

fn current_buffer(state: &EditorState) -> Result<BufferId, RuntimeError> {
    state
        .buffers
        .current()
        .ok_or_else(|| host_error("editor has no current buffer"))
}

fn resolve_buffer(
    state: &EditorState,
    argument: Option<&Value>,
) -> Result<Option<BufferId>, RuntimeError> {
    match argument {
        None => Ok(state.buffers.current()),
        Some(Value::String(value)) if value.as_ref() == "%" => Ok(state.buffers.current()),
        Some(Value::String(value)) if value.as_ref() == "#" => Ok(state.buffers.alternate()),
        Some(Value::Integer(number)) if *number > 0 => {
            let id = BufferId::new(*number as u64);
            Ok(id.filter(|id| state.buffers.get(*id).is_ok()))
        }
        Some(value) => Err(RuntimeError::coded(
            "E745",
            RuntimeErrorKind::TypeError,
            format!(
                "bufnr() argument must be a number, '%' or '#', got {}",
                value.type_name()
            ),
        )),
    }
}

fn line_range(snapshot: &vim_buffer::BufferSnapshot, line: i64) -> Result<TextRange, RuntimeError> {
    let row = u32::try_from(
        line.checked_sub(1)
            .ok_or_else(|| range_error(line, snapshot.row_count()))?,
    )
    .map_err(|_| range_error(line, snapshot.row_count()))?;
    if row >= snapshot.row_count() {
        return Err(range_error(line, snapshot.row_count()));
    }
    let start = snapshot
        .point_to_offset(Point::new(row, 0))
        .map_err(buffer_error)?;
    let end = snapshot
        .point_to_offset(Point::new(
            row,
            snapshot.line_len(row).map_err(buffer_error)?,
        ))
        .map_err(buffer_error)?;
    TextRange::new(ByteOffset(start.0), ByteOffset(end.0))
        .ok_or_else(|| host_error("invalid line range"))
}

fn integer_argument(request: &HostRequest, index: usize) -> Result<i64, RuntimeError> {
    match request.arguments.get(index) {
        Some(Value::Integer(value)) => Ok(*value),
        Some(value) => Err(RuntimeError::coded(
            "E745",
            RuntimeErrorKind::TypeError,
            format!(
                "argument {} to {} must be a number, got {}",
                index + 1,
                request.function,
                value.type_name()
            ),
        )),
        None => Err(RuntimeError::coded(
            "E119",
            RuntimeErrorKind::ArityError,
            "missing argument",
        )),
    }
}

fn string_argument(request: &HostRequest, index: usize) -> Result<String, RuntimeError> {
    match request.arguments.get(index) {
        Some(Value::String(value)) => Ok(value.to_string()),
        Some(value) => Err(RuntimeError::coded(
            "E730",
            RuntimeErrorKind::TypeError,
            format!(
                "argument {} to {} must be a string, got {}",
                index + 1,
                request.function,
                value.type_name()
            ),
        )),
        None => Err(RuntimeError::coded(
            "E119",
            RuntimeErrorKind::ArityError,
            "missing argument",
        )),
    }
}

fn range_error(line: i64, line_count: u32) -> RuntimeError {
    RuntimeError::coded(
        "E16",
        RuntimeErrorKind::IndexError,
        format!("line {line} is outside the valid range 1..={line_count}"),
    )
}

fn buffer_error(error: vim_buffer::BufferError) -> RuntimeError {
    RuntimeError::coded("E_BUFFER", RuntimeErrorKind::HostError, error.to_string())
}

fn host_error(message: impl Into<String>) -> RuntimeError {
    RuntimeError::coded("E605", RuntimeErrorKind::HostError, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boots_with_one_current_unnamed_buffer() {
        let editor = HeadlessEditor::new().unwrap();
        let buffer = editor.current_buffer().unwrap();

        assert_eq!(buffer.get(), 1);
        assert_eq!(editor.current_text().unwrap(), "");
        assert_eq!(editor.changedtick(buffer).unwrap(), 0);
    }

    #[test]
    fn script_can_read_and_atomically_edit_current_buffer() {
        let mut editor = HeadlessEditor::new().unwrap();
        let buffer = editor.current_buffer().unwrap();

        editor
            .eval(
                "edit.vim",
                "let g:before = await getline(1)\nlet g:id = await bufnr()\nlet g:status = await setline(1, 'hello')",
            )
            .unwrap();

        assert_eq!(editor.current_text().unwrap(), "hello");
        assert_eq!(editor.changedtick(buffer).unwrap(), 1);
        assert_eq!(
            editor.global("g:before"),
            Some(&Value::String(Arc::from("")))
        );
        assert_eq!(editor.global("g:id"), Some(&Value::Integer(1)));
        assert_eq!(editor.global("g:status"), Some(&Value::Integer(0)));
        assert_eq!(editor.global("b:changedtick"), Some(&Value::Integer(1)));
    }

    #[test]
    fn failed_script_edit_does_not_mutate_the_buffer() {
        let mut editor = HeadlessEditor::new().unwrap();
        let buffer = editor.current_buffer().unwrap();

        let error = editor
            .eval("bad.vim", "let g:status = await setline(2, 'no')")
            .unwrap_err();

        assert!(matches!(error, EditorError::Runtime(_)));
        assert_eq!(editor.current_text().unwrap(), "");
        assert_eq!(editor.changedtick(buffer).unwrap(), 0);
    }

    #[test]
    fn script_edits_are_one_undo_step() {
        let mut editor = HeadlessEditor::new().unwrap();
        let buffer = editor.current_buffer().unwrap();
        editor
            .eval("edit.vim", "let g:status = await setline(1, 'hello')")
            .unwrap();

        assert!(editor.undo().unwrap());
        assert_eq!(editor.current_text().unwrap(), "");
        assert_eq!(editor.changedtick(buffer).unwrap(), 2);
        assert!(!editor.undo().unwrap());
    }

    #[test]
    fn ex_lifecycle_commands_follow_pristine_and_forced_enew_rules() {
        let mut editor = HeadlessEditor::new().unwrap();
        let first = editor.current_buffer().unwrap();

        editor.eval("lifecycle.vim", "enew").unwrap();
        assert_eq!(editor.current_buffer().unwrap(), first);

        editor
            .eval("change.vim", "let g:status = await setline(1, 'changed')")
            .unwrap();
        let error = editor.eval("protected.vim", "enew").unwrap_err();
        assert!(matches!(error, EditorError::Runtime(_)));

        editor.eval("forced.vim", "enew!").unwrap();
        let second = editor.current_buffer().unwrap();
        assert_ne!(second, first);
        assert_eq!(editor.alternate_buffer().unwrap(), Some(first));
        assert!(editor.buffer_exists(first).unwrap());
        assert!(!editor.buffer_loaded(first).unwrap());
        assert!(!editor.listed_buffers().unwrap().contains(&first));

        editor
            .eval("wipe.vim", &format!("bwipeout! {}", first.get()))
            .unwrap();
        assert!(!editor.buffer_exists(first).unwrap());
        assert_eq!(editor.current_buffer().unwrap(), second);
    }

    #[test]
    fn write_command_saves_through_the_mutator() {
        let directory =
            std::env::temp_dir().join(format!("nxvim-headless-write-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("written.txt");
        let mut editor = HeadlessEditor::new().unwrap();
        editor
            .eval("change.vim", "let g:status = await setline(1, 'saved')")
            .unwrap();
        editor
            .eval("write.vim", &format!(":write {}", path.display()))
            .unwrap();

        assert_eq!(std::fs::read_to_string(&path).unwrap(), "saved\n");
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn user_commands_expand_arguments_and_bang() {
        let directory =
            std::env::temp_dir().join(format!("nxvim-user-command-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("command.txt");
        let mut editor = HeadlessEditor::new().unwrap();
        editor
            .eval(
                "define.vim",
                ":command -nargs=1 -bang SaveAs write<bang> <args>\nlet g:status = await setline(1, 'command')",
            )
            .unwrap();
        editor
            .eval("invoke.vim", &format!(":SaveAs! {}", path.display()))
            .unwrap();

        assert_eq!(std::fs::read_to_string(&path).unwrap(), "command\n");
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn set_and_setlocal_update_typed_buffer_options_in_order() {
        let mut editor = HeadlessEditor::new().unwrap();
        editor.clear_event_trace().unwrap();
        editor
            .eval(
                "options.vim",
                ":set ff=dos fenc=utf-8 ro bin noeol nofixeol noma",
            )
            .unwrap();

        assert_eq!(editor.option_value("ma").unwrap(), Value::Bool(false));
        assert_eq!(editor.option_value("readonly").unwrap(), Value::Bool(true));
        assert_eq!(editor.option_value("bin").unwrap(), Value::Bool(true));
        assert_eq!(editor.option_value("eol").unwrap(), Value::Bool(false));
        assert_eq!(editor.option_value("fixeol").unwrap(), Value::Bool(false));
        assert_eq!(
            editor.option_value("ff").unwrap(),
            Value::String(Arc::from("dos"))
        );
        assert_eq!(
            editor.option_value("fenc").unwrap(),
            Value::String(Arc::from("utf-8"))
        );
        assert_eq!(
            editor
                .event_trace()
                .unwrap()
                .into_iter()
                .filter(|(event, _)| *event == VimEvent::OptionSet)
                .count(),
            6
        );

        let error = editor
            .eval("invalid.vim", ":set noro ff=invalid")
            .unwrap_err();
        assert!(matches!(error, EditorError::Runtime(_)));
        assert_eq!(editor.option_value("ro").unwrap(), Value::Bool(false));

        editor
            .eval("reset.vim", ":setlocal ma! noro nobin eol fixeol ff=unix")
            .unwrap();
        assert_eq!(editor.option_value("ma").unwrap(), Value::Bool(true));
        assert_eq!(editor.option_value("ro").unwrap(), Value::Bool(false));
        assert_eq!(editor.option_value("bin").unwrap(), Value::Bool(false));
        assert_eq!(editor.option_value("eol").unwrap(), Value::Bool(true));
        assert_eq!(editor.option_value("fixeol").unwrap(), Value::Bool(true));
        assert_eq!(
            editor.option_value("ff").unwrap(),
            Value::String(Arc::from("unix"))
        );
    }

    #[test]
    fn option_expressions_assignments_and_resets_use_the_typed_host_boundary() {
        let mut editor = HeadlessEditor::new().unwrap();
        editor
            .eval(
                "option-expressions.vim",
                "let g:before = &ro\nlet &l:ro = true\nlet g:after = &g:ro\n:set ro&",
            )
            .unwrap();

        assert_eq!(editor.global("g:before"), Some(&Value::Bool(false)));
        assert_eq!(editor.global("g:after"), Some(&Value::Bool(true)));
        assert_eq!(editor.option_value("ro").unwrap(), Value::Bool(false));
    }

    #[test]
    fn delcommand_removes_runtime_owned_user_commands() {
        let mut editor = HeadlessEditor::new().unwrap();
        editor
            .eval("define-delete.vim", ":command Demo enew\n:delcommand Demo")
            .unwrap();

        let error = editor.eval("invoke.vim", ":Demo").unwrap_err();
        assert!(matches!(error, EditorError::Runtime(_)));
    }

    #[test]
    fn autocmds_run_after_mutation_and_once_handlers_are_consumed() {
        let mut editor = HeadlessEditor::new().unwrap();
        editor
            .eval(
                "autocmd.vim",
                "autocmd TextChanged * ++once undo\nlet g:first = await setline(1, 'first')",
            )
            .unwrap();
        assert_eq!(editor.current_text().unwrap(), "");

        editor
            .eval("second.vim", "let g:second = await setline(1, 'second')")
            .unwrap();
        assert_eq!(editor.current_text().unwrap(), "second");
    }

    #[test]
    fn non_nested_autocmds_suppress_generated_events_but_nested_handlers_allow_them() {
        fn run(nested: bool) -> String {
            let directory = std::env::temp_dir().join(format!(
                "nxvim-autocmd-nested-{}-{nested}",
                std::process::id()
            ));
            std::fs::create_dir_all(&directory).unwrap();
            let target = directory.join("target.txt");
            std::fs::write(&target, "target\n").unwrap();
            let mut editor = HeadlessEditor::new().unwrap();
            let nested_flag = if nested { " ++nested" } else { "" };
            let source = format!(
                ":autocmd BufEnter * ++once edit {}\n:autocmd TextChanged * ++once{nested_flag} enew!\nlet g:status = await setline(1, 'changed')",
                target.display()
            );
            editor.eval("nested.vim", &source).unwrap();
            let text = editor.current_text().unwrap();
            std::fs::remove_dir_all(directory).unwrap();
            text
        }

        assert_eq!(run(false), "");
        assert_eq!(run(true), "target\n");
    }

    #[test]
    #[ignore = "requires the pinned Vim 9.2.0843 oracle executable"]
    fn user_commands_and_options_match_pinned_vim_oracle() {
        use std::process::Command;

        let directory =
            std::env::temp_dir().join(format!("nxvim-phase-d-oracle-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let oracle_output = directory.join("oracle.txt");
        let vim_target = directory.join("vim-target.txt");
        let nxvim_target = directory.join("nxvim-target.txt");
        let script =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("oracle/headless-phase-d.vim");
        let status = Command::new("vim")
            .args([
                "--clean",
                "--not-a-term",
                "-N",
                "-es",
                "-X",
                "-i",
                "NONE",
                "-u",
                "NONE",
                "-U",
                "NONE",
                "-n",
                "-S",
            ])
            .arg(script)
            .env("NXVIM_ORACLE_OUTPUT", &oracle_output)
            .env("NXVIM_PHASE_D_TARGET", &vim_target)
            .status()
            .unwrap();
        assert!(status.success(), "pinned Vim oracle failed with {status}");

        let mut editor = HeadlessEditor::new().unwrap();
        editor
            .eval(
                "phase-d.vim",
                &format!(
                    ":command -nargs=1 -bang SaveAs write<bang> <args>\nlet g:status = await setline(1, 'phase-d')\n:SaveAs! {}\n:set ff=dos fenc=utf-8 ro bin noeol nofixeol noma",
                    nxvim_target.display()
                ),
            )
            .unwrap();
        let boolean = |name| match editor.option_value(name).unwrap() {
            Value::Bool(value) => u8::from(value),
            other => panic!("expected boolean option, got {other:?}"),
        };
        let string = |name| match editor.option_value(name).unwrap() {
            Value::String(value) => value.to_string(),
            other => panic!("expected string option, got {other:?}"),
        };
        let nxvim_output = format!(
            "{},{},{},{},{},{},{}\n",
            boolean("ma"),
            boolean("ro"),
            boolean("bin"),
            boolean("eol"),
            boolean("fixeol"),
            string("ff"),
            string("fenc")
        );

        assert_eq!(
            nxvim_output,
            std::fs::read_to_string(&oracle_output).unwrap()
        );
        assert_eq!(
            std::fs::read(&nxvim_target).unwrap(),
            std::fs::read(&vim_target).unwrap()
        );
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    #[ignore = "requires the pinned Vim 9.2.0843 oracle executable"]
    fn autocmd_once_matches_pinned_vim_oracle() {
        use std::process::Command;

        let output_path =
            std::env::temp_dir().join(format!("nxvim-autocmd-oracle-{}.txt", std::process::id()));
        let script =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("oracle/headless-autocmd.vim");
        let status = Command::new("vim")
            .args([
                "--clean",
                "--not-a-term",
                "-N",
                "-es",
                "-X",
                "-i",
                "NONE",
                "-u",
                "NONE",
                "-U",
                "NONE",
                "-n",
                "-S",
            ])
            .arg(script)
            .env("NXVIM_ORACLE_OUTPUT", &output_path)
            .status()
            .unwrap();
        assert!(status.success(), "pinned Vim oracle failed with {status}");
        let oracle = std::fs::read_to_string(&output_path).unwrap();

        let mut editor = HeadlessEditor::new().unwrap();
        editor
            .eval(
                "autocmd.vim",
                "autocmd TextChanged * ++once undo\nlet g:first = await setline(1, 'first')",
            )
            .unwrap();
        editor
            .eval("second.vim", "let g:second = await setline(1, 'second')")
            .unwrap();
        let actual = format!("{}\n", editor.current_text().unwrap());

        assert_eq!(actual, oracle);
        let _ = std::fs::remove_file(output_path);
    }

    #[test]
    #[ignore = "requires the pinned Vim 9.2.0843 oracle executable"]
    fn lifecycle_state_matches_pinned_vim_oracle() {
        use std::process::Command;

        let output_path =
            std::env::temp_dir().join(format!("nxvim-headless-oracle-{}.txt", std::process::id()));
        let script =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("oracle/headless-lifecycle.vim");
        let status = Command::new("vim")
            .args([
                "--clean",
                "--not-a-term",
                "-N",
                "-es",
                "-X",
                "-i",
                "NONE",
                "-u",
                "NONE",
                "-U",
                "NONE",
                "-n",
                "-S",
            ])
            .arg(script)
            .env("NXVIM_ORACLE_OUTPUT", &output_path)
            .status()
            .unwrap();
        assert!(status.success(), "pinned Vim oracle failed with {status}");
        let oracle = std::fs::read_to_string(&output_path).unwrap();

        let mut editor = HeadlessEditor::new().unwrap();
        let first = editor.current_buffer().unwrap();
        editor.eval("pristine.vim", "enew").unwrap();
        let after_pristine = editor.current_buffer().unwrap();
        editor
            .eval("change.vim", "let g:status = await setline(1, 'changed')")
            .unwrap();
        editor.eval("forced.vim", "enew!").unwrap();
        let second = editor.current_buffer().unwrap();
        let alternate = editor.alternate_buffer().unwrap().unwrap();
        let first_line = format!(
            "{},{},{},{},{},{},{}",
            first.get(),
            after_pristine.get(),
            second.get(),
            alternate.get(),
            u8::from(editor.buffer_exists(first).unwrap()),
            u8::from(editor.buffer_loaded(first).unwrap()),
            u8::from(editor.listed_buffers().unwrap().contains(&first))
        );
        editor
            .eval("wipe.vim", &format!("bwipeout! {}", first.get()))
            .unwrap();
        let second_line = format!(
            "{},{},{}",
            editor.current_buffer().unwrap().get(),
            u8::from(editor.buffer_exists(first).unwrap()),
            u8::from(editor.listed_buffers().unwrap().contains(&second))
        );
        let actual = format!("{first_line}\n{second_line}\n");

        assert_eq!(actual, oracle);
        let _ = std::fs::remove_file(output_path);
    }
}
