use crossterm::event::{Event, KeyCode as CKey, KeyEvent, KeyEventKind, KeyModifiers as CMod};
use vim_input::{Action, Key, KeyCode, Keymap, Mode, Modifiers, ResolveOutcome, Resolver};

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
    pub fn feed_event(&mut self, event: Event) -> Option<ControllerAction> {
        match event {
            Event::Key(key_event) => {
                if key_event.kind != KeyEventKind::Release {
                    let vim_key = translate_key(key_event)?;
                    match self.resolver.feed(vim_key, &self.keymap) {
                        ResolveOutcome::Resolved(resolved) => {
                            self.pending_display.clear();
                            Some(ControllerAction::Execute {
                                action: resolved.action,
                                register: resolved.register,
                            })
                        }
                        ResolveOutcome::Pending => {
                            self.pending_display = self.resolver.pending().to_string();
                            Some(ControllerAction::Pending(self.pending_display.clone()))
                        }
                        ResolveOutcome::Invalid(_) => {
                            self.pending_display.clear();
                            Some(ControllerAction::Invalid)
                        }
                        ResolveOutcome::Ignored => None,
                    }
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

/// Result of feeding a key to the controller.
#[derive(Debug, PartialEq, Eq)]
pub enum ControllerAction {
    /// A resolved action that should be executed.
    Execute {
        action: Action,
        register: Option<char>,
    },
    /// Input is pending (e.g., operator or count prefix).
    Pending(String),
    /// Invalid sequence was consumed.
    Invalid,
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
        assert_eq!(
            controller.feed_event(event_v),
            Some(ControllerAction::Execute {
                action: Action::SetToVisualBlock,
                register: None,
            })
        );

        controller.set_mode(Mode::Normal);
        let event_w = Event::Key(KeyEvent::new(CKey::Char('w'), control));
        assert_eq!(
            controller.feed_event(event_w),
            Some(ControllerAction::Pending("<C-w>".to_string()))
        );

        let event_v2 = Event::Key(KeyEvent::new(CKey::Char('v'), control));
        assert_eq!(
            controller.feed_event(event_v2),
            Some(ControllerAction::Execute {
                action: Action::SplitVertical { file_path: None },
                register: None,
            })
        );
    }
}
