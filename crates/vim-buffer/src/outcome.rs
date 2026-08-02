use crate::{
    BufferId, BufferOptions, ChangedTick, EditOrigin, EditSummary, Revision, SelectionSet,
};
use std::{path::PathBuf, sync::Arc};

#[derive(Clone, Debug)]
pub struct MutationOutcome {
    pub buffer: BufferId,
    pub old_revision: Revision,
    pub new_revision: Revision,
    pub changedtick: ChangedTick,
    pub transaction: Option<text::TransactionId>,
    pub edits: Arc<[EditSummary]>,
    pub origin: EditOrigin,
    pub selections: Option<SelectionSet>,
    pub modified_changed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OptionsOutcome {
    pub buffer: BufferId,
    pub old: BufferOptions,
    pub new: BufferOptions,
    pub modified_changed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveOutcome {
    pub buffer: BufferId,
    pub path: PathBuf,
    pub bytes_written: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ManagerOutcome {
    Added(BufferId),
    Existing(BufferId),
    Loaded(BufferId),
    Unloaded(BufferId),
    Deleted(BufferId),
    Wiped(BufferId),
    CurrentChanged {
        old: Option<BufferId>,
        new: BufferId,
    },
}
