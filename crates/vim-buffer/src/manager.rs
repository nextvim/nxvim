use crate::{Buffer, BufferError, BufferId};
use clock::ReplicaId;
use std::collections::HashMap;

#[derive(Default)]
pub struct BufferManager {
    buffers: HashMap<BufferId, Buffer>,
    next_id: u64,
    current: Option<BufferId>,
    alternate: Option<BufferId>,
    mru: Vec<BufferId>,
}

impl BufferManager {
    pub fn new() -> Self {
        Self {
            next_id: 1,
            ..Self::default()
        }
    }

    pub fn create(&mut self, initial_text: impl Into<String>) -> &mut Buffer {
        let id = BufferId::new(self.next_id).expect("buffer ID allocator overflowed");
        self.next_id = self
            .next_id
            .checked_add(1)
            .expect("buffer ID allocator overflowed");
        self.buffers
            .entry(id)
            .or_insert_with(|| Buffer::new(id, ReplicaId::LOCAL, initial_text))
    }

    pub fn get(&self, id: BufferId) -> Result<&Buffer, BufferError> {
        self.buffers.get(&id).ok_or(BufferError::UnknownBuffer(id))
    }

    pub fn get_mut(&mut self, id: BufferId) -> Result<&mut Buffer, BufferError> {
        self.buffers
            .get_mut(&id)
            .ok_or(BufferError::UnknownBuffer(id))
    }

    pub fn current(&self) -> Option<BufferId> {
        self.current
    }

    pub fn alternate(&self) -> Option<BufferId> {
        self.alternate
    }

    pub fn mru(&self) -> &[BufferId] {
        &self.mru
    }
}
