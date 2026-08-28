//! The editor's semantic mode.
//!
//! This is distinct from `vim_input::Mode`: that one only tracks how to
//! decode the *next* keystroke (does `d` start an operator, is a bare
//! character an insert?). `kernel::Mode` is what `Editor::execute()`
//! actually branches command dispatch on. Grown to match `vim_input::Mode`
//! (Replace, Visual*, Command, ...) as later milestones add those command
//! families.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Insert,
    Command,
}

impl Mode {
    pub const fn is_normal(self) -> bool {
        matches!(self, Mode::Normal)
    }

    pub const fn is_insert(self) -> bool {
        matches!(self, Mode::Insert)
    }

    pub const fn is_command(self) -> bool {
        matches!(self, Mode::Command)
    }
}
