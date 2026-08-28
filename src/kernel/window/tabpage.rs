//! Tabs own window layout, not buffer identity (`RESCUE.md` Rule 4.4).
//!
//! `TabPage` arranges windows; for this milestone that's just "one window"
//! since there is no split tree yet. It must never hold buffer text or
//! duplicate buffer options.

use std::collections::HashMap;

use crate::kernel::ids::{TabPageId, WindowId};

pub struct TabPage {
    window: WindowId,
}

impl TabPage {
    pub fn new(window: WindowId) -> Self {
        Self { window }
    }

    pub fn active_window(&self) -> WindowId {
        self.window
    }
}

pub struct TabStore {
    tabs: HashMap<TabPageId, TabPage>,
    next_id: u64,
}

impl TabStore {
    pub fn new() -> Self {
        Self {
            tabs: HashMap::new(),
            next_id: 0,
        }
    }

    pub fn insert(&mut self, tab: TabPage) -> TabPageId {
        let id = TabPageId::new(self.next_id);
        self.next_id += 1;
        self.tabs.insert(id, tab);
        id
    }

    pub fn get(&self, id: TabPageId) -> Option<&TabPage> {
        self.tabs.get(&id)
    }
}
