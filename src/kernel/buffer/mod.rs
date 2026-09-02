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

pub mod registers;

use std::collections::HashMap;

use textmate::BufferHighlightState;
use vim_buffer::{Buffer, BufferError, BufferId, BufferManager, ManagerOutcome, SaveOutcome};

/// Analysis state with the same lifetime and identity as one buffer.
pub struct BufferAnalysis {
    highlights: BufferHighlightState,
}

impl BufferAnalysis {
    fn new() -> Self {
        Self {
            highlights: BufferHighlightState::new(),
        }
    }

    pub fn highlights(&self) -> &BufferHighlightState {
        &self.highlights
    }

    pub fn highlights_mut(&mut self) -> &mut BufferHighlightState {
        &mut self.highlights
    }

    /// Clears all syntax-derived state after non-text inputs such as the
    /// buffer path, filetype, syntax selection, or theme change.
    pub fn invalidate_highlights(&mut self) {
        self.highlights.invalidate();
    }
}

pub struct BufferStore {
    manager: BufferManager,
    analysis: HashMap<BufferId, BufferAnalysis>,
}

impl BufferStore {
    pub fn new() -> Self {
        Self {
            manager: BufferManager::new(),
            analysis: HashMap::new(),
        }
    }

    /// Creates a new in-memory buffer seeded with `initial_text` and returns
    /// its id.
    pub fn insert(&mut self, initial_text: impl Into<String>) -> BufferId {
        let id = self.manager.create(initial_text).id();
        self.analysis.insert(id, BufferAnalysis::new());
        id
    }

    /// Loads `path` from disk into a new buffer (or returns the existing
    /// buffer already backing that path), matching Vim's own `:e path`.
    pub fn load(
        &mut self,
        path: impl AsRef<std::path::Path>,
    ) -> Result<(BufferId, ManagerOutcome), BufferError> {
        let (id, outcome) = self.manager.load(path)?;
        self.analysis.entry(id).or_insert_with(BufferAnalysis::new);
        Ok((id, outcome))
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
        let (id, outcome) = self.manager.create_named(path, initial_text)?;
        self.analysis.entry(id).or_insert_with(BufferAnalysis::new);
        Ok((id, outcome))
    }

    pub fn get(&self, id: BufferId) -> Option<&Buffer> {
        self.manager.get(id).ok()
    }

    pub fn get_mut(&mut self, id: BufferId) -> Option<&mut Buffer> {
        self.manager.get_mut(id).ok()
    }

    pub fn analysis(&self, id: BufferId) -> Option<&BufferAnalysis> {
        self.analysis.get(&id)
    }

    pub fn analysis_mut(&mut self, id: BufferId) -> Option<&mut BufferAnalysis> {
        self.analysis.get_mut(&id)
    }

    pub fn invalidate_all_highlights(&mut self) {
        for analysis in self.analysis.values_mut() {
            analysis.invalidate_highlights();
        }
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
        let old_path = self
            .manager
            .get(id)?
            .path()
            .map(std::path::Path::to_path_buf);
        let outcome = self.manager.write_to(id, path, force)?;
        let path_changed = self.manager.get(id)?.path() != old_path.as_deref();
        if path_changed {
            self.analysis
                .get_mut(&id)
                .expect("every live buffer has analysis state")
                .invalidate_highlights();
        }
        Ok(outcome)
    }

    pub fn list(&self) -> Vec<BufferId> {
        self.manager.list()
    }

    pub fn set_current(&mut self, id: BufferId) -> Result<vim_buffer::ManagerOutcome, BufferError> {
        self.manager.set_current(id)
    }

    pub fn delete(
        &mut self,
        id: BufferId,
        force: bool,
    ) -> Result<vim_buffer::ManagerOutcome, BufferError> {
        let res = self.manager.delete(id, force);
        if res.is_ok() {
            self.analysis.remove(&id);
        }
        res
    }

    pub fn reload(
        &mut self,
        id: BufferId,
        force: bool,
    ) -> Result<vim_buffer::MutationOutcome, BufferError> {
        self.manager.reload(id, force)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_buffer_has_exactly_one_analysis_state() {
        let mut store = BufferStore::new();
        let first = store.insert("first");
        let second = store.insert("second");

        assert_eq!(store.analysis.len(), 2);
        assert!(store.analysis(first).is_some());
        assert!(store.analysis(second).is_some());

        let first_state = store.analysis(first).unwrap() as *const BufferAnalysis;
        let same_state = store.analysis(first).unwrap() as *const BufferAnalysis;
        assert_eq!(first_state, same_state);
    }

    #[test]
    fn full_highlight_invalidation_clears_all_cached_state() {
        let mut store = BufferStore::new();
        let id = store.insert("fn main() {}\n");
        let snapshot = store.get(id).unwrap().snapshot().as_inner().clone();
        let highlights = store.analysis_mut(id).unwrap().highlights_mut();
        highlights.rows.insert(3, Vec::new());
        highlights.published_snapshot = Some(snapshot);

        store.analysis_mut(id).unwrap().invalidate_highlights();

        let highlights = store.analysis(id).unwrap().highlights();
        assert!(highlights.rows.is_empty());
        assert!(highlights.checkpoints.is_empty());
        assert!(highlights.published_snapshot.is_none());
    }
}
