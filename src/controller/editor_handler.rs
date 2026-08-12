use vim_input::Action;
use vim_ui::WindowId;

use crate::app::{editor::Editor, input::InputController, services::Services};
use crate::model::EditorModel;

use super::command::CommandOutcome;

pub struct EditorHandler;

impl EditorHandler {
    pub fn execute(
        model: &mut EditorModel,
        input: &mut InputController,
        editor: &Editor,
        services: &mut Services,
        active_window: WindowId,
        action: &Action,
    ) -> CommandOutcome {
        let Some(buffer_id) = model.window_buffer(active_window) else {
            return CommandOutcome::redraw();
        };

        let mut next_mode = None;
        let _ = model.with_mut(buffer_id, active_window, |buffer, context, window_state| {
            if let Ok(mode) = editor.execute(
                input.mode(),
                action,
                buffer,
                context,
                window_state,
                services,
            ) {
                next_mode = mode;
            }
        });
        if let Some(mode) = next_mode {
            input.set_mode(mode);
        }
        CommandOutcome::redraw()
    }
}
