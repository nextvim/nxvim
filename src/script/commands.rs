use std::path::PathBuf;
use vim_script::host::CommandRequest;
use vim_script::runtime::{RuntimeError, RuntimeErrorKind};

use crate::controller::Command;

/// Execute the Ex command request and translate it to a controller Command.
pub fn execute(request: CommandRequest) -> Result<Command, RuntimeError> {
    match request.command.name.as_str() {
        "quit" => quit(request),
        "bnext" | "nexttab" => buffers(request),
        "bprevious" | "bprev" | "previoustab" => buffers(request),
        "save" | "write" | "update" => files(request),
        "edit" | "enew" | "view" | "visual" | "ex" => edit(request),
        "saveas" => saveas(request),
        "qall" | "quitall" => qall(request),
        "cquit" => cquit(request),
        "wq" | "xit" | "exit" => wq(request),
        "wqall" => wqall(request),
        "read" => read(request),
        "file" => file(request),
        "pwd" | "cd" | "chdir" | "lcd" | "tcd" | "checktime" | "yank" | "put" | "copy" | "move" | "join" | "print" | "change"
        | "/" | "?" | "substitute" | "s" | "&" | "~" | "smagic" | "snomagic" | "global" | "g" | "vglobal" | "v" | "nohlsearch" | "nohl" | "vimgrep" | "vimgrepadd" => placeholders(request),
        "delete" => delete(request),
        name => Err(RuntimeError::coded(
            "E492",
            RuntimeErrorKind::InvalidCommand,
            format!("not an editor command: {name}"),
        )),
    }
}

fn quit(request: CommandRequest) -> Result<Command, RuntimeError> {
    Ok(Command::Quit {
        force: request.command.bang,
    })
}

fn buffers(request: CommandRequest) -> Result<Command, RuntimeError> {
    let count = request.command.count.unwrap_or(1) as u32;
    match request.command.name.as_str() {
        "bnext" | "nexttab" => Ok(Command::Editor {
            action: vim_input::Action::NextTab { count },
            register: None,
        }),
        "bprevious" | "bprev" | "previoustab" => Ok(Command::Editor {
            action: vim_input::Action::PreviousTab { count },
            register: None,
        }),
        _ => unreachable!(),
    }
}

fn files(request: CommandRequest) -> Result<Command, RuntimeError> {
    let argument = request.command.arguments.trim();
    Ok(Command::Save {
        path: (!argument.is_empty()).then(|| PathBuf::from(argument)),
        force: request.command.bang,
    })
}

fn edit(request: CommandRequest) -> Result<Command, RuntimeError> {
    let argument = request.command.arguments.trim();
    let path = if argument.is_empty() {
        None
    } else {
        Some(PathBuf::from(argument))
    };
    Ok(Command::Edit {
        path,
        force: request.command.bang,
    })
}

fn saveas(request: CommandRequest) -> Result<Command, RuntimeError> {
    let argument = request.command.arguments.trim();
    if argument.is_empty() {
        return Err(RuntimeError::coded(
            "E471",
            RuntimeErrorKind::InvalidCommand,
            "Argument required",
        ));
    }
    Ok(Command::Save {
        path: Some(PathBuf::from(argument)),
        force: request.command.bang,
    })
}

fn qall(request: CommandRequest) -> Result<Command, RuntimeError> {
    Ok(Command::Quit {
        force: request.command.bang,
    })
}

fn cquit(request: CommandRequest) -> Result<Command, RuntimeError> {
    Ok(Command::Quit {
        force: request.command.bang,
    })
}

fn wq(_request: CommandRequest) -> Result<Command, RuntimeError> {
    Err(RuntimeError::coded(
        "E_NOTIMPL",
        RuntimeErrorKind::HostError,
        "wq command is a placeholder/stub for now",
    ))
}

fn wqall(_request: CommandRequest) -> Result<Command, RuntimeError> {
    Err(RuntimeError::coded(
        "E_NOTIMPL",
        RuntimeErrorKind::HostError,
        "wqall command is a placeholder/stub for now",
    ))
}

fn read(_request: CommandRequest) -> Result<Command, RuntimeError> {
    Err(RuntimeError::coded(
        "E_NOTIMPL",
        RuntimeErrorKind::HostError,
        "read command is a placeholder/stub for now",
    ))
}

fn file(_request: CommandRequest) -> Result<Command, RuntimeError> {
    Err(RuntimeError::coded(
        "E_NOTIMPL",
        RuntimeErrorKind::HostError,
        "file command is a placeholder/stub for now",
    ))
}

fn placeholders(request: CommandRequest) -> Result<Command, RuntimeError> {
    Err(RuntimeError::coded(
        "E_NOTIMPL",
        RuntimeErrorKind::HostError,
        format!("{} command is a placeholder/stub for now", request.command.name),
    ))
}

fn delete(request: CommandRequest) -> Result<Command, RuntimeError> {
    Ok(Command::Delete {
        range: request.command.range,
        count: request.command.count,
        register: request.command.register,
    })
}
