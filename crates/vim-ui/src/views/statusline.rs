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
    fn draw(&self, area: Rect, context: &dyn UIContext, renderer: &mut dyn Renderer) {
        let mut bg = Color::Grey;
        let mut fg = Color::Black;

        if let Some(cs) = context.get_colorscheme() {
            if let Some(style) = cs.get_style("StatusLine") {
                if let Some(style_bg) = style.bg {
                    bg = style_bg;
                }
                if let Some(style_fg) = style.fg {
                    fg = style_fg;
                }
            }
        }

        renderer.set_bg(bg);
        renderer.set_fg(fg);

        // Clear line
        renderer.move_to(area.x, area.y);
        renderer.print(&" ".repeat(area.width as usize));

        // Draw left text
        renderer.move_to(area.x, area.y);
        renderer.print(&format!(" {} ", self.left_text));

        // Draw right text
        let right_len = self.right_text.chars().count() + 2;
        if area.width as usize > right_len + self.left_text.chars().count() + 4 {
            renderer.move_to(area.x + area.width - right_len as u16, area.y);
            renderer.print(&format!(" {} ", self.right_text));
        }

        renderer.reset_colors();
    }
}
