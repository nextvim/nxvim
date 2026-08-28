//! Events the kernel emits when editor state changes.
//!
//! One variant for now — just enough to validate the mutation contract this
//! milestone is about. More arrive (`BufEnter`, `CursorMoved`, `InsertEnter`,
//! ...) with the milestones that actually consume them (autocommands,
//! script host). No `EventQueue` type yet either: today's only consumer is
//! the `Vec<EditorEvent>` a single `Editor::execute()` call returns on its
//! `Outcome`; a persistent multi-listener queue is a "Script host" concern.

use vim_buffer::{BufferId, ChangedTick};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditorEvent {
    /// A buffer's text changed. Carries the `ChangedTick` so a listener can
    /// tell which revision this corresponds to.
    TextChanged { buffer: BufferId, tick: ChangedTick },
}
