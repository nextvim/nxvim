use vim_ui::{Controller, UIContext, UiEvent, EventResult};

pub struct StatusLineController;

impl StatusLineController {
    pub fn new() -> Self {
        Self
    }
}

impl Controller for StatusLineController {
    fn handle_event(&mut self, _event: &UiEvent, _context: &mut dyn UIContext) -> EventResult {
        EventResult::Ignored
    }
}
