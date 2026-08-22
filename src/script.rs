use std::sync::{Arc, Mutex, mpsc};
use std::{collections::HashMap, path::PathBuf};

use crate::controller::Command;

use vim_script::{
    compiler::Compiler,
    host::{
        Arity, Capability, CommandDefinition, CommandRequest, Host, HostFuture, HostRequest,
        HostRuntime,
    },
    lexer::Lexer,
    parser::Parser,
    resolver::{Resolver, ResolverConfig},
    runtime::{RuntimeError, RuntimeErrorKind, RuntimeResult, Scheduler, Value, Vm},
    source::SourceMap,
};

pub mod commands;
pub mod registry;

use registry::COMMAND_SPECS;

pub struct ScriptRuntime {
    scheduler: Scheduler,
    commands: mpsc::Receiver<Command>,
    globals: HashMap<String, Value>,
    sources: SourceMap,
}

impl ScriptRuntime {
    pub fn new() -> Self {
        let (sender, commands) = mpsc::channel();
        let mut host = HostRuntime::new(Arc::new(EditorHost {
            sender,
            state: Arc::new(Mutex::new(EditorState::default())),
        }));
        host.capabilities.grant(Capability::Editor);

        host.register_function("echo", Arity::Exact(1), vec![Capability::Editor]);
        host.register_function("message", Arity::Exact(1), vec![Capability::Editor]);
        host.register_function("echomsg", Arity::Exact(1), vec![Capability::Editor]);

        for spec in COMMAND_SPECS {
            host.register_command(CommandDefinition::from(spec));
        }

        let mut scheduler = Scheduler::default();
        scheduler.set_host(host);
        Self {
            scheduler,
            commands,
            globals: HashMap::new(),
            sources: SourceMap::default(),
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
        let vm = Vm::with_globals(module, self.globals.clone()).map_err(runtime_message)?;
        let task = self.scheduler.spawn(vm).map_err(runtime_message)?;
        let value = self
            .scheduler
            .run_until_complete(task)
            .map_err(runtime_message)?;
        if let Some(task) = self.scheduler.task(task) {
            self.globals = task.vm.globals.clone();
        }
        Ok(value)
    }

    pub fn try_next_command(&self) -> Option<Command> {
        self.commands.try_recv().ok()
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

fn integer_argument(request: &HostRequest, index: usize) -> RuntimeResult<i64> {
    match &request.arguments[index] {
        Value::Integer(value) => Ok(*value),
        value => Err(RuntimeError::coded(
            "E745",
            RuntimeErrorKind::TypeError,
            format!(
                "argument {} to {} must be a number, got {}",
                index + 1,
                request.function,
                value.type_name()
            ),
        )),
    }
}

fn string_argument(request: &HostRequest, index: usize) -> RuntimeResult<String> {
    match &request.arguments[index] {
        Value::String(value) => Ok(value.to_string()),
        value => Err(RuntimeError::coded(
            "E730",
            RuntimeErrorKind::TypeError,
            format!(
                "argument {} to {} must be a string, got {}",
                index + 1,
                request.function,
                value.type_name()
            ),
        )),
    }
}

#[derive(Clone, Debug)]
pub struct EditorState {
    pub messages: Vec<String>,
}

impl Default for EditorState {
    fn default() -> Self {
        Self {
            messages: Vec::new(),
        }
    }
}

struct EditorHost {
    sender: mpsc::Sender<Command>,
    pub state: Arc<Mutex<EditorState>>,
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

fn runtime_message(error: RuntimeError) -> String {
    match error.code {
        Some(code) => format!("{code}: {}", error.message),
        None => error.message,
    }
}

fn lock_error() -> RuntimeError {
    RuntimeError::coded(
        "E605",
        RuntimeErrorKind::HostError,
        "editor state lock is poisoned",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
