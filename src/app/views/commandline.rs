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
        context: &dyn UIContext,
        renderer: &mut dyn Renderer,
    ) -> std::io::Result<()> {
        let content = context.get_status_message().unwrap_or_else(|| self.content.clone());
        renderer.move_to(area.x, area.y)?;
        renderer.print(&content)?;
        let remaining = (area.width as usize).saturating_sub(content.len());
        if remaining > 0 {
            renderer.print(&" ".repeat(remaining))?;
        }
        Ok(())
    }
}
