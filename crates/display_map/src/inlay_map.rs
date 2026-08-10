use text::BufferSnapshot;

#[derive(Clone)]
pub struct InlayMap {
    buffer: BufferSnapshot,
}

impl InlayMap {
    pub fn new(buffer: BufferSnapshot) -> Self {
        Self { buffer }
    }
}
