use crate::{ChangedTick, MarkSet, SelectionSet};

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
    pub before_marks: MarkSet,
    pub after_marks: MarkSet,
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
        before_marks: MarkSet,
        after_marks: MarkSet,
    ) {
        if let Some(index) = self
            .nodes
            .iter()
            .position(|node| node.transaction == transaction)
        {
            self.nodes[index].after_selections = selections;
            self.nodes[index].after_marks = after_marks;
            self.nodes[index].changedtick = changedtick;
            self.current = Some(index);
            return;
        }
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
            before_marks,
            after_marks,
        });
        self.current = Some(index);
    }

    pub(crate) fn undo_state(
        &mut self,
        transaction: text::TransactionId,
    ) -> Option<(Option<SelectionSet>, MarkSet)> {
        let index = self
            .nodes
            .iter()
            .position(|node| node.transaction == transaction)?;
        let state = (
            self.nodes[index].before_selections.clone(),
            self.nodes[index].before_marks.clone(),
        );
        self.current = self.nodes[index].parent;
        Some(state)
    }

    pub(crate) fn redo_state(
        &mut self,
        transaction: text::TransactionId,
    ) -> Option<(Option<SelectionSet>, MarkSet)> {
        let index = self
            .nodes
            .iter()
            .position(|node| node.transaction == transaction)?;
        let state = (
            self.nodes[index].after_selections.clone(),
            self.nodes[index].after_marks.clone(),
        );
        self.current = Some(index);
        Some(state)
    }
}
