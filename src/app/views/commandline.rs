use vim_ui::{Rect, Renderer, UIContext, View};

pub struct CommandLineView {
    pub content: String,
}

impl CommandLineView {
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
        }
    }
}

impl View for CommandLineView {
    fn draw(
        &self,
        area: Rect,
        _context: &dyn UIContext,
        renderer: &mut dyn Renderer,
    ) -> std::io::Result<()> {
        renderer.move_to(area.x, area.y)?;
        renderer.print(&format!(":{}", self.content))?;
        let remaining = (area.width as usize).saturating_sub(self.content.len() + 1);
        if remaining > 0 {
            renderer.print(&" ".repeat(remaining))?;
        }
        Ok(())
    }
}
