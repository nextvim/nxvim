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
        match model.save_window(active_window, path, force) {
            Ok(saved) => {
                model.status = Some(format!(
                    "\"{}\" {} bytes written",
                    saved.path.display(),
                    saved.bytes_written
                ));
            }
            Err(error) => {
                model.status = Some(format!("Save failed: {error}"));
            }
        }
        CommandOutcome::redraw()
    }
}
