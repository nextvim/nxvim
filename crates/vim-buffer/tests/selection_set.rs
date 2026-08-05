use vim_buffer::{BufferManager, SelectionSet};

#[test]
fn collapsed_cursor_is_not_selected_text() {
    let mut manager = BufferManager::new();
    let buffer = manager.create("abc");
    let text_buffer = buffer.as_text_buffer();
    let mut selections = SelectionSet::new();
    selections.add(text_buffer, 1);

    assert_eq!(
        selections.is_selected(0, 0, text_buffer),
        (false, false, false)
    );
    assert_eq!(
        selections.is_selected(0, 1, text_buffer),
        (false, false, true)
    );
    assert_eq!(
        selections.is_selected(0, 2, text_buffer),
        (false, false, false)
    );
}

#[test]
fn non_selecting_motion_does_not_select_from_the_initial_cursor() {
    let mut manager = BufferManager::new();
    let buffer = manager.create("abc");
    let text_buffer = buffer.as_text_buffer();
    let mut selections = SelectionSet::new();
    selections.add(text_buffer, 0);
    selections.move_right(false, 2, text_buffer);

    assert_eq!(
        selections.is_selected(0, 0, text_buffer),
        (false, false, false)
    );
    assert_eq!(
        selections.is_selected(0, 1, text_buffer),
        (false, false, false)
    );
    assert_eq!(
        selections.is_selected(0, 2, text_buffer),
        (false, false, true)
    );
}

#[test]
fn non_empty_selection_reports_selected_cells_and_cursor_head() {
    let mut manager = BufferManager::new();
    let buffer = manager.create("abc");
    let text_buffer = buffer.as_text_buffer();
    let mut selections = SelectionSet::new();
    selections.add(text_buffer, 0);
    selections.selections[0].end = text_buffer.anchor_before(2);

    assert_eq!(
        selections.is_selected(0, 0, text_buffer),
        (true, true, false)
    );
    assert_eq!(
        selections.is_selected(0, 1, text_buffer),
        (true, true, false)
    );
    assert_eq!(
        selections.is_selected(0, 2, text_buffer),
        (true, true, true)
    );
}
