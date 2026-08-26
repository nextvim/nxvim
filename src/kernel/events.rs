use std::collections::VecDeque;

use super::{BufferId, WindowId};
use vim_buffer::ChangedTick;

/// Owned name of an option crossing the editor event boundary.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OptionName(String);

impl OptionName {
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for OptionName {
    fn from(name: String) -> Self {
        Self(name)
    }
}

impl From<&str> for OptionName {
    fn from(name: &str) -> Self {
        Self(name.to_owned())
    }
}

/// Application-level editor events with stable identities and owned payloads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorEvent {
    BufAdd { buffer: BufferId },
    BufRead { buffer: BufferId },
    BufEnter { buffer: BufferId, window: WindowId },
    BufLeave { buffer: BufferId, window: WindowId },
    BufWrite { buffer: BufferId },
    BufUnload { buffer: BufferId },
    BufDelete { buffer: BufferId },
    BufWipeout { buffer: BufferId },
    TextChanged { buffer: BufferId, tick: ChangedTick },
    CursorMoved { window: WindowId },
    InsertEnter { window: WindowId },
    InsertLeave { window: WindowId },
    OptionSet { name: OptionName },
    VimEnter,
    VimLeave,
}

impl EditorEvent {
    pub fn is_deferred(&self) -> bool {
        matches!(self, Self::TextChanged { .. } | Self::CursorMoved { .. })
    }
}

/// Kernel-owned event staging. Deferred events are drained only at an explicit
/// safe boundary so callbacks cannot run inside a mutation or focus change.
#[derive(Debug, Default)]
pub struct EventQueue {
    immediate: VecDeque<EditorEvent>,
    deferred: VecDeque<EditorEvent>,
}

impl EventQueue {
    pub fn push(&mut self, event: EditorEvent) {
        if event.is_deferred() {
            self.deferred.push_back(event);
        } else {
            self.immediate.push_back(event);
        }
    }

    pub fn pop_immediate(&mut self) -> Option<EditorEvent> {
        self.immediate.pop_front()
    }

    pub fn pop_deferred(&mut self) -> Option<EditorEvent> {
        self.deferred.pop_front()
    }

    pub fn drain_deferred(&mut self) -> Vec<EditorEvent> {
        self.deferred.drain(..).collect()
    }

    pub fn has_immediate(&self) -> bool {
        !self.immediate.is_empty()
    }

    pub fn has_deferred(&self) -> bool {
        !self.deferred.is_empty()
    }
}
