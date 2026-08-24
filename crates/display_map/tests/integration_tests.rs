use display_map::{Bias, DisplayMap, Fold};
use text::{Point, ToPoint};
use vim_buffer::{BufferManager, ByteOffset, EditOrigin};

#[test]
fn test_display_map_and_vim_buffer_integration() {
    // 1. Create a Vim Buffer Manager and initialize a buffer.
    let mut manager = BufferManager::new();
    let buffer = manager.create("Hello World!\nThis is an integration test for nextvim.\nIt verifies display_map and vim-buffer together.");

    // 2. Perform a Vim transaction (mutation).
    let mut transaction = buffer.transaction(EditOrigin::User);
    transaction.insert(None, ByteOffset(12), "\n[Vim Edit Insertion]");
    transaction
        .commit(None)
        .expect("Failed to commit transaction");

    // 3. Get the inner text snapshot and create the DisplayMap.
    let snapshot = buffer.snapshot();
    let text_snapshot = snapshot.as_inner().clone();

    // Create a DisplayMap with soft-wrapping at 25 characters.
    let mut display_map = DisplayMap::new(text_snapshot, Some(25));

    let snap = display_map.snapshot();
    assert_eq!(snap.line_text(0), "Hello World!");
    assert_eq!(snap.line_text(1), "[Vim Edit Insertion]");

    // 4. Add folds to the display map.
    let folds = vec![Fold {
        start: Point::new(2, 0),
        end: Point::new(3, 0),
    }];
    display_map.fold(folds, buffer.snapshot().as_inner().clone());

    let final_snap = display_map.snapshot();

    // Verify coordinate mappings with Points.
    let buffer_cursor = Point::new(3, 12); // Character in the 4th row (index 3)
    let display_cursor = final_snap.point_to_display_point(buffer_cursor);
    let mapped_back = final_snap.display_point_to_point(display_cursor);
    assert_eq!(buffer_cursor, mapped_back);

    // Verify coordinate mappings with stable Anchors.
    let anchor = snapshot.as_inner().anchor_before(buffer_cursor);
    let display_point_from_anchor = final_snap.anchor_to_display_point(anchor);
    assert_eq!(display_cursor, display_point_from_anchor);

    let anchor_back = final_snap.display_point_to_anchor(display_point_from_anchor, Bias::Left);
    let mapped_back_from_anchor = anchor_back.to_point(snapshot.as_inner());
    assert_eq!(buffer_cursor, mapped_back_from_anchor);
}
