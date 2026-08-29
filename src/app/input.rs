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
use std::collections::VecDeque;
use vim_input::{
    Key, KeyCode, Keymap, Mode, Modifiers, ResolveOutcome, ResolvedAction, Resolver,
    SharedMappingStore,
};

pub struct InputTranslator {
    resolver: Resolver,
    keymap: Keymap,
    mappings: Option<SharedMappingStore>,
    mapped_keys: VecDeque<(Key, bool)>,
}

impl InputTranslator {
    pub fn new() -> Self {
        Self {
            resolver: Resolver::new(Mode::Normal),
            keymap: Keymap::vim_defaults(),
            mappings: None,
            mapped_keys: VecDeque::new(),
        }
    }

    pub fn with_mappings(mappings: SharedMappingStore) -> Self {
        Self {
            resolver: Resolver::new(Mode::Normal),
            keymap: Keymap::vim_defaults(),
            mappings: Some(mappings),
            mapped_keys: VecDeque::new(),
        }
    }

    /// Translates one terminal event into a resolved action.
    ///
    /// Returns `None` for events that don't produce a complete action
    /// (partial key sequences, unmapped keys, ignored event kinds) — the
    /// caller has nothing to execute yet.
    pub fn translate(&mut self, event: Event) -> Option<ResolvedAction> {
        self.translate_with_buffer(event, None)
    }

    pub fn translate_with_buffer(
        &mut self,
        event: Event,
        current_buffer: Option<u64>,
    ) -> Option<ResolvedAction> {
        match event {
            Event::Key(key_event) if key_event.kind != KeyEventKind::Release => {
                let key = translate_key(key_event)?;
                let mut action = self.feed_key_with_buffer(key, current_buffer);
                while action.is_none() && !self.mapped_keys.is_empty() {
                    if let Some((k, allow)) = self.mapped_keys.pop_front() {
                        let outcome = if allow {
                            match &self.mappings {
                                Some(m) => self.resolver.feed_with_mappings(
                                    k,
                                    &self.keymap,
                                    m.clone(),
                                    current_buffer,
                                ),
                                None => self.resolver.feed(k, &self.keymap),
                            }
                        } else {
                            self.resolver.feed(k, &self.keymap)
                        };
                        action = self.handle_outcome(outcome, current_buffer);
                    }
                }
                action
            }
            _ => None,
        }
    }

    fn feed_key_with_buffer(&mut self, key: Key, buffer: Option<u64>) -> Option<ResolvedAction> {
        let outcome = match &self.mappings {
            Some(mappings) => {
                self.resolver
                    .feed_with_mappings(key, &self.keymap, mappings.clone(), buffer)
            }
            None => self.resolver.feed(key, &self.keymap),
        };
        self.handle_outcome(outcome, buffer)
    }

    fn handle_outcome(
        &mut self,
        outcome: ResolveOutcome,
        _buffer: Option<u64>,
    ) -> Option<ResolvedAction> {
        match outcome {
            ResolveOutcome::Resolved(resolved) => Some(resolved),
            ResolveOutcome::Mapping(mapping) => match mapping.expansion {
                vim_input::MappingExpansion::NoOp => Some(ResolvedAction {
                    action: vim_input::Action::NoOp,
                    register: None,
                }),
                vim_input::MappingExpansion::Keys(keys) => {
                    if let Ok(sequence) = vim_input::KeySequence::parse(&keys) {
                        let mut exact = VecDeque::new();
                        for pattern in sequence.items {
                            if let vim_input::KeyPattern::Exact(k) = pattern {
                                exact.push_back(k);
                            } else {
                                return None;
                            }
                        }
                        let allow_mappings = !mapping.flags.non_recursive;
                        self.mapped_keys
                            .extend(exact.into_iter().map(|k| (k, allow_mappings)));
                    }
                    None
                }
                _ => None,
            },
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RawKey {
    Char(char),
    Backspace,
    Enter,
    Escape,
}

/// Helper for raw-key bypass in Command mode.
pub fn translate_raw(event: &Event) -> Option<RawKey> {
    match event {
        Event::Key(key_event) if key_event.kind != KeyEventKind::Release => match key_event.code {
            CKey::Char(ch) => Some(RawKey::Char(ch)),
            CKey::Backspace => Some(RawKey::Backspace),
            CKey::Enter => Some(RawKey::Enter),
            CKey::Esc => Some(RawKey::Escape),
            _ => None,
        },
        _ => None,
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
