//! Kernel-owned identity types.
//!
//! Buffer identity belongs to `vim_buffer` (a buffer can exist with zero
//! windows attached, so its identity must not depend on window/tab types).
//! `WindowId` and `TabPageId` are newtypes owned here because windows and tab
//! pages are kernel concepts with no `vim-ui` counterpart.

pub use vim_buffer::BufferId;

/// Identifies a `kernel::window::Window` for as long as it exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WindowId(u64);

impl WindowId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Identifies a `kernel::window::tabpage::TabPage` for as long as it exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TabPageId(u64);

impl TabPageId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}
