use crate::controller::actions::Action;
use std::collections::HashMap;

pub struct MacroRecorder {
    macros: HashMap<String, Vec<Action>>,
    current_register: Option<String>,
    current_recording: Vec<Action>,
}

impl MacroRecorder {
    pub fn new() -> Self {
        Self {
            macros: HashMap::new(),
            current_register: None,
            current_recording: Vec::new(),
        }
    }

    pub fn begin(&mut self, register: String) {
        self.current_register = Some(register);
        self.current_recording.clear();
    }

    pub fn end(&mut self) {
        if let Some(register) = self.current_register.take() {
            self.macros.insert(register, self.current_recording.clone());
        }
    }

    pub fn update(&mut self, action: &Action) {
        if self.current_register.is_some() {
            self.current_recording.push(action.clone());
        }
    }

    pub fn get(&self, register: &str) -> Option<&Vec<Action>> {
        self.macros.get(register)
    }

    pub fn is_recording(&self) -> bool {
        self.current_register.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_macro_recording() {
        let mut recorder = MacroRecorder::new();
        assert!(!recorder.is_recording());

        recorder.begin("a".to_string());
        assert!(recorder.is_recording());

        let action1 = Action::MoveLeft {
            count: 1,
            select: false,
        };
        let action2 = Action::MoveRight {
            count: 2,
            select: false,
        };

        recorder.update(&action1);
        recorder.update(&action2);

        recorder.end();
        assert!(!recorder.is_recording());

        let recorded = recorder.get("a").expect("macro 'a' should exist");
        assert_eq!(recorded.len(), 2);
        assert_eq!(recorded[0], action1);
        assert_eq!(recorded[1], action2);
    }
}
