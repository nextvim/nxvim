//! App-owned search and substitution request routing.

use crate::app::App;
use crate::app::outcome::CommandOutcome;
use crate::app::prompt::PromptHandler;
use crate::app::substitute::SubstituteHandler;
use crate::app::legacy_command::Command;

pub fn dispatch(app: &mut App, command: Command) -> Result<CommandOutcome, Command> {
    let active_window = app.ui.focused_window_id();
    let outcome = match command {
        Command::SearchForward { pattern } => {
            app.model.search_pattern = Some(pattern.clone());
            app.model.search_regex =
                vim_regex::Regex::compile(&pattern, vim_regex::CompileOptions::default()).ok();
            app.model.search_range = None;
            app.model.substitute_text = None;
            let _ = crate::app::windows::WindowOps::edit_window(
                &mut app.ui,
                &mut app.model,
                active_window,
                |buffer, _, state| {
                    state
                        .selections
                        .move_to_next_match(&pattern, true, buffer.as_text_buffer())
                },
            );
            CommandOutcome::redraw()
        }
        Command::SearchBackward { pattern } => {
            app.model.search_pattern = Some(pattern.clone());
            app.model.search_regex =
                vim_regex::Regex::compile(&pattern, vim_regex::CompileOptions::default()).ok();
            app.model.search_range = None;
            app.model.substitute_text = None;
            let _ = crate::app::windows::WindowOps::edit_window(
                &mut app.ui,
                &mut app.model,
                active_window,
                |buffer, _, state| {
                    state
                        .selections
                        .move_to_previous_match(&pattern, true, buffer.as_text_buffer())
                },
            );
            CommandOutcome::redraw()
        }
        Command::Substitute {
            pattern,
            substitute_text,
            flags,
            range,
        } => SubstituteHandler::start(app, pattern, substitute_text, flags, range),
        Command::OpenPrompt { message } => {
            app.prompt = Some(crate::app::prompt::Prompt::script(message, active_window));
            CommandOutcome::statusline()
        }
        Command::PromptChoice { handler, choice } => match handler {
            PromptHandler::Substitute => SubstituteHandler::respond(app, choice),
            PromptHandler::Script => {
                app.prompt = None;
                app.model.status = Some(format!("Prompt response: {choice:?}"));
                CommandOutcome::statusline()
            }
        },
        command => return Err(command),
    };
    Ok(outcome)
}
