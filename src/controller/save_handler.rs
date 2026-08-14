use std::path::Path;

use vim_ui::WindowId;

use crate::model::EditorModel;

use super::command::CommandOutcome;

pub struct SaveHandler;

impl SaveHandler {
    pub fn execute(
        model: &mut EditorModel,
        active_window: WindowId,
        path: Option<&Path>,
        force: bool,
    ) -> CommandOutcome {
        super::shared_operations::SharedOperations::write(model, active_window, path, force)
    }
}
