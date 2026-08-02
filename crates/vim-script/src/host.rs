use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::ast::{ExCommand, MapMode, MappingOptions, UserCommandAttributes};
use crate::ex_parser::ExLineParser;
use crate::integration::{
    CompiledMapping, Event, EventAction, EventBus, EventHandler, EventHandlerId, KeymapStore,
    MappingExpansion, MappingId,
};
use crate::runtime::{HostObjectId, RuntimeError, RuntimeErrorKind, RuntimeResult, Value, Vm};
use crate::source::SourceId;

pub type HostFuture = Pin<Box<dyn Future<Output = RuntimeResult<Value>> + Send + 'static>>;

/// Application boundary for asynchronous operations. Requests own all data so
/// returned futures can outlive a VM quantum and move to an I/O executor.
pub trait Host: Send + Sync + 'static {
    fn call(&self, request: HostRequest) -> HostFuture;

    fn option(&self, request: OptionRequest) -> HostFuture {
        Box::pin(async move {
            Err(RuntimeError::coded(
                "E_HOST",
                RuntimeErrorKind::HostError,
                format!("host does not implement option access for {}", request.name),
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
    pub keymaps: KeymapStore,
    pub events: EventBus,
    pub current_augroup: Option<String>,
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
            .field("keymaps", &self.keymaps)
            .field("events", &self.events)
            .field("current_augroup", &self.current_augroup)
            .finish_non_exhaustive()
    }
}

impl HostRuntime {
    pub fn new(host: Arc<dyn Host>) -> Self {
        Self {
            host,
            capabilities: CapabilitySet::default(),
            functions: HostFunctionRegistry::default(),
            commands: CommandRegistry::default(),
            user_commands: HashMap::new(),
            keymaps: KeymapStore::default(),
            events: EventBus::default(),
            current_augroup: None,
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
        self.user_commands
            .insert(definition.name.clone(), definition);
        Ok(())
    }

    pub fn remove_user_command(&mut self, name: &str) -> bool {
        self.user_commands.remove(name).is_some()
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
    ) -> Option<&CompiledMapping> {
        self.keymaps.resolve(mode, lhs, buffer)
    }

    pub fn event_commands(&mut self, event: &Event, context: HostContext) -> Vec<CommandRequest> {
        self.events
            .handlers_for(event)
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
                removed |= self.keymaps.unmap(mode, lhs, buffer).is_some();
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
        self.keymaps.register(CompiledMapping {
            id,
            modes,
            lhs: lhs.to_owned(),
            expansion,
            options,
            buffer,
        });
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
        self.current_augroup = (group != "END").then(|| group.to_owned());
        Ok(())
    }

    fn handle_autocmd(&mut self, request: &CommandRequest) -> RuntimeResult<()> {
        if request.command.bang {
            if let Some(group) = &self.current_augroup {
                self.events.remove_group(group);
            } else {
                self.events.handlers.clear();
            }
            if request.command.arguments.trim().is_empty() {
                return Ok(());
            }
        }
        let arguments = request.command.arguments.as_str();
        let (events, mut cursor) = word_at(arguments, 0).ok_or_else(|| {
            RuntimeError::coded(
                "E216",
                RuntimeErrorKind::InvalidCommand,
                "autocmd requires an event",
            )
        })?;
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
        let patterns: Vec<_> = patterns.split(',').map(str::to_owned).collect();
        for event in events.split(',') {
            let id = EventHandlerId(self.next_event_handler_id);
            self.next_event_handler_id += 1;
            self.events.register(EventHandler {
                id,
                group: self.current_augroup.clone(),
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

#[derive(Clone, Debug)]
pub struct CommandDefinition {
    pub name: String,
    pub minimum_abbreviation: usize,
    pub accepts_bang: bool,
    pub accepts_range: bool,
    pub accepts_count: bool,
    pub accepts_register: bool,
    pub required_capabilities: Vec<Capability>,
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
        let mut matches = self.commands.values().filter(|command| {
            name.len() >= command.minimum_abbreviation && command.name.starts_with(name)
        });
        let Some(command) = matches.next() else {
            return Err(RuntimeError::coded(
                "E492",
                RuntimeErrorKind::InvalidCommand,
                format!("not an editor command: {name}"),
            ));
        };
        if matches.next().is_some() {
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
