use text::BufferSnapshot;

#[derive(Clone)]
pub struct TabMap {
    buffer: BufferSnapshot,
}

impl TabMap {
    pub fn new(buffer: BufferSnapshot) -> Self {
        Self { buffer }
    }
}
