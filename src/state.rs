use crate::controller::InputController;
use text::{ToOffset, ToPoint};
use vim_buffer::{Buffer, BufferError, BufferId, BufferManager, Point, SelectionSet};
use vim_input::Mode;
use vim_ui::{Rect, Ui, WindowId};

use std::collections::HashMap;

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

    pub fn reset_buffer(&mut self, name: impl Into<String>, buffer: &Buffer) {
        self.name = name.into();
        self.active_buffer_id = buffer.id();
        self.selections = SelectionSet::new();
        self.selections.add(buffer.as_text_buffer(), 0);
        self.scroll_row = 0;
        self.scroll_col = 0;
    }
}

pub struct PopupWindows {
    pub command_line: WindowId,
    pub autocomplete: WindowId,
    pub dialog: WindowId,
}

pub struct AppState {
    pub buffers: BufferManager,
    pub tabs: Vec<TabPage>,
    pub active_tab_index: usize,
    pub mode: Mode,
    pub running: bool,
    pub command_line: String,
    pub controller: InputController,
    pub ui: Ui,
    pub window_tabs: HashMap<WindowId, usize>,
    pub popups: PopupWindows,
    pub dialog_message: Option<String>,
}

impl AppState {
    pub fn active_tab(&self) -> &TabPage {
        &self.tabs[self.active_tab_index]
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
