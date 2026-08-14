use vim_input::Action;
use vim_ui::{NavigationDirection, WindowId};

use super::command::CommandOutcome;

pub struct WindowHandler;

impl WindowHandler {
    pub fn handles(action: &Action) -> bool {
        matches!(
            action,
            Action::SplitHorizontal { .. }
                | Action::SplitVertical { .. }
                | Action::FocusLeftWindow
                | Action::FocusRightWindow
                | Action::FocusUpWindow
                | Action::FocusDownWindow
        )
    }

    pub fn execute(active_window: WindowId, action: &Action) -> CommandOutcome {
        match action {
            Action::SplitHorizontal { .. } => {
                super::shared_operations::SharedOperations::split_window(active_window, true)
            }
            Action::SplitVertical { .. } => {
                super::shared_operations::SharedOperations::split_window(active_window, false)
            }
            Action::FocusLeftWindow => {
                super::shared_operations::SharedOperations::focus_window(NavigationDirection::Left)
            }
            Action::FocusRightWindow => {
                super::shared_operations::SharedOperations::focus_window(NavigationDirection::Right)
            }
            Action::FocusUpWindow => {
                super::shared_operations::SharedOperations::focus_window(NavigationDirection::Up)
            }
            Action::FocusDownWindow => {
                super::shared_operations::SharedOperations::focus_window(NavigationDirection::Down)
            }
            _ => CommandOutcome::default(),
        }
    }
}
