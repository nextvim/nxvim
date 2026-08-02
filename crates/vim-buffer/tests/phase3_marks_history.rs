use vim_buffer::{BufferError, BufferManager, ByteOffset, ChangeList, EditOrigin, TextRange};

fn range(start: usize, end: usize) -> TextRange {
    TextRange::new(ByteOffset(start), ByteOffset(end)).unwrap()
}

#[test]
fn local_marks_follow_insertions_and_are_erased_with_their_line() {
    let mut manager = BufferManager::new();
    let buffer = manager.create("one\ntwo\nthree");
    buffer.set_mark('a', ByteOffset(5)).unwrap();

    let mut insert = buffer.transaction(EditOrigin::User);
    insert.insert(None, ByteOffset(0), "zero\n");
    insert.commit(None).unwrap();
    assert_eq!(buffer.resolve_mark('a'), Some(ByteOffset(10)));

    let mut delete_line = buffer.transaction(EditOrigin::User);
    delete_line.delete(None, range(9, 13));
    delete_line.commit(None).unwrap();
    assert_eq!(buffer.resolve_mark('a'), None);

    buffer.undo().unwrap().unwrap();
    assert_eq!(buffer.resolve_mark('a'), Some(ByteOffset(10)));
    buffer.redo().unwrap().unwrap();
    assert_eq!(buffer.resolve_mark('a'), None);
}

#[test]
fn validates_mark_names_and_deletion_rules() {
    let mut manager = BufferManager::new();
    let buffer = manager.create("text");

    assert!(matches!(
        buffer.set_mark('A', ByteOffset(0)),
        Err(BufferError::InvalidMark('A'))
    ));
    buffer.set_mark('a', ByteOffset(0)).unwrap();
    assert!(buffer.delete_mark('a').unwrap());
    assert!(!buffer.delete_mark('a').unwrap());
    assert!(matches!(
        buffer.delete_mark('\''),
        Err(BufferError::InvalidMark('\''))
    ));
}

#[test]
fn transactions_set_last_changed_area_and_change_mark() {
    let mut manager = BufferManager::new();
    let buffer = manager.create("abcdef");
    let mut transaction = buffer.transaction(EditOrigin::User);
    transaction.replace(None, range(2, 4), "XYZ");
    transaction.commit(None).unwrap();

    assert_eq!(buffer.resolve_mark('['), Some(ByteOffset(2)));
    assert_eq!(buffer.resolve_mark(']'), Some(ByteOffset(4)));
    assert_eq!(buffer.resolve_mark('.'), Some(ByteOffset(2)));
}

#[test]
fn join_previous_delegates_undo_grouping_to_zed() {
    let mut manager = BufferManager::new();
    let buffer = manager.create("");

    let mut first = buffer.transaction(EditOrigin::InsertMode);
    first.insert(None, ByteOffset(0), "a");
    let first_outcome = first.commit(None).unwrap();

    let mut joined = buffer.transaction(EditOrigin::InsertMode);
    joined.join_previous();
    joined.insert(None, ByteOffset(1), "b");
    let joined_outcome = joined.commit(None).unwrap();
    assert_eq!(joined_outcome.transaction, first_outcome.transaction);
    assert_eq!(buffer.snapshot().as_inner().text(), "ab");

    buffer.undo().unwrap().unwrap();
    assert_eq!(buffer.snapshot().as_inner().text(), "");
    buffer.redo().unwrap().unwrap();
    assert_eq!(buffer.snapshot().as_inner().text(), "ab");
}

#[test]
fn changelist_coalesces_nearby_changes_and_retains_undone_entries() {
    let mut manager = BufferManager::new();
    let buffer = manager.create("first line\nsecond line\n");

    let mut first = buffer.transaction(EditOrigin::User);
    first.insert(None, ByteOffset(1), "x");
    first.commit(None).unwrap();

    let mut nearby = buffer.transaction(EditOrigin::User);
    nearby.insert(None, ByteOffset(4), "y");
    nearby.commit(None).unwrap();
    assert_eq!(buffer.change_list().entries().len(), 1);

    let second_line = buffer
        .snapshot()
        .point_to_offset(text::Point::new(1, 2))
        .unwrap();
    let mut distant = buffer.transaction(EditOrigin::User);
    distant.insert(None, second_line, "z");
    distant.commit(None).unwrap();
    assert_eq!(buffer.change_list().entries().len(), 2);

    buffer.undo().unwrap().unwrap();
    assert_eq!(buffer.change_list().entries().len(), 2);

    let snapshot = buffer.snapshot();
    let latest = buffer.change_list().entries().last().unwrap();
    assert!(ChangeList::resolve(latest, &snapshot).is_some());
}
