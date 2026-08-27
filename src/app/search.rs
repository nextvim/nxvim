//! Application projection for kernel-owned search semantics.

use crate::app::App;
use crate::app::command::SemanticRequest;
use crate::app::outcome::AppCommandOutcome;
use crate::app::substitute::SubstituteHandler;

pub fn execute(app: &mut App, pattern: String, forward: bool) -> AppCommandOutcome {
    let window = app.ui.focused_window_id();
    let Some(context) = app.model.kernel().current() else {
        app.model.status = Some("No current editor context".to_owned());
        return AppCommandOutcome::statusline();
    };
    app.model.kernel_mut().search_mut().set_pattern(&pattern);
    let mut kernel_outcome = None;
    let _ = crate::app::windows::WindowOps::edit_window(
        &mut app.ui,
        &mut app.model,
        window,
        |buffer, _, state| {
            kernel_outcome = Some(crate::kernel::search::move_cursor(
                context,
                &pattern,
                forward,
                buffer.as_text_buffer(),
                state,
            ));
        },
    );
    kernel_outcome.map_or_else(AppCommandOutcome::redraw, AppCommandOutcome::from_kernel)
}

pub fn dispatch(
    app: &mut App,
    command: SemanticRequest,
) -> Result<AppCommandOutcome, SemanticRequest> {
    let outcome = match command {
        SemanticRequest::SearchForward { pattern } => execute(app, pattern, true),
        SemanticRequest::SearchBackward { pattern } => execute(app, pattern, false),
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
