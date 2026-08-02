use crate::rect::Rect;
use crate::renderer::Renderer;
use crate::types::Color;
use crate::window::{UIContext, View};

pub struct TabLineView {
    pub tabs: Vec<String>,
    pub active_index: usize,
}

impl TabLineView {
    pub fn new(tabs: Vec<String>, active_index: usize) -> Self {
        Self { tabs, active_index }
    }
}

impl View for TabLineView {
    fn draw(&self, area: Rect, _context: &dyn UIContext, renderer: &mut dyn Renderer) {
        renderer.set_bg(Color::DarkGrey);
        renderer.set_fg(Color::White);

        renderer.move_to(area.x, area.y);
        renderer.print(&" ".repeat(area.width as usize));

        let mut x = area.x;
        for (i, tab) in self.tabs.iter().enumerate() {
            if i == self.active_index {
                renderer.set_bg(Color::Grey);
                renderer.set_fg(Color::Black);
            } else {
                renderer.set_bg(Color::DarkGrey);
                renderer.set_fg(Color::White);
            }

            let text = format!(" {} ", tab);
            renderer.move_to(x, area.y);
            renderer.print(&text);
            x += text.len() as u16;

            if x >= area.x + area.width {
                break;
            }
        }

        renderer.reset_colors();
    }
}
