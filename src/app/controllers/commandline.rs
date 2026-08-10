use vim_ui::{Controller, UIContext, UiEvent, EventResult};

pub struct CommandLineController;

impl CommandLineController {
    pub fn new() -> Self {
        Self
    }
}

impl Controller for CommandLineController {
    fn handle_event(&mut self, _event: &UiEvent, _context: &mut dyn UIContext) -> EventResult {
        EventResult::Ignored
    }
}
