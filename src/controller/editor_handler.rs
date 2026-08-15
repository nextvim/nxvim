use vim_input::Action;
use vim_ui::{Ui, WindowId};

use crate::app::services::Services;
use crate::app::windows::WindowOps;
use crate::controller::input::InputController;
use crate::model::EditorModel;

use super::command::CommandOutcome;

pub struct EditorHandler;

impl EditorHandler {
    pub fn execute(
        ui: &mut Ui,
        model: &mut EditorModel,
        input: &mut InputController,
        services: &mut Services,
        active_window: WindowId,
        action: &Action,
    ) -> CommandOutcome {
        if WindowOps::window_buffer(ui, active_window).is_none() {
            return CommandOutcome::redraw();
        }

        let mut next_mode = None;
        let _ =
            WindowOps::edit_window(ui, model, active_window, |buffer, context, window_state| {
                if let Ok(mode) = super::editor::Editor::new().execute(
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
