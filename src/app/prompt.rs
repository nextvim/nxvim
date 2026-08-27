//! Application-owned interactive prompt state.

use vim_ui::WindowId;

use crate::app::App;
use crate::app::command::PromptRequest;
use crate::app::outcome::AppCommandOutcome;
use crate::app::substitute::SubstituteHandler;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptChoice {
    Yes,
    No,
    All,
    Quit,
    Last,
}

pub struct Prompt {
    pub handler: PromptHandler,
    pub message: String,
    pub(crate) window_id: WindowId,
    pub(crate) substitution: Option<crate::kernel::SubstitutionSession>,
}

impl Prompt {
    pub fn script(message: String, window_id: WindowId) -> Self {
        Self {
            handler: PromptHandler::Script,
            message,
            window_id,
            substitution: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptHandler {
    Substitute,
    Script,
}

pub fn dispatch(app: &mut App, request: PromptRequest) -> AppCommandOutcome {
    let active_window = app.ui.focused_window_id();
    match request {
        PromptRequest::Open { message } => {
            app.prompt = Some(Prompt::script(message, active_window));
            AppCommandOutcome::statusline()
        }
        PromptRequest::Choice { handler, choice } => match handler {
            PromptHandler::Substitute => SubstituteHandler::respond(app, choice),
            PromptHandler::Script => {
                app.prompt = None;
                app.model.status = Some(format!("Prompt response: {choice:?}"));
                AppCommandOutcome::statusline()
            }
        },
    }
}
