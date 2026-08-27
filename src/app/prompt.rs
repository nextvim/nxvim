//! Application-owned interactive prompt state.

use vim_ui::WindowId;

use crate::app::App;
use crate::app::command::PromptRequest;
use crate::app::outcome::CommandOutcome;
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
    pub(crate) pattern: String,
    pub(crate) replacement: String,
    pub(crate) global: bool,
    pub(crate) row: u32,
    pub(crate) end_row: u32,
    pub(crate) search_offset: usize,
    pub(crate) current_match: Option<(usize, usize)>,
}

impl Prompt {
    pub fn script(message: String, window_id: WindowId) -> Self {
        Self {
            handler: PromptHandler::Script,
            message,
            window_id,
            pattern: String::new(),
            replacement: String::new(),
            global: false,
            row: 0,
            end_row: 0,
            search_offset: 0,
            current_match: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptHandler {
    Substitute,
    Script,
}

pub fn dispatch(app: &mut App, request: PromptRequest) -> CommandOutcome {
    let active_window = app.ui.focused_window_id();
    match request {
        PromptRequest::Open { message } => {
            app.prompt = Some(Prompt::script(message, active_window));
            CommandOutcome::statusline()
        }
        PromptRequest::Choice { handler, choice } => match handler {
            PromptHandler::Substitute => SubstituteHandler::respond(app, choice),
            PromptHandler::Script => {
                app.prompt = None;
                app.model.status = Some(format!("Prompt response: {choice:?}"));
                CommandOutcome::statusline()
            }
        },
    }
}
