use std::path::PathBuf;
use std::collections::HashMap;
use vim_buffer::{BufferManager as VimBufferManager, BufferId};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TabId(pub u64);

pub struct BufferContext {
    pub treesitter: Result<vim_treesitter::SyntaxTree, String>,
    pub index: Result<vim_indexer::IndexTaskResult, String>,
}

pub struct BufferDisplayContext {
    pub display_map: display_map::DisplayMap,
    pub highlights: Vec<textmate::HighlightSpan>,
    pub selections: vim_buffer::SelectionSet,
}

pub struct BufferManager {
    inner: VimBufferManager,
    contexts: HashMap<BufferId, BufferContext>,
    display_contexts: HashMap<(BufferId, TabId), BufferDisplayContext>,
}

impl BufferManager {
    pub fn new() -> Self {
        let mut inner = VimBufferManager::new();
        
        let args: Vec<String> = std::env::args().skip(1).collect();
        let first_buffer_id = if args.is_empty() {
            inner.create("").id()
        } else {
            let mut first_id = None;
            for file in args {
                let path = PathBuf::from(file);
                match inner.load(&path) {
                    Ok((id, _)) => {
                        if first_id.is_none() {
                            first_id = Some(id);
                        }
                    }
                    Err(_) => {
                        if let Ok((id, _)) = inner.create_named(&path, "") {
                            if first_id.is_none() {
                                first_id = Some(id);
                            }
                        }
                    }
                }
            }
            first_id.unwrap_or_else(|| inner.create("").id())
        };
        
        let _ = inner.set_current(first_buffer_id);

        Self {
            inner,
            contexts: HashMap::new(),
            display_contexts: HashMap::new(),
        }
    }

    pub fn get_current(&self) -> Option<BufferId> {
        self.inner.current()
    }

    pub fn create(&mut self, initial_text: impl Into<String>) -> BufferId {
        self.inner.create(initial_text).id()
    }

    pub fn create_named(
        &mut self,
        name: impl AsRef<std::path::Path>,
        initial_text: impl Into<String>,
    ) -> Result<(BufferId, vim_buffer::ManagerOutcome), vim_buffer::BufferError> {
        self.inner.create_named(name, initial_text)
    }

    pub fn load(
        &mut self,
        path: impl AsRef<std::path::Path>,
    ) -> Result<(BufferId, vim_buffer::ManagerOutcome), vim_buffer::BufferError> {
        self.inner.load(path)
    }

    pub fn unload(&mut self, id: BufferId, force: bool) -> Result<vim_buffer::ManagerOutcome, vim_buffer::BufferError> {
        let res = self.inner.unload(id, force);
        if res.is_ok() {
            self.contexts.remove(&id);
            self.display_contexts.retain(|(bid, _), _| *bid != id);
        }
        res
    }

    pub fn delete(&mut self, id: BufferId, force: bool) -> Result<vim_buffer::ManagerOutcome, vim_buffer::BufferError> {
        let res = self.inner.delete(id, force);
        if res.is_ok() {
            self.contexts.remove(&id);
            self.display_contexts.retain(|(bid, _), _| *bid != id);
        }
        res
    }

    pub fn wipe(&mut self, id: BufferId, force: bool) -> Result<vim_buffer::ManagerOutcome, vim_buffer::BufferError> {
        let res = self.inner.wipe(id, force);
        if res.is_ok() {
            self.contexts.remove(&id);
            self.display_contexts.retain(|(bid, _), _| *bid != id);
        }
        res
    }

    pub fn get_buffer(&self, id: BufferId) -> Result<&vim_buffer::Buffer, vim_buffer::BufferError> {
        self.inner.get(id)
    }

    pub fn list(&self) -> Vec<BufferId> {
        self.inner.list()
    }

    pub fn get_buffer_mut(&mut self, id: BufferId) -> Result<&mut vim_buffer::Buffer, vim_buffer::BufferError> {
        self.inner.get_mut(id)
    }

    pub fn get_buffer_context(&self, id: BufferId) -> Option<&BufferContext> {
        self.contexts.get(&id)
    }

    pub fn get_buffer_context_mut(&mut self, id: BufferId) -> Option<&mut BufferContext> {
        self.contexts.get_mut(&id)
    }

    pub fn set_buffer_context(&mut self, id: BufferId, context: BufferContext) {
        self.contexts.insert(id, context);
    }

    pub fn get_buffer_display_context(&self, buffer_id: BufferId, tab_id: TabId) -> Option<&BufferDisplayContext> {
        self.display_contexts.get(&(buffer_id, tab_id))
    }

    pub fn get_buffer_display_context_mut(&mut self, buffer_id: BufferId, tab_id: TabId) -> Option<&mut BufferDisplayContext> {
        self.display_contexts.get_mut(&(buffer_id, tab_id))
    }

    pub fn set_buffer_display_context(&mut self, buffer_id: BufferId, tab_id: TabId, context: BufferDisplayContext) {
        self.display_contexts.insert((buffer_id, tab_id), context);
    }
}
