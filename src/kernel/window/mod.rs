//! A window is a view into a buffer, not a second copy of it.
//!
//! Per `RESCUE.md` Rule 4.2, `Window` owns exactly the state that only makes
//! sense in the context of *looking at* a buffer: cursor/selection state
//! today, folds/viewport/scroll intent once those milestones land. It never
//! stores buffer text and never mutates it directly — motions update the
//! window's selection in place; edits go through `kernel::transaction`
//! against the buffer this window names.

pub mod tabpage;

use std::collections::HashMap;
use text::{Selection, SelectionGoal};
use vim_buffer::{Buffer, BufferId, SelectionId, SelectionSet};

use crate::kernel::ids::WindowId;

#[derive(Clone)]
pub struct Window {
    buffer: BufferId,
    selections: SelectionSet,
}

impl Window {
    /// Creates a window showing `buffer`, with its cursor at the start of
    /// the buffer.
    pub fn new(buffer_id: BufferId, buffer: &Buffer) -> Self {
        let anchor = buffer.as_text_buffer().anchor_before(0);
        let initial = Selection {
            id: 0,
            start: anchor,
            end: anchor,
            reversed: false,
            goal: SelectionGoal::None,
        };
        let selections = SelectionSet::from_selections(SelectionId::new(0), vec![initial])
            .expect("a single selection at buffer start is always a valid SelectionSet");
        Self {
            buffer: buffer_id,
            selections,
        }
    }

    pub fn buffer_id(&self) -> BufferId {
        self.buffer
    }

    pub fn selections(&self) -> &SelectionSet {
        &self.selections
    }

    pub fn selections_mut(&mut self) -> &mut SelectionSet {
        &mut self.selections
    }

    pub fn set_buffer(&mut self, buffer_id: BufferId) {
        self.buffer = buffer_id;
    }
}

pub struct WindowStore {
    windows: HashMap<WindowId, Window>,
    next_id: u64,
}

impl WindowStore {
    pub fn new() -> Self {
        Self {
            windows: HashMap::new(),
            next_id: 0,
        }
    }

    pub fn insert(&mut self, window: Window) -> WindowId {
        let id = WindowId::new(self.next_id);
        self.next_id += 1;
        self.windows.insert(id, window);
        id
    }

    pub fn get(&self, id: WindowId) -> Option<&Window> {
        self.windows.get(&id)
    }

    pub fn get_mut(&mut self, id: WindowId) -> Option<&mut Window> {
        self.windows.get_mut(&id)
    }

    pub fn remove(&mut self, id: WindowId) -> Option<Window> {
        self.windows.remove(&id)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&WindowId, &Window)> {
        self.windows.iter()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&WindowId, &mut Window)> {
        self.windows.iter_mut()
    }
}

