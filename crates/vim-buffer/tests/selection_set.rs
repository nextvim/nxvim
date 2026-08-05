use vim_buffer::{BufferManager, SelectionCellState, SelectionSet};

const UNSELECTED: SelectionCellState = SelectionCellState {
    selected_cell: false,
    selected_line: false,
    at_cursor_head: false,
    at_primary_cursor_head: false,
};

const PRIMARY_CURSOR: SelectionCellState = SelectionCellState {
    selected_cell: false,
    selected_line: false,
    at_cursor_head: true,
    at_primary_cursor_head: true,
};

const SELECTED: SelectionCellState = SelectionCellState {
    selected_cell: true,
    selected_line: true,
    ..UNSELECTED
};

const SELECTED_PRIMARY_CURSOR: SelectionCellState = SelectionCellState {
    selected_cell: true,
    selected_line: true,
    ..PRIMARY_CURSOR
};

#[test]
fn collapsed_cursor_is_not_selected_text() {
    let mut manager = BufferManager::new();
    let buffer = manager.create("abc");
    let text_buffer = buffer.as_text_buffer();
    let mut selections = SelectionSet::new();
    selections.add(text_buffer, 1);

    assert_eq!(selections.is_selected(0, 0, text_buffer), UNSELECTED);
    assert_eq!(selections.is_selected(0, 1, text_buffer), PRIMARY_CURSOR);
    assert_eq!(selections.is_selected(0, 2, text_buffer), UNSELECTED);
}

#[test]
fn distinguishes_primary_from_secondary_cursor_heads() {
    let mut manager = BufferManager::new();
    let buffer = manager.create("abc");
    let text_buffer = buffer.as_text_buffer();
    let mut selections = SelectionSet::new();
    selections.add(text_buffer, 0);
    selections.add(text_buffer, 2);

    assert_eq!(selections.is_selected(0, 0, text_buffer), PRIMARY_CURSOR);
    assert_eq!(
        selections.is_selected(0, 2, text_buffer),
        SelectionCellState {
            at_cursor_head: true,
            ..UNSELECTED
        }
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

    assert_eq!(selections.is_selected(0, 0, text_buffer), UNSELECTED);
    assert_eq!(selections.is_selected(0, 1, text_buffer), UNSELECTED);
    assert_eq!(selections.is_selected(0, 2, text_buffer), PRIMARY_CURSOR);
}

#[test]
fn non_empty_selection_reports_selected_cells_and_cursor_head() {
    let mut manager = BufferManager::new();
    let buffer = manager.create("abc");
    let text_buffer = buffer.as_text_buffer();
    let mut selections = SelectionSet::new();
    selections.add(text_buffer, 0);
    selections.selections[0].end = text_buffer.anchor_before(2);

    assert_eq!(selections.is_selected(0, 0, text_buffer), SELECTED);
    assert_eq!(selections.is_selected(0, 1, text_buffer), SELECTED);
    assert_eq!(
        selections.is_selected(0, 2, text_buffer),
        SELECTED_PRIMARY_CURSOR
    );
}
