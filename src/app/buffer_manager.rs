use std::collections::HashMap;
use std::path::PathBuf;
use vim_buffer::{BufferId, BufferManager as VimBufferManager};

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
    pub sequence: std::sync::Arc<std::sync::atomic::AtomicU64>,
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

    pub fn unload(
        &mut self,
        id: BufferId,
        force: bool,
    ) -> Result<vim_buffer::ManagerOutcome, vim_buffer::BufferError> {
        let res = self.inner.unload(id, force);
        if res.is_ok() {
            self.contexts.remove(&id);
            self.display_contexts.retain(|(bid, _), _| *bid != id);
        }
        res
    }

    pub fn delete(
        &mut self,
        id: BufferId,
        force: bool,
    ) -> Result<vim_buffer::ManagerOutcome, vim_buffer::BufferError> {
        let res = self.inner.delete(id, force);
        if res.is_ok() {
            self.contexts.remove(&id);
            self.display_contexts.retain(|(bid, _), _| *bid != id);
        }
        res
    }

    pub fn wipe(
        &mut self,
        id: BufferId,
        force: bool,
    ) -> Result<vim_buffer::ManagerOutcome, vim_buffer::BufferError> {
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

    pub fn get_buffer_mut(
        &mut self,
        id: BufferId,
    ) -> Result<&mut vim_buffer::Buffer, vim_buffer::BufferError> {
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

    pub fn get_buffer_display_context(
        &self,
        buffer_id: BufferId,
        tab_id: TabId,
    ) -> Option<&BufferDisplayContext> {
        self.display_contexts.get(&(buffer_id, tab_id))
    }

    pub fn get_buffer_display_context_mut(
        &mut self,
        buffer_id: BufferId,
        tab_id: TabId,
    ) -> Option<&mut BufferDisplayContext> {
        self.display_contexts.get_mut(&(buffer_id, tab_id))
    }

    pub fn set_buffer_display_context(
        &mut self,
        buffer_id: BufferId,
        tab_id: TabId,
        context: BufferDisplayContext,
    ) {
        self.display_contexts.insert((buffer_id, tab_id), context);
    }

    pub fn display_contexts_mut(&mut self) -> &mut HashMap<(BufferId, TabId), BufferDisplayContext> {
        &mut self.display_contexts
    }

    pub fn with_mut<F, R>(&mut self, id: BufferId, tab_id: TabId, f: F) -> Result<R, vim_buffer::BufferError>
    where
        F: FnOnce(&mut vim_buffer::Buffer, &mut BufferContext, &mut BufferDisplayContext) -> R,
    {
        if !self.contexts.contains_key(&id) {
            self.contexts.insert(id, BufferContext {
                treesitter: Err("Not loaded".to_string()),
                index: Err("Not loaded".to_string()),
            });
        }
        if !self.display_contexts.contains_key(&(id, tab_id)) {
            let buffer = self.inner.get(id)?;
            let snapshot = buffer.snapshot().as_inner().clone();
            let display_map = display_map::DisplayMap::new(snapshot, None);
            self.display_contexts.insert((id, tab_id), BufferDisplayContext {
                display_map,
                highlights: Vec::new(),
                selections: vim_buffer::SelectionSet::new(),
                sequence: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
            });
        }

        let buffer = self.inner.get_mut(id)?;
        let context = self.contexts.get_mut(&id).unwrap();
        let display_context = self.display_contexts.get_mut(&(id, tab_id)).unwrap();

        Ok(f(buffer, context, display_context))
    }
}

impl BufferDisplayContext {
    pub fn new(
        snapshot: text::BufferSnapshot,
        layout_width: u32,
        height: u32,
        has_border: bool,
        buffer: Option<&vim_buffer::Buffer>,
    ) -> Self {
        let mut display_map = display_map::DisplayMap::new(snapshot, None);
        display_map.set_layout_width(Some(layout_width), has_border);
        let mut selections = vim_buffer::SelectionSet::new();
        if let Some(buf) = buffer {
            selections.add(buf.as_text_buffer(), 0);
        }
        let cursor_anchor = selections.primary().head();
        let display_cursor = display_map.snapshot().anchor_to_display_point(cursor_anchor);
        let wrap_width = display_map.wrap_width.unwrap_or(layout_width);
        display_map.scroll_to_cursor(
            display_cursor,
            height as i32,
            wrap_width as i32,
        );
        Self {
            display_map,
            highlights: Vec::new(),
            selections,
            sequence: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }

    pub fn update(&mut self, snapshot: text::BufferSnapshot, layout_width: u32, height: u32, has_border: bool) {
        self.display_map.sync(snapshot);
        self.display_map.set_layout_width(Some(layout_width), has_border);
        let cursor_anchor = self.selections.primary().head();
        let display_cursor = self.display_map.snapshot().anchor_to_display_point(cursor_anchor);
        let wrap_width = self.display_map.wrap_width.unwrap_or(layout_width);
        self.display_map.scroll_to_cursor(
            display_cursor,
            height as i32,
            wrap_width as i32,
        );
    }

    pub fn update_async(
        &mut self,
        snapshot: text::BufferSnapshot,
        layout_width: u32,
        height: u32,
        has_border: bool,
        buffer_id: vim_buffer::BufferId,
        tab_id: crate::app::buffer_manager::TabId,
        services: &crate::app::services::Services,
    ) {
        let mut display_map = self.display_map.clone();
        let owner_id = crate::app::services::OwnerId {
            buffer_id: Some(buffer_id),
            tab_id: Some(tab_id),
        };
        let sequence = self.sequence.clone();
        services.spawn_task(
            "display_map",
            sequence,
            owner_id,
            crate::app::services::TaskType::DisplayMap,
            move || {
                display_map.sync(snapshot);
                display_map.set_layout_width(Some(layout_width), has_border);
                (display_map, height, layout_width)
            },
        );
    }
}
