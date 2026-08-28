//! Buffer storage for the kernel.
//!
//! Per `RESCUE.md` Rule 4.1, a buffer is UI-agnostic: it must be fully
//! queryable and editable with zero windows attached. `BufferStore` owns
//! that state and nothing else — no cursor, no selection, no window
//! reference. `vim_buffer::BufferManager` already implements buffer
//! lifecycle (id allocation, load/save) correctly; `BufferStore` is the
//! kernel's narrow, in-memory-only slice of that surface for this
//! milestone (no file I/O yet).

use vim_buffer::{Buffer, BufferId, BufferManager};

pub struct BufferStore {
    manager: BufferManager,
}

impl BufferStore {
    pub fn new() -> Self {
        Self {
            manager: BufferManager::new(),
        }
    }

    /// Creates a new in-memory buffer seeded with `initial_text` and returns
    /// its id.
    pub fn insert(&mut self, initial_text: impl Into<String>) -> BufferId {
        self.manager.create(initial_text).id()
    }

    pub fn get(&self, id: BufferId) -> Option<&Buffer> {
        self.manager.get(id).ok()
    }

    pub fn get_mut(&mut self, id: BufferId) -> Option<&mut Buffer> {
        self.manager.get_mut(id).ok()
    }
}
