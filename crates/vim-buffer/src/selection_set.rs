use crate::{BufferError, SelectionId, VimSelection};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq)]
pub struct SelectionSet {
    primary: SelectionId,
    selections: Vec<VimSelection>,
}

impl SelectionSet {
    pub fn new(primary: SelectionId, selections: Vec<VimSelection>) -> Result<Self, BufferError> {
        let mut ids = HashSet::with_capacity(selections.len());
        let valid = selections
            .first()
            .is_some_and(|selection| selection.id() == primary)
            && selections
                .iter()
                .all(|selection| ids.insert(selection.id()));
        if !valid {
            return Err(BufferError::InvalidSelectionSet);
        }
        Ok(Self {
            primary,
            selections,
        })
    }

    pub fn primary(&self) -> SelectionId {
        self.primary
    }

    pub fn selections(&self) -> &[VimSelection] {
        &self.selections
    }

    pub fn primary_selection(&self) -> &VimSelection {
        self.selections
            .first()
            .expect("SelectionSet invariant requires a primary selection")
    }

    pub fn replace_primary(&mut self, selection: VimSelection) -> Result<(), BufferError> {
        if selection.id() != self.primary {
            return Err(BufferError::InvalidSelectionSet);
        }
        self.selections[0] = selection;
        Ok(())
    }

    pub fn len(&self) -> usize {
        self.selections.len()
    }

    pub fn is_empty(&self) -> bool {
        false
    }
}
