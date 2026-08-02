use crate::{
    BufferOptions, ChangeList, FileMetadata, MarkSet, Revision, UndoTree, snapshot::BufferSnapshot,
};
use clock::ReplicaId;
use std::{num::NonZeroU64, path::Path, time::Duration};

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

    pub fn is_modified(&self) -> bool {
        self.text.snapshot().has_edits_since(&self.saved.revision)
            || self.options != self.saved.options
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
}
