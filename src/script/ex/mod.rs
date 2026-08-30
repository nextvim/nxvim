//! Script-owned Ex command handlers.
//!
//! These commands mutate scripting/input configuration rather than editor
//! buffers, so they are handled by `ScriptHost` before kernel admission.

use vim_script::ast::ExCommand;
use vim_script::host::{CommandRequest, HostContext};
use vim_script::runtime::{RuntimeError, RuntimeErrorKind, RuntimeResult};

use super::{Abbreviation, AbbreviationMode, ScriptHost};

impl ScriptHost {
    fn handle_abbreviation_command(&mut self, command: &ExCommand) -> RuntimeResult<()> {
        let name = command.name.as_str();

        match name {
            "abclear" => {
                self.abbreviations.clear();
                return Ok(());
            }
            "iabclear" => {
                self.abbreviations
                    .retain(|abbreviation| !abbreviation.modes.contains(&AbbreviationMode::Insert));
                return Ok(());
            }
            "cabclear" => {
                self.abbreviations.retain(|abbreviation| {
                    !abbreviation.modes.contains(&AbbreviationMode::CommandLine)
                });
                return Ok(());
            }
            _ => {}
        }

        let (modes, non_recursive, remove) = match name {
            "abbreviate" => (
                vec![AbbreviationMode::Insert, AbbreviationMode::CommandLine],
                false,
                false,
            ),
            "iabbrev" => (vec![AbbreviationMode::Insert], false, false),
            "cabbrev" => (vec![AbbreviationMode::CommandLine], false, false),
            "noreabbrev" => (
                vec![AbbreviationMode::Insert, AbbreviationMode::CommandLine],
                true,
                false,
            ),
            "inoreabbrev" => (vec![AbbreviationMode::Insert], true, false),
            "cnoreabbrev" => (vec![AbbreviationMode::CommandLine], true, false),
            "unabbreviate" => (
                vec![AbbreviationMode::Insert, AbbreviationMode::CommandLine],
                false,
                true,
            ),
            "iunabbrev" => (vec![AbbreviationMode::Insert], false, true),
            "cunabbrev" => (vec![AbbreviationMode::CommandLine], false, true),
            _ => {
                return Err(RuntimeError::coded(
                    "E474",
                    RuntimeErrorKind::InvalidCommand,
                    format!("unknown abbreviation command: {name}"),
                ));
            }
        };

        let arguments = command.arguments.trim();
        if arguments.is_empty() {
            return Err(RuntimeError::coded(
                "E471",
                RuntimeErrorKind::InvalidCommand,
                "argument required",
            ));
        }

        let mut parts = arguments.splitn(2, char::is_whitespace);
        let lhs = parts.next().unwrap().to_owned();

        if remove {
            let mut removed = false;
            self.abbreviations.retain(|abbreviation| {
                let matches = abbreviation.lhs == lhs
                    && abbreviation.modes.iter().any(|mode| modes.contains(mode));
                removed |= matches;
                !matches
            });
            return if removed {
                Ok(())
            } else {
                Err(RuntimeError::coded(
                    "E31",
                    RuntimeErrorKind::InvalidCommand,
                    format!("no such abbreviation: {lhs}"),
                ))
            };
        }

        let rhs = parts
            .next()
            .ok_or_else(|| {
                RuntimeError::coded("E471", RuntimeErrorKind::InvalidCommand, "rhs required")
            })?
            .trim_start()
            .to_owned();

        self.abbreviations.retain(|abbreviation| {
            !(abbreviation.lhs == lhs && abbreviation.modes.iter().any(|mode| modes.contains(mode)))
        });
        self.abbreviations.push(Abbreviation {
            lhs,
            rhs,
            non_recursive,
            modes,
        });
        Ok(())
    }

    fn handle_digraph_command(&mut self, command: &ExCommand) -> RuntimeResult<()> {
        let arguments = command.arguments.trim();
        if arguments.is_empty() {
            return Ok(());
        }

        let mut parts = arguments.split_whitespace();
        let pair = parts.next().ok_or_else(|| {
            RuntimeError::coded("E471", RuntimeErrorKind::InvalidCommand, "pair required")
        })?;
        let code = parts
            .next()
            .ok_or_else(|| {
                RuntimeError::coded(
                    "E471",
                    RuntimeErrorKind::InvalidCommand,
                    "code point required",
                )
            })?
            .parse::<u32>()
            .map_err(|_| {
                RuntimeError::coded(
                    "E474",
                    RuntimeErrorKind::InvalidCommand,
                    "invalid code point number",
                )
            })?;

        let mut characters = pair.chars();
        let (Some(first), Some(second), None) =
            (characters.next(), characters.next(), characters.next())
        else {
            return Err(RuntimeError::coded(
                "E474",
                RuntimeErrorKind::InvalidCommand,
                "pair must be exactly 2 characters",
            ));
        };
        let target = char::from_u32(code).ok_or_else(|| {
            RuntimeError::coded(
                "E474",
                RuntimeErrorKind::InvalidCommand,
                "invalid unicode code point",
            )
        })?;
        self.digraphs.register(first, second, target);
        Ok(())
    }

    pub fn try_handle_registration(&mut self, command: &ExCommand) -> Option<RuntimeResult<()>> {
        match command.name.as_str() {
            "command" => Some(
                self.scheduler
                    .host_mut()
                    .unwrap()
                    .define_user_command(command),
            ),
            "delcommand" => Some(
                self.scheduler
                    .host_mut()
                    .unwrap()
                    .delete_user_command(command),
            ),
            "abbreviate" | "iabbrev" | "cabbrev" | "noreabbrev" | "inoreabbrev" | "cnoreabbrev"
            | "unabbreviate" | "iunabbrev" | "cunabbrev" | "abclear" | "iabclear" | "cabclear" => {
                Some(self.handle_abbreviation_command(command))
            }
            "digraph" | "dig" | "digraphs" => Some(self.handle_digraph_command(command)),
            _ => {
                let request = CommandRequest {
                    command: command.clone(),
                    context: HostContext::default(),
                };
                self.scheduler
                    .host_mut()
                    .unwrap()
                    .handle_registration_command(&request)
            }
        }
    }

    pub fn expand_user_command(&self, command: ExCommand) -> RuntimeResult<ExCommand> {
        let request = CommandRequest {
            command,
            context: HostContext::default(),
        };
        let prepared = self.scheduler.host().unwrap().prepare_command(request)?;
        Ok(prepared.command)
    }
}
