use std::sync::{Arc, Mutex, mpsc};
use std::{collections::HashMap, path::PathBuf};

use crate::controller::Command;
use text::{BufferId, BufferSnapshot};

use vim_script::{
    compiler::Compiler,
    host::{
        Capability, CommandDefinition, CommandRequest, Host, HostFuture, HostRequest, HostRuntime,
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

pub struct ScriptRuntime {
    scheduler: Scheduler,
    commands: mpsc::Receiver<Command>,
    globals: HashMap<String, Value>,
    builtins: BuiltinRegistry,
    sources: SourceMap,
    state: Arc<Mutex<EditorState>>,
}

impl ScriptRuntime {
    pub fn new() -> Self {
        let (sender, commands) = mpsc::channel();
        let state = Arc::new(Mutex::new(EditorState::default()));
        let mut host = HostRuntime::new(Arc::new(EditorHost {
            sender,
            state: state.clone(),
        }));
        host.capabilities.grant(Capability::Editor);
        host.capabilities.grant(Capability::BufferRead);

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
        }
    }

    pub fn execute(&mut self, source: &str) -> Result<Value, String> {
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
        Ok(value)
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

    pub fn try_next_command(&self) -> Option<Command> {
        self.commands.try_recv().ok()
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
    sender: mpsc::Sender<Command>,
    state: Arc<Mutex<EditorState>>,
}

impl Host for EditorHost {
    fn call(&self, request: HostRequest) -> HostFuture {
        let sender = self.sender.clone();
        Box::pin(async move {
            match request.function.as_str() {
                "echo" | "message" | "echomsg" => {
                    expect_arity(&request, 1)?;
                    let message = request.arguments[0].to_string();
                    sender.send(Command::Echo { message }).map_err(|_| {
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

    fn execute_command(&self, request: CommandRequest) -> HostFuture {
        let sender = self.sender.clone();
        Box::pin(async move {
            let command = commands::execute(request)?;
            sender.send(command).map_err(|_| {
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
    fn navigation_commands_are_dispatched() {
        for (source, forward) in [("bnext", true), ("previoustab", false)] {
            let mut runtime = ScriptRuntime::new();
            runtime.execute(source).unwrap();
            assert!(match runtime.try_next_command() {
                Some(Command::Editor {
                    action: vim_input::Action::NextTab { count: 1 },
                    register: None,
                }) => forward,
                Some(Command::Editor {
                    action: vim_input::Action::PreviousTab { count: 1 },
                    register: None,
                }) => !forward,
                _ => false,
            });
        }
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
            assert_eq!(operation, crate::controller::RangeOperation::Delete);
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
