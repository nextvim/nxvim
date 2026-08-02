use text::{Selection, SelectionGoal};
use vim_buffer::{Buffer, BufferManager, OperationText, Point, SelectionKind, VimSelection};

fn selection(
    buffer: &Buffer,
    start: usize,
    end: usize,
    reversed: bool,
    kind: SelectionKind,
    inclusive: bool,
) -> VimSelection {
    VimSelection::new(
        Selection {
            id: 1,
            start: buffer.as_text_buffer().anchor_before(start),
            end: buffer.as_text_buffer().anchor_before(end),
            reversed,
            goal: SelectionGoal::None,
        },
        kind,
        inclusive,
    )
}

#[test]
fn characterwise_payload_respects_utf8_and_inclusive_endpoints() {
    let mut manager = BufferManager::new();
    let buffer = manager.create("aé🙂z");
    let snapshot = buffer.snapshot();
    let start = snapshot.point_to_offset(Point::new(0, 1)).unwrap().0;
    let end = snapshot.point_to_offset(Point::new(0, 3)).unwrap().0;

    let operation = selection(
        buffer,
        start,
        end,
        false,
        SelectionKind::Characterwise,
        true,
    );
    assert_eq!(
        operation.operation_text(&snapshot).unwrap(),
        OperationText::Characterwise("é🙂".into())
    );
}

#[test]
fn linewise_payload_adds_vim_register_newline_at_eof() {
    let mut manager = BufferManager::new();
    let buffer = manager.create("first\nlast");
    let snapshot = buffer.snapshot();
    let start = snapshot.point_to_offset(Point::new(1, 0)).unwrap().0;
    let end = snapshot.point_to_offset(Point::new(1, 4)).unwrap().0;

    let operation = selection(buffer, start, end, false, SelectionKind::Linewise, true);
    assert_eq!(
        operation.operation_text(&snapshot).unwrap(),
        OperationText::Linewise("last\n".into())
    );
}

#[test]
fn blockwise_payload_keeps_one_fragment_per_row_in_reverse_direction() {
    let mut manager = BufferManager::new();
    let buffer = manager.create("abcd\nx\n1234");
    let snapshot = buffer.snapshot();
    let bottom_left = snapshot.point_to_offset(Point::new(2, 1)).unwrap().0;
    let top_right = snapshot.point_to_offset(Point::new(0, 3)).unwrap().0;

    let operation = selection(
        buffer,
        bottom_left,
        top_right,
        true,
        SelectionKind::Blockwise,
        true,
    );
    assert_eq!(
        operation.operation_text(&snapshot).unwrap(),
        OperationText::Blockwise(vec!["bcd".into(), String::new(), "234".into()])
    );
}
