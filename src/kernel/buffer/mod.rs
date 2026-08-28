//! Buffer storage for the kernel.
//!
//! Per `RESCUE.md` Rule 4.1, a buffer is UI-agnostic: it must be fully
//! queryable and editable with zero windows attached. `BufferStore` owns
//! that state and nothing else — no cursor, no selection, no window
//! reference. `vim_buffer::BufferManager` already implements buffer
//! lifecycle (id allocation, load/save) correctly; `BufferStore` is the
//! kernel's narrow slice of that surface: in-memory buffer creation
//! (`insert`), loading from disk (`load`/`create_named`, used by
//! `kernel::Editor::open` for command-line file arguments), and saving
//! (`save`/`write_to`, used by the `:w` Ex command).

use vim_buffer::{Buffer, BufferError, BufferId, BufferManager, ManagerOutcome, SaveOutcome};

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

    /// Loads `path` from disk into a new buffer (or returns the existing
    /// buffer already backing that path), matching Vim's own `:e path`.
    pub fn load(
        &mut self,
        path: impl AsRef<std::path::Path>,
    ) -> Result<(BufferId, ManagerOutcome), BufferError> {
        self.manager.load(path)
    }

    /// Creates (or returns the existing) buffer named `path` without
    /// reading it from disk — used when `load` fails (e.g. the file
    /// doesn't exist yet), matching Vim's "edit a new file" behavior of
    /// opening an empty buffer bound to that name.
    pub fn create_named(
        &mut self,
        path: impl AsRef<std::path::Path>,
        initial_text: impl Into<String>,
    ) -> Result<(BufferId, ManagerOutcome), BufferError> {
        self.manager.create_named(path, initial_text)
    }

    pub fn get(&self, id: BufferId) -> Option<&Buffer> {
        self.manager.get(id).ok()
    }

    pub fn get_mut(&mut self, id: BufferId) -> Option<&mut Buffer> {
        self.manager.get_mut(id).ok()
    }

    pub fn save(&mut self, id: BufferId, force: bool) -> Result<SaveOutcome, BufferError> {
        self.manager.save(id, force)
    }

    pub fn write_to(
        &mut self,
        id: BufferId,
        path: impl AsRef<std::path::Path>,
        force: bool,
    ) -> Result<SaveOutcome, BufferError> {
        self.manager.write_to(id, path, force)
    }
}
