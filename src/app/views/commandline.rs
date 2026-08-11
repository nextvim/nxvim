use vim_ui::{Rect, Renderer, TextView, UIContext, View, WindowId};

pub struct CommandLineView {
    inner: TextView,
}

impl CommandLineView {
    pub const fn new(window_id: WindowId) -> Self {
        Self {
            inner: TextView::new(window_id),
        }
    }
}

impl View for CommandLineView {
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
