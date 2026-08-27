use std::path::PathBuf;

use vim_input::Action;

#[cfg(test)]
use crate::app::outcome::CommandOutcome;
use crate::app::prompt::{PromptChoice, PromptHandler};
use crate::app::range_ops::RangeOperation;

pub use super::typed_command::*;

pub enum ExCommand {
    Editor {
        action: Action,
        register: Option<char>,
    },
    PendingInput(crate::kernel::PendingCommandState),
    InvalidInput,
    PromptChoice {
        handler: PromptHandler,
        choice: PromptChoice,
    },
    OpenPrompt {
        message: String,
    },
    Save {
        path: Option<PathBuf>,
        force: bool,
    },
    Quit {
        force: bool,
    },
    QuitAll {
        force: bool,
    },
    Edit {
        path: Option<PathBuf>,
        force: bool,
    },
    SplitNew {
        vertical: bool,
    },
    TabNew {
        path: Option<PathBuf>,
    },
    TabNext {
        count: usize,
    },
    TabPrevious {
        count: usize,
    },
    TabClose,
    BufferNext {
        count: usize,
    },
    BufferPrevious {
        count: usize,
    },
    WriteQuit {
        path: Option<PathBuf>,
        force: bool,
    },
    WriteQuitAll {
        force: bool,
    },
    RangeOp {
        operation: RangeOperation,
        bang: bool,
        range: Option<vim_script::ast::CommandRange>,
        count: Option<u64>,
        register: Option<char>,
    },
    Task(crate::app::services::TaskResult),
    ClearSearchHighlight,
    Colorscheme {
        name: Option<String>,
    },
    Set {
        arguments: String,
    },
    SetOption {
        name: String,
        value: vim_script::runtime::Value,
        scope: vim_script::host::OptionRequestScope,
    },
    ReplaceBuffer {
        buffer: u64,
        range: vim_script::host::OwnedTextRange,
        text: String,
    },
    Syntax {
        enable: bool,
    },
    Treesitter {
        enable: bool,
    },
    Indexer {
        enable: bool,
    },
    Inspect {
        enable: bool,
    },
    Echo {
        message: String,
    },
    ExecuteScript(String),
    CommandLine(crate::kernel::CommandLineRequest),
    SearchForward {
        pattern: String,
    },
    SearchBackward {
        pattern: String,
    },
    Substitute {
        pattern: String,
        substitute_text: String,
        flags: String,
        range: Option<vim_script::ast::CommandRange>,
    },
}

use self::ExCommand as Command;

impl std::fmt::Debug for ExCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Command::Editor { action, register } => f
                .debug_struct("Editor")
                .field("action", action)
                .field("register", register)
                .finish(),
            Command::PendingInput(seq) => f.debug_tuple("PendingInput").field(seq).finish(),
            Command::InvalidInput => write!(f, "InvalidInput"),
            Command::PromptChoice { handler, choice } => f
                .debug_struct("PromptChoice")
                .field("handler", handler)
                .field("choice", choice)
                .finish(),
            Command::OpenPrompt { message } => f
                .debug_struct("OpenPrompt")
                .field("message", message)
                .finish(),
            Command::Save { path, force } => f
                .debug_struct("Save")
                .field("path", path)
                .field("force", force)
                .finish(),
            Command::Quit { force } => f.debug_struct("Quit").field("force", force).finish(),
            Command::QuitAll { force } => f.debug_struct("QuitAll").field("force", force).finish(),
            Command::Edit { path, force } => f
                .debug_struct("Edit")
                .field("path", path)
                .field("force", force)
                .finish(),
            Command::SplitNew { vertical } => f
                .debug_struct("SplitNew")
                .field("vertical", vertical)
                .finish(),
            Command::TabNew { path } => f.debug_struct("TabNew").field("path", path).finish(),
            Command::TabNext { count } => f.debug_struct("TabNext").field("count", count).finish(),
            Command::TabPrevious { count } => {
                f.debug_struct("TabPrevious").field("count", count).finish()
            }
            Command::TabClose => write!(f, "TabClose"),
            Command::BufferNext { count } => {
                f.debug_struct("BufferNext").field("count", count).finish()
            }
            Command::BufferPrevious { count } => f
                .debug_struct("BufferPrevious")
                .field("count", count)
                .finish(),
            Command::WriteQuit { path, force } => f
                .debug_struct("WriteQuit")
                .field("path", path)
                .field("force", force)
                .finish(),
            Command::WriteQuitAll { force } => f
                .debug_struct("WriteQuitAll")
                .field("force", force)
                .finish(),
            Command::RangeOp {
                operation,
                bang,
                range,
                count,
                register,
            } => f
                .debug_struct("RangeOp")
                .field("operation", operation)
                .field("bang", bang)
                .field("range", range)
                .field("count", count)
                .field("register", register)
                .finish(),
            Command::Task(_) => write!(f, "Task(...)"),
            Command::ClearSearchHighlight => write!(f, "ClearSearchHighlight"),
            Command::Colorscheme { name } => {
                f.debug_struct("Colorscheme").field("name", name).finish()
            }
            Command::Set { arguments } => {
                f.debug_struct("Set").field("arguments", arguments).finish()
            }
            Command::SetOption { name, value, scope } => f
                .debug_struct("SetOption")
                .field("name", name)
                .field("value", value)
                .field("scope", scope)
                .finish(),
            Command::ReplaceBuffer {
                buffer,
                range,
                text,
            } => f
                .debug_struct("ReplaceBuffer")
                .field("buffer", buffer)
                .field("range", range)
                .field("text", text)
                .finish(),
            Command::Syntax { enable } => f.debug_struct("Syntax").field("enable", enable).finish(),
            Command::Treesitter { enable } => f
                .debug_struct("Treesitter")
                .field("enable", enable)
                .finish(),
            Command::Indexer { enable } => {
                f.debug_struct("Indexer").field("enable", enable).finish()
            }
            Command::Inspect { enable } => {
                f.debug_struct("Inspect").field("enable", enable).finish()
            }
            Command::Echo { message } => f.debug_struct("Echo").field("message", message).finish(),
            Command::ExecuteScript(script) => f.debug_tuple("ExecuteScript").field(script).finish(),
            Command::CommandLine(request) => f.debug_tuple("CommandLine").field(request).finish(),
            Command::SearchForward { pattern } => f
                .debug_struct("SearchForward")
                .field("pattern", pattern)
                .finish(),
            Command::SearchBackward { pattern } => f
                .debug_struct("SearchBackward")
                .field("pattern", pattern)
                .finish(),
            Command::Substitute {
                pattern,
                substitute_text,
                flags,
                range,
            } => f
                .debug_struct("Substitute")
                .field("pattern", pattern)
                .field("substitute_text", substitute_text)
                .field("flags", flags)
                .field("range", range)
                .finish(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kernel_effects_survive_the_app_boundary() {
        let kernel = crate::kernel::CommandOutcome {
            effects: vec![
                crate::kernel::CommandEffect::Message("done".to_string()),
                crate::kernel::CommandEffect::QuitRequested,
            ],
            redraw: crate::kernel::RedrawRequest::View,
            invalidations: Vec::new(),
        };
        let outcome = CommandOutcome::from_kernel(kernel);
        assert_eq!(outcome.redraw, crate::kernel::RedrawRequest::View);
        assert!(outcome.quit);
        assert_eq!(outcome.kernel_effects.len(), 2);
    }
}
