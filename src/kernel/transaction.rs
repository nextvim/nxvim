//! Kernel-owned entry point for buffer mutations.
//!
//! The underlying transaction and history implementation remains in
//! `vim-buffer`; this module supplies the semantic boundary used by commands.

use super::MutationOutcome;
use vim_buffer::{Buffer, EditOrigin, SelectionSet};

/// Execute one mutation transaction against a buffer.
///
/// The callback may queue multiple replacements/inserts/deletes. They commit
/// as one undo unit and produce one typed outcome. The callback cannot retain
/// the transaction or buffer borrow beyond the call.
pub(crate) fn transaction(
    buffer: &mut Buffer,
    origin: EditOrigin,
    selections: Option<SelectionSet>,
    edit: impl FnOnce(&mut vim_buffer::Transaction<'_>),
) -> Result<MutationOutcome, String> {
    let mut transaction = buffer.transaction(origin);
    edit(&mut transaction);
    transaction
        .commit(selections)
        .map(MutationOutcome::from_buffer)
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_multiple_edits_and_reports_tick_and_ranges() {
        let mut buffer = Buffer::new(
            vim_buffer::BufferId::new(1).unwrap(),
            clock::ReplicaId::LOCAL,
            "abc",
        );
        let outcome = transaction(&mut buffer, EditOrigin::VimScript, None, |transaction| {
            transaction.replace(
                None,
                vim_buffer::TextRange::new(vim_buffer::ByteOffset(0), vim_buffer::ByteOffset(1))
                    .unwrap(),
                "x",
            );
            transaction.replace(
                None,
                vim_buffer::TextRange::new(vim_buffer::ByteOffset(2), vim_buffer::ByteOffset(3))
                    .unwrap(),
                "z",
            );
        })
        .unwrap();

        assert_eq!(buffer.as_text_buffer().as_rope().to_string(), "xbz");
        assert_eq!(outcome.buffer, buffer.id());
        assert_eq!(outcome.changed_tick, buffer.changedtick());
        assert_eq!(outcome.changed_ranges.len(), 2);
        assert!(outcome.transaction.is_some());
    }
}
