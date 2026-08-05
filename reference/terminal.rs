use std::io::{self, stdout};

use crossterm::{
    cursor::Show,
    event::{self, Event, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{
        EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode, size,
    },
};
use vim_input::{Key, KeyCode, Modifiers};

use crate::{AppEvent, ScreenSize};

pub trait EventSource {
    fn next_event(&mut self) -> io::Result<AppEvent>;
}

#[derive(Debug, Default)]
pub struct CrosstermEventSource;

impl EventSource for CrosstermEventSource {
    fn next_event(&mut self) -> io::Result<AppEvent> {
        loop {
            match event::read()? {
                Event::Key(key)
                    if matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) =>
                {
                    if let Some(key) = translate_key(key) {
                        return Ok(AppEvent::Key(key));
                    }
                }
                Event::Resize(columns, rows) => {
                    return Ok(AppEvent::Resize(ScreenSize::new(columns, rows)));
                }
                _ => {}
            }
        }
    }
}

pub struct TerminalSession {
    restored: bool,
}

impl TerminalSession {
    pub fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        if let Err(error) = execute!(stdout(), EnterAlternateScreen, Show) {
            let _ = disable_raw_mode();
            return Err(error);
        }
        Ok(Self { restored: false })
    }

    pub fn size(&self) -> io::Result<ScreenSize> {
        let (columns, rows) = size()?;
        Ok(ScreenSize::new(columns, rows))
    }

    pub fn restore(&mut self) -> io::Result<()> {
        if self.restored {
            return Ok(());
        }

        let screen_result = execute!(stdout(), Show, LeaveAlternateScreen);
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

fn translate_key(key: KeyEvent) -> Option<Key> {
    let code = match key.code {
        event::KeyCode::Backspace => KeyCode::Backspace,
        event::KeyCode::Enter => KeyCode::Enter,
        event::KeyCode::Left => KeyCode::Left,
        event::KeyCode::Right => KeyCode::Right,
        event::KeyCode::Up => KeyCode::Up,
        event::KeyCode::Down => KeyCode::Down,
        event::KeyCode::Home => KeyCode::Home,
        event::KeyCode::End => KeyCode::End,
        event::KeyCode::PageUp => KeyCode::PageUp,
        event::KeyCode::PageDown => KeyCode::PageDown,
        event::KeyCode::Tab => KeyCode::Tab,
        event::KeyCode::BackTab => KeyCode::BackTab,
        event::KeyCode::Delete => KeyCode::Delete,
        event::KeyCode::Insert => KeyCode::Insert,
        event::KeyCode::F(number) => KeyCode::Function(number),
        event::KeyCode::Char(character) => KeyCode::Char(character),
        event::KeyCode::Esc => KeyCode::Escape,
        _ => return None,
    };

    let mut modifiers = Modifiers::NONE;
    if key.modifiers.contains(KeyModifiers::SHIFT) {
        modifiers.insert(Modifiers::SHIFT);
    }
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        modifiers.insert(Modifiers::CONTROL);
    }
    if key.modifiers.contains(KeyModifiers::ALT) {
        modifiers.insert(Modifiers::ALT);
    }
    if key.modifiers.contains(KeyModifiers::SUPER) {
        modifiers.insert(Modifiers::SUPER);
    }

    Some(Key::new(code, modifiers).normalized())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translates_control_character() {
        let event = KeyEvent::new(event::KeyCode::Char('c'), KeyModifiers::CONTROL);

        assert_eq!(
            translate_key(event),
            Some(Key::new(KeyCode::Char('c'), Modifiers::CONTROL))
        );
    }

    #[test]
    fn ignores_unsupported_terminal_keys() {
        let event = KeyEvent::new(event::KeyCode::Null, KeyModifiers::NONE);

        assert_eq!(translate_key(event), None);
    }
}
