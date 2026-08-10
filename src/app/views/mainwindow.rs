use vim_ui::{Rect, Renderer, UIContext, View, TextView, WindowId};
use vim_buffer::BufferId;

#[derive(Clone, Debug)]
pub struct MainWindowState {
    pub window_buffers: std::collections::HashMap<WindowId, BufferId>,
}

impl MainWindowState {
    pub fn new() -> Self {
        let mut window_buffers = std::collections::HashMap::new();
        // The initial editor window is WindowId::new(3)
        window_buffers.insert(WindowId::new(3), BufferId::new(1).unwrap());
        Self { window_buffers }
    }
}

pub struct MainWindowView {
    inner: TextView,
}

impl MainWindowView {
    pub const fn new(window_id: WindowId) -> Self {
        Self {
            inner: TextView::new(window_id),
        }
    }
}

impl View for MainWindowView {
    fn draw(
        &self,
        area: Rect,
        context: &dyn UIContext,
        renderer: &mut dyn Renderer,
    ) -> std::io::Result<()> {
        self.inner.draw(area, context, renderer)
    }

    fn cursor_screen_pos(&self, area: Rect, context: &dyn UIContext) -> Option<(u16, u16)> {
        self.inner.cursor_screen_pos(area, context)
    }
}
