use text::{Selection, SelectionGoal};
use vim_buffer::{Buffer, BufferManager, Point, SelectionExt};

fn selection(buffer: &Buffer, start: usize, end: usize, reversed: bool) -> Selection<text::Anchor> {
    Selection {
        id: 1,
        start: buffer.as_text_buffer().anchor_before(start),
        end: buffer.as_text_buffer().anchor_before(end),
        reversed,
        goal: SelectionGoal::None,
    }
}

#[test]
fn characterwise_payload_respects_utf8_and_endpoints() {
    let mut manager = BufferManager::new();
    let buffer = manager.create("aé🙂z");
    let snapshot = buffer.snapshot();
    let start = snapshot.point_to_offset(Point::new(0, 1)).unwrap().0;
    let end = snapshot.point_to_offset(Point::new(0, 3)).unwrap().0;

    let operation = selection(buffer, start, end, false);
    // Inclusive characterwise
    assert_eq!(
        operation.operation_text(&snapshot, true).unwrap(),
        "é🙂".to_string()
    );

    // Exclusive characterwise
    assert_eq!(
        operation.operation_text(&snapshot, false).unwrap(),
        "é".to_string()
    );
}
