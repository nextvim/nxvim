use crate::{Buffer, BufferError, EditOrigin, MutationOutcome, PlannedEdit, SelectionSet};

pub struct Transaction<'a> {
    buffer: &'a mut Buffer,
    origin: EditOrigin,
    edits: Vec<PlannedEdit>,
}

impl<'a> Transaction<'a> {
    pub fn new(buffer: &'a mut Buffer, origin: EditOrigin) -> Self {
        Self {
            buffer,
            origin,
            edits: Vec::new(),
        }
    }

    pub fn push(&mut self, edit: PlannedEdit) {
        self.edits.push(edit);
    }

    pub fn planned_edits(&self) -> &[PlannedEdit] {
        &self.edits
    }

    pub fn commit(self, _selections: Option<SelectionSet>) -> Result<MutationOutcome, BufferError> {
        let _ = (self.buffer, self.origin, self.edits);
        Err(BufferError::NotImplemented("transaction commit"))
    }
}
