use crate::{
    BufferOptions, ChangeList, EditOrigin, FileMetadata, MarkSet, MutationOutcome, OptionsOutcome,
    Revision, SelectionSet, UndoTree, edit::summaries_for_patch, snapshot::BufferSnapshot,
};
use clock::ReplicaId;
use std::{num::NonZeroU64, path::Path, sync::Arc, time::Duration};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BufferId(NonZeroU64);

impl BufferId {
    pub fn new(value: u64) -> Option<Self> {
        NonZeroU64::new(value).map(Self)
    }

    pub fn get(self) -> u64 {
        self.0.get()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ChangedTick(u64);

impl ChangedTick {
    pub const INITIAL: Self = Self(0);

    pub fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BufferLifecycle {
    #[default]
    Loaded,
    Unloaded,
    Deleted,
    Wiped,
}

#[derive(Clone)]
struct SavedState {
    revision: Revision,
    options: BufferOptions,
}

pub struct Buffer {
    id: BufferId,
    text: text::Buffer,
    changedtick: ChangedTick,
    saved: SavedState,
    options: BufferOptions,
    file: FileMetadata,
    lifecycle: BufferLifecycle,
    listed: bool,
    marks: MarkSet,
    changes: ChangeList,
    undo_metadata: UndoTree,
}

impl Buffer {
    pub fn new(id: BufferId, replica: ReplicaId, initial_text: impl Into<String>) -> Self {
        let text_id = text::BufferId::new(id.get()).expect("vim BufferId is non-zero");
        let mut text = text::Buffer::new(replica, text_id, initial_text);
        text.set_group_interval(Duration::ZERO);
        let mut options = BufferOptions::default();
        options.fileformat = text.snapshot().line_ending().into();
        let saved = SavedState {
            revision: text.version(),
            options: options.clone(),
        };
        Self {
            id,
            text,
            changedtick: ChangedTick::INITIAL,
            saved,
            options,
            file: FileMetadata::default(),
            lifecycle: BufferLifecycle::Loaded,
            listed: true,
            marks: MarkSet::default(),
            changes: ChangeList::default(),
            undo_metadata: UndoTree::default(),
        }
    }

    pub fn id(&self) -> BufferId {
        self.id
    }

    pub fn revision(&self) -> Revision {
        self.text.version()
    }

    pub fn changedtick(&self) -> ChangedTick {
        self.changedtick
    }

    pub fn transaction(&mut self, origin: crate::EditOrigin) -> crate::Transaction<'_> {
        crate::Transaction::new(self, origin)
    }

    pub fn is_modified(&self) -> bool {
        self.text.snapshot().has_edits_since(&self.saved.revision)
            || !self.options.file_state_eq(&self.saved.options)
    }

    pub fn mark_saved(&mut self) {
        self.saved = SavedState {
            revision: self.text.version(),
            options: self.options.clone(),
        };
    }

    pub fn is_listed(&self) -> bool {
        self.listed
    }

    pub fn lifecycle(&self) -> BufferLifecycle {
        self.lifecycle
    }

    pub fn path(&self) -> Option<&Path> {
        self.file.path.as_deref()
    }

    pub fn options(&self) -> &BufferOptions {
        &self.options
    }

    pub fn set_options(
        &mut self,
        options: BufferOptions,
    ) -> Result<Option<OptionsOutcome>, crate::BufferError> {
        if !options.fileencoding.eq_ignore_ascii_case("utf-8") {
            return Err(crate::BufferError::UnsupportedEncoding(
                options.fileencoding.clone(),
            ));
        }
        if options == self.options {
            return Ok(None);
        }
        let modified_before = self.is_modified();
        let old = self.options.clone();
        if options.fileformat != self.options.fileformat {
            let line_ending = text::LineEnding::try_from(options.fileformat)
                .map_err(|_| crate::BufferError::UnsupportedFileFormat)?;
            self.text.set_line_ending(line_ending);
        }
        self.options = options.clone();
        Ok(Some(OptionsOutcome {
            buffer: self.id,
            old,
            new: options,
            modified_changed: modified_before != self.is_modified(),
        }))
    }

    pub fn marks(&self) -> &MarkSet {
        &self.marks
    }

    pub fn change_list(&self) -> &ChangeList {
        &self.changes
    }

    pub fn undo_metadata(&self) -> &UndoTree {
        &self.undo_metadata
    }

    pub fn snapshot(&self) -> BufferSnapshot {
        BufferSnapshot {
            id: self.id,
            changedtick: self.changedtick,
            inner: self.text.snapshot().clone(),
        }
    }

    pub fn as_text_buffer(&self) -> &text::Buffer {
        &self.text
    }

    pub fn undo(&mut self) -> Result<Option<MutationOutcome>, crate::BufferError> {
        self.apply_history_change(EditOrigin::Undo)
    }

    pub fn redo(&mut self) -> Result<Option<MutationOutcome>, crate::BufferError> {
        self.apply_history_change(EditOrigin::Redo)
    }

    fn apply_history_change(
        &mut self,
        origin: EditOrigin,
    ) -> Result<Option<MutationOutcome>, crate::BufferError> {
        if self.lifecycle != BufferLifecycle::Loaded {
            return Err(crate::BufferError::InvalidLifecycleTransition);
        }
        let before = self.snapshot();
        let modified_before = self.is_modified();
        let subscription = self.text.subscribe();
        let operation = match origin {
            EditOrigin::Undo => self.text.undo(),
            EditOrigin::Redo => self.text.redo(),
            _ => unreachable!("history changes only support undo and redo"),
        };
        let Some((transaction, _operation)) = operation else {
            return Ok(None);
        };
        let patch = subscription.consume();
        let after = self.text.snapshot().clone();
        self.increment_changedtick();
        let selections = match origin {
            EditOrigin::Undo => self.undo_metadata.undo_selections(transaction),
            EditOrigin::Redo => self.undo_metadata.redo_selections(transaction),
            _ => None,
        };
        let edits = summaries_for_patch(before.as_inner(), &after, &patch);
        Ok(Some(MutationOutcome {
            buffer: self.id,
            old_revision: before.revision().clone(),
            new_revision: after.version.clone(),
            changedtick: self.changedtick,
            transaction: Some(transaction),
            edits: Arc::from(edits),
            origin,
            selections,
            modified_changed: modified_before != self.is_modified(),
        }))
    }

    pub(crate) fn apply_text_edits(
        &mut self,
        edits: Vec<(std::ops::Range<usize>, std::sync::Arc<str>)>,
    ) -> Option<text::TransactionId> {
        self.text.edit(edits);
        self.text
            .finalize_last_transaction()
            .map(|transaction| transaction.id)
    }

    pub(crate) fn increment_changedtick(&mut self) {
        self.changedtick.0 = self.changedtick.0.wrapping_add(1);
    }

    pub(crate) fn record_undo_metadata(
        &mut self,
        transaction: text::TransactionId,
        selections: Option<SelectionSet>,
    ) {
        self.undo_metadata
            .record(transaction, selections, self.changedtick);
    }
}
