use crate::{
    controller::InputController,
    display::{DisplayMap, Fold},
    script::ScriptRuntime,
    services::{Services, background::TaskId},
};
use text::{ToOffset, ToPoint};
use vim_buffer::{Buffer, BufferError, BufferId, BufferManager, Point, SelectionSet};
use vim_input::Mode;
use vim_ui::{Rect, Ui, WindowId};

use std::{
    collections::HashMap,
    sync::{Arc, atomic::AtomicU64},
};

pub struct TabPage {
    pub name: String,
    pub active_buffer_id: BufferId,
    pub selections: SelectionSet,
    pub scroll_row: usize,
    pub scroll_col: usize,
}

impl TabPage {
    pub fn new(name: impl Into<String>, active_buffer_id: BufferId, buffer: &Buffer) -> Self {
        let mut selections = SelectionSet::new();
        selections.add(buffer.as_text_buffer(), 0);
        Self {
            name: name.into(),
            active_buffer_id,
            selections,
            scroll_row: 0,
            scroll_col: 0,
        }
    }

    pub fn cursor_point(&self, buffer: &Buffer) -> Point {
        self.selections
            .primary()
            .head()
            .to_point(buffer.as_text_buffer())
    }

    pub fn set_primary_cursor(
        &mut self,
        buffer: &Buffer,
        row: usize,
        column: usize,
    ) -> Result<(), BufferError> {
        let text_buffer = buffer.as_text_buffer();
        let point = Point::new(row as u32, column as u32);
        let anchor = text_buffer.anchor_at(point.to_offset(text_buffer), sum_tree::Bias::Left);
        let mut primary = self.selections.primary().clone();
        primary.start = anchor;
        primary.end = anchor;
        primary.reversed = false;
        self.selections.replace_primary(primary)
    }
}

pub struct DisplayState {
    pub map: Option<DisplayMap>,
    pub folds: Vec<Fold>,
    pub latest_task_id: Arc<AtomicU64>,
    pub pending_task_id: Option<TaskId>,
    pub requested_buffer_id: Option<u64>,
    pub applied_buffer_id: Option<u64>,
    pub requested_changedtick: Option<u64>,
    pub applied_changedtick: Option<u64>,
    pub requested_wrap_width: Option<u32>,
    pub requested_inner_height: Option<u16>,
    pub requested_buffer_window: Option<std::ops::Range<u32>>,
    pub syncedtick: Option<u64>,
}

impl DisplayState {
    pub fn new() -> Self {
        Self {
            map: None,
            folds: Vec::new(),
            latest_task_id: Arc::new(AtomicU64::new(0)),
            pending_task_id: None,
            requested_buffer_id: None,
            applied_buffer_id: None,
            requested_changedtick: None,
            applied_changedtick: None,
            requested_wrap_width: None,
            requested_inner_height: None,
            requested_buffer_window: None,
            syncedtick: None,
        }
    }

    pub fn set_folds(&mut self, folds: Vec<Fold>) {
        if self.folds != folds {
            self.folds = folds;
            self.requested_changedtick = None;
        }
    }

    pub fn clear_folds(&mut self) {
        self.set_folds(Vec::new());
    }
}

pub struct PopupWindows {
    pub autocomplete: WindowId,
    pub dialog: WindowId,
}

pub struct AppState {
    pub buffers: BufferManager,
    pub tabs: Vec<TabPage>,
    pub active_tab_index: usize,
    pub mode: Mode,
    pub running: bool,
    pub command_buffer_id: BufferId,
    pub command_selections: SelectionSet,
    pub command_return_focus: WindowId,
    pub command_line_focused: bool,
    pub controller: InputController,
    pub script: ScriptRuntime,
    pub services: Services,
    pub ui: Ui,
    pub window_tabs: HashMap<WindowId, usize>,
    pub display_states: HashMap<WindowId, DisplayState>,
    pub popups: PopupWindows,
    pub dialog_message: Option<String>,
}

impl AppState {
    pub fn command_text(&self) -> Result<String, BufferError> {
        Ok(self
            .buffers
            .get(self.command_buffer_id)?
            .snapshot()
            .chunks()
            .collect())
    }

    pub fn clear_command_buffer(&mut self) -> Result<(), BufferError> {
        let buffer = self.buffers.get_mut(self.command_buffer_id)?;
        let len = buffer.snapshot().len_bytes();
        if len > 0 {
            let mut transaction = buffer.transaction(vim_buffer::EditOrigin::InsertMode);
            transaction.delete(
                None,
                vim_buffer::TextRange {
                    start: vim_buffer::ByteOffset(0),
                    end: vim_buffer::ByteOffset(len),
                },
            );
            transaction.commit(None)?;
        }
        self.command_selections = SelectionSet::new();
        self.command_selections.add(buffer.as_text_buffer(), 0);
        Ok(())
    }

    pub fn sync_active_tab_to_focus(&mut self) {
        if let Some(&tab_index) = self.window_tabs.get(&self.ui.focused_window_id()) {
            self.active_tab_index = tab_index.min(self.tabs.len().saturating_sub(1));
        }
    }

    pub fn resize_ui(&mut self, screen: Rect) {
        self.ui.resize(editor_rect(screen));
    }

    pub fn switch_focused_tab(&mut self, delta: isize) {
        if self.tabs.is_empty() {
            return;
        }
        self.sync_active_tab_to_focus();
        let next = cycle_index(self.active_tab_index, self.tabs.len(), delta);
        self.active_tab_index = next;
        let focused = self.ui.focused_window_id();
        if let Some(tab_index) = self.window_tabs.get_mut(&focused) {
            *tab_index = next;
        }
    }
}

fn cycle_index(current: usize, len: usize, delta: isize) -> usize {
    (current as isize + delta).rem_euclid(len as isize) as usize
}

#[cfg(test)]
mod tests {
    use super::cycle_index;

    #[test]
    fn tab_index_wraps_in_both_directions() {
        assert_eq!(cycle_index(2, 3, 1), 0);
        assert_eq!(cycle_index(0, 3, -1), 2);
        assert_eq!(cycle_index(0, 3, 5), 2);
    }
}

pub fn editor_rect(screen: Rect) -> Rect {
    Rect::new(0, 1, screen.width, screen.height.saturating_sub(3))
}
