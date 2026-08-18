pub mod buffer;
pub mod crossterm;

pub use self::buffer::{Cell, ScreenBuffer};
pub use self::crossterm::CrosstermRenderer;

use crate::rect::Rect;
use crate::types::Color;
use unicode_width::UnicodeWidthChar;

pub trait Renderer {
    fn move_to(&mut self, x: u16, y: u16) -> std::io::Result<()>;
    fn print(&mut self, text: &str) -> std::io::Result<()>;
    fn set_fg(&mut self, color: Color) -> std::io::Result<()>;
    fn set_bg(&mut self, color: Color) -> std::io::Result<()>;
    fn reset_colors(&mut self) -> std::io::Result<()>;

    fn set_style(&mut self, style: crate::colorscheme::Style) -> std::io::Result<()> {
        self.set_fg(style.fg.unwrap_or(Color::Reset))?;
        self.set_bg(style.bg.unwrap_or(Color::Reset))
    }

    fn show_cursor(
        &mut self,
        x: u16,
        y: u16,
        _shape: crate::model::CursorShape,
    ) -> std::io::Result<()> {
        self.move_to(x, y)
    }

    fn hide_cursor(&mut self) -> std::io::Result<()> {
        Ok(())
    }

    fn draw_window_frame(
        &mut self,
        rect: Rect,
        title: Option<&str>,
        style: crate::colorscheme::Style,
    ) -> std::io::Result<()> {
        self.set_style(style)?;
        self.draw_rect(rect)?;
        if let Some(title) = title.filter(|title| !title.is_empty()) {
            let max_width = rect.width.saturating_sub(4) as usize;
            if max_width > 0 {
                let title: String = title.chars().take(max_width).collect();
                let title_width = title.chars().count();
                if title_width + 4 < rect.width as usize {
                    let x = rect.x + 1 + ((rect.width as usize - title_width - 4) / 2) as u16;
                    self.move_to(x, rect.y)?;
                    self.print(&format!(" {title} "))?;
                }
            }
        }
        self.reset_colors()
    }

    fn draw_rect(&mut self, rect: Rect) -> std::io::Result<()> {
        self.move_to(rect.x, rect.y)?;
        self.print("┌")?;
        if rect.width > 2 {
            self.print(&"─".repeat(rect.width as usize - 2))?;
        }
        if rect.width > 1 {
            self.print("┐")?;
        }

        for y in 1..rect.height.saturating_sub(1) {
            self.move_to(rect.x, rect.y + y)?;
            self.print("│")?;
            if rect.width > 1 {
                self.move_to(rect.x + rect.width - 1, rect.y + y)?;
                self.print("│")?;
            }
        }

        if rect.height > 1 {
            self.move_to(rect.x, rect.y + rect.height - 1)?;
            self.print("└")?;
            if rect.width > 2 {
                self.print(&"─".repeat(rect.width as usize - 2))?;
            }
            if rect.width > 1 {
                self.print("┘")?;
            }
        }
        Ok(())
    }
}

pub struct BufferedRenderer {
    pub current: ScreenBuffer,
    pub last: ScreenBuffer,
    cursor_x: u16,
    cursor_y: u16,
    final_cursor_x: u16,
    final_cursor_y: u16,
    cursor_visible: bool,
    cursor_shape: crate::model::CursorShape,
    current_fg: Color,
    current_bg: Color,
}

fn cell_width(symbol: char) -> u16 {
    symbol.width().unwrap_or(1).max(1) as u16
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
            cursor_visible: false,
            cursor_shape: crate::model::CursorShape::Block,
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
            cursor::{Hide, SetCursorStyle, Show},
            execute, queue,
            style::{Print, ResetColor, SetBackgroundColor, SetForegroundColor},
            terminal::{Clear, ClearType},
        };

        queue!(writer, Hide)?;

        // ScreenBuffer columns are logical character slots, while the terminal
        // cursor uses display columns. Find the first changed display column for
        // each row, then clear and repaint only that suffix. Besides avoiding a
        // visible full-screen erase on every redraw, using the old and new widths
        // ensures deleting or replacing a wide character clears its tail.
        for y in 0..self.current.height {
            let mut current_x = 0u16;
            let mut last_x = 0u16;
            let mut first_changed = None;

            for x in 0..self.current.width {
                let current_cell = self.current.get_cell(x, y).unwrap();
                let last_cell = self.last.get_cell(x, y).unwrap();
                if current_cell != last_cell {
                    first_changed = Some(current_x.min(last_x));
                    break;
                }
                current_x = current_x.saturating_add(cell_width(current_cell.symbol));
                last_x = last_x.saturating_add(cell_width(last_cell.symbol));
            }

            let Some(first_changed) = first_changed else {
                continue;
            };

            queue!(
                writer,
                MoveTo(first_changed, y),
                Clear(ClearType::UntilNewLine),
                ResetColor
            )?;

            let mut display_x = 0u16;
            for x in 0..self.current.width {
                let current_cell = self.current.get_cell(x, y).unwrap();
                let width = cell_width(current_cell.symbol);
                if display_x >= self.current.width {
                    break;
                }
                if display_x >= first_changed {
                    queue!(writer, MoveTo(display_x, y))?;
                    queue!(writer, SetForegroundColor(current_cell.fg.into()))?;
                    queue!(writer, SetBackgroundColor(current_cell.bg.into()))?;
                    queue!(writer, Print(current_cell.symbol))?;
                }
                display_x = display_x.saturating_add(width);
            }
        }

        let cursor_shape = match self.cursor_shape {
            crate::model::CursorShape::Block => SetCursorStyle::SteadyBlock,
            crate::model::CursorShape::Bar => SetCursorStyle::SteadyBar,
            crate::model::CursorShape::Underline => SetCursorStyle::SteadyUnderScore,
            crate::model::CursorShape::BlinkingBlock => SetCursorStyle::BlinkingBlock,
            crate::model::CursorShape::BlinkingBar => SetCursorStyle::BlinkingBar,
            crate::model::CursorShape::BlinkingUnderline => SetCursorStyle::BlinkingUnderScore,
        };
        execute!(
            writer,
            ResetColor,
            MoveTo(self.final_cursor_x, self.final_cursor_y),
            cursor_shape
        )?;
        if self.cursor_visible {
            execute!(writer, Show)?;
        } else {
            execute!(writer, Hide)?;
        }

        // Swap buffers
        std::mem::swap(&mut self.current, &mut self.last);
        self.current.clear();

        Ok(())
    }
}

impl Renderer for BufferedRenderer {
    fn move_to(&mut self, x: u16, y: u16) -> std::io::Result<()> {
        self.cursor_x = x;
        self.cursor_y = y;
        self.final_cursor_x = x;
        self.final_cursor_y = y;
        Ok(())
    }

    fn print(&mut self, text: &str) -> std::io::Result<()> {
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
        Ok(())
    }

    fn show_cursor(
        &mut self,
        x: u16,
        y: u16,
        shape: crate::model::CursorShape,
    ) -> std::io::Result<()> {
        self.final_cursor_x = x;
        self.final_cursor_y = y;
        self.cursor_shape = shape;
        self.cursor_visible = true;
        Ok(())
    }

    fn hide_cursor(&mut self) -> std::io::Result<()> {
        self.cursor_visible = false;
        Ok(())
    }

    fn set_fg(&mut self, color: Color) -> std::io::Result<()> {
        self.current_fg = color;
        Ok(())
    }

    fn set_bg(&mut self, color: Color) -> std::io::Result<()> {
        self.current_bg = color;
        Ok(())
    }

    fn reset_colors(&mut self) -> std::io::Result<()> {
        self.current_fg = Color::Reset;
        self.current_bg = Color::Reset;
        Ok(())
    }
}
