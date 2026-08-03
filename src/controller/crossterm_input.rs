use crossterm::event::{KeyCode as CrosstermKeyCode, KeyEvent, KeyModifiers};
use vim_input::{Key, KeyCode, Modifiers};

pub(crate) fn key_from_crossterm(event: &KeyEvent) -> Option<Key> {
    let code = match event.code {
        CrosstermKeyCode::Backspace => KeyCode::Backspace,
        CrosstermKeyCode::Enter => KeyCode::Enter,
        CrosstermKeyCode::Left => KeyCode::Left,
        CrosstermKeyCode::Right => KeyCode::Right,
        CrosstermKeyCode::Up => KeyCode::Up,
        CrosstermKeyCode::Down => KeyCode::Down,
        CrosstermKeyCode::Home => KeyCode::Home,
        CrosstermKeyCode::End => KeyCode::End,
        CrosstermKeyCode::PageUp => KeyCode::PageUp,
        CrosstermKeyCode::PageDown => KeyCode::PageDown,
        CrosstermKeyCode::Tab => KeyCode::Tab,
        CrosstermKeyCode::BackTab => KeyCode::BackTab,
        CrosstermKeyCode::Delete => KeyCode::Delete,
        CrosstermKeyCode::Insert => KeyCode::Insert,
        CrosstermKeyCode::F(number) => KeyCode::Function(number),
        CrosstermKeyCode::Char(ch) => KeyCode::Char(ch),
        CrosstermKeyCode::Esc => KeyCode::Escape,
        _ => return None,
    };

    let mut modifiers = Modifiers::NONE;
    if event.modifiers.contains(KeyModifiers::SHIFT) {
        modifiers.insert(Modifiers::SHIFT);
    }
    if event.modifiers.contains(KeyModifiers::CONTROL) {
        modifiers.insert(Modifiers::CONTROL);
    }
    if event.modifiers.contains(KeyModifiers::ALT) {
        modifiers.insert(Modifiers::ALT);
    }
    if event.modifiers.contains(KeyModifiers::SUPER) {
        modifiers.insert(Modifiers::SUPER);
    }

    Some(Key::new(code, modifiers))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEventKind, KeyEventState};

    fn event(code: CrosstermKeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    #[test]
    fn converts_supported_crossterm_key_codes() {
        let cases = [
            (CrosstermKeyCode::Backspace, KeyCode::Backspace),
            (CrosstermKeyCode::Enter, KeyCode::Enter),
            (CrosstermKeyCode::Left, KeyCode::Left),
            (CrosstermKeyCode::Right, KeyCode::Right),
            (CrosstermKeyCode::Up, KeyCode::Up),
            (CrosstermKeyCode::Down, KeyCode::Down),
            (CrosstermKeyCode::Home, KeyCode::Home),
            (CrosstermKeyCode::End, KeyCode::End),
            (CrosstermKeyCode::PageUp, KeyCode::PageUp),
            (CrosstermKeyCode::PageDown, KeyCode::PageDown),
            (CrosstermKeyCode::Tab, KeyCode::Tab),
            (CrosstermKeyCode::BackTab, KeyCode::BackTab),
            (CrosstermKeyCode::Delete, KeyCode::Delete),
            (CrosstermKeyCode::Insert, KeyCode::Insert),
            (CrosstermKeyCode::F(12), KeyCode::Function(12)),
            (CrosstermKeyCode::Char('x'), KeyCode::Char('x')),
            (CrosstermKeyCode::Esc, KeyCode::Escape),
        ];

        for (source, expected) in cases {
            assert_eq!(
                key_from_crossterm(&event(source, KeyModifiers::NONE)),
                Some(Key::new(expected, Modifiers::NONE))
            );
        }
    }

    #[test]
    fn converts_all_supported_modifiers() {
        let source = event(
            CrosstermKeyCode::Char('x'),
            KeyModifiers::SHIFT | KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER,
        );
        let converted = key_from_crossterm(&source).unwrap();

        assert!(converted.modifiers.contains(Modifiers::SHIFT));
        assert!(converted.modifiers.contains(Modifiers::CONTROL));
        assert!(converted.modifiers.contains(Modifiers::ALT));
        assert!(converted.modifiers.contains(Modifiers::SUPER));
    }

    #[test]
    fn ignores_unsupported_crossterm_key_codes() {
        assert_eq!(
            key_from_crossterm(&event(CrosstermKeyCode::CapsLock, KeyModifiers::NONE)),
            None
        );
    }
}
