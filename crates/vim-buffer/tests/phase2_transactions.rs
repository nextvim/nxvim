use text::{Selection, SelectionGoal};
use vim_buffer::{
    BufferError, BufferManager, ByteOffset, EditOrigin, SelectionId, SelectionSet, TextRange,
};

fn range(start: usize, end: usize) -> TextRange {
    TextRange::new(ByteOffset(start), ByteOffset(end)).unwrap()
}

#[test]
fn commits_one_sorted_batch_against_the_pre_edit_snapshot() {
    let mut manager = BufferManager::new();
    let buffer = manager.create("abcdef");
    let before = buffer.snapshot();

    let mut transaction = buffer.transaction(EditOrigin::User);
    transaction.insert(Some(SelectionId::new(3)), ByteOffset(6), "!");
    transaction.replace(Some(SelectionId::new(2)), range(2, 4), "XY");
    transaction.insert(Some(SelectionId::new(1)), ByteOffset(0), "[");
    let outcome = transaction.commit(None).unwrap();

    assert_eq!(before.as_inner().text(), "abcdef");
    assert_eq!(buffer.snapshot().as_inner().text(), "[abXYef!");
    assert_eq!(outcome.old_revision, before.revision().clone());
    assert_eq!(outcome.new_revision, buffer.revision());
    assert_ne!(outcome.old_revision, outcome.new_revision);
    assert_eq!(outcome.changedtick.get(), 1);
    assert!(outcome.transaction.is_some());
    assert!(outcome.modified_changed);
    assert_eq!(outcome.edits.len(), 3);
    assert_eq!(outcome.edits[0].new_range, range(0, 1));
    assert_eq!(outcome.edits[1].new_range, range(3, 5));
    assert_eq!(outcome.edits[2].new_range, range(7, 8));
}

#[test]
fn normalizes_duplicate_carets_and_orders_distinct_insertions_by_selection_id() {
    let mut manager = BufferManager::new();
    let buffer = manager.create("ab");
    let mut transaction = buffer.transaction(EditOrigin::User);
    transaction.insert(Some(SelectionId::new(4)), ByteOffset(1), "X");
    transaction.insert(Some(SelectionId::new(2)), ByteOffset(1), "X");
    transaction.insert(Some(SelectionId::new(3)), ByteOffset(1), "B");
    transaction.insert(Some(SelectionId::new(1)), ByteOffset(1), "A");
    let outcome = transaction.commit(None).unwrap();

    assert_eq!(buffer.snapshot().as_inner().text(), "aAXBb");
    assert_eq!(outcome.edits.len(), 3);
    assert_eq!(outcome.changedtick.get(), 1);
}

#[test]
fn maps_returned_anchor_selections_through_the_zed_edit() {
    let mut manager = BufferManager::new();
    let buffer = manager.create("ab");
    let anchor = buffer.as_text_buffer().anchor_before(1);
    let selection = Selection {
        id: 7,
        start: anchor,
        end: anchor,
        reversed: false,
        goal: SelectionGoal::None,
    };
    let selections = SelectionSet::from_selections(SelectionId::new(7), vec![selection]).unwrap();

    let mut transaction = buffer.transaction(EditOrigin::User);
    transaction.insert(None, ByteOffset(0), "X");
    let outcome = transaction.commit(Some(selections)).unwrap();
    let mapped = outcome.selections.unwrap();
    let mapped_anchor = mapped.selections()[0].head();

    assert_eq!(buffer.as_text_buffer().offset_for_anchor(&mapped_anchor), 2);
    assert_eq!(mapped.primary_id(), SelectionId::new(7));
}

#[test]
fn normalizes_inserted_line_endings_before_reporting_geometry() {
    let mut manager = BufferManager::new();
    let buffer = manager.create("");
    let mut transaction = buffer.transaction(EditOrigin::InsertMode);
    transaction.insert(None, ByteOffset(0), "one\r\ntwo\rthree");
    let outcome = transaction.commit(None).unwrap();

    assert_eq!(buffer.snapshot().as_inner().text(), "one\ntwo\nthree");
    assert_eq!(outcome.edits[0].new_extent.bytes, 13);
    assert_eq!(outcome.edits[0].new_extent.lines, 2);
    assert_eq!(outcome.edits[0].new_extent.last_line_bytes, 5);
    assert_eq!(outcome.edits[0].new_range, range(0, 13));
}

#[test]
fn rejects_overlaps_atomically() {
    let mut manager = BufferManager::new();
    let buffer = manager.create("abcdef");
    let revision = buffer.revision();

    let mut transaction = buffer.transaction(EditOrigin::User);
    transaction.delete(None, range(1, 4));
    transaction.insert(None, ByteOffset(2), "X");
    let error = transaction.commit(None).unwrap_err();

    assert!(matches!(error, BufferError::OverlappingEdits));
    assert_eq!(buffer.snapshot().as_inner().text(), "abcdef");
    assert_eq!(buffer.revision(), revision);
    assert_eq!(buffer.changedtick().get(), 0);
}

#[test]
fn rejects_invalid_utf8_boundaries_atomically() {
    let mut manager = BufferManager::new();
    let buffer = manager.create("aéz");
    let mut transaction = buffer.transaction(EditOrigin::User);
    transaction.delete(None, range(2, 3));

    assert!(matches!(
        transaction.commit(None),
        Err(BufferError::NotCharBoundary(2))
    ));
    assert_eq!(buffer.snapshot().as_inner().text(), "aéz");
    assert_eq!(buffer.changedtick().get(), 0);
}

#[test]
fn empty_transactions_are_noops() {
    let mut manager = BufferManager::new();
    let buffer = manager.create("text");
    let revision = buffer.revision();
    let transaction = buffer.transaction(EditOrigin::User);
    let outcome = transaction.commit(None).unwrap();

    assert_eq!(outcome.old_revision, revision);
    assert_eq!(outcome.new_revision, revision);
    assert!(outcome.transaction.is_none());
    assert!(outcome.edits.is_empty());
    assert_eq!(buffer.changedtick().get(), 0);
}
