use std::collections::HashMap;
use std::path::PathBuf;
use text::ToPoint;
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
    pub last_version: Option<clock::Global>,
    pub last_layout_width: Option<u32>,
    pub last_height: Option<u32>,
    pub last_has_border: Option<bool>,
}

pub struct BufferManager {
    inner: VimBufferManager,
    contexts: HashMap<BufferId, BufferContext>,
    display_contexts: HashMap<(BufferId, TabId), BufferDisplayContext>,
    pub window_buffers: HashMap<vim_ui::WindowId, BufferId>,
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

        let mut window_buffers = HashMap::new();
        window_buffers.insert(vim_ui::WindowId::new(3), first_buffer_id);

        Self {
            inner,
            contexts: HashMap::new(),
            display_contexts: HashMap::new(),
            window_buffers,
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

    pub fn display_contexts_mut(
        &mut self,
    ) -> &mut HashMap<(BufferId, TabId), BufferDisplayContext> {
        &mut self.display_contexts
    }

    pub fn with_mut<F, R>(
        &mut self,
        id: BufferId,
        tab_id: TabId,
        f: F,
    ) -> Result<R, vim_buffer::BufferError>
    where
        F: FnOnce(&mut vim_buffer::Buffer, &mut BufferContext, &mut BufferDisplayContext) -> R,
    {
        if !self.contexts.contains_key(&id) {
            self.contexts.insert(
                id,
                BufferContext {
                    treesitter: Err("Not loaded".to_string()),
                    index: Err("Not loaded".to_string()),
                },
            );
        }
        if !self.display_contexts.contains_key(&(id, tab_id)) {
            let buffer = self.inner.get(id)?;
            let snapshot = buffer.snapshot().as_inner().clone();
            let end_row = 100.min(snapshot.row_count());
            let display_map = display_map::DisplayMap::new_windowed(snapshot, None, 0..end_row);
            self.display_contexts.insert(
                (id, tab_id),
                BufferDisplayContext {
                    display_map,
                    highlights: Vec::new(),
                    selections: vim_buffer::SelectionSet::new(),
                    sequence: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
                    last_version: None,
                    last_layout_width: None,
                    last_height: None,
                    last_has_border: None,
                },
            );
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
        let digit_count = snapshot.row_count().max(1).to_string().len();
        let gutter_width = (digit_count.max(2) + 2) as u32;
        let border_width = if has_border { 2 } else { 0 };
        let wrap_width = layout_width.saturating_sub(gutter_width + border_width);

        let mut selections = vim_buffer::SelectionSet::new();
        if let Some(buf) = buffer {
            selections.add(buf.as_text_buffer(), 0);
        }
        let cursor_row = if !selections.selections.is_empty() {
            selections.primary().head().to_point(&snapshot).row
        } else {
            0
        };
        let window_size = height.max(24) * 2;
        let end_row = (cursor_row + window_size).min(snapshot.row_count());

        let mut display_map =
            display_map::DisplayMap::new_windowed(snapshot, Some(wrap_width), 0..end_row);
        let cursor_anchor = selections.primary().head();
        let display_cursor = display_map
            .snapshot()
            .anchor_to_display_point(cursor_anchor);
        display_map.scroll_to_cursor(display_cursor, height as i32, wrap_width as i32);
        Self {
            display_map,
            highlights: Vec::new(),
            selections,
            sequence: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
            last_version: None,
            last_layout_width: None,
            last_height: None,
            last_has_border: None,
        }
    }

    pub fn update(
        &mut self,
        snapshot: text::BufferSnapshot,
        layout_width: u32,
        height: u32,
        has_border: bool,
    ) {
        // Obsolete any pending async updates
        self.sequence
            .store(u64::MAX, std::sync::atomic::Ordering::Relaxed);

        self.last_version = Some(snapshot.version.clone());
        self.last_layout_width = Some(layout_width);
        self.last_height = Some(height);
        self.last_has_border = Some(has_border);

        let digit_count = snapshot.row_count().max(1).to_string().len();
        let gutter_width = (digit_count.max(2) + 2) as u32;
        let border_width = if has_border { 2 } else { 0 };
        let wrap_width = layout_width.saturating_sub(gutter_width + border_width);

        let cursor_row = if !self.selections.selections.is_empty() {
            self.selections.primary().head().to_point(&snapshot).row
        } else {
            0
        };
        let window_size = height.max(24) * 2;
        let end_row = (cursor_row + window_size).min(snapshot.row_count());

        self.display_map.sync_windowed(snapshot, 0..end_row);
        self.display_map.set_wrap_width(Some(wrap_width));
        if !self.selections.selections.is_empty() {
            let cursor_anchor = self.selections.primary().head();
            let display_cursor = self
                .display_map
                .snapshot()
                .anchor_to_display_point(cursor_anchor);
            self.display_map
                .scroll_to_cursor(display_cursor, height as i32, wrap_width as i32);
        }
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
        if self.last_version.as_ref() == Some(&snapshot.version)
            && self.last_layout_width == Some(layout_width)
            && self.last_height == Some(height)
            && self.last_has_border == Some(has_border)
        {
            return;
        }

        self.last_version = Some(snapshot.version.clone());
        self.last_layout_width = Some(layout_width);
        self.last_height = Some(height);
        self.last_has_border = Some(has_border);

        let owner_id = crate::app::services::OwnerId {
            buffer_id: Some(buffer_id),
            tab_id: Some(tab_id),
        };
        let sequence = self.sequence.clone();
        let cursor_row = if !self.selections.selections.is_empty() {
            self.selections.primary().head().to_point(&snapshot).row
        } else {
            0
        };
        let window_size = height.max(24) * 2;
        let end_row = (cursor_row + window_size).min(snapshot.row_count());

        services.spawn_task(
            "display_map",
            sequence,
            owner_id,
            crate::app::services::TaskType::DisplayMap,
            move || {
                let digit_count = snapshot.row_count().max(1).to_string().len();
                let gutter_width = (digit_count.max(2) + 2) as u32;
                let border_width = if has_border { 2 } else { 0 };
                let wrap_width = layout_width.saturating_sub(gutter_width + border_width);

                let display_map =
                    display_map::DisplayMap::new_windowed(snapshot, Some(wrap_width), 0..end_row);
                (display_map, height, layout_width)
            },
        );
    }
}
