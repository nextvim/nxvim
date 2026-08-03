//! Application wrapper around `vim-buffer`'s buffer manager.
//!
//! The legacy `buffers` module remains unchanged while the new document and
//! selection implementations migrate onto this manager.

use crate::{editor::document::VimDocument, services};
use std::{collections::HashMap, path::Path};
use vim_buffer::{
    Buffer, BufferError, BufferId, BufferManager as VimBufferManager, EditOrigin, ManagerOutcome,
    MutationOutcome, SaveOutcome, TextRange, Transaction,
};

pub struct VimBufferEntry {
    pub id: BufferId,
    pub file_path: String,
    pub grammar: Option<services::treesitter::grammars::Grammar>,
    pub syntax_tree: Option<services::treesitter::SyntaxTree>,
}

pub struct VimBuffers {
    pub manager: VimBufferManager,
    entries: HashMap<BufferId, VimBufferEntry>,
}

impl Default for VimBuffers {
    fn default() -> Self {
        Self::new()
    }
}

impl VimBuffers {
    pub fn new() -> Self {
        Self {
            manager: VimBufferManager::new(),
            entries: HashMap::new(),
        }
    }

    pub fn get(&self, id: BufferId) -> Result<&Buffer, BufferError> {
        self.manager.get(id)
    }
    pub fn get_mut(&mut self, id: BufferId) -> Result<&mut Buffer, BufferError> {
        self.manager.get_mut(id)
    }
    pub fn entry(&self, id: BufferId) -> Option<&VimBufferEntry> {
        self.entries.get(&id)
    }
    pub fn entry_mut(&mut self, id: BufferId) -> Option<&mut VimBufferEntry> {
        self.entries.get_mut(&id)
    }

    pub fn create(&mut self, initial_text: impl Into<String>) -> BufferId {
        let buffer = self.manager.create(initial_text);
        let id = buffer.id();
        self.entries.insert(
            id,
            VimBufferEntry {
                id,
                file_path: String::new(),
                grammar: None,
                syntax_tree: None,
            },
        );
        id
    }

    pub fn find_by_path(&self, path: &str) -> Option<&VimBufferEntry> {
        self.entries.values().find(|entry| entry.file_path == path)
    }

    pub fn find_by_path_mut(&mut self, path: &str) -> Option<&mut VimBufferEntry> {
        self.entries
            .values_mut()
            .find(|entry| entry.file_path == path)
    }

    pub fn add_buffer_for_path(
        &mut self,
        path: &str,
    ) -> Result<&mut VimBufferEntry, Box<dyn std::error::Error>> {
        if self.find_by_path(path).is_some() {
            return Ok(self.find_by_path_mut(path).expect("entry exists"));
        }
        let id = if path.starts_with('#') || path.is_empty() {
            let id = self.create(String::new());
            self.entries.get_mut(&id).expect("created entry").file_path = path.to_owned();
            id
        } else if Path::new(path).exists() {
            self.load(path)?.0
        } else {
            self.create_named(path, String::new())?.0
        };
        Ok(self.entries.get_mut(&id).expect("created entry"))
    }

    pub fn create_scratch_buffer(
        &mut self,
    ) -> Result<&mut VimBufferEntry, Box<dyn std::error::Error>> {
        let mut index = 1;
        loop {
            let path = format!("#scratch-{index}");
            if self.find_by_path(&path).is_none() {
                return self.add_buffer_for_path(&path);
            }
            index += 1;
        }
    }

    pub fn create_named(
        &mut self,
        path: &str,
        initial_text: impl Into<String>,
    ) -> Result<(BufferId, ManagerOutcome), BufferError> {
        let (id, outcome) = self.manager.create_named(path, initial_text)?;
        self.ensure_entry(id, path);
        Ok((id, outcome))
    }

    pub fn find_by_name(&self, path: &str) -> Result<Option<BufferId>, BufferError> {
        self.manager.find_by_name(path)
    }

    pub fn list(&self) -> Vec<BufferId> {
        self.manager.list()
    }

    pub fn listed(&self) -> Vec<BufferId> {
        self.manager.listed()
    }

    pub fn current(&self) -> Option<BufferId> {
        self.manager.current()
    }

    pub fn alternate(&self) -> Option<BufferId> {
        self.manager.alternate()
    }

    pub fn set_current(&mut self, id: BufferId) -> Result<ManagerOutcome, BufferError> {
        self.manager.set_current(id)
    }

    pub fn load(&mut self, path: &str) -> Result<(BufferId, ManagerOutcome), BufferError> {
        let (id, outcome) = self.manager.load(path)?;
        self.ensure_entry(id, path);
        Ok((id, outcome))
    }

    pub fn create_scratch(&mut self) -> BufferId {
        let id = self.create(String::new());
        let index = self
            .entries
            .values()
            .filter(|entry| entry.file_path.starts_with("#scratch-"))
            .count()
            + 1;
        self.entries.get_mut(&id).expect("created entry").file_path = format!("#scratch-{index}");
        id
    }

    pub fn document(&self, id: BufferId) -> Result<VimDocument, BufferError> {
        let file_path = self
            .entry(id)
            .map(|entry| entry.file_path.as_str())
            .unwrap_or_default();
        VimDocument::new_with_file_path(id.get() as usize, self.get(id)?, file_path)
    }

    pub fn transaction(
        &mut self,
        id: BufferId,
        origin: EditOrigin,
    ) -> Result<Transaction<'_>, BufferError> {
        self.manager.transaction(id, origin)
    }

    pub fn replace(
        &mut self,
        id: BufferId,
        origin: EditOrigin,
        range: TextRange,
        replacement: impl Into<std::sync::Arc<str>>,
    ) -> Result<MutationOutcome, BufferError> {
        self.manager.replace(id, origin, range, replacement)
    }

    pub fn save(&mut self, id: BufferId, force: bool) -> Result<SaveOutcome, BufferError> {
        self.manager.save(id, force)
    }

    pub fn file_buffers(&self) -> impl Iterator<Item = &VimBufferEntry> {
        self.entries
            .values()
            .filter(|entry| !entry.file_path.is_empty() && !entry.file_path.starts_with('#'))
    }

    pub fn special_buffers(&self) -> impl Iterator<Item = &VimBufferEntry> {
        self.entries
            .values()
            .filter(|entry| entry.file_path.starts_with('#'))
    }

    pub fn is_special(&self, id: BufferId) -> bool {
        self.entry(id)
            .is_some_and(|entry| entry.file_path.starts_with('#'))
    }

    pub fn is_file_backed(&self, id: BufferId) -> bool {
        self.entry(id)
            .is_some_and(|entry| !entry.file_path.is_empty() && !entry.file_path.starts_with('#'))
    }

    fn ensure_entry(&mut self, id: BufferId, path: &str) {
        self.entries.entry(id).or_insert_with(|| VimBufferEntry {
            id,
            file_path: path.to_owned(),
            grammar: services::treesitter::grammars::Grammar::from_path(path),
            syntax_tree: None,
        });
    }

    pub fn path(&self, id: BufferId) -> Option<&Path> {
        self.entry(id).and_then(|entry| {
            (!entry.file_path.is_empty() && !entry.file_path.starts_with('#'))
                .then(|| Path::new(entry.file_path.as_str()))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_vim_buffer_for_new_document() {
        let mut buffers = VimBuffers::new();
        let id = buffers.create("hello");
        assert_eq!(
            buffers
                .get(id)
                .unwrap()
                .snapshot()
                .chunks()
                .collect::<String>(),
            "hello"
        );
        assert!(!buffers.is_file_backed(id));
    }

    #[test]
    fn scratch_buffers_are_special() {
        let mut buffers = VimBuffers::new();
        let id = buffers.create_scratch();
        assert!(buffers.is_special(id));
        assert_eq!(buffers.special_buffers().count(), 1);
    }
}
