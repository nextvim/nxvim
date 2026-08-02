use clock::ReplicaId;
use text::{Buffer, BufferId, BufferSnapshot};

fn assert_send_sync<T: Send + Sync>() {}

#[test]
fn snapshots_can_cross_background_service_boundaries() {
    assert_send_sync::<BufferSnapshot>();
}

#[test]
fn applies_multi_cursor_insertions_as_one_batch_against_a_snapshot() {
    let mut buffer = Buffer::new(
        ReplicaId::LOCAL,
        BufferId::new(1).expect("buffer ID must be non-zero"),
        "alpha beta",
    );
    let before = buffer.snapshot().clone();

    // Both offsets refer to `before`, as multi-cursor edits must. The text
    // buffer applies the replacements together instead of letting the first
    // insertion shift the second cursor's offset.
    buffer.edit([(0..0, "["), (6..6, "]")]);

    assert_eq!(buffer.text(), "[alpha ]beta");
    assert_eq!(before.text(), "alpha beta");
    assert_eq!(buffer.snapshot().text(), "[alpha ]beta");
}
