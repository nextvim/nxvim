use vim_input::Action;
use vim_ui::{NavigationDirection, SplitAxis, WindowId};

pub enum Command {
    Editor {
        action: Action,
        register: Option<char>,
    },
    PendingInput(String),
    InvalidInput,
    Task(crate::app::services::TaskResult),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewEffect {
    Focus(WindowId),
    Split { source: WindowId, axis: SplitAxis },
    FocusDirection(NavigationDirection),
}

#[derive(Debug, Default)]
pub struct CommandOutcome {
    pub redraw: bool,
    pub quit: bool,
    pub view_effects: Vec<ViewEffect>,
}

impl CommandOutcome {
    pub fn redraw() -> Self {
        Self {
            redraw: true,
            ..Self::default()
        }
    }

    pub fn quit() -> Self {
        Self {
            redraw: true,
            quit: true,
            ..Self::default()
        }
    }

    pub fn with_effect(effect: ViewEffect) -> Self {
        Self {
            redraw: true,
            view_effects: vec![effect],
            ..Self::default()
        }
    }

    pub fn merge(&mut self, mut other: Self) {
        self.redraw |= other.redraw;
        self.quit |= other.quit;
        self.view_effects.append(&mut other.view_effects);
    }
}
