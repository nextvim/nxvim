use vim_buffer::{BufferManager, BufferSnapshot, TEXT_BACKEND};

fn assert_send_sync<T: Send + Sync>() {}

#[test]
fn manager_creates_editor_agnostic_buffers_with_snapshots() {
    let mut manager = BufferManager::new();
    let buffer = manager.create("alpha\nbeta");
    let id = buffer.id();
    let revision = buffer.revision();
    let snapshot = buffer.snapshot();

    assert_eq!(id.get(), 1);
    assert_eq!(snapshot.id(), id);
    assert_eq!(snapshot.revision(), &revision);
    assert_eq!(snapshot.as_inner().text(), "alpha\nbeta");
    assert_eq!(TEXT_BACKEND, "Zed text::Buffer (Rope + SumTree)");
}

#[test]
fn wrapped_snapshots_can_cross_service_threads() {
    assert_send_sync::<BufferSnapshot>();
}
