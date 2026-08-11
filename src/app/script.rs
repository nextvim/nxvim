use std::{collections::HashMap, sync::Arc, sync::mpsc};

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EditorCommand {
    BNext,
    BPrev,
    Quit,
}

pub struct ScriptRuntime {
    scheduler: Scheduler,
    commands: mpsc::Receiver<EditorCommand>,
    globals: HashMap<String, Value>,
    sources: SourceMap,
}

impl ScriptRuntime {
    pub fn new() -> Self {
        let (sender, commands) = mpsc::channel();
        let mut host = HostRuntime::new(Arc::new(EditorHost { sender }));
        host.capabilities.grant(Capability::Editor);
        host.register_command(CommandDefinition {
            name: "quit".to_owned(),
            minimum_abbreviation: 1,
            accepts_bang: true,
            accepts_range: false,
            accepts_count: false,
            accepts_register: false,
            required_capabilities: vec![Capability::Editor],
        });

        host.register_command(CommandDefinition {
            name: "bnext".to_owned(),
            minimum_abbreviation: 2,
            accepts_bang: true,
            accepts_range: false,
            accepts_count: false,
            accepts_register: false,
            required_capabilities: vec![Capability::Editor],
        });

        host.register_command(CommandDefinition {
            name: "bprev".to_owned(),
            minimum_abbreviation: 2,
            accepts_bang: true,
            accepts_range: false,
            accepts_count: false,
            accepts_register: false,
            required_capabilities: vec![Capability::Editor],
        });

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

    pub fn try_next_command(&self) -> Option<EditorCommand> {
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
    sender: mpsc::Sender<EditorCommand>,
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
            let command = match request.command.name.as_str() {
                "quit" => EditorCommand::Quit,
                "bnext" => EditorCommand::BNext,
                "bprev" => EditorCommand::BPrev,
                name => {
                    return Err(RuntimeError::coded(
                        "E492",
                        RuntimeErrorKind::InvalidCommand,
                        format!("not an editor command: {name}"),
                    ));
                }
            };
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
    fn quit_and_its_abbreviation_are_dispatched() {
        for source in ["q", "quit"] {
            let mut runtime = ScriptRuntime::new();
            runtime.execute(source).unwrap();
            assert_eq!(runtime.try_next_command(), Some(EditorCommand::Quit));
        }
    }

    #[test]
    fn unknown_commands_are_rejected_by_the_engine() {
        let mut runtime = ScriptRuntime::new();
        assert!(runtime.execute("missing").is_err());
        assert_eq!(runtime.try_next_command(), None);
    }
}
