use super::{BufferId, WindowId};
use std::collections::HashMap;

/// Kernel-facing name for the semantic portion of a projected window.
/// Concrete storage remains in the UI until window projection is fully moved.
pub type SemanticWindow = vim_ui::WindowState;

/// Semantic identity and buffer association for an editor window.
///
/// Geometry and rendering state remain in `vim-ui::Window`. This record is
/// intentionally small: it is the kernel's ownership boundary for identity,
/// buffer association, and active/previous window state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowRecord {
    pub id: WindowId,
    pub buffer: BufferId,
}

#[derive(Debug, Default)]
pub struct Windows {
    records: HashMap<WindowId, WindowRecord>,
    active: Option<WindowId>,
    previous: Option<WindowId>,
}

impl Windows {
    pub fn register(&mut self, id: WindowId, buffer: BufferId) {
        self.records.insert(id, WindowRecord { id, buffer });
        if self.active.is_none() {
            self.active = Some(id);
        }
    }

    pub fn split(
        &mut self,
        source: WindowId,
        new_id: WindowId,
    ) -> Result<WindowRecord, &'static str> {
        if self.records.contains_key(&new_id) {
            return Err("semantic window already exists");
        }
        let source = self.record(source).ok_or("unknown source window")?;
        let record = WindowRecord {
            id: new_id,
            buffer: source.buffer,
        };
        self.records.insert(new_id, record);
        self.focus(new_id)?;
        Ok(record)
    }

    pub fn close(&mut self, id: WindowId) -> Option<WindowRecord> {
        self.unregister(id)
    }

    pub fn unregister(&mut self, id: WindowId) -> Option<WindowRecord> {
        let removed = self.records.remove(&id);
        if self.active == Some(id) {
            self.active = self.records.keys().next().copied();
        }
        if self.previous == Some(id) {
            self.previous = None;
        }
        removed
    }

    pub fn record(&self, id: WindowId) -> Option<WindowRecord> {
        self.records.get(&id).copied()
    }

    pub fn set_buffer(&mut self, id: WindowId, buffer: BufferId) -> Result<(), &'static str> {
        let record = self.records.get_mut(&id).ok_or("unknown semantic window")?;
        record.buffer = buffer;
        Ok(())
    }

    pub fn focus(&mut self, id: WindowId) -> Result<(), &'static str> {
        if !self.records.contains_key(&id) {
            return Err("unknown semantic window");
        }
        if self.active != Some(id) {
            self.previous = self.active;
            self.active = Some(id);
        }
        Ok(())
    }

    pub fn active(&self) -> Option<WindowId> {
        self.active
    }

    pub fn previous(&self) -> Option<WindowId> {
        self.previous
    }

    pub fn iter(&self) -> impl Iterator<Item = WindowRecord> + '_ {
        self.records.values().copied()
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }
}
