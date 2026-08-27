//! App-owned tab, buffer, and window request orchestration.

use crate::app::App;
use crate::app::outcome::CommandOutcome;
use crate::app::legacy_command::Command;

/// Handles navigation/layout requests. Other application commands are
/// returned for the temporary compatibility path.
pub fn dispatch(app: &mut App, command: Command) -> Result<CommandOutcome, Command> {
    let outcome = match command {
        Command::SplitNew { vertical } => {
            let active_window = app.ui.focused_window_id();
            app.command_queue.push_back(
                Command::Edit {
                    path: None,
                    force: true,
                }
                .into(),
            );
            crate::app::operations::SharedOperations::split_window(active_window, !vertical)
        }
        Command::TabNew { path } => {
            let buffer = match path {
                Some(path) => app.model.open_path(path),
                None => app.model.create(""),
            };
            match app.new_tab(buffer) {
                Ok(_) => CommandOutcome::layout(),
                Err(error) => {
                    app.model.status = Some(error);
                    CommandOutcome::redraw()
                }
            }
        }
        Command::TabNext { count } => {
            if let Err(error) = app.next_tab(count) {
                app.model.status = Some(error);
            }
            CommandOutcome::layout()
        }
        Command::TabPrevious { count } => {
            if let Err(error) = app.previous_tab(count) {
                app.model.status = Some(error);
            }
            CommandOutcome::layout()
        }
        Command::TabClose => {
            if let Err(error) = app.close_active_tab() {
                app.model.status = Some(error);
            }
            CommandOutcome::layout()
        }
        Command::BufferNext { count } => {
            let active = app.ui.focused_window_id();
            crate::app::operations::SharedOperations::switch_buffer(
                &mut app.ui,
                &mut app.model,
                active,
                true,
                count,
            )
        }
        Command::BufferPrevious { count } => {
            let active = app.ui.focused_window_id();
            crate::app::operations::SharedOperations::switch_buffer(
                &mut app.ui,
                &mut app.model,
                active,
                false,
                count,
            )
        }
        command => return Err(command),
    };
    Ok(outcome)
}
