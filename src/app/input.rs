//! Terminal keys -> `vim_input::Action` translation. Infra only — no kernel
//! semantics live here; this module only ever *describes* what a key means,
//! it never applies it.
//!
//! Reuses `vim_input::Resolver`/`Keymap` (already a complete, tested key
//! decoder) rather than hand-rolling a second one restricted to this
//! milestone's `h`/`j`/`k`/`l`/`i`/`Esc` subset — the kernel only knows how
//! to handle that subset today, but nothing here needs to change when later
//! milestones teach it more.

use crossterm::event::{Event, KeyCode as CKey, KeyEvent, KeyEventKind, KeyModifiers as CMod};
use vim_input::{Key, KeyCode, Keymap, Mode, Modifiers, ResolveOutcome, ResolvedAction, Resolver};

pub struct InputTranslator {
    resolver: Resolver,
    keymap: Keymap,
}

impl InputTranslator {
    pub fn new() -> Self {
        Self {
            resolver: Resolver::new(Mode::Normal),
            keymap: Keymap::vim_defaults(),
        }
    }

    /// Translates one terminal event into a resolved action.
    ///
    /// Returns `None` for events that don't produce a complete action
    /// (partial key sequences, unmapped keys, ignored event kinds) — the
    /// caller has nothing to execute yet.
    pub fn translate(&mut self, event: Event) -> Option<ResolvedAction> {
        match event {
            Event::Key(key_event) if key_event.kind != KeyEventKind::Release => {
                let key = translate_key(key_event)?;
                match self.resolver.feed(key, &self.keymap) {
                    ResolveOutcome::Resolved(resolved) => Some(resolved),
                    _ => None,
                }
            }
            _ => None,
        }
    }
}

/// Ported from `src_/app/input.rs::translate_key`.
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

    fn key_event(ch: char) -> Event {
        Event::Key(KeyEvent::new(CKey::Char(ch), CMod::NONE))
    }

    #[test]
    fn h_j_k_l_translate_to_move_actions() {
        let mut input = InputTranslator::new();
        let resolved = input.translate(key_event('l')).expect("l should resolve");
        assert_eq!(
            resolved.action,
            vim_input::Action::MoveRight {
                count: 1,
                select: false
            }
        );

        let resolved = input.translate(key_event('h')).expect("h should resolve");
        assert_eq!(
            resolved.action,
            vim_input::Action::MoveLeft {
                count: 1,
                select: false
            }
        );

        let resolved = input.translate(key_event('j')).expect("j should resolve");
        assert_eq!(
            resolved.action,
            vim_input::Action::MoveDown {
                count: 1,
                select: false
            }
        );

        let resolved = input.translate(key_event('k')).expect("k should resolve");
        assert_eq!(
            resolved.action,
            vim_input::Action::MoveUp {
                count: 1,
                select: false
            }
        );
    }
}
