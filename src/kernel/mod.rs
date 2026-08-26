//! Migration seam for the Rust-native Vim semantic kernel.
//!
//! Phase 0 intentionally keeps this module independent from the existing
//! runtime and controller. Later phases will move authoritative editor state
//! behind these types without duplicating the existing stores.

mod command;
mod events;
mod ex;
mod ids;
pub(crate) mod insert;
pub(crate) mod normal;
mod outcome;
mod state;
pub(crate) mod structural;
mod tabs;
mod transaction;
mod windows;

pub use command::{
    CaseChange, CommandContext, CommandKind, CommandLineKind, CommandLineRequest, NormalCommand,
    PendingCommandState, SearchDirection,
};
pub use events::{EditorEvent, EventQueue, OptionName};
pub use ex::ExDispatcher;
pub use ids::{BufferId, TabPageId, WindowId};
pub use outcome::{
    CommandEffect, CommandOutcome, MutationOutcome, RedrawInvalidation, RedrawInvalidationKind,
    RedrawRequest,
};
pub use state::{EditorContext, EditorState};
pub use tabs::{TabPage, TabPages};
pub(crate) use transaction::transaction;
pub use windows::{WindowRecord, Windows};
