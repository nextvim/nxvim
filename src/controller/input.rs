use crossterm::event::{Event, KeyCode as CKey, KeyEvent, KeyEventKind, KeyModifiers as CMod};
use vim_input::{Key, KeyCode, Keymap, Mode, Modifiers, ResolveOutcome, Resolver};

use super::Command;

/// Application-level input controller that translates Crossterm events
/// into Vim actions using `vim_input::Resolver`.
pub struct InputController {
    resolver: Resolver,
    keymap: Keymap,
    pending_display: String,
}

impl InputController {
    pub fn new(initial_mode: Mode) -> Self {
        Self {
            resolver: Resolver::new(initial_mode),
            keymap: Keymap::vim_defaults(),
            pending_display: String::new(),
        }
    }

    pub fn mode(&self) -> Mode {
        self.resolver.mode()
    }

    pub fn set_mode(&mut self, mode: Mode) {
        self.resolver.set_mode(mode);
        self.pending_display.clear();
    }

    /// Translate a Crossterm event to a `vim_input::Key` and feed it to the resolver.
    pub fn feed_event(&mut self, event: Event) -> Option<Command> {
        match event {
            Event::Key(key_event) => {
                if key_event.kind != KeyEventKind::Release {
                    let vim_key = translate_key(key_event)?;
                    match self.resolver.feed(vim_key, &self.keymap) {
                        ResolveOutcome::Resolved(resolved) => {
                            self.pending_display.clear();
                            Some(Command::Editor {
                                action: resolved.action,
                                register: resolved.register,
                            })
                        }
                        ResolveOutcome::Pending => {
                            self.pending_display = self.resolver.pending().to_string();
                            Some(Command::PendingInput(self.pending_display.clone()))
                        }
                        ResolveOutcome::Invalid(_) => {
                            self.pending_display.clear();
                            Some(Command::InvalidInput)
                        }
                        ResolveOutcome::Ignored => None,
                    }
                } else {
                    None
                }
            }
            Event::Paste(text) => Some(Command::Editor {
                action: vim_input::Action::InsertText(text),
                register: None,
            }),
            _ => None,
        }
    }

    pub fn set_in_recording(&mut self, in_recording: bool) {
        self.resolver.set_in_recording(in_recording);
    }

    pub fn in_recording(&self) -> bool {
        self.resolver.in_recording()
    }
}

/// Translate a Crossterm `KeyEvent` into a `vim_input::Key`.
fn translate_key(key: KeyEvent) -> Option<Key> {
    let code = match key.code {
        CKey::Char(ch) => KeyCode::Char(ch),
        CKey::Enter => KeyCode::Enter,
        CKey::Esc => KeyCode::Escape,
        CKey::Backspace => KeyCode::Backspace,
        CKey::Tab => KeyCode::Tab,
        CKey::BackTab => KeyCode::BackTab,
        CKey::Left => KeyCode::Left,
        CKey::Right => KeyCode::Right,
        CKey::Up => KeyCode::Up,
        CKey::Down => KeyCode::Down,
        CKey::Home => KeyCode::Home,
        CKey::End => KeyCode::End,
        CKey::PageUp => KeyCode::PageUp,
        CKey::PageDown => KeyCode::PageDown,
        CKey::Delete => KeyCode::Delete,
        CKey::Insert => KeyCode::Insert,
        CKey::F(n) => KeyCode::Function(n),
        _ => return None,
    };

    let mut modifiers = Modifiers::NONE;
    if key.modifiers.contains(CMod::SHIFT) {
        modifiers.insert(Modifiers::SHIFT);
    }
    if key.modifiers.contains(CMod::CONTROL) {
        modifiers.insert(Modifiers::CONTROL);
    }
    if key.modifiers.contains(CMod::ALT) {
        modifiers.insert(Modifiers::ALT);
    }

    Some(Key::new(code, modifiers))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_v_and_control_w_control_v_resolve_to_distinct_actions() {
        let control = CMod::CONTROL;
        let mut controller = InputController::new(Mode::Normal);

        let event_v = Event::Key(KeyEvent::new(CKey::Char('v'), control));
        assert!(matches!(
            controller.feed_event(event_v),
            Some(Command::Editor {
                action: vim_input::Action::SetToVisualBlock,
                register: None,
            })
        ));

        controller.set_mode(Mode::Normal);
        let event_w = Event::Key(KeyEvent::new(CKey::Char('w'), control));
        assert!(matches!(
            controller.feed_event(event_w),
            Some(Command::PendingInput(sequence)) if sequence == "<C-w>"
        ));

        let event_v2 = Event::Key(KeyEvent::new(CKey::Char('v'), control));
        assert!(matches!(
            controller.feed_event(event_v2),
            Some(Command::Editor {
                action: vim_input::Action::SplitVertical { file_path: None },
                register: None,
            })
        ));
    }

    #[test]
    fn test_paste_event_resolves_to_insert_text() {
        let mut controller = InputController::new(Mode::Normal);
        let event = Event::Paste("hello world".to_string());
        assert!(matches!(
            controller.feed_event(event),
            Some(Command::Editor {
                action: vim_input::Action::InsertText(text),
                register: None,
            }) if text == "hello world"
        ));
    }
}
