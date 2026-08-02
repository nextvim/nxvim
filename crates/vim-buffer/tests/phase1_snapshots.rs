use vim_buffer::{
    BufferError, BufferManager, ByteOffset, FileFormat, Point, PointUtf16, TextRange,
};

fn snapshot_for(text: &str) -> (BufferManager, vim_buffer::BufferId) {
    let mut manager = BufferManager::new();
    let id = manager.create(text).id();
    (manager, id)
}

#[test]
fn delegates_unicode_metrics_and_checked_coordinate_conversions() {
    let (manager, id) = snapshot_for("aé😀\r\nβ");
    let buffer = manager.get(id).unwrap();
    let snapshot = buffer.snapshot();

    assert_eq!(snapshot.len_bytes(), 10);
    assert_eq!(snapshot.len_chars(), 5);
    assert_eq!(snapshot.len_utf16().0, 6);
    assert_eq!(snapshot.row_count(), 2);
    assert_eq!(snapshot.line_len(0).unwrap(), 7);
    assert_eq!(snapshot.line_len(1).unwrap(), 2);
    assert_eq!(buffer.options().fileformat, FileFormat::Dos);

    assert_eq!(
        snapshot.offset_to_point(ByteOffset(3)).unwrap(),
        Point::new(0, 3)
    );
    assert_eq!(
        snapshot.point_to_offset(Point::new(0, 7)).unwrap(),
        ByteOffset(7)
    );
    assert_eq!(
        snapshot.offset_to_point_utf16(ByteOffset(7)).unwrap(),
        PointUtf16::new(0, 4)
    );
    assert_eq!(
        snapshot
            .point_utf16_to_offset(PointUtf16::new(0, 4))
            .unwrap(),
        ByteOffset(7)
    );
}

#[test]
fn rejects_out_of_bounds_and_non_character_boundaries_before_delegating() {
    let (manager, id) = snapshot_for("aé😀");
    let snapshot = manager.get(id).unwrap().snapshot();

    assert!(matches!(
        snapshot.offset_to_point(ByteOffset(2)),
        Err(BufferError::NotCharBoundary(2))
    ));
    assert!(matches!(
        snapshot.offset_to_point(ByteOffset(8)),
        Err(BufferError::OffsetOutOfBounds(8))
    ));
    assert!(matches!(
        snapshot.point_to_offset(Point::new(0, 2)),
        Err(BufferError::InvalidPoint(_))
    ));
    assert!(matches!(
        snapshot.line_len(1),
        Err(BufferError::InvalidPoint(_))
    ));
}

#[test]
fn validates_ranges_and_streams_chunks_without_flattening() {
    let (manager, id) = snapshot_for("alpha βeta");
    let snapshot = manager.get(id).unwrap().snapshot();
    let range = TextRange::new(ByteOffset(6), ByteOffset(11)).unwrap();

    let selected = snapshot.text_for_range(range).unwrap().collect::<String>();
    assert_eq!(selected, "βeta");

    let invalid = TextRange {
        start: ByteOffset(7),
        end: ByteOffset(11),
    };
    assert!(matches!(
        snapshot.text_for_range(invalid),
        Err(BufferError::NotCharBoundary(7))
    ));
}
