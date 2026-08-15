use std::collections::HashMap;

#[derive(Default)]
pub struct LayoutSnapshot {
    windows: HashMap<vim_ui::WindowId, WindowLayout>,
}

#[derive(Debug, Clone, Copy)]
pub struct WindowLayout {
    pub rect: vim_ui::Rect,
    pub draws_border: bool,
}

impl LayoutSnapshot {
    pub fn insert(&mut self, window_id: vim_ui::WindowId, rect: vim_ui::Rect, draws_border: bool) {
        self.windows
            .insert(window_id, WindowLayout { rect, draws_border });
    }

    pub fn get(&self, window_id: vim_ui::WindowId) -> Option<WindowLayout> {
        self.windows.get(&window_id).copied()
    }
}
