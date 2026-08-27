use vim_input::Action;
use vim_ui::{NavigationDirection, WindowId};

use crate::app::outcome::CommandOutcome;

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
                crate::app::operations::SharedOperations::split_window(active_window, true)
            }
            Action::SplitVertical { .. } => {
                crate::app::operations::SharedOperations::split_window(active_window, false)
            }
            Action::FocusLeftWindow => {
                crate::app::operations::SharedOperations::focus_window(NavigationDirection::Left)
            }
            Action::FocusRightWindow => {
                crate::app::operations::SharedOperations::focus_window(NavigationDirection::Right)
            }
            Action::FocusUpWindow => {
                crate::app::operations::SharedOperations::focus_window(NavigationDirection::Up)
            }
            Action::FocusDownWindow => {
                crate::app::operations::SharedOperations::focus_window(NavigationDirection::Down)
            }
            _ => CommandOutcome::default(),
        }
    }
}
