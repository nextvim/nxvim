//! The editor's semantic mode.
//!
//! This is distinct from `vim_input::Mode`: that one only tracks how to
//! decode the *next* keystroke (does `d` start an operator, is a bare
//! character an insert?). `kernel::Mode` is what `Editor::execute()`
//! actually branches command dispatch on. Grown to one-to-one match
//! `vim_input::Mode` (`Replace`/`VirtualReplace`/`Visual*`/`Command`) as of
//! the "Other modes" milestone.

/// Which kind of Visual selection is active -- the per-window "how do I
/// render/interpret the current selection" fact (`RESCUE.md` Rule 4 item 2),
/// carried on `kernel::Mode::Visual` and mirrored onto `Window::visual_kind`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VisualKind {
    Char,
    Line,
    Block,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Insert,
    Replace,
    VirtualReplace,
    Visual(VisualKind),
    Command,
}

impl Mode {
    pub const fn is_normal(self) -> bool {
        matches!(self, Mode::Normal)
    }

    pub const fn is_insert(self) -> bool {
        matches!(self, Mode::Insert | Mode::Replace | Mode::VirtualReplace)
    }

    pub const fn is_visual(self) -> bool {
        matches!(self, Mode::Visual(_))
    }

    pub const fn is_replace(self) -> bool {
        matches!(self, Mode::Replace | Mode::VirtualReplace)
    }

    pub const fn is_command(self) -> bool {
        matches!(self, Mode::Command)
    }
}
