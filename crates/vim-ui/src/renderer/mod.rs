pub mod buffer;
pub mod crossterm;

pub use self::buffer::{Cell, ScreenBuffer};
pub use self::crossterm::CrosstermRenderer;

use crate::rect::Rect;
use crate::types::Color;

pub trait Renderer {
    fn move_to(&mut self, x: u16, y: u16);
    fn print(&mut self, text: &str);
    fn set_fg(&mut self, color: Color);
    fn set_bg(&mut self, color: Color);
    fn reset_colors(&mut self);

    fn draw_rect(&mut self, rect: Rect) {
        self.move_to(rect.x, rect.y);
        self.print("┌");
        if rect.width > 2 {
            self.print(&"─".repeat(rect.width as usize - 2));
        }
        if rect.width > 1 {
            self.print("┐");
        }

        for y in 1..rect.height.saturating_sub(1) {
            self.move_to(rect.x, rect.y + y);
            self.print("│");
            if rect.width > 1 {
                self.move_to(rect.x + rect.width - 1, rect.y + y);
                self.print("│");
            }
        }

        if rect.height > 1 {
            self.move_to(rect.x, rect.y + rect.height - 1);
            self.print("└");
            if rect.width > 2 {
                self.print(&"─".repeat(rect.width as usize - 2));
            }
            if rect.width > 1 {
                self.print("┘");
            }
        }
    }
}

pub struct BufferedRenderer {
    pub current: ScreenBuffer,
    pub last: ScreenBuffer,
    cursor_x: u16,
    cursor_y: u16,
    final_cursor_x: u16,
    final_cursor_y: u16,
    current_fg: Color,
    current_bg: Color,
}

impl BufferedRenderer {
    pub fn new(width: u16, height: u16) -> Self {
        let mut last = ScreenBuffer::new(width, height);
        for cell in last.cells.iter_mut() {
            cell.symbol = '\0';
        }
        Self {
            current: ScreenBuffer::new(width, height),
            last,
            cursor_x: 0,
            cursor_y: 0,
            final_cursor_x: 0,
            final_cursor_y: 0,
            current_fg: Color::Reset,
            current_bg: Color::Reset,
        }
    }

    pub fn resize(&mut self, width: u16, height: u16) {
        self.current.resize(width, height);
        self.last.resize(width, height);
        // Ensure last is different from current to force a redraw after resize
        for cell in self.last.cells.iter_mut() {
            cell.symbol = '\0'; // Null character will never match current's spaces
        }
    }

    pub fn flush<W: std::io::Write>(&mut self, writer: &mut W) -> std::io::Result<()> {
        use ::crossterm::{
            cursor::MoveTo,
            execute, queue,
            style::{Print, ResetColor, SetBackgroundColor, SetForegroundColor},
        };

        let mut last_fg = Color::Reset;
        let mut last_bg = Color::Reset;

        for y in 0..self.current.height {
            for x in 0..self.current.width {
                let current_cell = self.current.get_cell(x, y).unwrap();
                let last_cell = self.last.get_cell(x, y).unwrap();

                if current_cell != last_cell {
                    queue!(writer, MoveTo(x, y))?;

                    if current_cell.fg != last_fg {
                        queue!(writer, SetForegroundColor(current_cell.fg.into()))?;
                        last_fg = current_cell.fg;
                    }
                    if current_cell.bg != last_bg {
                        queue!(writer, SetBackgroundColor(current_cell.bg.into()))?;
                        last_bg = current_cell.bg;
                    }

                    queue!(writer, Print(current_cell.symbol))?;
                }
            }
        }

        execute!(
            writer,
            ResetColor,
            MoveTo(self.final_cursor_x, self.final_cursor_y)
        )?;

        // Swap buffers
        std::mem::swap(&mut self.current, &mut self.last);
        self.current.clear();

        Ok(())
    }
}

impl Renderer for BufferedRenderer {
    fn move_to(&mut self, x: u16, y: u16) {
        self.cursor_x = x;
        self.cursor_y = y;
        self.final_cursor_x = x;
        self.final_cursor_y = y;
    }

    fn print(&mut self, text: &str) {
        for c in text.chars() {
            self.current.set_cell(
                self.cursor_x,
                self.cursor_y,
                Cell {
                    symbol: c,
                    fg: self.current_fg,
                    bg: self.current_bg,
                },
            );
            self.cursor_x += 1;
        }
    }

    fn set_fg(&mut self, color: Color) {
        self.current_fg = color;
    }

    fn set_bg(&mut self, color: Color) {
        self.current_bg = color;
    }

    fn reset_colors(&mut self) {
        self.current_fg = Color::Reset;
        self.current_bg = Color::Reset;
    }
}
