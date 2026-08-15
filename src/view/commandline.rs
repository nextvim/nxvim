use vim_ui::{Rect, Renderer, TextView, UIContext, View, WindowId};

pub struct CommandLineView {
    inner: TextView,
    mode: char,
}

impl CommandLineView {
    pub const fn new(window_id: WindowId) -> Self {
        Self {
            inner: TextView::new(window_id),
            mode: ':',
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
        if area.width == 0 || area.height == 0 {
            return Ok(());
        }

        renderer.move_to(area.x, area.y)?;
        if let Some(model) = context.get_text_model(self.inner.window_id()) {
            renderer.set_style(model.default_style)?;
        }
        renderer.print(&self.mode.to_string())?;

        let inner_area = Rect {
            x: area.x + 1,
            y: area.y,
            width: area.width.saturating_sub(1),
            height: area.height,
        };
        self.inner.draw(inner_area, context, renderer)
    }

    fn cursor_screen_pos(&self, area: Rect, context: &dyn UIContext) -> Option<(u16, u16)> {
        let inner_area = Rect {
            x: area.x + 1,
            y: area.y,
            width: area.width.saturating_sub(1),
            height: area.height,
        };
        self.inner.cursor_screen_pos(inner_area, context)
    }

    fn set_mode(&mut self, mode: char) {
        self.mode = mode;
    }
}
