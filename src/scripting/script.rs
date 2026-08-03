use std::collections::HashMap;
use std::{sync::Arc, sync::mpsc};

use vim_script::{
    compiler::Compiler,
    host::{
        Capability, CommandDefinition, CommandRequest, Host, HostFuture, HostRequest, HostRuntime,
    },
    lexer::Lexer,
    parser::Parser,
    resolver::{Resolver, ResolverConfig},
    runtime::{RuntimeError, RuntimeErrorKind, Scheduler, Value, Vm},
    source::{Diagnostic, Severity, SourceMap},
};

use crate::controller::ex::Ex;

pub struct ScriptRuntime {
    scheduler: Scheduler,
    commands: mpsc::Receiver<Ex>,
}

impl ScriptRuntime {
    pub fn new() -> Self {
        let (sender, commands) = mpsc::channel();
        let mut host = HostRuntime::new(Arc::new(EditorHost { sender }));
        host.capabilities.grant(Capability::Editor);
        register_editor_commands(&mut host);

        let mut scheduler = Scheduler::default();
        scheduler.set_host(host);
        Self {
            scheduler,
            commands,
        }
    }

    pub fn try_next_command(&self) -> Option<Ex> {
        self.commands.try_recv().ok()
    }

    pub fn is_command_registered(&self, name: &str) -> bool {
        self.scheduler
            .host()
            .is_some_and(|host| host.commands.resolve(name).is_ok())
    }

    pub fn host_runtime(&self) -> Option<&HostRuntime> {
        self.scheduler.host()
    }
}

impl Default for ScriptRuntime {
    fn default() -> Self {
        Self::new()
    }
}

struct EditorHost {
    sender: mpsc::Sender<Ex>,
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
                "quit" | "qall" => Ex::Quit,
                "edit" => Ex::Edit,
                "delete" => Ex::Delete,
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

fn register_editor_commands(host: &mut HostRuntime) {
    for (name, minimum_abbreviation, accepts_bang) in
        [("quit", 1, true), ("qall", 2, true), ("edit", 1, true)]
    {
        host.register_command(CommandDefinition {
            name: name.to_owned(),
            minimum_abbreviation,
            accepts_bang,
            accepts_range: false,
            accepts_count: false,
            accepts_register: false,
            required_capabilities: vec![Capability::Editor],
        });
    }

    for (name, minimum_abbreviation) in [("delete", 1)] {
        host.register_command(CommandDefinition {
            name: name.to_owned(),
            minimum_abbreviation,
            accepts_bang: false,
            accepts_range: true,
            accepts_count: true,
            accepts_register: false,
            required_capabilities: vec![Capability::Editor],
        });
    }
}

pub fn execute_source(
    source: &str,
    globals: &mut HashMap<String, Value>,
    host: &HostRuntime,
    sources: &mut SourceMap,
) -> Option<Value> {
    let source_id = sources.add("repl_input", source);

    let lexed = Lexer::new(source_id, source).lex();
    if !lexed.diagnostics.is_empty() {
        for diagnostic in &lexed.diagnostics {
            print!("{}", sources.render(diagnostic));
        }
        return None;
    }

    let parsed = Parser::new_with_source(&lexed.tokens, source).parse();
    if !parsed.diagnostics.is_empty() {
        for diagnostic in &parsed.diagnostics {
            print!("{}", sources.render(diagnostic));
        }
        return None;
    }
    let Some(program) = parsed.program else {
        return None;
    };

    let mut config = ResolverConfig::default();
    for name in host.functions.names() {
        config.builtins.insert(name.to_string());
    }
    let resolved = Resolver::new(config).resolve(program);
    if !resolved.diagnostics.is_empty() {
        for diagnostic in &resolved.diagnostics {
            print!("{}", sources.render(diagnostic));
        }
        return None;
    }
    let Some(resolved_program) = resolved.program else {
        return None;
    };

    let compiled = Compiler::new(&resolved_program).compile();
    if !compiled.diagnostics.is_empty() {
        for diagnostic in &compiled.diagnostics {
            print!("{}", sources.render(diagnostic));
        }
        return None;
    }
    let Some(module) = compiled.module else {
        return None;
    };

    let vm = match Vm::with_globals(module, globals.clone()) {
        Ok(vm) => vm,
        Err(err) => {
            println!("VM error: {}", err.message);
            return None;
        }
    };

    let mut scheduler = Scheduler::new(10_000);
    scheduler.set_host(host.clone());

    let task = match scheduler.spawn(vm) {
        Ok(task) => task,
        Err(err) => {
            println!("Scheduler error: {}", err.message);
            return None;
        }
    };

    match scheduler.run_until_complete(task) {
        Ok(val) => {
            if let Some(finished_task) = scheduler.task(task) {
                *globals = finished_task.vm.globals.clone();
            }
            Some(val)
        }
        Err(err) => {
            println!("Runtime error: {}", err.message);
            if let Some(span) = err.span {
                let diag = Diagnostic {
                    code: err.code.clone(),
                    severity: Severity::Error,
                    message: err.message.clone(),
                    primary: span,
                    labels: Vec::new(),
                    notes: err.notes.to_vec(),
                    suggestions: Vec::new(),
                };
                print!("{}", sources.render(&diag));
            }
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registers_initial_editor_commands_and_abbreviations() {
        let runtime = ScriptRuntime::new();

        assert!(runtime.is_command_registered("quit"));
        assert!(runtime.is_command_registered("q"));
        assert!(runtime.is_command_registered("qall"));
        assert!(runtime.is_command_registered("qa"));
        assert!(runtime.is_command_registered("enew"));
        assert!(runtime.is_command_registered("ene"));
        assert!(!runtime.is_command_registered("write"));
    }
}
