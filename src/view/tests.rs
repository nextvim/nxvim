use super::*;
use crate::app::view_sync::WindowProjection;
use crate::kernel::ids::WindowId;
use text::ReplicaId;
use vim_buffer::{Buffer, BufferId, SelectionId};

#[test]
fn test_view_model_validation_and_caching() {
    let mut render_state = RenderState::new();

    let buf_id = BufferId::new(1).unwrap();
    let buffer = Buffer::new(buf_id, ReplicaId::LOCAL, "line 1\nline 2\nline 3\n");
    let snapshot = buffer.snapshot();

    // Construct valid SelectionSet using Buffer's helper
    let anchor = buffer.as_text_buffer().anchor_before(0);
    let initial = text::Selection {
        id: 0,
        start: anchor,
        end: anchor,
        reversed: false,
        goal: text::SelectionGoal::None,
    };
    let selections =
        vim_buffer::SelectionSet::from_selections(SelectionId::new(0), vec![initial]).unwrap();

    let win_id = WindowId::new(1);
    let projection = WindowProjection {
        window: win_id,
        buffer: buf_id,
        snapshot: snapshot.into_inner(), // Get the inner text::BufferSnapshot
        selections: selections.clone(),
        is_current: true,
        scroll_top: 0,
    };

    // Lazy cache creation
    let cache = render_state
        .windows
        .entry(win_id)
        .or_insert_with(|| WindowRenderCache {
            display_map: DisplayMap::new_windowed(
                projection.snapshot.clone(),
                None,
                0..projection.snapshot.row_count(),
            ),
            buffer: projection.buffer,
            retained: HashMap::new(),
        });

    assert_eq!(cache.buffer.get(), 1);
    assert_eq!(cache.display_map.snapshot().row_count(), 4);

    // Swap buffers to test retention
    let new_buf_id = BufferId::new(2).unwrap();
    let new_buffer = Buffer::new(new_buf_id, ReplicaId::LOCAL, "another buffer content");

    let new_projection = WindowProjection {
        window: win_id,
        buffer: new_buf_id,
        snapshot: new_buffer.snapshot().into_inner(),
        selections: selections.clone(),
        is_current: true,
        scroll_top: 0,
    };

    // Perform swapping logic
    if cache.buffer != new_projection.buffer {
        let old_map = std::mem::replace(
            &mut cache.display_map,
            DisplayMap::new_windowed(
                new_projection.snapshot.clone(),
                None,
                0..new_projection.snapshot.row_count(),
            ),
        );
        cache.retained.insert(cache.buffer, old_map);
        cache.buffer = new_projection.buffer;
    }

    assert_eq!(cache.buffer.get(), 2);
    assert!(cache.retained.contains_key(&BufferId::new(1).unwrap()));
}
