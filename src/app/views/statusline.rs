use vim_ui::{Rect, Renderer, UIContext, View};

pub struct StatusLineView {
    left: String,
    right: String,
}

impl StatusLineView {
    pub fn new(left: String, right: String) -> Self {
        Self { left, right }
    }
}

impl View for StatusLineView {
    fn draw(
        &self,
        area: Rect,
        _context: &dyn UIContext,
        renderer: &mut dyn Renderer,
    ) -> std::io::Result<()> {
        renderer.move_to(area.x, area.y)?;

        let total_width = area.width as usize;
        let left_width = self.left.chars().count();
        let right_width = self.right.chars().count();

        if left_width + right_width >= total_width {
            let combined = format!("{}{}", self.left, self.right);
            let truncated: String = combined.chars().take(total_width).collect();
            renderer.print(&truncated)?;
        } else {
            renderer.print(&self.left)?;
            let padding = total_width - left_width - right_width;
            renderer.print(&" ".repeat(padding))?;
            renderer.print(&self.right)?;
        }
        Ok(())
    }
}
