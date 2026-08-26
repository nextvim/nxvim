//! Stable identities used across command, event, and asynchronous boundaries.
//!
//! Buffer identity is owned by `vim-buffer`; window and tab identities are
//! owned by `vim-ui`. Re-exporting them here avoids introducing duplicate ID
//! types during the migration.

pub use vim_buffer::BufferId;
pub use vim_ui::{TabPageId, WindowId};
