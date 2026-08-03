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
    fn draw(
        &self,
        area: Rect,
        context: &dyn UIContext,
        renderer: &mut dyn Renderer,
    ) -> std::io::Result<()> {
        let mut fill_bg = Color::DarkGrey;
        let mut fill_fg = Color::White;

        let mut active_bg = Color::Grey;
        let mut active_fg = Color::Black;

        let mut inactive_bg = Color::DarkGrey;
        let mut inactive_fg = Color::White;

        if let Some(cs) = context.get_colorscheme() {
            // Default filler background
            if let Some(style) = cs.get_style("TabLineFill") {
                if let Some(b) = style.bg {
                    fill_bg = b;
                }
                if let Some(f) = style.fg {
                    fill_fg = f;
                }
            } else if let Some(style) = cs.get_style("TabLine") {
                if let Some(b) = style.bg {
                    fill_bg = b;
                }
                if let Some(f) = style.fg {
                    fill_fg = f;
                }
            }

            // Active tab style
            if let Some(style) = cs.get_style("TabLineSel") {
                if let Some(b) = style.bg {
                    active_bg = b;
                }
                if let Some(f) = style.fg {
                    active_fg = f;
                }
            }

            // Inactive tab style
            if let Some(style) = cs.get_style("TabLine") {
                if let Some(b) = style.bg {
                    inactive_bg = b;
                }
                if let Some(f) = style.fg {
                    inactive_fg = f;
                }
            }
        }

        renderer.set_bg(fill_bg)?;
        renderer.set_fg(fill_fg)?;

        renderer.move_to(area.x, area.y)?;
        renderer.print(&" ".repeat(area.width as usize))?;

        let mut x = area.x;
        for (i, tab) in self.tabs.iter().enumerate() {
            if i == self.active_index {
                renderer.set_bg(active_bg)?;
                renderer.set_fg(active_fg)?;
            } else {
                renderer.set_bg(inactive_bg)?;
                renderer.set_fg(inactive_fg)?;
            }

            let text = format!(" {} ", tab);
            renderer.move_to(x, area.y)?;
            renderer.print(&text)?;
            x += text.len() as u16;

            if x >= area.x + area.width {
                break;
            }
        }

        renderer.reset_colors()?;
        Ok(())
    }
}
