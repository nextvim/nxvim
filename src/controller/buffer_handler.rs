use vim_input::Action;
use vim_ui::{Ui, WindowId};

use crate::model::EditorModel;

use super::command::CommandOutcome;

pub struct BufferHandler;

impl BufferHandler {
    pub fn handles(action: &Action) -> bool {
        matches!(action, Action::NextTab { .. } | Action::PreviousTab { .. })
    }

    pub fn execute(
        ui: &mut Ui,
        model: &EditorModel,
        active_window: WindowId,
        action: &Action,
    ) -> CommandOutcome {
        match action {
            Action::NextTab { count } => super::shared_operations::SharedOperations::switch_buffer(
                ui,
                model,
                active_window,
                true,
                *count as usize,
            ),
            Action::PreviousTab { count } => {
                super::shared_operations::SharedOperations::switch_buffer(
                    ui,
                    model,
                    active_window,
                    false,
                    *count as usize,
                )
            }
            _ => CommandOutcome::default(),
        }
    }
}
