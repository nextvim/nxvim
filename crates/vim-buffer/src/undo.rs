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

    pub(crate) fn record(
        &mut self,
        transaction: text::TransactionId,
        selections: Option<SelectionSet>,
        changedtick: ChangedTick,
    ) {
        let parent = self.current;
        let index = self.nodes.len();
        if let Some(parent) = parent {
            self.nodes[parent].children.push(index);
        }
        self.nodes.push(UndoNode {
            transaction,
            parent,
            children: Vec::new(),
            before_selections: selections.clone(),
            after_selections: selections,
            changedtick,
        });
        self.current = Some(index);
    }

    pub(crate) fn undo_selections(
        &mut self,
        transaction: text::TransactionId,
    ) -> Option<SelectionSet> {
        let index = self
            .nodes
            .iter()
            .position(|node| node.transaction == transaction)?;
        let selections = self.nodes[index].before_selections.clone();
        self.current = self.nodes[index].parent;
        selections
    }

    pub(crate) fn redo_selections(
        &mut self,
        transaction: text::TransactionId,
    ) -> Option<SelectionSet> {
        let index = self
            .nodes
            .iter()
            .position(|node| node.transaction == transaction)?;
        let selections = self.nodes[index].after_selections.clone();
        self.current = Some(index);
        selections
    }
}
