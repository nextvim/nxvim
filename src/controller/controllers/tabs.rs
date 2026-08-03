use crate::controller::ControllerResult;
use crate::controller::ViewController;
use crate::controller::actions::Action;
use crate::editor::Editor;
use crate::ui::Ui;

pub struct TabsController {}

impl TabsController {
    pub fn new() -> Self {
        TabsController {}
    }
}

impl ViewController for TabsController {
    fn handle_action(
        &mut self,
        action: Action,
        editor: &mut Editor,
        buffer_manager: &mut crate::editor::buffers::BufferManager,
        ui: &mut crate::ui::Ui,
        window_id: usize,
    ) -> Result<ControllerResult, Box<dyn std::error::Error>> {
        Ok(ControllerResult::None)
    }
}

