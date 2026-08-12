use vim_input::Action;
use vim_ui::{NavigationDirection, SplitAxis, WindowId};

use super::command::{CommandOutcome, ViewEffect};

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
        let effect = match action {
            Action::SplitHorizontal { .. } => ViewEffect::Split {
                source: active_window,
                axis: SplitAxis::Rows,
            },
            Action::SplitVertical { .. } => ViewEffect::Split {
                source: active_window,
                axis: SplitAxis::Columns,
            },
            Action::FocusLeftWindow => ViewEffect::FocusDirection(NavigationDirection::Left),
            Action::FocusRightWindow => ViewEffect::FocusDirection(NavigationDirection::Right),
            Action::FocusUpWindow => ViewEffect::FocusDirection(NavigationDirection::Up),
            Action::FocusDownWindow => ViewEffect::FocusDirection(NavigationDirection::Down),
            _ => return CommandOutcome::default(),
        };
        CommandOutcome::with_effect(effect)
    }
}
