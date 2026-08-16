use std::io::{self, stdout};

use crossterm::{
    cursor::Show,
    event::{DisableBracketedPaste, EnableBracketedPaste},
    execute,
    terminal::{
        EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode, size,
    },
};
use vim_ui::Rect;

/// Restores raw mode and the alternate screen on normal return, errors, and panics.
pub struct TerminalSession {
    restored: bool,
}

impl TerminalSession {
    pub fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        if let Err(error) = execute!(stdout(), EnterAlternateScreen, Show, EnableBracketedPaste) {
            let _ = disable_raw_mode();
            return Err(error);
        }
        Ok(Self { restored: false })
    }

    pub fn size(&self) -> io::Result<Rect> {
        let (columns, rows) = size()?;
        Ok(Rect::new(0, 0, columns, rows))
    }

    pub fn restore(&mut self) -> io::Result<()> {
        if self.restored {
            return Ok(());
        }
        let screen_result = execute!(stdout(), Show, LeaveAlternateScreen, DisableBracketedPaste);
        let raw_result = disable_raw_mode();
        self.restored = true;
        screen_result.and(raw_result)
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}
