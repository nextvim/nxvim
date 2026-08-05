use std::{sync::Arc, sync::mpsc};

use vim_script::{
    host::{
        Capability, CommandDefinition, CommandRequest, Host, HostFuture, HostRequest, HostRuntime,
    },
    runtime::{RuntimeError, RuntimeErrorKind, Scheduler, Value},
};

use crate::EditorCommand;

pub struct ScriptRuntime {
    scheduler: Scheduler,
    commands: mpsc::Receiver<EditorCommand>,
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

    pub fn try_next_command(&self) -> Option<EditorCommand> {
        self.commands.try_recv().ok()
    }

    pub fn is_command_registered(&self, name: &str) -> bool {
        self.scheduler
            .host()
            .is_some_and(|host| host.commands.resolve(name).is_ok())
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
                "quit" | "qall" => EditorCommand::Quit,
                "enew" => EditorCommand::NewBuffer,
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
        [("quit", 1, true), ("qall", 2, true), ("enew", 2, true)]
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
