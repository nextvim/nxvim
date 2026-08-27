use crossterm::event::{Event, KeyCode as CKey, KeyEvent, KeyEventKind, KeyModifiers as CMod};
use std::collections::VecDeque;
use vim_input::{Key, KeyCode, Keymap, Mode, Modifiers, ResolveOutcome, Resolver};

use crate::app::command::{AppCommand, InputRequest, SemanticRequest};

/// Application-level input controller that translates Crossterm events
/// into Vim actions using `vim_input::Resolver`.
pub struct InputAdapter {
    resolver: Resolver,
    keymap: Keymap,
    pending_display: String,
    mappings: Option<vim_input::SharedMappingStore>,
    mapped_keys: VecDeque<(Key, bool)>,
}

impl InputAdapter {
    pub fn new(initial_mode: Mode) -> Self {
        Self {
            resolver: Resolver::new(initial_mode),
            keymap: Keymap::vim_defaults(),
            pending_display: String::new(),
            mappings: None,
            mapped_keys: VecDeque::new(),
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
    pub fn feed_event(&mut self, event: Event) -> Option<AppCommand> {
        self.feed_event_with_buffer(event, None)
    }

    pub fn feed_event_with_buffer(
        &mut self,
        event: Event,
        buffer: Option<u64>,
    ) -> Option<AppCommand> {
        match event {
            Event::Key(key_event) => {
                if key_event.kind != KeyEventKind::Release {
                    let vim_key = translate_key(key_event)?;
                    self.feed_key_with_buffer(vim_key, buffer)
                } else {
                    None
                }
            }
            Event::Paste(text) => Some(AppCommand::Semantic(SemanticRequest::Editor {
                action: vim_input::Action::InsertText(text),
                register: None,
            })),
            _ => None,
        }
    }

    pub fn set_mapping_store(&mut self, mappings: vim_input::SharedMappingStore) {
        self.mappings = Some(mappings);
    }

    pub fn feed_key(&mut self, key: Key) -> Option<AppCommand> {
        self.feed_key_with_buffer(key, None)
    }

    pub fn feed_key_with_buffer(&mut self, key: Key, buffer: Option<u64>) -> Option<AppCommand> {
        if let Some((mapped_key, allow_mappings)) = self.mapped_keys.pop_front() {
            self.mapped_keys.push_front((key, true));
            return if allow_mappings {
                self.feed_key_with_buffer(mapped_key, buffer)
            } else {
                self.feed_key_without_mappings(mapped_key)
            };
        }
        let outcome = match self.mappings.clone() {
            Some(mappings) => self
                .resolver
                .feed_with_mappings(key, &self.keymap, mappings, buffer),
            None => self.resolver.feed(key, &self.keymap),
        };
        self.handle_outcome(outcome)
    }

    fn feed_key_without_mappings(&mut self, key: Key) -> Option<AppCommand> {
        let outcome = self.resolver.feed(key, &self.keymap);
        self.handle_outcome(outcome)
    }

    fn handle_outcome(&mut self, outcome: ResolveOutcome) -> Option<AppCommand> {
        match outcome {
            ResolveOutcome::Resolved(resolved) => {
                self.pending_display.clear();
                Some(AppCommand::Semantic(SemanticRequest::Editor {
                    action: resolved.action,
                    register: resolved.register,
                }))
            }
            ResolveOutcome::Mapping(mapping) => {
                self.pending_display.clear();
                match mapping.expansion {
                    vim_input::MappingExpansion::NoOp => {
                        Some(AppCommand::Semantic(SemanticRequest::Editor {
                            action: vim_input::Action::NoOp,
                            register: None,
                        }))
                    }
                    vim_input::MappingExpansion::Script(script)
                    | vim_input::MappingExpansion::Expression(script) => Some(AppCommand::Script(
                        crate::app::command::ScriptRequest::Execute(script),
                    )),
                    vim_input::MappingExpansion::Keys(keys) => {
                        let sequence = vim_input::KeySequence::parse(&keys).ok()?;
                        let mut exact = VecDeque::new();
                        for pattern in sequence.items {
                            if let vim_input::KeyPattern::Exact(key) = pattern {
                                exact.push_back(key);
                            } else {
                                return None;
                            }
                        }
                        let allow_mappings = !mapping.flags.non_recursive;
                        self.mapped_keys
                            .extend(exact.into_iter().map(|key| (key, allow_mappings)));
                        self.mapped_keys
                            .pop_front()
                            .and_then(|(key, allow_mappings)| {
                                if allow_mappings {
                                    self.feed_key_with_buffer(key, None)
                                } else {
                                    self.feed_key_without_mappings(key)
                                }
                            })
                    }
                }
            }
            ResolveOutcome::Pending => {
                let pending =
                    crate::kernel::PendingCommandState::from_decoder(self.resolver.pending());
                self.pending_display.clone_from(&pending.display);
                Some(AppCommand::Input(InputRequest::Pending(pending)))
            }
            ResolveOutcome::Invalid(_) => {
                self.pending_display.clear();
                Some(AppCommand::Input(InputRequest::Invalid))
            }
            ResolveOutcome::Ignored => None,
        }
    }

    pub fn set_in_recording(&mut self, in_recording: bool) {
        self.resolver.set_in_recording(in_recording);
    }

    pub fn in_recording(&self) -> bool {
        self.resolver.in_recording()
    }
}

/// Converts a terminal event into a response to the active confirmation prompt.
pub fn prompt_choice(event: &crossterm::event::Event) -> Option<crate::app::prompt::PromptChoice> {
    use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
    let Event::Key(key) = event else {
        return None;
    };
    if key.kind == KeyEventKind::Release {
        return None;
    }
    match key.code {
        KeyCode::Char('y' | 'Y') => Some(crate::app::prompt::PromptChoice::Yes),
        KeyCode::Char('n' | 'N') => Some(crate::app::prompt::PromptChoice::No),
        KeyCode::Char('a' | 'A') => Some(crate::app::prompt::PromptChoice::All),
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(crate::app::prompt::PromptChoice::Quit)
        }
        KeyCode::Char('q' | 'Q') | KeyCode::Esc => Some(crate::app::prompt::PromptChoice::Quit),
        KeyCode::Char('l' | 'L') => Some(crate::app::prompt::PromptChoice::Last),
        _ => None,
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
        let mut controller = InputAdapter::new(Mode::Normal);

        let event_v = Event::Key(KeyEvent::new(CKey::Char('v'), control));
        assert!(matches!(
            controller.feed_event(event_v),
            Some(AppCommand::Semantic(SemanticRequest::Editor {
                action: vim_input::Action::SetToVisualBlock,
                register: None,
            }))
        ));

        controller.set_mode(Mode::Normal);
        let event_w = Event::Key(KeyEvent::new(CKey::Char('w'), control));
        assert!(matches!(
            controller.feed_event(event_w),
            Some(AppCommand::Input(InputRequest::Pending(state))) if state.display == "<C-w>"
        ));

        let event_v2 = Event::Key(KeyEvent::new(CKey::Char('v'), control));
        assert!(matches!(
            controller.feed_event(event_v2),
            Some(AppCommand::Semantic(SemanticRequest::Editor {
                action: vim_input::Action::SplitVertical { file_path: None },
                register: None,
            }))
        ));
    }

    #[test]
    fn shared_script_mapping_is_consumed_by_live_input() {
        let mappings =
            std::sync::Arc::new(std::sync::RwLock::new(vim_input::MappingStore::default()));
        mappings.write().unwrap().register(
            vim_input::Mapping::new(
                vim_input::MappingId(1),
                vec![vim_input::MappingMode::Normal],
                "<leader>w".into(),
                vim_input::MappingExpansion::Script(":write<CR>".into()),
                vim_input::MappingFlags::default(),
                vim_input::MappingScope::Global,
                vim_input::MappingOrigin::Script,
                vim_input::MappingScriptContext::default(),
            )
            .unwrap(),
        );
        mappings.write().unwrap().register(
            vim_input::Mapping::new(
                vim_input::MappingId(2),
                vec![vim_input::MappingMode::Normal],
                "<leader>w".into(),
                vim_input::MappingExpansion::Script(":local-write<CR>".into()),
                vim_input::MappingFlags::default(),
                vim_input::MappingScope::Buffer(7),
                vim_input::MappingOrigin::Script,
                vim_input::MappingScriptContext::default(),
            )
            .unwrap(),
        );
        let mut controller = InputAdapter::new(Mode::Normal);
        controller.set_mapping_store(mappings);
        assert!(matches!(
            controller.feed_key_with_buffer(Key::char('\\'), Some(7)),
            Some(AppCommand::Input(InputRequest::Pending(_)))
        ));
        assert!(matches!(
            controller.feed_key_with_buffer(Key::char('w'), Some(7)),
            Some(AppCommand::Script(crate::app::command::ScriptRequest::Execute(script)))
                            if script == ":local-write<CR>"
        ));
    }

    #[test]
    fn test_paste_event_resolves_to_insert_text() {
        let mut controller = InputAdapter::new(Mode::Normal);
        let event = Event::Paste("hello world".to_string());
        assert!(matches!(
            controller.feed_event(event),
            Some(AppCommand::Semantic(SemanticRequest::Editor {
                action: vim_input::Action::InsertText(text),
                register: None,
            })) if text == "hello world"
        ));
    }
}
