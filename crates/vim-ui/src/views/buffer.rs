use crate::id::BufferId;
use crate::rect::Rect;
use crate::renderer::Renderer;
use crate::types::Color;
use crate::window::{UIContext, View};

pub struct BufferView {
    pub buffer_id: BufferId,
    pub show_line_numbers: bool,
    pub scroll_row: usize,
    pub scroll_col: usize,
}

impl BufferView {
    pub fn new(buffer_id: BufferId, show_line_numbers: bool) -> Self {
        Self {
            buffer_id,
            show_line_numbers,
            scroll_row: 0,
            scroll_col: 0,
        }
    }
}

impl View for BufferView {
    fn draw(&self, area: Rect, context: &dyn UIContext, renderer: &mut dyn Renderer) {
        let margin = if self.show_line_numbers { 4 } else { 0 };

        if let Some(model) = context.get_buffer_model(self.buffer_id) {
            let total_lines = model.lines.len();
            for i in 0..area.height as usize {
                let doc_row = i + self.scroll_row;
                if doc_row >= total_lines {
                    break;
                }
                renderer.move_to(area.x, area.y + i as u16);

                if self.show_line_numbers {
                    renderer.set_fg(Color::DarkGrey);
                    renderer.print(&format!("{:3} ", doc_row + 1));
                    renderer.reset_colors();
                }

                if let Some(line) = model.lines.get_line(doc_row) {
                    let max_width = area.width.saturating_sub(margin) as usize;
                    let scrolled_line: String = line.chars().skip(self.scroll_col).collect();
                    let content: String = scrolled_line.chars().take(max_width).collect();
                    renderer.print(&content);
                }
            }

            let cursor_row = model.cursor.row;
            let cursor_col = model.cursor.col;
            if cursor_row >= self.scroll_row && cursor_row < self.scroll_row + area.height as usize
            {
                if cursor_col >= self.scroll_col
                    && cursor_col < self.scroll_col + (area.width.saturating_sub(margin)) as usize
                {
                    let x = area.x + margin + (cursor_col - self.scroll_col) as u16;
                    let y = area.y + (cursor_row - self.scroll_row) as u16;
                    renderer.move_to(x, y);
                }
            }
        }
    }

    fn cursor_screen_pos(&self, area: Rect, context: &dyn UIContext) -> Option<(u16, u16)> {
        let margin = if self.show_line_numbers { 4 } else { 0 };
        let model = context.get_buffer_model(self.buffer_id)?;
        let cursor_row = model.cursor.row;
        let cursor_col = model.cursor.col;
        if cursor_row >= self.scroll_row && cursor_row < self.scroll_row + area.height as usize {
            if cursor_col >= self.scroll_col
                && cursor_col < self.scroll_col + (area.width.saturating_sub(margin)) as usize
            {
                let x = area.x + margin + (cursor_col - self.scroll_col) as u16;
                let y = area.y + (cursor_row - self.scroll_row) as u16;
                return Some((x, y));
            }
        }
        None
    }
}
