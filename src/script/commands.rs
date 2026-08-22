use std::path::PathBuf;
use vim_script::host::CommandRequest;
use vim_script::runtime::{RuntimeError, RuntimeErrorKind};

use crate::controller::{Command, RangeOperation};

/// Execute the Ex command request and translate it to a controller Command.
pub fn execute(request: CommandRequest) -> Result<Command, RuntimeError> {
    match request.command.name.as_str() {
        "quit" => quit(request),
        "bnext" | "nexttab" => buffers(request),
        "bprevious" | "bprev" | "previoustab" => buffers(request),
        "save" | "write" | "update" => files(request),
        "edit" | "enew" | "view" | "visual" | "ex" => edit(request),
        "split" | "vsplit" => split(request),
        "saveas" => saveas(request),
        "qall" | "quitall" => qall(request),
        "cquit" => cquit(request),
        "wq" | "xit" | "exit" | "x" => wq(request),
        "wqall" | "wqa" | "xa" | "xall" => wqall(request),
        "read" => read(request),
        "file" => file(request),
        "nohlsearch" | "nohl" => Ok(Command::ClearSearchHighlight),
        "pwd" | "cd" | "chdir" | "lcd" | "tcd" | "checktime" | "copy" | "move" | "join"
        | "print" | "change" | "/" | "?" | "substitute" | "s" | "&" | "~" | "smagic"
        | "snomagic" | "global" | "g" | "vglobal" | "v" | "vimgrep" | "vimgrepadd" => {
            placeholders(request)
        }
        "delete" => delete(request),
        "yank" => yank(request),
        "put" => put(request),
        "colorscheme" => colorscheme(request),
        "set" => set(request),
        "syntax" => syntax(request),
        "treesitter" => treesitter(request),
        "indexer" => indexer(request),
        "echo" => echo(request),
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

fn wq(request: CommandRequest) -> Result<Command, RuntimeError> {
    let argument = request.command.arguments.trim();
    Ok(Command::WriteQuit {
        path: (!argument.is_empty()).then(|| PathBuf::from(argument)),
        force: request.command.bang,
    })
}

fn wqall(request: CommandRequest) -> Result<Command, RuntimeError> {
    Ok(Command::WriteQuitAll {
        force: request.command.bang,
    })
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
        format!(
            "{} command is a placeholder/stub for now",
            request.command.name
        ),
    ))
}

fn delete(request: CommandRequest) -> Result<Command, RuntimeError> {
    Ok(Command::RangeOp {
        operation: RangeOperation::Delete,
        bang: request.command.bang,
        range: request.command.range,
        count: request.command.count,
        register: request.command.register,
    })
}

fn yank(request: CommandRequest) -> Result<Command, RuntimeError> {
    Ok(Command::RangeOp {
        operation: RangeOperation::Yank,
        bang: request.command.bang,
        range: request.command.range,
        count: request.command.count,
        register: request.command.register,
    })
}

fn put(request: CommandRequest) -> Result<Command, RuntimeError> {
    Ok(Command::RangeOp {
        operation: RangeOperation::Put,
        bang: request.command.bang,
        range: request.command.range,
        count: request.command.count,
        register: request.command.register,
    })
}

fn split(request: CommandRequest) -> Result<Command, RuntimeError> {
    let argument = request.command.arguments.trim();
    let file_path = if argument.is_empty() {
        None
    } else {
        Some(argument.to_string())
    };
    match request.command.name.as_str() {
        "split" => Ok(Command::Editor {
            action: vim_input::Action::SplitHorizontal { file_path },
            register: None,
        }),
        "vsplit" => Ok(Command::Editor {
            action: vim_input::Action::SplitVertical { file_path },
            register: None,
        }),
        _ => unreachable!(),
    }
}

fn colorscheme(request: CommandRequest) -> Result<Command, RuntimeError> {
    let name = request.command.arguments.trim();
    let name_opt = if name.is_empty() {
        None
    } else {
        Some(name.to_owned())
    };
    Ok(Command::Colorscheme { name: name_opt })
}

fn set(request: CommandRequest) -> Result<Command, RuntimeError> {
    Ok(Command::Set {
        arguments: request.command.arguments,
    })
}

fn syntax(request: CommandRequest) -> Result<Command, RuntimeError> {
    let arg = request.command.arguments.trim();
    if arg == "on" {
        Ok(Command::Syntax { enable: true })
    } else if arg == "off" {
        Ok(Command::Syntax { enable: false })
    } else {
        Err(RuntimeError::coded(
            "E474",
            RuntimeErrorKind::InvalidCommand,
            format!("Invalid argument: {}", arg),
        ))
    }
}

fn treesitter(request: CommandRequest) -> Result<Command, RuntimeError> {
    let arg = request.command.arguments.trim();
    if arg == "on" {
        Ok(Command::Treesitter { enable: true })
    } else if arg == "off" {
        Ok(Command::Treesitter { enable: false })
    } else {
        Err(RuntimeError::coded(
            "E474",
            RuntimeErrorKind::InvalidCommand,
            format!("Invalid argument: {}", arg),
        ))
    }
}

fn indexer(request: CommandRequest) -> Result<Command, RuntimeError> {
    let arg = request.command.arguments.trim();
    if arg == "on" {
        Ok(Command::Indexer { enable: true })
    } else if arg == "off" {
        Ok(Command::Indexer { enable: false })
    } else {
        Err(RuntimeError::coded(
            "E474",
            RuntimeErrorKind::InvalidCommand,
            format!("Invalid argument: {}", arg),
        ))
    }
}

fn echo(request: CommandRequest) -> Result<Command, RuntimeError> {
    Ok(Command::Echo {
        message: request.command.arguments,
    })
}
