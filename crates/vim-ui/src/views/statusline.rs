use crate::rect::Rect;
use crate::renderer::Renderer;
use crate::types::Color;
use crate::window::{UIContext, View};

pub struct StatusLineView {
    pub left_text: String,
    pub right_text: String,
}

impl StatusLineView {
    pub fn new(left: String, right: String) -> Self {
        Self {
            left_text: left,
            right_text: right,
        }
    }
}

impl View for StatusLineView {
    fn draw(&self, area: Rect, _context: &dyn UIContext, renderer: &mut dyn Renderer) {
        renderer.set_bg(Color::Grey);
        renderer.set_fg(Color::Black);

        // Clear line
        renderer.move_to(area.x, area.y);
        renderer.print(&" ".repeat(area.width as usize));

        // Draw left text
        renderer.move_to(area.x, area.y);
        renderer.print(&format!(" {} ", self.left_text));

        // Draw right text
        let right_len = self.right_text.len() + 2;
        if area.width as usize > right_len + self.left_text.len() + 4 {
            renderer.move_to(area.x + area.width - right_len as u16, area.y);
            renderer.print(&format!(" {} ", self.right_text));
        }

        renderer.reset_colors();
    }
}
