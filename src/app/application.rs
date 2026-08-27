//! Application-owned handling for non-semantic configuration and UI toggles.

use crate::app::{App, InspectKind};
use crate::app::legacy_command::Command;

use super::outcome::CommandOutcome;

/// Handles application-level commands that do not require semantic editor
/// dispatch. Unrecognized commands are returned for the temporary legacy path.
pub fn dispatch(app: &mut App, command: Command) -> Result<CommandOutcome, Command> {
    let outcome = match command {
        Command::ClearSearchHighlight => {
            crate::app::lifecycle_ops::LifecycleHandler::clear_search_highlight(
                &mut app.model,
            )
        }
        Command::Colorscheme { name } => {
            crate::app::lifecycle_ops::LifecycleHandler::colorscheme(
                &mut app.ui,
                &mut app.model,
                &mut app.colorscheme,
                &mut app.highlighter,
                name.as_deref(),
            )
        }
        Command::Set { arguments } => {
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
            CommandOutcome::redraw()
        }
        Command::SetOption { .. } => {
            app.model.status = Some("Typed host mutation requires the script host boundary".into());
            CommandOutcome::statusline()
        }
        Command::Syntax { enable } => {
            app.syntax_highlight = enable;
            app.model.invalidate_all_highlights();
            CommandOutcome::global_redraw(crate::kernel::RedrawInvalidationKind::SyntaxHighlighting)
        }
        Command::Treesitter { enable } => {
            app.treesitter_enabled = enable;
            CommandOutcome::global_redraw(crate::kernel::RedrawInvalidationKind::SyntaxHighlighting)
        }
        Command::Indexer { enable } => {
            app.indexer_enabled = enable;
            CommandOutcome::global_redraw(crate::kernel::RedrawInvalidationKind::Statusline)
        }
        Command::Inspect { enable } => {
            app.inspect = enable;
            CommandOutcome::global_redraw(crate::kernel::RedrawInvalidationKind::Statusline)
        }
        Command::Echo { message } => {
            app.model.status = Some(message.clone());
            app.message = message.clone();
            app.messages.push(message);
            CommandOutcome::statusline()
        }
        command => return Err(command),
    };
    Ok(outcome)
}
