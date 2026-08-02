use crate::{BufferSnapshot, ByteOffset, Revision};
use text::Anchor;

const CHANGE_LIST_CAPACITY: usize = 100;
const DEFAULT_NEARBY_COLUMNS: u32 = 79;

#[derive(Clone, Debug)]
pub struct ChangeEntry {
    pub transaction: Option<text::TransactionId>,
    pub revision: Revision,
    pub position: Anchor,
}

#[derive(Clone, Debug, Default)]
pub struct ChangeList {
    entries: Vec<ChangeEntry>,
    cursor: usize,
}

impl ChangeList {
    pub fn entries(&self) -> &[ChangeEntry] {
        &self.entries
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn older(&mut self, count: usize) -> Option<&ChangeEntry> {
        if self.entries.is_empty() {
            return None;
        }
        self.cursor = self.cursor.saturating_sub(count.max(1));
        self.entries.get(self.cursor)
    }

    pub fn newer(&mut self, count: usize) -> Option<&ChangeEntry> {
        if self.entries.is_empty() {
            return None;
        }
        self.cursor = self
            .cursor
            .saturating_add(count.max(1))
            .min(self.entries.len().saturating_sub(1));
        self.entries.get(self.cursor)
    }

    pub fn resolve(entry: &ChangeEntry, snapshot: &BufferSnapshot) -> Option<ByteOffset> {
        snapshot
            .as_inner()
            .can_resolve(&entry.position)
            .then(|| ByteOffset(snapshot.as_inner().offset_for_anchor(&entry.position)))
    }

    pub(crate) fn record(&mut self, entry: ChangeEntry, snapshot: &BufferSnapshot) {
        if let Some(last) = self.entries.last()
            && nearby(last, &entry, snapshot)
        {
            *self.entries.last_mut().expect("last entry exists") = entry;
            self.cursor = self.entries.len();
            return;
        }
        self.entries.push(entry);
        if self.entries.len() > CHANGE_LIST_CAPACITY {
            self.entries.remove(0);
        }
        self.cursor = self.entries.len();
    }
}

fn nearby(left: &ChangeEntry, right: &ChangeEntry, snapshot: &BufferSnapshot) -> bool {
    if !snapshot.as_inner().can_resolve(&left.position)
        || !snapshot.as_inner().can_resolve(&right.position)
    {
        return false;
    }
    let left = snapshot
        .as_inner()
        .offset_to_point(snapshot.as_inner().offset_for_anchor(&left.position));
    let right = snapshot
        .as_inner()
        .offset_to_point(snapshot.as_inner().offset_for_anchor(&right.position));
    left.row == right.row && left.column.abs_diff(right.column) < DEFAULT_NEARBY_COLUMNS
}
