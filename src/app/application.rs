//! Application-owned handling for non-semantic configuration and UI toggles.

use crate::app::command::ApplicationRequest;
use crate::app::{App, InspectKind};

use super::outcome::AppCommandOutcome;

/// Handles application-level requests that do not require semantic editor
/// dispatch.
pub fn dispatch(app: &mut App, command: ApplicationRequest) -> AppCommandOutcome {
    match command {
        ApplicationRequest::ClearSearchHighlight => {
            crate::app::lifecycle::LifecycleHandler::clear_search_highlight(&mut app.model)
        }
        ApplicationRequest::Colorscheme { name } => {
            crate::app::lifecycle::LifecycleHandler::colorscheme(
                &mut app.ui,
                &mut app.model,
                &mut app.colorscheme,
                &mut app.highlighter,
                name.as_deref(),
            )
        }
        ApplicationRequest::Set { arguments } => {
            let active_window = app.ui.focused_window_id();
            let buffer_id = crate::app::windows::WindowOps::window_buffer(&app.ui, active_window);
            let result = app
                .config
                .write()
                .expect("config store lock poisoned")
                .execute_set_command(&arguments, buffer_id, Some(active_window));
            match result {
                Ok(Some(message)) => app.model.status = Some(message),
                Ok(None) => {}
                Err(error) => app.model.status = Some(format!("Error: {error}")),
            }
            let inspect = app.config.read().expect("config store lock poisoned").get(
                "inspect",
                buffer_id,
                Some(active_window),
            );
            if let Some(value) = inspect {
                if let Some(value) = value.as_string() {
                    app.inspect_what = match value {
                        "treesitter" => InspectKind::TreeSitter,
                        "textmate" => InspectKind::Textmate,
                        "indexer" => InspectKind::Indexer,
                        _ => InspectKind::None,
                    };
                }
            }
            AppCommandOutcome::redraw()
        }
        ApplicationRequest::SetOption { .. } => {
            app.model.status = Some("Typed host mutation requires the script host boundary".into());
            AppCommandOutcome::statusline()
        }
        ApplicationRequest::Syntax { enable } => {
            app.syntax_highlight = enable;
            app.model.invalidate_all_highlights();
            AppCommandOutcome::global_redraw(
                crate::kernel::RedrawInvalidationKind::SyntaxHighlighting,
            )
        }
        ApplicationRequest::Treesitter { enable } => {
            app.treesitter_enabled = enable;
            AppCommandOutcome::global_redraw(
                crate::kernel::RedrawInvalidationKind::SyntaxHighlighting,
            )
        }
        ApplicationRequest::Indexer { enable } => {
            app.indexer_enabled = enable;
            AppCommandOutcome::global_redraw(crate::kernel::RedrawInvalidationKind::Statusline)
        }
        ApplicationRequest::Inspect { enable } => {
            app.inspect = enable;
            AppCommandOutcome::global_redraw(crate::kernel::RedrawInvalidationKind::Statusline)
        }
        ApplicationRequest::Echo { message } => {
            app.model.status = Some(message.clone());
            app.message = message.clone();
            app.messages.push(message);
            AppCommandOutcome::statusline()
        }
    }
}
