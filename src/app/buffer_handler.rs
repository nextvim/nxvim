use vim_input::Action;
use vim_ui::{Ui, WindowId};

use crate::model::EditorModel;

use crate::app::outcome::CommandOutcome;

pub struct BufferHandler;

impl BufferHandler {
    pub fn handles(action: &Action) -> bool {
        matches!(action, Action::NextTab { .. } | Action::PreviousTab { .. })
    }

    pub fn execute(
        ui: &mut Ui,
        model: &mut EditorModel,
        active_window: WindowId,
        action: &Action,
    ) -> CommandOutcome {
        match action {
            Action::NextTab { count } => crate::app::operations::SharedOperations::switch_buffer(
                ui,
                model,
                active_window,
                true,
                *count as usize,
            ),
            Action::PreviousTab { count } => {
                crate::app::operations::SharedOperations::switch_buffer(
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
