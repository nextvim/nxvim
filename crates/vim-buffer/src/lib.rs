//! Editor-agnostic Vim-compatible buffer primitives.
//!
//! `vim-buffer` is a Vim compatibility layer over [`text::Buffer`]. Zed owns
//! text storage, snapshots, anchors, versions, primitive transactions, and
//! undo/redo. This crate owns Vim lifecycle, options, `changedtick`, selection
//! policy, marks, outcomes, and synchronous callback sequencing. It does not
//! depend on a renderer, terminal UI, editor document, or async runtime.

mod buffer;
mod callback;
mod edit;
mod error;
mod history;
mod io;
mod manager;
mod marks;
mod movement;
mod mutator;
mod options;
mod outcome;
mod position;
mod search;
mod selection;
mod selection_set;
mod snapshot;
mod transaction;
mod undo;

pub use buffer::{Buffer, BufferId, BufferLifecycle, ChangedTick};

/// Revision identity is Zed's version vector from `text::BufferSnapshot`.
pub type Revision = clock::Global;
pub use callback::{Callback, CallbackContext, CallbackRegistry, VimEvent};
pub use edit::{Edit, EditOrigin, EditSummary, PlannedEdit};
pub use error::BufferError;
pub use history::{ChangeEntry, ChangeList};
pub use io::{DecodedFile, ExternalFileStatus, FileMetadata, LoadSource, decode_utf8, encode_utf8};
pub use manager::BufferManager;
pub use marks::MarkSet;
pub use movement::Motions;
pub use mutator::{Action, ActionOutcome, Mutator};
pub use options::{BufferOptions, FileFormat, UnsupportedFileFormat};
pub use outcome::{ManagerOutcome, MutationOutcome, OptionsOutcome, SaveOutcome};
pub use position::{ByteOffset, TextExtent, TextRange};
pub use search::{TextSearch, compile};
pub use selection::SelectionExt;
pub use selection_set::{SelectionCellState, SelectionCollection, SelectionSet};
pub use snapshot::{BufferSnapshot, TextChunks};
pub use text::{OffsetUtf16, Point, PointUtf16};
pub use transaction::Transaction;
pub use undo::{UndoNode, UndoTree};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SelectionId(usize);

impl SelectionId {
    pub const fn new(value: usize) -> Self {
        Self(value)
    }

    pub const fn get(self) -> usize {
        self.0
    }
}

/// Compile-time description of the current storage backend.
pub const TEXT_BACKEND: &str = "Zed text::Buffer (Rope + SumTree)";
