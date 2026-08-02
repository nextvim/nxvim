use vim_buffer::{BufferError, BufferManager, ByteOffset, EditOrigin, TextRange};

fn text(manager: &BufferManager, id: vim_buffer::BufferId) -> String {
    manager.get(id).unwrap().snapshot().chunks().collect()
}

#[test]
fn formatter_replaces_complete_contents_without_replacing_buffer_identity() {
    let mut manager = BufferManager::new();
    let id = manager.create("fn  main( ){\r\n}").id();
    let before = manager.get(id).unwrap().revision();

    let outcome = manager
        .replace_all(id, EditOrigin::Formatter, "fn main() {\n}\n")
        .unwrap();

    assert_eq!(outcome.buffer, id);
    assert_eq!(outcome.old_revision, before);
    assert_ne!(outcome.new_revision, before);
    assert_eq!(outcome.origin, EditOrigin::Formatter);
    assert_eq!(outcome.edits.len(), 1);
    assert_eq!(text(&manager, id), "fn main() {\n}\n");
    assert_eq!(manager.get(id).unwrap().id(), id);

    manager.get_mut(id).unwrap().undo().unwrap().unwrap();
    assert_eq!(text(&manager, id), "fn  main( ){\n}");
}

#[test]
fn formatter_can_commit_multiple_replacements_as_one_transaction() {
    let mut manager = BufferManager::new();
    let id = manager.create("let  a=1;\nlet  b=2;").id();

    let mut transaction = manager.transaction(id, EditOrigin::Formatter).unwrap();
    transaction.replace(
        None,
        TextRange::new(ByteOffset(3), ByteOffset(5)).unwrap(),
        " ",
    );
    transaction.replace(
        None,
        TextRange::new(ByteOffset(13), ByteOffset(15)).unwrap(),
        " ",
    );
    let outcome = transaction.commit(None).unwrap();

    assert_eq!(outcome.edits.len(), 2);
    assert_eq!(text(&manager, id), "let a=1;\nlet b=2;");

    manager.get_mut(id).unwrap().undo().unwrap().unwrap();
    assert_eq!(text(&manager, id), "let  a=1;\nlet  b=2;");
}

#[test]
fn checked_replacement_rejects_invalid_ranges_without_mutating() {
    let mut manager = BufferManager::new();
    let id = manager.create("aéz").id();
    let before = manager.get(id).unwrap().revision();

    let result = manager.replace(
        id,
        EditOrigin::Formatter,
        TextRange::new(ByteOffset(2), ByteOffset(3)).unwrap(),
        "e",
    );

    assert!(matches!(result, Err(BufferError::NotCharBoundary(2))));
    assert_eq!(manager.get(id).unwrap().revision(), before);
    assert_eq!(text(&manager, id), "aéz");
}

#[test]
fn replacement_honors_buffer_lifecycle_and_modifiable_option() {
    let mut manager = BufferManager::new();
    let id = manager.create("text").id();
    let mut options = manager.get(id).unwrap().options().clone();
    options.modifiable = false;
    manager.get_mut(id).unwrap().set_options(options).unwrap();

    assert!(matches!(
        manager.replace_all(id, EditOrigin::Formatter, "formatted"),
        Err(BufferError::Unmodifiable(found)) if found == id
    ));
    assert_eq!(text(&manager, id), "text");
}
