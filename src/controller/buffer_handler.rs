use vim_input::Action;
use vim_ui::WindowId;

use crate::model::EditorModel;

use super::command::CommandOutcome;

pub struct BufferHandler;

impl BufferHandler {
    pub fn handles(action: &Action) -> bool {
        matches!(action, Action::NextTab { .. } | Action::PreviousTab { .. })
    }

    pub fn execute(
        model: &mut EditorModel,
        active_window: WindowId,
        action: &Action,
    ) -> CommandOutcome {
        match action {
            Action::NextTab { .. } => {
                model.switch_next_buffer(active_window);
            }
            Action::PreviousTab { .. } => {
                model.switch_previous_buffer(active_window);
            }
            _ => return CommandOutcome::default(),
        }
        CommandOutcome::redraw()
    }
}
