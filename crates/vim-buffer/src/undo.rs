use crate::{ChangedTick, SelectionSet};

/// Vim navigation metadata for a transaction owned by `text::Buffer`.
///
/// This type never stores or applies inverse text edits. Zed's CRDT-aware
/// history remains authoritative for all undo and redo mutations.
#[derive(Clone, Debug)]
pub struct UndoNode {
    pub transaction: text::TransactionId,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    pub before_selections: Option<SelectionSet>,
    pub after_selections: Option<SelectionSet>,
    pub changedtick: ChangedTick,
}

#[derive(Clone, Debug, Default)]
pub struct UndoTree {
    nodes: Vec<UndoNode>,
    current: Option<usize>,
}

impl UndoTree {
    pub fn current(&self) -> Option<&UndoNode> {
        self.current.and_then(|index| self.nodes.get(index))
    }
}
