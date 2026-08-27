use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::ast::{ExCommand, MapMode, MappingOptions, UserCommandAttributes};
use crate::ex_parser::ExLineParser;
use crate::integration::{
    CompiledMapping, Event, EventAction, EventBus, EventHandler, EventHandlerId, KeymapStore,
    MappingExpansion, MappingId, SharedKeymapStore,
};
use crate::runtime::{HostObjectId, RuntimeError, RuntimeErrorKind, RuntimeResult, Value, Vm};
use crate::source::SourceId;

pub type HostFuture = Pin<Box<dyn Future<Output = RuntimeResult<Value>> + Send + 'static>>;

/// Application boundary for asynchronous operations. Requests own all data so
/// returned futures can outlive a VM quantum and move to an I/O executor.
pub trait Host: Send + Sync + 'static {
    fn call(&self, request: HostRequest) -> HostFuture;

    fn call_sync(&self, _request: HostRequest) -> Option<RuntimeResult<Value>> {
        None
    }

    fn option(&self, request: OptionRequest) -> HostFuture {
        Box::pin(async move {
            Err(RuntimeError::coded(
                "E_HOST",
                RuntimeErrorKind::HostError,
                format!("host does not implement option access for {}", request.name),
            ))
        })
    }

    fn external_runtime(&self, request: ExternalRuntimeRequest) -> HostFuture {
        Box::pin(async move {
            let _ = request;
            Err(RuntimeError::coded(
                "E_NOTIMPL",
                RuntimeErrorKind::HostError,
                "external runtime requests are reserved for Phase 7",
            ))
        })
    }

    fn editor(&self, request: EditorRequest) -> HostFuture {
        Box::pin(async move {
            Err(RuntimeError::coded(
                "E_HOST",
                RuntimeErrorKind::HostError,
                format!(
                    "host does not implement editor request {:?}",
                    request.operation
                ),
            ))
        })
    }

    fn execute_command(&self, request: CommandRequest) -> HostFuture {
        Box::pin(async move {
            Err(RuntimeError::coded(
                "E492",
                RuntimeErrorKind::InvalidCommand,
                format!("host does not implement command {}", request.command.name),
            ))
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct HostRequest {
    pub target: HostTarget,
    pub function: String,
    pub arguments: Vec<Value>,
    pub context: HostContext,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EditorRequest {
    pub operation: EditorRequestOperation,
    pub context: HostContext,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExternalRuntimeRequest {
    Timer { delay_ms: u64, repeat: bool },
    Job { command: String },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExternalRuntimeResponse {
    Pending { id: u64 },
    Unsupported,
}

#[derive(Clone, Debug, PartialEq)]
pub enum EditorRequestOperation {
    CurrentContext,
    BufferText {
        buffer: u64,
        range: OwnedTextRange,
    },
    Selection {
        window: u64,
    },
    Register {
        name: char,
    },
    Mark {
        buffer: u64,
        name: char,
    },
    ReplaceBuffer {
        buffer: u64,
        range: OwnedTextRange,
        text: String,
    },
    Window(WindowRequestOperation),
    Tab(TabRequestOperation),
    Message {
        text: String,
    },
    Prompt {
        message: String,
    },
    RegisterEvent {
        event: String,
        pattern: String,
        command: String,
        once: bool,
        nested: bool,
    },
}

impl EditorRequestOperation {
    pub fn required_capability(&self) -> Capability {
        match self {
            Self::CurrentContext => Capability::Editor,
            Self::BufferText { .. } | Self::Selection { .. } | Self::Mark { .. } => {
                Capability::BufferRead
            }
            Self::Register { .. } => Capability::Editor,
            Self::ReplaceBuffer { .. } => Capability::BufferWrite,
            Self::Window(_) => Capability::Window,
            Self::Tab(_) => Capability::Editor,
            Self::Message { .. } | Self::Prompt { .. } => Capability::UserInterface,
            Self::RegisterEvent { .. } => Capability::Editor,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WindowRequestOperation {
    SplitHorizontal,
    SplitVertical,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TabRequestOperation {
    Next { count: usize },
    Previous { count: usize },
    Close,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OwnedTextRange {
    pub start: u64,
    pub end: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum EditorResponse {
    Context(HostContext),
    Text(String),
    Range(OwnedTextRange),
    Register(Value),
    Mark(Option<u64>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct OptionRequest {
    pub operation: OptionRequestOperation,
    pub name: String,
    pub scope: OptionRequestScope,
    pub context: HostContext,
}

#[derive(Clone, Debug, PartialEq)]
pub enum OptionRequestOperation {
    Get,
    Set(Value),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OptionRequestScope {
    Unqualified,
    Local,
    Global,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HostTarget {
    Global,
    Namespace(String),
    Object(HostObjectId),
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct HostContext {
    pub script_name: Option<String>,
    pub current_buffer: Option<u64>,
    pub current_window: Option<u64>,
    pub current_tab: Option<u64>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum Capability {
    Editor,
    BufferRead,
    BufferWrite,
    Window,
    Settings,
    FileSystemRead,
    FileSystemWrite,
    Network,
    ClipboardRead,
    ClipboardWrite,
    Terminal,
    Process,
    UserInterface,
    Custom(String),
}

#[derive(Clone, Debug, Default)]
pub struct CapabilitySet {
    pub granted: HashSet<Capability>,
}

impl CapabilitySet {
    pub fn allows(&self, capability: &Capability) -> bool {
        self.granted.contains(capability)
    }
    pub fn grant(&mut self, capability: Capability) -> bool {
        self.granted.insert(capability)
    }
    pub fn revoke(&mut self, capability: &Capability) -> bool {
        self.granted.remove(capability)
    }
    pub fn allows_all(&self, capabilities: &[Capability]) -> bool {
        capabilities
            .iter()
            .all(|capability| self.allows(capability))
    }
}

impl<const N: usize> From<[Capability; N]> for CapabilitySet {
    fn from(capabilities: [Capability; N]) -> Self {
        Self {
            granted: capabilities.into_iter().collect(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Arity {
    Exact(u16),
    Range { min: u16, max: u16 },
    Variadic { min: u16 },
}

impl Arity {
    pub fn accepts(self, count: usize) -> bool {
        match self {
            Self::Exact(expected) => count == expected as usize,
            Self::Range { min, max } => (min as usize..=max as usize).contains(&count),
            Self::Variadic { min } => count >= min as usize,
        }
    }
}

#[derive(Clone, Debug)]
pub struct HostFunctionRegistration {
    pub name: String,
    pub target: HostTarget,
    pub arity: Arity,
    pub required_capabilities: Vec<Capability>,
}

#[derive(Clone, Debug, Default)]
pub struct HostFunctionRegistry {
    pub functions: HashMap<String, HostFunctionRegistration>,
}

impl HostFunctionRegistry {
    pub fn register(&mut self, registration: HostFunctionRegistration) {
        self.functions
            .insert(registration.name.clone(), registration);
    }
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.functions.keys().map(String::as_str)
    }
    pub fn get(&self, name: &str) -> Option<&HostFunctionRegistration> {
        self.functions.get(name)
    }
}

#[derive(Clone)]
pub struct HostRuntime {
    pub host: Arc<dyn Host>,
    pub capabilities: CapabilitySet,
    pub functions: HostFunctionRegistry,
    pub commands: CommandRegistry,
    pub user_commands: HashMap<String, UserCommand>,
    pub keymaps: SharedKeymapStore,
    pub events: EventBus,
    pub current_augroup: Option<String>,
    pub augroups: HashSet<String>,
    eventignore: HashSet<String>,
    autocmd_depth: u8,
    autocmd_enabled: bool,
    registered_user_commands: Vec<String>,
    removed_user_commands: Vec<String>,
    next_mapping_id: u64,
    next_event_handler_id: u64,
}

impl std::fmt::Debug for HostRuntime {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("HostRuntime")
            .field("capabilities", &self.capabilities)
            .field("functions", &self.functions)
            .field("commands", &self.commands)
            .field("user_commands", &self.user_commands)
            .field(
                "keymaps",
                &self.keymaps.read().expect("keymap store lock poisoned"),
            )
            .field("events", &self.events)
            .field("current_augroup", &self.current_augroup)
            .field("augroups", &self.augroups)
            .field("eventignore", &self.eventignore)
            .field("autocmd_depth", &self.autocmd_depth)
            .field("autocmd_enabled", &self.autocmd_enabled)
            .finish_non_exhaustive()
    }
}

impl HostRuntime {
    pub fn new(host: Arc<dyn Host>) -> Self {
        Self::with_keymaps(
            host,
            Arc::new(std::sync::RwLock::new(KeymapStore::default())),
        )
    }

    pub fn with_keymaps(host: Arc<dyn Host>, keymaps: SharedKeymapStore) -> Self {
        Self {
            host,
            capabilities: CapabilitySet::default(),
            functions: HostFunctionRegistry::default(),
            commands: CommandRegistry::default(),
            user_commands: HashMap::new(),
            keymaps,
            events: EventBus::default(),
            current_augroup: None,
            augroups: HashSet::new(),
            eventignore: HashSet::new(),
            autocmd_depth: 0,
            autocmd_enabled: true,
            registered_user_commands: Vec::new(),
            removed_user_commands: Vec::new(),
            next_mapping_id: 0,
            next_event_handler_id: 0,
        }
    }

    pub fn register_function(
        &mut self,
        name: impl Into<String>,
        arity: Arity,
        required_capabilities: Vec<Capability>,
    ) {
        let name = name.into();
        self.functions.register(HostFunctionRegistration {
            name: name.clone(),
            target: HostTarget::Global,
            arity,
            required_capabilities,
        });
    }

    pub fn register_command(&mut self, definition: CommandDefinition) {
        self.commands.register(definition);
    }

    pub fn define_user_command(&mut self, command: &ExCommand) -> RuntimeResult<()> {
        let definition = UserCommand::parse(command)?;
        if self.user_commands.contains_key(&definition.name) && !command.bang {
            return Err(RuntimeError::coded(
                "E174",
                RuntimeErrorKind::InvalidCommand,
                format!("command already exists: {}", definition.name),
            ));
        }
        let name = definition.name.clone();
        self.user_commands.insert(name.clone(), definition);
        self.registered_user_commands.push(name);
        Ok(())
    }

    pub fn take_registered_user_commands(&mut self) -> Vec<String> {
        std::mem::take(&mut self.registered_user_commands)
    }

    pub fn take_removed_user_commands(&mut self) -> Vec<String> {
        std::mem::take(&mut self.removed_user_commands)
    }

    pub fn remove_user_command(&mut self, name: &str) -> bool {
        let removed = self.user_commands.remove(name).is_some();
        if removed {
            self.removed_user_commands.push(name.to_owned());
        }
        removed
    }

    pub fn delete_user_command(&mut self, command: &ExCommand) -> RuntimeResult<()> {
        let mut arguments = command.arguments.split_whitespace();
        let name = arguments.next().ok_or_else(|| {
            RuntimeError::coded(
                "E184",
                RuntimeErrorKind::InvalidCommand,
                "user command name is required",
            )
        })?;
        if command.bang || command.range.is_some() || arguments.next().is_some() {
            return Err(RuntimeError::coded(
                "E488",
                RuntimeErrorKind::InvalidCommand,
                "invalid :delcommand arguments",
            ));
        }
        if self.remove_user_command(name) {
            Ok(())
        } else {
            Err(RuntimeError::coded(
                "E184",
                RuntimeErrorKind::InvalidCommand,
                format!("no such user-defined command: {name}"),
            ))
        }
    }

    pub fn list_user_commands(&self, prefix: Option<&str>) -> Vec<UserCommand> {
        let mut commands = self
            .user_commands
            .values()
            .filter(|command| prefix.is_none_or(|prefix| command.name.starts_with(prefix)))
            .cloned()
            .collect::<Vec<_>>();
        commands.sort_by(|left, right| left.name.cmp(&right.name));
        commands
    }

    /// Handles mapping and autocommand registration commands internally.
    /// Returns `None` when the command should continue to normal dispatch.
    pub fn handle_registration_command(
        &mut self,
        request: &CommandRequest,
    ) -> Option<RuntimeResult<()>> {
        let name = request.command.name.as_str();
        if name == "augroup" {
            return Some(self.handle_augroup(&request.command));
        }
        if name == "autocmd" || name == "autocmd!" {
            return Some(self.handle_autocmd(request));
        }
        if mapping_modes(name).is_some() {
            return Some(self.handle_mapping(request));
        }
        None
    }

    pub fn mapping(
        &self,
        mode: MapMode,
        lhs: &str,
        buffer: Option<u64>,
    ) -> Option<CompiledMapping> {
        self.keymaps
            .read()
            .expect("keymap store lock poisoned")
            .resolve(input_mapping_mode(mode), lhs, buffer)
            .ok()
            .flatten()
            .cloned()
    }

    pub fn event_commands(&mut self, event: &Event, context: HostContext) -> Vec<CommandRequest> {
        if !self.autocmd_enabled || self.event_is_ignored(&event.name) || self.autocmd_depth >= 10 {
            return Vec::new();
        }
        self.events
            .handlers_for_with_nesting(event, self.autocmd_depth == 0)
            .into_iter()
            .filter_map(|handler| match handler.action {
                EventAction::Command(command) => Some(CommandRequest {
                    command,
                    context: context.clone(),
                }),
                EventAction::Bytecode(_) => None,
            })
            .collect()
    }

    pub fn set_eventignore(&mut self, events: impl IntoIterator<Item = String>) {
        self.eventignore = events.into_iter().collect();
    }

    pub fn set_autocmd_enabled(&mut self, enabled: bool) {
        self.autocmd_enabled = enabled;
    }

    pub fn begin_autocmd(&mut self) -> RuntimeResult<()> {
        if self.autocmd_depth >= 10 {
            return Err(RuntimeError::coded(
                "E218",
                RuntimeErrorKind::InvalidCommand,
                "autocommand nesting exceeds 10 levels",
            ));
        }
        self.autocmd_depth += 1;
        Ok(())
    }

    pub fn end_autocmd(&mut self) {
        self.autocmd_depth = self.autocmd_depth.saturating_sub(1);
    }

    fn event_is_ignored(&self, event: &str) -> bool {
        self.eventignore.contains("all") && !self.eventignore.contains(&format!("-{event}"))
            || self.eventignore.contains(event)
    }

    fn handle_mapping(&mut self, request: &CommandRequest) -> RuntimeResult<()> {
        let name = request.command.name.as_str();
        let (modes, non_recursive, unmap) = mapping_modes(name).expect("mapping command checked");
        let (options, rest) = parse_mapping_options(&request.command.arguments)?;
        let mut parts = rest.splitn(2, char::is_whitespace);
        let lhs = parts
            .next()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                RuntimeError::coded(
                    "E471",
                    RuntimeErrorKind::InvalidCommand,
                    "mapping requires a left-hand side",
                )
            })?;
        let buffer = if options.buffer_local {
            Some(request.context.current_buffer.ok_or_else(|| {
                RuntimeError::coded(
                    "E86",
                    RuntimeErrorKind::HostError,
                    "buffer-local mapping requires a current buffer",
                )
            })?)
        } else {
            None
        };
        if unmap {
            let mut removed = false;
            for mode in modes {
                removed |= self
                    .keymaps
                    .write()
                    .expect("keymap store lock poisoned")
                    .unmap(input_mapping_mode(mode), lhs, buffer)
                    .map_err(|error| {
                        RuntimeError::coded(
                            "E474",
                            RuntimeErrorKind::InvalidCommand,
                            format!("invalid mapping key sequence: {error}"),
                        )
                    })?
                    .is_some();
            }
            return if removed {
                Ok(())
            } else {
                Err(RuntimeError::coded(
                    "E31",
                    RuntimeErrorKind::InvalidCommand,
                    format!("no such mapping: {lhs}"),
                ))
            };
        }
        let rhs = parts.next().unwrap_or("").trim_start();
        if rhs.is_empty() {
            return Err(RuntimeError::coded(
                "E471",
                RuntimeErrorKind::InvalidCommand,
                "mapping requires a right-hand side",
            ));
        }
        let expansion = if rhs.eq_ignore_ascii_case("<nop>") {
            MappingExpansion::NoOp
        } else {
            MappingExpansion::Keys(rhs.to_owned())
        };
        let id = MappingId(self.next_mapping_id);
        self.next_mapping_id += 1;
        let mut options = options;
        options.non_recursive |= non_recursive;
        let mapping = CompiledMapping::new(
            id,
            modes.into_iter().map(input_mapping_mode).collect(),
            lhs.to_owned(),
            expansion,
            vim_input::MappingFlags {
                non_recursive: options.non_recursive,
                silent: options.silent,
                nowait: options.nowait,
                expr: options.expr,
                unique: options.unique,
                script: options.script,
            },
            buffer.map_or(
                vim_input::MappingScope::Global,
                vim_input::MappingScope::Buffer,
            ),
            vim_input::MappingOrigin::Script,
            vim_input::MappingScriptContext {
                script_name: request.context.script_name.clone(),
            },
        )
        .map_err(|error| {
            RuntimeError::coded(
                "E474",
                RuntimeErrorKind::InvalidCommand,
                format!("invalid mapping key sequence: {error}"),
            )
        })?;
        self.keymaps
            .write()
            .expect("keymap store lock poisoned")
            .register(mapping);
        Ok(())
    }

    fn handle_augroup(&mut self, command: &ExCommand) -> RuntimeResult<()> {
        let group = command.arguments.trim();
        if group.is_empty() {
            return Err(RuntimeError::coded(
                "E471",
                RuntimeErrorKind::InvalidCommand,
                "augroup requires a name",
            ));
        }
        if group == "END" {
            self.current_augroup = None;
        } else {
            self.augroups.insert(group.to_owned());
            self.current_augroup = Some(group.to_owned());
        }
        Ok(())
    }

    fn handle_autocmd(&mut self, request: &CommandRequest) -> RuntimeResult<()> {
        let arguments = request.command.arguments.as_str();
        if request.command.bang && arguments.trim().is_empty() {
            if let Some(group) = self.current_augroup.as_deref() {
                self.events.remove_group(group);
            } else {
                self.events.handlers.clear();
            }
            return Ok(());
        }
        let (first, mut cursor) = word_at(arguments, 0).ok_or_else(|| {
            RuntimeError::coded(
                "E216",
                RuntimeErrorKind::InvalidCommand,
                "autocmd requires an event",
            )
        })?;
        let explicit_group = self.augroups.contains(first);
        let group = if explicit_group {
            if let Some((_, end)) = word_at(arguments, cursor) {
                cursor = end;
            } else if request.command.bang {
                self.events.remove_matching(Some(first), None, None);
                return Ok(());
            } else {
                return Err(RuntimeError::coded(
                    "E216",
                    RuntimeErrorKind::InvalidCommand,
                    "autocmd requires an event",
                ));
            }
            Some(first)
        } else {
            self.current_augroup.as_deref()
        };
        let events = if explicit_group {
            word_at(arguments, word_at(arguments, 0).unwrap().1)
                .expect("explicit group has an event")
                .0
        } else {
            first
        };

        if request.command.bang {
            let pattern = word_at(arguments, cursor).map(|(pattern, _)| pattern);
            let event_names: Option<Vec<_>> = if explicit_group || !events.is_empty() {
                Some(events.split(',').collect())
            } else {
                None
            };
            let pattern_names = pattern.map(|pattern| vec![pattern]);
            self.events
                .remove_matching(group, event_names.as_deref(), pattern_names.as_deref());
            // `:autocmd!` with no event/pattern clears the selected group.
            return Ok(());
        }

        let (patterns, end) = word_at(arguments, cursor).ok_or_else(|| {
            RuntimeError::coded(
                "E216",
                RuntimeErrorKind::InvalidCommand,
                "autocmd requires a pattern",
            )
        })?;
        cursor = end;
        let mut once = false;
        let mut nested = false;
        while let Some((flag, end)) = word_at(arguments, cursor) {
            match flag {
                "++once" => once = true,
                "++nested" => nested = true,
                _ => break,
            }
            cursor = end;
        }
        let source = arguments[cursor..].trim_start().to_owned();
        if source.is_empty() {
            return Err(RuntimeError::coded(
                "E216",
                RuntimeErrorKind::InvalidCommand,
                "autocmd requires a command",
            ));
        }
        let action = ExLineParser::new(SourceId(0), &source, 0)
            .parse()
            .map(|parsed| EventAction::Command(parsed.command))
            .map_err(|diagnostic| {
                RuntimeError::coded(
                    "E488",
                    RuntimeErrorKind::InvalidCommand,
                    diagnostic.message.clone(),
                )
            })?;
        let patterns: Vec<_> = split_autocmd_patterns(patterns)
            .into_iter()
            .map(|pattern| expand_autocmd_pattern(&pattern))
            .collect();
        for event in events.split(',') {
            if event == "*" || !is_supported_autocmd_event(event) {
                return Err(RuntimeError::coded(
                    "E216",
                    RuntimeErrorKind::InvalidCommand,
                    format!("unknown autocommand event: {event}"),
                ));
            }
            let id = EventHandlerId(self.next_event_handler_id);
            self.next_event_handler_id += 1;
            self.events.register(EventHandler {
                id,
                group: group.map(str::to_owned),
                event: event.to_owned(),
                patterns: patterns.clone(),
                action: action.clone(),
                once,
                nested,
            });
        }
        Ok(())
    }

    pub fn prepare_command(&self, mut request: CommandRequest) -> RuntimeResult<CommandRequest> {
        for _ in 0..32 {
            let Some(command) = self.user_commands.get(&request.command.name) else {
                return Ok(request);
            };
            request.command = command.expand(&request.command)?;
        }
        Err(RuntimeError::coded(
            "E169",
            RuntimeErrorKind::InvalidCommand,
            "user command expansion is recursive",
        ))
    }

    pub fn install_globals(&self, vm: &mut Vm) {
        for name in self.functions.names() {
            let function = Value::HostFunction(Arc::from(name));
            vm.globals.insert(format!(":{name}"), function.clone());
            vm.globals.insert(format!("g:{name}"), function);
        }
    }

    pub fn dispatch(&self, mut request: HostRequest) -> RuntimeResult<HostFuture> {
        let registration = self.functions.get(&request.function).ok_or_else(|| {
            RuntimeError::coded(
                "E117",
                RuntimeErrorKind::NameError,
                format!("unknown host function: {}", request.function),
            )
        })?;
        if !registration.arity.accepts(request.arguments.len()) {
            return Err(RuntimeError::coded(
                "E119",
                RuntimeErrorKind::ArityError,
                format!("invalid argument count for {}", request.function),
            ));
        }
        if let Some(missing) = registration
            .required_capabilities
            .iter()
            .find(|capability| !self.capabilities.allows(capability))
        {
            return Err(RuntimeError::coded(
                "E_PERM",
                RuntimeErrorKind::PermissionDenied,
                format!(
                    "host function {} requires capability {missing:?}",
                    request.function
                ),
            ));
        }
        request.target = registration.target.clone();
        Ok(self.host.call(request))
    }

    pub fn dispatch_sync(
        &self,
        mut request: HostRequest,
    ) -> RuntimeResult<Option<RuntimeResult<Value>>> {
        let Some(sync_result) = self.host.call_sync(request.clone()) else {
            return Ok(None);
        };
        let registration = self.functions.get(&request.function).ok_or_else(|| {
            RuntimeError::coded(
                "E117",
                RuntimeErrorKind::NameError,
                format!("unknown host function: {}", request.function),
            )
        })?;
        if !registration.arity.accepts(request.arguments.len()) {
            return Err(RuntimeError::coded(
                "E119",
                RuntimeErrorKind::ArityError,
                format!("invalid argument count for {}", request.function),
            ));
        }
        if let Some(missing) = registration
            .required_capabilities
            .iter()
            .find(|capability| !self.capabilities.allows(capability))
        {
            return Err(RuntimeError::coded(
                "E_PERM",
                RuntimeErrorKind::PermissionDenied,
                format!(
                    "host function {} requires capability {missing:?}",
                    request.function
                ),
            ));
        }
        request.target = registration.target.clone();
        Ok(Some(sync_result))
    }

    pub fn dispatch_editor(&self, request: EditorRequest) -> RuntimeResult<HostFuture> {
        let capability = request.operation.required_capability();
        if !self.capabilities.allows(&capability) {
            return Err(RuntimeError::coded(
                "E_PERM",
                RuntimeErrorKind::PermissionDenied,
                format!("editor request requires capability {capability:?}"),
            ));
        }
        Ok(self.host.editor(request))
    }

    pub fn dispatch_external_runtime(
        &self,
        request: ExternalRuntimeRequest,
    ) -> RuntimeResult<HostFuture> {
        let capability = match &request {
            ExternalRuntimeRequest::Timer { .. } => Capability::Editor,
            ExternalRuntimeRequest::Job { .. } => Capability::Process,
        };
        if !self.capabilities.allows(&capability) {
            return Err(RuntimeError::coded(
                "E_PERM",
                RuntimeErrorKind::PermissionDenied,
                format!("external runtime request requires capability {capability:?}"),
            ));
        }
        Ok(self.host.external_runtime(request))
    }

    pub fn dispatch_option(&self, request: OptionRequest) -> RuntimeResult<HostFuture> {
        if !self.capabilities.allows(&Capability::Settings) {
            return Err(RuntimeError::coded(
                "E_PERM",
                RuntimeErrorKind::PermissionDenied,
                format!("option {} requires capability Settings", request.name),
            ));
        }
        Ok(self.host.option(request))
    }

    pub fn dispatch_command(&self, mut request: CommandRequest) -> RuntimeResult<HostFuture> {
        let definition = self.commands.resolve(&request.command.name)?;
        request.command.name = definition.name.clone();
        if request.command.bang && !definition.accepts_bang {
            return Err(RuntimeError::coded(
                "E477",
                RuntimeErrorKind::InvalidCommand,
                format!("{} does not accept !", definition.name),
            ));
        }
        if request.command.range.is_some() && !definition.accepts_range {
            return Err(RuntimeError::coded(
                "E481",
                RuntimeErrorKind::InvalidCommand,
                format!("{} does not accept a range", definition.name),
            ));
        }
        if definition.accepts_count || definition.accepts_register {
            let (count, register, remaining) = parse_count_and_register(
                &request.command.arguments,
                definition.accepts_count,
                definition.accepts_register,
            );
            request.command.count = count;
            request.command.register = register;
            request.command.arguments = remaining;
        }
        if !self
            .capabilities
            .allows_all(&definition.required_capabilities)
        {
            return Err(RuntimeError::coded(
                "E_PERM",
                RuntimeErrorKind::PermissionDenied,
                format!("command {} lacks required capabilities", definition.name),
            ));
        }
        Ok(self.host.execute_command(request))
    }
}

fn parse_count_and_register(
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

fn input_mapping_mode(mode: MapMode) -> vim_input::MappingMode {
    match mode {
        MapMode::Normal => vim_input::MappingMode::Normal,
        MapMode::Visual => vim_input::MappingMode::Visual,
        MapMode::Select => vim_input::MappingMode::Select,
        MapMode::OperatorPending => vim_input::MappingMode::OperatorPending,
        MapMode::Insert => vim_input::MappingMode::Insert,
        MapMode::CommandLine => vim_input::MappingMode::CommandLine,
        MapMode::LangArg => vim_input::MappingMode::LangArg,
        MapMode::Terminal => vim_input::MappingMode::Terminal,
    }
}

fn mapping_modes(name: &str) -> Option<(Vec<MapMode>, bool, bool)> {
    let (stem, unmap) = name
        .strip_suffix("unmap")
        .map_or((name, false), |prefix| (prefix, true));
    let (prefix, non_recursive) = stem.strip_suffix("noremap").map_or_else(
        || stem.strip_suffix("map").map(|prefix| (prefix, false)),
        |prefix| Some((prefix, true)),
    )?;
    let modes = match prefix {
        "" => vec![
            MapMode::Normal,
            MapMode::Visual,
            MapMode::Select,
            MapMode::OperatorPending,
        ],
        "n" => vec![MapMode::Normal],
        "v" => vec![MapMode::Visual, MapMode::Select],
        "x" => vec![MapMode::Visual],
        "s" => vec![MapMode::Select],
        "o" => vec![MapMode::OperatorPending],
        "i" => vec![MapMode::Insert],
        "c" => vec![MapMode::CommandLine],
        "l" => vec![MapMode::LangArg],
        "t" => vec![MapMode::Terminal],
        _ => return None,
    };
    Some((modes, non_recursive, unmap))
}

fn parse_mapping_options(arguments: &str) -> RuntimeResult<(MappingOptions, &str)> {
    let mut options = MappingOptions::default();
    let mut rest = arguments.trim_start();
    loop {
        if !rest.starts_with('<') {
            break;
        }
        let Some(end) = rest.find('>') else {
            return Err(RuntimeError::coded(
                "E475",
                RuntimeErrorKind::InvalidCommand,
                "unterminated mapping attribute",
            ));
        };
        let attribute = rest[1..end].to_ascii_lowercase();
        match attribute.as_str() {
            "buffer" => options.buffer_local = true,
            "silent" => options.silent = true,
            "expr" => options.expr = true,
            "nowait" => options.nowait = true,
            "unique" => options.unique = true,
            "script" => options.script = true,
            _ => break,
        }
        rest = rest[end + 1..].trim_start();
    }
    Ok((options, rest))
}

#[derive(Clone, Debug)]
pub struct UserCommand {
    pub name: String,
    pub replacement: String,
    pub attributes: UserCommandAttributes,
}

impl UserCommand {
    pub fn parse(command: &ExCommand) -> RuntimeResult<Self> {
        let source = command.arguments.as_str();
        let mut cursor = 0;
        let mut attributes = UserCommandAttributes {
            nargs: Some("0".into()),
            ..UserCommandAttributes::default()
        };
        while word_at(source, cursor).is_some_and(|(word, _)| word.starts_with('-')) {
            let (attribute, end) = word_at(source, cursor).expect("checked");
            match attribute {
                "-bang" => attributes.bang = true,
                "-bar" => attributes.bar = true,
                "-range" | "-range=%" => attributes.range = true,
                "-count" | "-count=0" => attributes.count = true,
                "-register" => attributes.register = true,
                value if value.starts_with("-nargs=") => {
                    let nargs = &value[7..];
                    if !matches!(nargs, "0" | "1" | "?" | "*" | "+") {
                        return Err(RuntimeError::coded(
                            "E176",
                            RuntimeErrorKind::InvalidCommand,
                            format!("invalid -nargs value: {nargs}"),
                        ));
                    }
                    attributes.nargs = Some(nargs.to_owned());
                }
                value if value.starts_with("-complete=") => {
                    attributes.complete = Some(value[10..].to_owned())
                }
                _ => {
                    return Err(RuntimeError::coded(
                        "E181",
                        RuntimeErrorKind::InvalidCommand,
                        format!("invalid user command attribute: {attribute}"),
                    ));
                }
            }
            cursor = end;
        }
        let Some((name, end)) = word_at(source, cursor) else {
            return Err(RuntimeError::coded(
                "E182",
                RuntimeErrorKind::InvalidCommand,
                "user command name is required",
            ));
        };
        if !name.chars().next().is_some_and(char::is_uppercase)
            || !name.chars().all(|ch| ch.is_ascii_alphanumeric())
        {
            return Err(RuntimeError::coded(
                "E183",
                RuntimeErrorKind::InvalidCommand,
                "user-defined commands must start with an uppercase letter",
            ));
        }
        let replacement = source[end..].trim_start().to_owned();
        if replacement.is_empty() {
            return Err(RuntimeError::coded(
                "E184",
                RuntimeErrorKind::InvalidCommand,
                "user command replacement is required",
            ));
        }
        Ok(Self {
            name: name.to_owned(),
            replacement,
            attributes,
        })
    }

    pub fn expand(&self, invocation: &ExCommand) -> RuntimeResult<ExCommand> {
        let arguments = invocation.arguments.trim();
        let argument_count = if arguments.is_empty() {
            0
        } else {
            arguments.split_whitespace().count()
        };
        let valid = match self.attributes.nargs.as_deref().unwrap_or("0") {
            "0" => argument_count == 0,
            "1" => argument_count == 1,
            "?" => argument_count <= 1,
            "*" => true,
            "+" => argument_count >= 1,
            _ => false,
        };
        if !valid {
            return Err(RuntimeError::coded(
                "E471",
                RuntimeErrorKind::ArityError,
                format!("invalid arguments for user command {}", self.name),
            ));
        }
        if invocation.bang && !self.attributes.bang {
            return Err(RuntimeError::coded(
                "E477",
                RuntimeErrorKind::InvalidCommand,
                format!("{} does not accept !", self.name),
            ));
        }
        if invocation.range.is_some() && !self.attributes.range {
            return Err(RuntimeError::coded(
                "E481",
                RuntimeErrorKind::InvalidCommand,
                format!("{} does not accept a range", self.name),
            ));
        }
        if invocation.register.is_some() && !self.attributes.register {
            return Err(RuntimeError::coded(
                "E850",
                RuntimeErrorKind::InvalidCommand,
                format!("{} does not accept a register", self.name),
            ));
        }
        let quoted = format!("'{}'", arguments.replace('\'', "''"));
        let bang = if invocation.bang { "!" } else { "" };
        let count = invocation.count.unwrap_or(0).to_string();
        let register = invocation
            .register
            .map_or(String::new(), |value| value.to_string());
        let (line1, line2) = command_lines(invocation);
        let expanded = self
            .replacement
            .replace("<q-args>", &quoted)
            .replace("<args>", arguments)
            .replace("<bang>", bang)
            .replace("<count>", &count)
            .replace("<reg>", &register)
            .replace("<line1>", &line1.to_string())
            .replace("<line2>", &line2.to_string())
            .replace("<lt>", "<");
        ExLineParser::new(SourceId(0), &expanded, 0)
            .parse()
            .map(|parsed| parsed.command)
            .map_err(|diagnostic| {
                RuntimeError::coded(
                    "E488",
                    RuntimeErrorKind::InvalidCommand,
                    diagnostic.message.clone(),
                )
            })
    }
}

fn split_autocmd_patterns(patterns: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut escaped = false;
    for character in patterns.chars() {
        if escaped {
            current.push('\\');
            current.push(character);
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if character == ',' {
            result.push(current);
            current = String::new();
        } else {
            current.push(character);
        }
    }
    if escaped {
        current.push('\\');
    }
    result.push(current);
    result
}

fn expand_autocmd_pattern(pattern: &str) -> String {
    let mut expanded = String::new();
    let mut chars = pattern.chars().peekable();
    if pattern.starts_with("~/") {
        if let Ok(home) = std::env::var("HOME") {
            expanded.push_str(home.trim_end_matches('/'));
            chars.next();
        }
    }
    while let Some(character) = chars.next() {
        if character == '$' {
            let mut name = String::new();
            while chars
                .peek()
                .is_some_and(|value| value.is_ascii_alphanumeric() || *value == '_')
            {
                name.push(chars.next().expect("peeked environment variable character"));
            }
            if name.is_empty() {
                expanded.push('$');
            } else if let Ok(value) = std::env::var(&name) {
                expanded.push_str(&value);
            } else {
                expanded.push('$');
                expanded.push_str(&name);
            }
        } else {
            expanded.push(character);
        }
    }
    expanded
}

fn is_supported_autocmd_event(event: &str) -> bool {
    matches!(
        event,
        "BufAdd"
            | "BufRead"
            | "BufReadPost"
            | "BufEnter"
            | "BufLeave"
            | "BufWrite"
            | "BufWritePost"
            | "BufUnload"
            | "BufDelete"
            | "BufWipeout"
            | "TextChanged"
            | "CursorMoved"
            | "InsertEnter"
            | "InsertLeave"
            | "OptionSet"
            | "VimEnter"
            | "VimLeave"
            | "BufNewFile"
            | "FileType"
            | "WinEnter"
            | "WinLeave"
            | "ModeChanged"
            | "SafeState"
            | "User"
    )
}

fn word_at(source: &str, mut cursor: usize) -> Option<(&str, usize)> {
    while source[cursor..]
        .chars()
        .next()
        .is_some_and(char::is_whitespace)
    {
        cursor += source[cursor..].chars().next()?.len_utf8();
    }
    let start = cursor;
    while source[cursor..]
        .chars()
        .next()
        .is_some_and(|ch| !ch.is_whitespace())
    {
        cursor += source[cursor..].chars().next()?.len_utf8();
    }
    (cursor > start).then_some((&source[start..cursor], cursor))
}

fn command_lines(command: &ExCommand) -> (u64, u64) {
    use crate::ast::Address;
    let Some(range) = &command.range else {
        return (0, 0);
    };
    let line = |address: &Address| {
        if let Address::Line(line) = address {
            *line
        } else {
            0
        }
    };
    (
        line(&range.start),
        range.end.as_ref().map_or_else(|| line(&range.start), line),
    )
}

pub type CommandFuture = HostFuture;

#[derive(Clone, Debug, PartialEq)]
pub struct CommandRequest {
    pub command: ExCommand,
    pub context: HostContext,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilenameBehavior {
    None,
    Optional,
    Required,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BarBehavior {
    Chainable,
    Argument,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AddressInterpretation {
    Lines,
    Buffers,
    Windows,
    Tabs,
    None,
}

#[derive(Clone, Debug)]
pub struct CommandDefinition {
    pub name: String,
    pub minimum_abbreviation: usize,
    pub aliases: Vec<(String, usize)>,
    pub accepts_bang: bool,
    pub accepts_range: bool,
    pub accepts_count: bool,
    pub accepts_register: bool,
    pub accepts_opt: bool,
    pub accepts_cmd: bool,
    pub filename_behavior: FilenameBehavior,
    pub bar_behavior: BarBehavior,
    pub allowed_modifiers: Vec<String>,
    pub required_capabilities: Vec<Capability>,
    pub default_range: Option<String>,
    pub address_interpretation: AddressInterpretation,
    pub handler_id: String,
    pub vim_error_behavior: String,
    pub is_extension: bool,
}

impl CommandDefinition {
    pub fn new(name: impl Into<String>, minimum_abbreviation: usize) -> Self {
        Self {
            name: name.into(),
            minimum_abbreviation,
            aliases: Vec::new(),
            accepts_bang: false,
            accepts_range: false,
            accepts_count: false,
            accepts_register: false,
            accepts_opt: false,
            accepts_cmd: false,
            filename_behavior: FilenameBehavior::None,
            bar_behavior: BarBehavior::Chainable,
            allowed_modifiers: Vec::new(),
            required_capabilities: Vec::new(),
            default_range: None,
            address_interpretation: AddressInterpretation::None,
            handler_id: String::new(),
            vim_error_behavior: String::new(),
            is_extension: false,
        }
    }

    pub fn with_bang(mut self, value: bool) -> Self {
        self.accepts_bang = value;
        self
    }

    pub fn with_range(mut self, value: bool) -> Self {
        self.accepts_range = value;
        self
    }

    pub fn with_count(mut self, value: bool) -> Self {
        self.accepts_count = value;
        self
    }

    pub fn with_register(mut self, value: bool) -> Self {
        self.accepts_register = value;
        self
    }

    pub fn with_capabilities(mut self, caps: Vec<Capability>) -> Self {
        self.required_capabilities = caps;
        self
    }
}

#[derive(Clone, Debug, Default)]
pub struct CommandRegistry {
    pub commands: HashMap<String, CommandDefinition>,
}

impl CommandRegistry {
    pub fn register(&mut self, definition: CommandDefinition) {
        self.commands.insert(definition.name.clone(), definition);
    }
    pub fn resolve(&self, name: &str) -> RuntimeResult<&CommandDefinition> {
        if let Some(command) = self.commands.get(name) {
            return Ok(command);
        }
        for command in self.commands.values() {
            for (alias, _) in &command.aliases {
                if alias == name {
                    return Ok(command);
                }
            }
        }
        let mut matches = Vec::new();
        for command in self.commands.values() {
            if name.len() >= command.minimum_abbreviation && command.name.starts_with(name) {
                matches.push(command);
            } else {
                for (alias, min_abbr) in &command.aliases {
                    if name.len() >= *min_abbr && alias.starts_with(name) {
                        matches.push(command);
                        break;
                    }
                }
            }
        }
        let mut matches_iter = matches.into_iter();
        let Some(command) = matches_iter.next() else {
            return Err(RuntimeError::coded(
                "E492",
                RuntimeErrorKind::InvalidCommand,
                format!("not an editor command: {name}"),
            ));
        };
        if matches_iter.next().is_some() {
            return Err(RuntimeError::coded(
                "E464",
                RuntimeErrorKind::InvalidCommand,
                format!("ambiguous command: {name}"),
            ));
        }
        Ok(command)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestHost;
    impl Host for TestHost {
        fn call(&self, _request: HostRequest) -> HostFuture {
            Box::pin(async { Ok(Value::Null) })
        }
    }

    fn command(source: &str) -> ExCommand {
        ExLineParser::new(SourceId(0), source, 0)
            .parse()
            .unwrap()
            .command
    }

    #[test]
    fn user_commands_validate_and_expand_placeholders() {
        let mut runtime = HostRuntime::new(Arc::new(TestHost));
        runtime
            .define_user_command(&command(
                "command! -nargs=1 -bang -range Demo write <args>-<bang>-<line1>-<line2>-<q-args>",
            ))
            .unwrap();
        let expanded = runtime
            .prepare_command(CommandRequest {
                command: command("1,2Demo! value"),
                context: HostContext::default(),
            })
            .unwrap();
        assert_eq!(expanded.command.name, "write");
        assert_eq!(expanded.command.arguments, "value-!-1-2-'value'");
    }

    #[test]
    fn user_commands_enforce_arity_and_replacement_rules() {
        let mut runtime = HostRuntime::new(Arc::new(TestHost));
        runtime
            .define_user_command(&command("command -nargs=0 Demo write"))
            .unwrap();
        let duplicate = runtime
            .define_user_command(&command("command -nargs=0 Demo write"))
            .unwrap_err();
        assert_eq!(duplicate.code.as_deref(), Some("E174"));
        let arity = runtime
            .prepare_command(CommandRequest {
                command: command("Demo extra"),
                context: HostContext::default(),
            })
            .unwrap_err();
        assert_eq!(arity.code.as_deref(), Some("E471"));
        runtime
            .define_user_command(&command("command! -nargs=* Demo write <args>"))
            .unwrap();
        assert_eq!(
            runtime
                .prepare_command(CommandRequest {
                    command: command("Demo one two"),
                    context: HostContext::default()
                })
                .unwrap()
                .command
                .arguments,
            "one two"
        );
    }
}

pub trait RangeStateProvider {
    fn cursor_line(&self) -> usize;
    fn line_count(&self) -> usize;
    fn get_mark(&self, name: char) -> Option<usize>;
    fn search_pattern(&self, pattern: &str, forward: bool, start_line: usize) -> Option<usize>;
}

struct SemicolonProvider<'a, P: RangeStateProvider> {
    base: &'a P,
    temp_cursor: usize,
}

impl<'a, P: RangeStateProvider> RangeStateProvider for SemicolonProvider<'a, P> {
    fn cursor_line(&self) -> usize {
        self.temp_cursor
    }
    fn line_count(&self) -> usize {
        self.base.line_count()
    }
    fn get_mark(&self, name: char) -> Option<usize> {
        self.base.get_mark(name)
    }
    fn search_pattern(&self, pattern: &str, forward: bool, start_line: usize) -> Option<usize> {
        self.base.search_pattern(pattern, forward, start_line)
    }
}

pub fn resolve_address<P: RangeStateProvider>(
    address: &crate::ast::Address,
    provider: &P,
) -> RuntimeResult<usize> {
    use crate::ast::Address;
    match address {
        Address::Current => Ok(provider.cursor_line()),
        Address::Last => Ok(provider.line_count()),
        Address::Line(line) => {
            let l = *line as usize;
            if l <= provider.line_count() {
                Ok(l)
            } else {
                Err(RuntimeError::coded(
                    "E16",
                    RuntimeErrorKind::InvalidCommand,
                    format!("Invalid address: line {line} is beyond buffer end"),
                ))
            }
        }
        Address::WholeFile => Ok(1),
        Address::Mark(mark) => {
            if let Some(line) = provider.get_mark(*mark) {
                Ok(line)
            } else {
                Err(RuntimeError::coded(
                    "E20",
                    RuntimeErrorKind::InvalidCommand,
                    format!("Mark '{mark}' not set"),
                ))
            }
        }
        Address::Search { pattern, forward } => {
            if let Some(line) = provider.search_pattern(pattern, *forward, provider.cursor_line()) {
                Ok(line)
            } else {
                Err(RuntimeError::coded(
                    "E486",
                    RuntimeErrorKind::InvalidCommand,
                    format!("Pattern not found: {pattern}"),
                ))
            }
        }
        Address::Offset { base, amount } => {
            let base_val = resolve_address(base, provider)?;
            let final_val = if *amount >= 0 {
                base_val.saturating_add(*amount as usize)
            } else {
                base_val.saturating_sub((-*amount) as usize)
            };
            if final_val > provider.line_count() {
                Ok(provider.line_count())
            } else {
                Ok(final_val)
            }
        }
    }
}

pub fn resolve_range<P: RangeStateProvider>(
    range: &crate::ast::CommandRange,
    provider: &P,
) -> RuntimeResult<(usize, usize)> {
    let start = resolve_address(&range.start, provider)?;
    let end = if let Some(end_addr) = &range.end {
        if let Some(crate::ast::RangeSeparator::Semicolon) = range.separator {
            let relative_provider = SemicolonProvider {
                base: provider,
                temp_cursor: start,
            };
            resolve_address(end_addr, &relative_provider)?
        } else {
            resolve_address(end_addr, provider)?
        }
    } else {
        match &range.start {
            crate::ast::Address::WholeFile => provider.line_count(),
            _ => start,
        }
    };

    let (final_start, final_end) = if start > end {
        (end, start)
    } else {
        (start, end)
    };

    Ok((final_start, final_end))
}

#[cfg(test)]
mod range_tests {
    use super::*;

    struct MockProvider;
    impl RangeStateProvider for MockProvider {
        fn cursor_line(&self) -> usize {
            10
        }
        fn line_count(&self) -> usize {
            100
        }
        fn get_mark(&self, name: char) -> Option<usize> {
            if name == 'a' { Some(15) } else { None }
        }
        fn search_pattern(&self, pattern: &str, forward: bool, start_line: usize) -> Option<usize> {
            if pattern == "match" {
                if forward {
                    Some(start_line + 5)
                } else {
                    Some(start_line - 5)
                }
            } else {
                None
            }
        }
    }

    #[test]
    fn test_resolve_address_and_range() {
        let provider = MockProvider;

        // Current line (.)
        let addr = crate::ast::Address::Current;
        assert_eq!(resolve_address(&addr, &provider).unwrap(), 10);

        // Last line ($)
        let addr = crate::ast::Address::Last;
        assert_eq!(resolve_address(&addr, &provider).unwrap(), 100);

        // Mark
        let addr = crate::ast::Address::Mark('a');
        assert_eq!(resolve_address(&addr, &provider).unwrap(), 15);

        // Search forward
        let addr = crate::ast::Address::Search {
            pattern: "match".to_owned(),
            forward: true,
        };
        assert_eq!(resolve_address(&addr, &provider).unwrap(), 15);

        // Offset
        let addr = crate::ast::Address::Offset {
            base: Box::new(crate::ast::Address::Current),
            amount: 7,
        };
        assert_eq!(resolve_address(&addr, &provider).unwrap(), 17);
    }

    #[test]
    fn test_parse_count_and_register() {
        // Both register and count present
        let (count, reg, rem) = parse_count_and_register("a 5 rest of args", true, true);
        assert_eq!(count, Some(5));
        assert_eq!(reg, Some('a'));
        assert_eq!(rem, "rest of args");

        // Only count present (register accepted, but first arg is a number)
        let (count, reg, rem) = parse_count_and_register("5 rest of args", true, true);
        assert_eq!(count, Some(5));
        assert_eq!(reg, None);
        assert_eq!(rem, "rest of args");

        // Only register present
        let (count, reg, rem) = parse_count_and_register("x rest of args", false, true);
        assert_eq!(count, None);
        assert_eq!(reg, Some('x'));
        assert_eq!(rem, "rest of args");
    }
}
