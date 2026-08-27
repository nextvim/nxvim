//! Migration seam for the Rust-native Vim semantic kernel.
//!
//! Phase 0 intentionally keeps this module independent from the existing
//! runtime and controller. Later phases will move authoritative editor state
//! behind these types without duplicating the existing stores.

mod command;
pub(crate) mod commandline;
pub(crate) mod editor;
mod events;
mod ex;
mod ids;
pub(crate) mod insert;
pub(crate) mod normal;
mod outcome;
pub(crate) mod range;
mod registers;
pub(crate) mod search;
mod state;
pub(crate) mod structural;
mod substitute;
mod tabs;
mod transaction;
mod windows;

pub use command::{
    CaseChange, CommandContext, CommandKind, CommandLineKind, CommandLineRequest, CommandMetadata,
    NormalCommand, PendingCommandState, RangeCommand, RangeOperation, SearchDirection,
};
pub use commandline::CommandLineState;
pub use events::{EditorEvent, EventQueue, OptionName};
pub use ex::ExAdmission;
pub use ids::{BufferId, ChannelId, JobId, TabPageId, TerminalId, TimerId, WindowId};
pub use outcome::{
    CommandEffect, CommandOutcome, MutationOutcome, RedrawInvalidation, RedrawInvalidationKind,
    RedrawRequest,
};
pub use registers::RegisterStore;
pub use search::SearchState;
pub use state::{EditorContext, EditorState};
pub use substitute::SubstitutionSession;
pub use tabs::{TabPage, TabPages};
pub(crate) use transaction::transaction;
pub use windows::{SemanticWindow, WindowRecord, Windows};

pub(crate) fn invalidate_folds(
    folds: &mut Vec<display_map::Fold>,
    buffer: &text::Buffer,
    start: usize,
    end: usize,
) {
    use text::ToOffset;

    folds.retain(|fold| {
        let fold_start = fold.start.to_offset(buffer);
        let fold_end = fold.end.to_offset(buffer);
        !(end > fold_start.saturating_sub(1) && start < fold_end.saturating_add(1))
    });
}
