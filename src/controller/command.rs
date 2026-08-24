use std::path::PathBuf;

use vim_input::Action;
use vim_ui::{NavigationDirection, SplitAxis, WindowId};

use super::range::RangeOperation;

pub enum Command {
    Editor {
        action: Action,
        register: Option<char>,
    },
    PendingInput(String),
    InvalidInput,
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

impl std::fmt::Debug for Command {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Command::Editor { action, register } => f
                .debug_struct("Editor")
                .field("action", action)
                .field("register", register)
                .finish(),
            Command::PendingInput(seq) => f.debug_tuple("PendingInput").field(seq).finish(),
            Command::InvalidInput => write!(f, "InvalidInput"),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewEffect {
    Focus(WindowId),
    Split { source: WindowId, axis: SplitAxis },
    FocusDirection(NavigationDirection),
    Close(WindowId),
    Hide(WindowId),
    Resize { width: u16, height: u16 },
    SetCommandLineMode(char),
}

#[derive(Debug, Default)]
pub struct CommandOutcome {
    pub redraw: bool,
    pub quit: bool,
    pub view_effects: Vec<ViewEffect>,
}

impl CommandOutcome {
    pub fn redraw() -> Self {
        Self {
            redraw: true,
            ..Self::default()
        }
    }

    pub fn quit() -> Self {
        Self {
            redraw: true,
            quit: true,
            ..Self::default()
        }
    }

    pub fn with_effect(effect: ViewEffect) -> Self {
        Self {
            redraw: true,
            view_effects: vec![effect],
            ..Self::default()
        }
    }

    pub fn merge(&mut self, mut other: Self) {
        self.redraw |= other.redraw;
        self.quit |= other.quit;
        self.view_effects.append(&mut other.view_effects);
    }
}
