//! App-owned search and substitution request routing.

use crate::app::App;
use crate::app::outcome::CommandOutcome;

use crate::app::command::SemanticRequest;

use crate::app::substitute::SubstituteHandler;

pub fn dispatch(
    app: &mut App,
    command: SemanticRequest,
) -> Result<CommandOutcome, SemanticRequest> {
    let active_window = app.ui.focused_window_id();
    let outcome = match command {
        SemanticRequest::SearchForward { pattern } => {
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
        SemanticRequest::SearchBackward { pattern } => {
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
        SemanticRequest::Substitute {
            pattern,
            substitute_text,
            flags,
            range,
        } => SubstituteHandler::start(app, pattern, substitute_text, flags, range),

        command => return Err(command),
    };
    Ok(outcome)
}
