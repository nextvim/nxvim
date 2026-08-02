use crate::{
    BufferOptions, ByteOffset, ChangeEntry, ChangeList, EditOrigin, EditSummary, FileMetadata,
    MarkSet, MutationOutcome, OptionsOutcome, Revision, SelectionSet, UndoTree,
    edit::summaries_for_patch, snapshot::BufferSnapshot,
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
    Hidden,
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

    pub fn is_loaded(&self) -> bool {
        matches!(
            self.lifecycle,
            BufferLifecycle::Loaded | BufferLifecycle::Hidden
        )
    }

    pub(crate) fn set_lifecycle(&mut self, lifecycle: BufferLifecycle) {
        self.lifecycle = lifecycle;
    }

    pub(crate) fn set_listed(&mut self, listed: bool) {
        self.listed = listed;
    }

    pub(crate) fn set_file_metadata(&mut self, file: FileMetadata) {
        self.file = file;
    }

    pub fn file_metadata(&self) -> &FileMetadata {
        &self.file
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
        if options.fileformat != self.options.fileformat
            && let Ok(line_ending) = text::LineEnding::try_from(options.fileformat)
        {
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

    pub fn set_mark(&mut self, name: char, offset: ByteOffset) -> Result<(), crate::BufferError> {
        let snapshot = self.snapshot();
        let offset = snapshot.validate_offset(offset)?;
        let anchor = self.text.anchor_before(offset);
        self.marks.set(name, anchor)?;
        Ok(())
    }

    pub fn delete_mark(&mut self, name: char) -> Result<bool, crate::BufferError> {
        Ok(self.marks.remove(name)?.is_some())
    }

    pub fn resolve_mark(&self, name: char) -> Option<ByteOffset> {
        self.marks.resolve(name, &self.snapshot())
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
        if !self.is_loaded() {
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
        let state = match origin {
            EditOrigin::Undo => self.undo_metadata.undo_state(transaction),
            EditOrigin::Redo => self.undo_metadata.redo_state(transaction),
            _ => None,
        };
        let selections = state
            .as_ref()
            .and_then(|(selections, _)| selections.clone());
        if let Some((_, marks)) = state {
            self.marks = marks;
        }
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

    pub(crate) fn last_transaction_id(&self) -> Option<text::TransactionId> {
        self.text
            .peek_undo_stack()
            .map(text::HistoryEntry::transaction_id)
    }

    pub(crate) fn merge_transaction(
        &mut self,
        source: text::TransactionId,
        destination: text::TransactionId,
    ) {
        self.text.merge_transactions(source, destination);
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

    pub(crate) fn finish_change_metadata(
        &mut self,
        transaction: text::TransactionId,
        selections: Option<SelectionSet>,
        before_marks: MarkSet,
        before: &BufferSnapshot,
        edits: &[EditSummary],
    ) {
        let deleted = edits.iter().map(|edit| edit.old_range).collect::<Vec<_>>();
        self.marks.remove_marks_on_deleted_lines(before, &deleted);

        let after = self.snapshot();
        if let (Some(first), Some(last)) = (edits.first(), edits.last()) {
            let start = first.new_range.start.0.min(after.len_bytes());
            let last_offset = if last.new_range.end.0 > last.new_range.start.0 {
                after
                    .as_inner()
                    .as_rope()
                    .floor_char_boundary(last.new_range.end.0.saturating_sub(1))
            } else {
                last.new_range.start.0.min(after.len_bytes())
            };
            let start_anchor = self.text.anchor_before(start);
            let end_anchor = self.text.anchor_after(last_offset);
            let _ = self.marks.set('[', start_anchor);
            let _ = self.marks.set(']', end_anchor);
            let _ = self.marks.set('.', self.text.anchor_before(start));
            self.changes.record(
                ChangeEntry {
                    transaction: Some(transaction),
                    revision: after.revision().clone(),
                    position: self.text.anchor_before(start),
                },
                &after,
            );
        }

        let after_marks = self.marks.clone();
        self.undo_metadata.record(
            transaction,
            selections,
            self.changedtick,
            before_marks,
            after_marks,
        );
    }
}
