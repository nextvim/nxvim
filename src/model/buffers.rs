use std::collections::HashMap;
use std::path::Path;
use vim_buffer::{BufferId, BufferManager as VimBufferManager};

use super::buffer_state::BufferState;

/// Owns editor buffers and their buffer-scoped analysis state.
pub struct Buffers {
    pub(super) inner: VimBufferManager,
    pub(super) states: HashMap<BufferId, BufferState>,
}

impl Buffers {
    /// Creates deterministic buffer storage with one unnamed buffer.
    pub fn new() -> Self {
        let mut inner = VimBufferManager::new();
        let initial_buffer_id = inner.create("").id();
        let _ = inner.set_current(initial_buffer_id);

        Self {
            inner,
            states: HashMap::new(),
        }
    }

    pub fn open_paths<I, P>(&mut self, paths: I) -> BufferId
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        let mut first_opened = None;

        for path in paths {
            let path = path.as_ref();
            let opened = self
                .inner
                .load(path)
                .or_else(|_| self.inner.create_named(path, ""));
            if let Ok((buffer_id, _)) = opened {
                first_opened.get_or_insert(buffer_id);
            }
        }

        if let Some(buffer_id) = first_opened {
            let initial_id = self.inner.current();
            let _ = self.inner.set_current(buffer_id);
            if let Some(initial_id) = initial_id {
                if initial_id != buffer_id {
                    let _ = self.inner.wipe(initial_id, true);
                    self.states.remove(&initial_id);
                }
            }
            buffer_id
        } else {
            self.inner
                .current()
                .expect("Buffers always contains an initial buffer")
        }
    }

    /// Opens or creates a named buffer without changing or removing the
    /// buffer that is currently displayed. The caller decides which window
    /// should display the returned buffer.
    pub fn open_path(&mut self, path: impl AsRef<Path>) -> BufferId {
        let path = path.as_ref();
        self.inner
            .load(path)
            .or_else(|_| self.inner.create_named(path, ""))
            .map(|(buffer_id, _)| buffer_id)
            .expect("opening a buffer path should produce a buffer")
    }

    pub fn current(&self) -> BufferId {
        self.inner
            .current()
            .expect("Buffers always has a current buffer")
    }

    pub fn state(&self, id: BufferId) -> Option<&BufferState> {
        self.states.get(&id)
    }

    pub fn state_mut(&mut self, id: BufferId) -> &mut BufferState {
        self.states
            .entry(id)
            .or_insert_with(|| BufferState::unloaded())
    }

    /// Borrows a buffer and its analysis state together. Exists because the
    /// two live in separate fields on this struct; callers outside this
    /// module can't split the borrow themselves.
    pub fn get_mut_with_state(
        &mut self,
        id: BufferId,
    ) -> Result<(&mut vim_buffer::Buffer, &mut BufferState), vim_buffer::BufferError> {
        let buffer = self.inner.get_mut(id)?;
        let state = self.states.entry(id).or_insert_with(BufferState::unloaded);
        Ok((buffer, state))
    }

    pub fn create(&mut self, initial_text: impl Into<String>) -> BufferId {
        self.inner.create(initial_text).id()
    }

    pub fn create_named(
        &mut self,
        name: impl AsRef<Path>,
        initial_text: impl Into<String>,
    ) -> Result<(BufferId, vim_buffer::ManagerOutcome), vim_buffer::BufferError> {
        self.inner.create_named(name, initial_text)
    }

    pub fn save(
        &mut self,
        id: BufferId,
        path: Option<&Path>,
        force: bool,
    ) -> Result<vim_buffer::SaveOutcome, vim_buffer::BufferError> {
        match path {
            Some(path) => self.inner.write_to(id, path, force),
            None => self.inner.save(id, force),
        }
    }

    pub fn wipe(
        &mut self,
        id: BufferId,
        force: bool,
    ) -> Result<vim_buffer::ManagerOutcome, vim_buffer::BufferError> {
        let result = self.inner.wipe(id, force);
        if result.is_ok() {
            self.states.remove(&id);
        }
        result
    }

    pub fn get(&self, id: BufferId) -> Result<&vim_buffer::Buffer, vim_buffer::BufferError> {
        self.inner.get(id)
    }

    pub fn get_mut(
        &mut self,
        id: BufferId,
    ) -> Result<&mut vim_buffer::Buffer, vim_buffer::BufferError> {
        self.inner.get_mut(id)
    }

    pub fn list(&self) -> Vec<BufferId> {
        self.inner.list()
    }

    pub fn listed(&self) -> Vec<BufferId> {
        self.inner.listed()
    }

    pub fn set_listed(
        &mut self,
        id: BufferId,
        listed: bool,
    ) -> Result<(), vim_buffer::BufferError> {
        self.inner.set_listed(id, listed)
    }
}

impl Default for Buffers {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn construction_is_independent_of_process_arguments() {
        let buffers = Buffers::new();
        assert_eq!(buffers.list().len(), 1);
        assert_eq!(buffers.current(), buffers.list()[0]);
    }

    #[test]
    fn opening_missing_path_creates_named_buffer_and_replaces_initial_buffer() {
        let mut buffers = Buffers::new();
        let path =
            std::env::temp_dir().join(format!("nxvim-phase-2-missing-{}", std::process::id()));
        let opened = buffers.open_paths([&path]);

        assert_eq!(buffers.current(), opened);
        assert_eq!(buffers.list().len(), 1);
        assert_eq!(buffers.get(opened).unwrap().path(), Some(path.as_path()));
    }

    #[test]
    fn opening_a_path_for_edit_keeps_the_current_buffer() {
        let mut buffers = Buffers::new();
        let original = buffers.current();
        let path = std::env::temp_dir().join(format!(
            "nxvim-edit-buffer-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));

        let opened = buffers.open_path(&path);

        assert_ne!(opened, original);
        assert_eq!(buffers.current(), original);
        assert_eq!(buffers.list().len(), 2);
        assert_eq!(buffers.get(original).unwrap().path(), None);
        assert_eq!(buffers.get(opened).unwrap().path(), Some(path.as_path()));
    }
}
