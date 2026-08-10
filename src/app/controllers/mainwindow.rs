use vim_ui::{Controller, UIContext, UiEvent, EventResult};

pub struct MainWindowController;

impl MainWindowController {
    pub fn new() -> Self {
        Self
    }
}

impl Controller for MainWindowController {
    fn handle_event(&mut self, _event: &UiEvent, _context: &mut dyn UIContext) -> EventResult {
        EventResult::Ignored
    }
}
