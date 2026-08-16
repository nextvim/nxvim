use std::{collections::HashMap, path::PathBuf, sync::Arc, sync::mpsc};

use crate::controller::Command;

use vim_script::{
    compiler::Compiler,
    host::{
        Capability, CommandDefinition, CommandRequest, Host, HostFuture, HostRequest, HostRuntime,
    },
    lexer::Lexer,
    parser::Parser,
    resolver::{Resolver, ResolverConfig},
    runtime::{RuntimeError, RuntimeErrorKind, Scheduler, Value, Vm},
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
        let mut host = HostRuntime::new(Arc::new(EditorHost { sender }));
        host.capabilities.grant(Capability::Editor);
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

struct EditorHost {
    sender: mpsc::Sender<Command>,
}

impl Host for EditorHost {
    fn call(&self, request: HostRequest) -> HostFuture {
        Box::pin(async move {
            Err(RuntimeError::coded(
                "E_NOTIMPL",
                RuntimeErrorKind::HostError,
                format!("host function is not implemented: {}", request.function),
            ))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_central_registry_specifications() {
        let mut host = HostRuntime::new(Arc::new(EditorHost {
            sender: mpsc::channel().0,
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
            Some(Command::WriteQuit { path: None, force: false })
        ));

        let mut runtime = ScriptRuntime::new();
        runtime.execute(":xa").unwrap();
        assert!(matches!(
            runtime.try_next_command(),
            Some(Command::WriteQuitAll { force: false })
        ));
    }
}

