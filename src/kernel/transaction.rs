//! The single mutation entry point (`RESCUE.md` Rule 4.6).
//!
//! Every command family that changes text — Normal, Insert, Ex, script,
//! autocommand-triggered — must call `apply` here. Nothing else is allowed
//! to reach into a `vim_buffer::Buffer` and edit it directly; this is what
//! keeps undo grouping, `TextChanged` events, and redraw invalidation
//! uniform no matter what triggered the edit.

use vim_buffer::{Buffer, BufferError, EditOrigin, MutationOutcome, PlannedEdit, SelectionSet};

/// A batch of edits to commit to one buffer as a single transaction.
pub struct EditDescription {
    pub origin: EditOrigin,
    pub edits: Vec<PlannedEdit>,
    /// The selection state to record alongside this transaction, so undo/redo
    /// can restore the cursor to where it was. `None` leaves selections
    /// untouched by the commit itself.
    pub selections: Option<SelectionSet>,
    pub join_previous: bool,
}

/// Applies `description` to `buffer` as one transaction and returns what
/// changed. This is the only function in the kernel allowed to call
/// `Buffer::transaction`.
pub fn apply(
    buffer: &mut Buffer,
    description: EditDescription,
) -> Result<MutationOutcome, BufferError> {
    let mut transaction = buffer.transaction(description.origin);
    if description.join_previous {
        transaction.join_previous();
    }
    for edit in description.edits {
        transaction.push(edit);
    }
    transaction.commit(description.selections)
}

/// Reverts the buffer's last transaction. Undo/redo replay history rather
/// than going through `Buffer::transaction`, but this is still the only
/// function in the kernel allowed to call `Buffer::undo` — every buffer
/// mutation, forward or backward, is grep-able to this one file.
pub fn undo(buffer: &mut Buffer) -> Result<Option<MutationOutcome>, BufferError> {
    buffer.undo()
}

/// Re-applies the last transaction undone by [`undo`].
pub fn redo(buffer: &mut Buffer) -> Result<Option<MutationOutcome>, BufferError> {
    buffer.redo()
}
