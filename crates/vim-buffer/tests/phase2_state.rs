use text::{Selection, SelectionGoal};
use vim_buffer::{
    BufferError, BufferManager, BufferOptions, ByteOffset, EditOrigin, FileFormat, SelectionExt,
    TextRange,
};

fn range(start: usize, end: usize) -> TextRange {
    TextRange::new(ByteOffset(start), ByteOffset(end)).unwrap()
}

fn selection(
    buffer: &vim_buffer::Buffer,
    id: usize,
    start: usize,
    end: usize,
) -> Selection<text::Anchor> {
    Selection {
        id,
        start: buffer.as_text_buffer().anchor_before(start),
        end: buffer.as_text_buffer().anchor_before(end),
        reversed: false,
        goal: SelectionGoal::None,
    }
}

#[test]
fn undo_and_redo_delegate_to_zed_and_update_vim_state() {
    let mut manager = BufferManager::new();
    let buffer = manager.create("abc");
    let mut transaction = buffer.transaction(EditOrigin::User);
    transaction.insert(None, ByteOffset(3), "d");
    transaction.commit(None).unwrap();
    assert!(buffer.is_modified());

    let undo = buffer.undo().unwrap().unwrap();
    assert_eq!(undo.origin, EditOrigin::Undo);
    assert_eq!(buffer.snapshot().as_inner().text(), "abc");
    assert_eq!(buffer.changedtick().get(), 2);
    assert!(!buffer.is_modified());
    assert!(!undo.edits.is_empty());

    let redo = buffer.redo().unwrap().unwrap();
    assert_eq!(redo.origin, EditOrigin::Redo);
    assert_eq!(buffer.snapshot().as_inner().text(), "abcd");
    assert_eq!(buffer.changedtick().get(), 3);
    assert!(buffer.is_modified());
    assert!(!redo.edits.is_empty());
}

#[test]
fn save_points_and_checked_options_control_modified_state() {
    let mut manager = BufferManager::new();
    let buffer = manager.create("abc");
    let mut transaction = buffer.transaction(EditOrigin::User);
    transaction.insert(None, ByteOffset(3), "d");
    transaction.commit(None).unwrap();
    buffer.mark_saved();
    assert!(!buffer.is_modified());

    let mut options = buffer.options().clone();
    options.readonly = true;
    let outcome = buffer.set_options(options).unwrap().unwrap();
    assert!(!outcome.modified_changed);
    assert!(!buffer.is_modified());
    assert_eq!(buffer.changedtick().get(), 1);

    let mut options = buffer.options().clone();
    options.modifiable = false;
    buffer.set_options(options).unwrap();
    let mut rejected = buffer.transaction(EditOrigin::User);
    rejected.insert(None, ByteOffset(0), "x");
    assert!(matches!(
        rejected.commit(None),
        Err(BufferError::Unmodifiable(_))
    ));

    let mut options = buffer.options().clone();
    options.modifiable = true;
    options.fileformat = FileFormat::Dos;
    let outcome = buffer.set_options(options).unwrap().unwrap();
    assert!(outcome.modified_changed);
    assert!(buffer.is_modified());
    assert_eq!(buffer.snapshot().line_ending(), text::LineEnding::Windows);

    let mut invalid = BufferOptions::default();
    invalid.fileencoding = "latin1".into();
    assert!(matches!(
        buffer.set_options(invalid),
        Err(BufferError::UnsupportedEncoding(_))
    ));
}

#[test]
fn vim_selections_resolve_to_character_ranges() {
    let mut manager = BufferManager::new();
    let buffer = manager.create("aéz");
    let snapshot = buffer.snapshot();
    let character = selection(buffer, 1, 1, 1);
    assert_eq!(
        character.edit_ranges(&snapshot, true).unwrap(),
        vec![range(1, 3)]
    );
    assert_eq!(
        character.edit_ranges(&snapshot, false).unwrap(),
        vec![range(1, 1)]
    );
}
