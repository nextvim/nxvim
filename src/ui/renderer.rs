use std::io::Write;
use crossterm::{
    cursor::MoveTo,
    execute,
    style::{Color, Print, ResetColor, SetForegroundColor},
};
use super::layout::Rect;

pub struct Renderer;

impl Renderer {
    pub fn new() -> Self {
        Self
    }

    pub fn draw_border<W: Write>(
        &self,
        w: &mut W,
        rect: Rect,
        is_focused: bool,
        ui: &super::Ui,
    ) -> std::io::Result<()> {
        let border_fg = if is_focused {
            Color::Magenta
        } else {
            Color::DarkGrey
        };

        // Draw border
        execute!(w, SetForegroundColor(border_fg))?;

        // Draw top border
        execute!(w, MoveTo(rect.x, rect.y))?;
        if rect.width > 2 {
            execute!(
                w,
                Print(format!("┌{}┐", "─".repeat(rect.width as usize - 2)))
            )?;
        } else {
            execute!(
                w,
                Print("┌┐".chars().take(rect.width as usize).collect::<String>())
            )?;
        }

        // Draw sides
        for y in 1..rect.height.saturating_sub(1) {
            execute!(w, MoveTo(rect.x, rect.y + y))?;
            if rect.width > 1 {
                execute!(w, Print("│"))?;
                execute!(w, MoveTo(rect.x + rect.width - 1, rect.y + y))?;
                execute!(w, Print("│"))?;
            } else {
                execute!(w, Print("│"))?;
            }
        }

        // Draw bottom border
        if rect.height > 1 {
            execute!(w, MoveTo(rect.x, rect.y + rect.height - 1))?;
            if rect.width > 1 {
                execute!(
                    w,
                    Print(format!("└{}┘", "─".repeat(rect.width as usize - 2)))
                )?;
            } else {
                execute!(w, Print("└"))?;
            }
        }

        execute!(w, ResetColor)?;
        Ok(())
    }

    pub fn draw_title<W: Write>(
        &self,
        w: &mut W,
        rect: Rect,
        title: &str,
        is_focused: bool,
        ui: &super::Ui,
    ) -> std::io::Result<()> {
        let title_len = title.chars().count();
        if rect.width > 4 && title_len + 4 < rect.width as usize {
            let left_len = (rect.width as usize - title_len - 4) / 2;
            let x = rect.x + 1 + left_len as u16;
            let y = rect.y;
            let border_fg = if is_focused {
                Color::Magenta
            } else {
                Color::DarkGrey
            };
            execute!(
                w,
                SetForegroundColor(border_fg),
                MoveTo(x, y),
                Print(format!(" {} ", title)),
                ResetColor
            )?;
        }
        Ok(())
    }
}

pub struct Scrollbar {
    enabled: bool,
    x_pos: u16,
    y_pos: u16,
    height: u32,
    handle_y: u32,
    handle_h: u32,
    cursor_scrollbar_y: Option<u32>,
}

impl Scrollbar {
    pub fn new(
        enabled: bool,
        inner_rect: Rect,
        total_rows: u32,
        scroll_y: u32,
        cursor_row: Option<u32>,
    ) -> Self {
        let height = inner_rect.height as u32;
        let handle_h = if total_rows > 0 {
            ((height as f32 / total_rows as f32) * height as f32)
                .round()
                .max(1.0) as u32
        } else {
            height
        };
        let handle_h = handle_h.min(height);

        let start_y = if total_rows > height {
            ((scroll_y as f32 / (total_rows - height) as f32)
                * (height - handle_h) as f32)
                .round() as u32
        } else {
            0
        };

        let cursor_scrollbar_y = cursor_row.and_then(|row| {
            if total_rows > 0 {
                let y = ((row as f32 / total_rows as f32) * height as f32).floor() as u32;
                Some(y.min(height.saturating_sub(1)))
            } else {
                None
            }
        });

        Self {
            enabled,
            x_pos: inner_rect.x + inner_rect.width.saturating_sub(1),
            y_pos: inner_rect.y,
            height,
            handle_y: start_y,
            handle_h,
            cursor_scrollbar_y,
        }
    }

    pub fn is_scrollbar(&self, x: u16, _y: u16) -> bool {
        self.enabled && x == self.x_pos
    }

    pub fn is_handle(&self, x: u16, y: u16) -> bool {
        if !self.is_scrollbar(x, y) {
            return false;
        }
        let relative_y = y.saturating_sub(self.y_pos) as u32;
        relative_y >= self.handle_y && relative_y < self.handle_y + self.handle_h
    }

    pub fn is_cursor(&self, x: u16, y: u16) -> bool {
        if !self.is_scrollbar(x, y) {
            return false;
        }
        let relative_y = y.saturating_sub(self.y_pos) as u32;
        if let Some(cy) = self.cursor_scrollbar_y {
            relative_y == cy
        } else {
            false
        }
    }
}

