use crate::rect::Rect;
use crate::renderer::Renderer;
use crate::types::Color;
use crossterm::{
    cursor::MoveTo,
    execute,
    style::{Print, ResetColor, SetBackgroundColor, SetForegroundColor},
};
use std::io::Write;

pub struct CrosstermRenderer<W: Write> {
    writer: W,
}

impl<W: Write> CrosstermRenderer<W> {
    pub fn new(writer: W) -> Self {
        Self { writer }
    }
}

impl<W: Write> Renderer for CrosstermRenderer<W> {
    fn move_to(&mut self, x: u16, y: u16) {
        let _ = execute!(self.writer, MoveTo(x, y));
    }

    fn print(&mut self, text: &str) {
        let _ = execute!(self.writer, Print(text));
    }

    fn set_fg(&mut self, color: Color) {
        let _ = execute!(self.writer, SetForegroundColor(color.into()));
    }

    fn set_bg(&mut self, color: Color) {
        let _ = execute!(self.writer, SetBackgroundColor(color.into()));
    }

    fn reset_colors(&mut self) {
        let _ = execute!(self.writer, ResetColor);
    }

    fn draw_rect(&mut self, rect: Rect) {
        // Simple border drawing logic
        self.move_to(rect.x, rect.y);
        self.print("┌");
        self.print(&"─".repeat(rect.width.saturating_sub(2) as usize));
        self.print("┐");

        for y in 1..rect.height.saturating_sub(1) {
            self.move_to(rect.x, rect.y + y);
            self.print("│");
            self.move_to(rect.x + rect.width.saturating_sub(1), rect.y + y);
            self.print("│");
        }

        if rect.height > 1 {
            self.move_to(rect.x, rect.y + rect.height - 1);
            self.print("└");
            self.print(&"─".repeat(rect.width.saturating_sub(2) as usize));
            self.print("┘");
        }
    }
}
