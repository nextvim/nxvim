use text::BufferSnapshot;

#[derive(Clone)]
pub struct BlockMap {
    buffer: BufferSnapshot,
}

impl BlockMap {
    pub fn new(buffer: BufferSnapshot) -> Self {
        Self { buffer }
    }
}
