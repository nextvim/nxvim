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
        let valid = !selections.is_empty()
            && selections
                .iter()
                .all(|selection| ids.insert(selection.id()))
            && ids.contains(&primary);
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
}
