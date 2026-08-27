pub(super) mod registry;

use std::path::PathBuf;
use vim_script::host::CommandRequest;
use vim_script::runtime::{RuntimeError, RuntimeErrorKind};

use crate::app::legacy_command::Command;
use crate::app::range_ops::RangeOperation;

/// Execute the Ex command request and translate it to a controller Command.
pub fn execute(request: CommandRequest) -> Result<Command, RuntimeError> {
    match request.command.name.as_str() {
        "quit" => quit(request),
        "bnext" => buffers(request),
        "bprevious" | "bprev" => buffers(request),
        "tabnext" | "nexttab" | "tabprevious" | "previoustab" => tabs(request),
        "tabnew" => tab_new(request),
        "tabclose" => Ok(Command::TabClose),
        "save" | "write" | "update" => files(request),
        "edit" | "enew" | "view" | "visual" | "ex" => edit(request),
        "split" | "hsplit" | "vsplit" => split(request),
        "new" | "vnew" => split_new(request),
        "saveas" => saveas(request),
        "qall" | "quitall" => qall(request),
        "cquit" => cquit(request),
        "wq" | "xit" | "exit" | "x" => wq(request),
        "wqall" | "wqa" | "xa" | "xall" => wqall(request),
        "read" => read(request),
        "file" => file(request),
        "nohlsearch" | "nohl" => Ok(Command::ClearSearchHighlight),
        "pwd" | "cd" | "chdir" | "lcd" | "tcd" | "checktime" | "copy" | "move" | "join"
        | "print" | "change" | "global" | "g" | "vglobal" | "v" | "vimgrep" | "vimgrepadd" => {
            placeholders(request)
        }
        "substitute" | "s" | "&" | "~" | "smagic" | "snomagic" => substitute(request),
        "" => Ok(Command::RangeOp {
            operation: RangeOperation::Goto,
            bang: request.command.bang,
            range: request.command.range,
            count: request.command.count,
            register: request.command.register,
        }),
        "/" => search_forward(request),
        "?" => search_backward(request),
        "delete" => delete(request),
        "yank" => yank(request),
        "put" => put(request),
        "colorscheme" => colorscheme(request),
        "set" => set(request),
        "syntax" => syntax(request),
        "treesitter" => treesitter(request),
        "indexer" => indexer(request),
        "inspect" => inspect(request),
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

fn tabs(request: CommandRequest) -> Result<Command, RuntimeError> {
    let count = request.command.count.unwrap_or(1) as usize;
    match request.command.name.as_str() {
        "tabnext" | "nexttab" => Ok(Command::TabNext { count }),
        "tabprevious" | "previoustab" => Ok(Command::TabPrevious { count }),
        _ => unreachable!(),
    }
}

fn tab_new(request: CommandRequest) -> Result<Command, RuntimeError> {
    let path = request.command.arguments.trim();
    Ok(Command::TabNew {
        path: (!path.is_empty()).then(|| PathBuf::from(path)),
    })
}

fn buffers(request: CommandRequest) -> Result<Command, RuntimeError> {
    let count = request.command.count.unwrap_or(1) as usize;
    match request.command.name.as_str() {
        "bnext" => Ok(Command::BufferNext { count }),
        "bprevious" | "bprev" => Ok(Command::BufferPrevious { count }),

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
    Ok(Command::QuitAll {
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
        "split" | "hsplit" => Ok(Command::Editor {
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

fn split_new(request: CommandRequest) -> Result<Command, RuntimeError> {
    Ok(Command::SplitNew {
        vertical: request.command.name == "vnew",
    })
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

fn inspect(request: CommandRequest) -> Result<Command, RuntimeError> {
    let arg = request.command.arguments.trim();
    if arg == "on" {
        Ok(Command::Inspect { enable: true })
    } else if arg == "off" {
        Ok(Command::Inspect { enable: false })
    } else {
        Err(RuntimeError::coded(
            "E474",
            RuntimeErrorKind::InvalidCommand,
            format!("Invalid argument: {}", arg),
        ))
    }
}

fn search_forward(request: CommandRequest) -> Result<Command, RuntimeError> {
    Ok(Command::SearchForward {
        pattern: request.command.arguments,
    })
}

fn search_backward(request: CommandRequest) -> Result<Command, RuntimeError> {
    Ok(Command::SearchBackward {
        pattern: request.command.arguments,
    })
}

fn substitute(request: CommandRequest) -> Result<Command, RuntimeError> {
    let args = request.command.arguments.trim();
    let (pattern, substitute_text, flags) = if args.is_empty() {
        (String::new(), String::new(), String::new())
    } else {
        let mut chars = args.chars();
        let delimiter = chars.next().unwrap_or('/');
        if delimiter.is_alphanumeric() || delimiter.is_whitespace() || delimiter == '\\' {
            (args.to_string(), String::new(), String::new())
        } else {
            let mut pat = String::new();
            let mut rep = String::new();
            let mut flg = String::new();
            let mut escaped = false;
            let mut delimiter_count = 1;
            for ch in chars {
                if escaped {
                    if delimiter_count == 1 {
                        pat.push(ch);
                    } else if delimiter_count == 2 {
                        rep.push(ch);
                    } else {
                        flg.push(ch);
                    }
                    escaped = false;
                    continue;
                }
                if ch == '\\' {
                    escaped = true;
                    if delimiter_count == 1 {
                        pat.push(ch);
                    } else if delimiter_count == 2 {
                        rep.push(ch);
                    } else {
                        flg.push(ch);
                    }
                    continue;
                }
                if ch == delimiter {
                    delimiter_count += 1;
                    if delimiter_count > 3 {
                        flg.push(ch);
                    }
                    continue;
                }
                if delimiter_count == 1 {
                    pat.push(ch);
                } else if delimiter_count == 2 {
                    rep.push(ch);
                } else {
                    flg.push(ch);
                }
            }
            (pat, rep, flg)
        }
    };
    let range = request.command.range.clone();
    Ok(Command::Substitute {
        pattern,
        substitute_text,
        flags,
        range,
    })
}
