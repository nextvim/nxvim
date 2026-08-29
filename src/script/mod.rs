//! Script host interface, managing user mappings, commands, and autocommands.

use std::collections::HashMap;
use std::sync::Arc;
use vim_script::ast::ExCommand;
use vim_script::host::{CommandRequest, Host, HostContext, HostRuntime};
use vim_script::integration::{Event, SharedKeymapStore};
use vim_script::runtime::RuntimeResult;

pub struct ScriptHost {
    runtime: HostRuntime,
    keymaps: SharedKeymapStore,
}

impl ScriptHost {
    pub fn new(host: Arc<dyn Host>, keymaps: SharedKeymapStore) -> Self {
        let runtime = HostRuntime::with_keymaps(host, keymaps.clone());
        Self { runtime, keymaps }
    }

    pub fn shared_keymaps(&self) -> SharedKeymapStore {
        self.keymaps.clone()
    }

    pub fn try_handle_registration(&mut self, command: &ExCommand) -> Option<RuntimeResult<()>> {
        match command.name.as_str() {
            "command" => Some(self.runtime.define_user_command(command)),
            "delcommand" => Some(self.runtime.delete_user_command(command)),
            _ => {
                let request = CommandRequest {
                    command: command.clone(),
                    context: HostContext::default(),
                };
                self.runtime.handle_registration_command(&request)
            }
        }
    }

    pub fn expand_user_command(&self, command: ExCommand) -> RuntimeResult<ExCommand> {
        let request = CommandRequest {
            command,
            context: HostContext::default(),
        };
        let prepared = self.runtime.prepare_command(request)?;
        Ok(prepared.command)
    }

    pub fn fire_event(&mut self, name: &str, pattern: Option<&str>) -> Vec<ExCommand> {
        let event = Event {
            name: name.to_owned(),
            pattern: pattern.map(String::from),
            payload: HashMap::new(),
        };
        let requests = self.runtime.event_commands(&event, HostContext::default());
        requests.into_iter().map(|req| req.command).collect()
    }
}
