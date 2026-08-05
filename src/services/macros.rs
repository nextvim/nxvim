use std::collections::HashMap;

use vim_input::Action;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedAction {
    pub action: Action,
    pub register: Option<char>,
}

/// Stores editor actions recorded into Vim macro registers.
#[derive(Debug, Default)]
pub struct MacroRecorder {
    macros: HashMap<String, Vec<RecordedAction>>,
    current_register: Option<String>,
    current_recording: Vec<RecordedAction>,
}

impl MacroRecorder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn begin(&mut self, register: impl Into<String>) {
        let register = register.into();
        let append = register.chars().next().is_some_and(char::is_uppercase);
        let register = normalize_register(&register);
        self.current_recording = if append {
            self.macros.get(&register).cloned().unwrap_or_default()
        } else {
            Vec::new()
        };
        self.current_register = Some(register);
    }

    pub fn end(&mut self) {
        if let Some(register) = self.current_register.take() {
            self.macros
                .insert(register, std::mem::take(&mut self.current_recording));
        }
    }

    pub fn record(&mut self, action: Action, register: Option<char>) {
        if self.is_recording() {
            self.current_recording
                .push(RecordedAction { action, register });
        }
    }

    pub fn get(&self, register: &str) -> Option<&[RecordedAction]> {
        self.macros
            .get(&normalize_register(register))
            .map(Vec::as_slice)
    }

    pub fn replay(&self, register: &str, count: u32) -> Vec<RecordedAction> {
        let Some(actions) = self.get(register) else {
            return Vec::new();
        };
        let capacity = actions.len().saturating_mul(count as usize);
        let mut replay = Vec::with_capacity(capacity);
        for _ in 0..count {
            replay.extend_from_slice(actions);
        }
        replay
    }

    pub fn is_recording(&self) -> bool {
        self.current_register.is_some()
    }

    pub fn current_register(&self) -> Option<&str> {
        self.current_register.as_deref()
    }
}

fn normalize_register(register: &str) -> String {
    register.to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn move_left() -> Action {
        Action::MoveLeft {
            count: 1,
            select: false,
        }
    }

    #[test]
    fn records_and_replays_actions() {
        let mut recorder = MacroRecorder::new();
        recorder.begin("a");
        recorder.record(move_left(), None);
        recorder.record(Action::Put { count: 1 }, Some('b'));
        recorder.end();

        let replay = recorder.replay("a", 2);
        assert_eq!(replay.len(), 4);
        assert_eq!(replay[0].action, move_left());
        assert_eq!(replay[1].register, Some('b'));
        assert_eq!(replay[2..], replay[..2]);
    }

    #[test]
    fn uppercase_register_appends_to_an_existing_macro() {
        let mut recorder = MacroRecorder::new();
        recorder.begin("a");
        recorder.record(move_left(), None);
        recorder.end();
        recorder.begin("A");
        recorder.record(
            Action::MoveRight {
                count: 1,
                select: false,
            },
            None,
        );
        recorder.end();

        assert_eq!(recorder.get("a").unwrap().len(), 2);
    }

    #[test]
    fn ending_replaces_the_register_and_resets_recording() {
        let mut recorder = MacroRecorder::new();
        recorder.begin("A");
        recorder.record(move_left(), None);
        recorder.end();
        assert!(!recorder.is_recording());

        recorder.begin("a");
        recorder.end();
        assert!(recorder.get("a").unwrap().is_empty());
    }
}
