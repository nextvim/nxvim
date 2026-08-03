use crate::controller::actions::{Action, Mode};
use crossterm::event::{KeyCode as CrosstermKeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use vim_input::{Key, KeyCode, Keymap, Modifiers, ResolveOutcome, Resolver};

pub struct VimInput {
    resolver: Resolver,
    keymap: Keymap,
    pub last_register: Option<char>,
    pub is_macro_recording: bool,
}

impl VimInput {
    pub fn new() -> Self {
        Self {
            resolver: Resolver::new(Mode::Normal),
            keymap: Keymap::vim_defaults(),
            last_register: None,
            is_macro_recording: false,
        }
    }

    pub fn mode(&self) -> Mode {
        self.resolver.mode()
    }

    pub fn set_mode(&mut self, mode: Mode) {
        if self.resolver.mode() != mode {
            self.resolver.set_mode(mode);
        }
    }

    pub fn is_busy(&self) -> bool {
        self.resolver.is_pending()
    }

    pub fn clear(&mut self) {
        self.resolver.reset();
        self.last_register = None;
    }

    pub fn handle_event(&mut self, key_event: &KeyEvent) -> Action {
        if key_event.kind == KeyEventKind::Release {
            return Action::NoOp;
        }

        let Some(key) = key_from_crossterm(key_event) else {
            self.last_register = None;
            return Action::NoOp;
        };

        match self.resolver.feed(key, &self.keymap) {
            ResolveOutcome::Resolved(resolved) => {
                self.last_register = resolved.register;
                resolved.action
            }
            ResolveOutcome::Pending | ResolveOutcome::Ignored | ResolveOutcome::Invalid(_) => {
                self.last_register = None;
                Action::NoOp
            }
        }
    }

    pub fn resolved_op(&self) -> Action {
        self.resolver
            .pending()
            .operator
            .cloned()
            .unwrap_or(Action::NoOp)
    }

    pub fn pending_keys_str(&self) -> String {
        self.resolver.pending().to_string()
    }
}

fn key_from_crossterm(event: &KeyEvent) -> Option<Key> {
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
