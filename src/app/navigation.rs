//! App-owned tab, buffer, and window request orchestration.

use crate::app::App;
use crate::app::command::{AppCommand, LifecycleRequest, NavigationRequest};
use crate::app::outcome::CommandOutcome;

/// Handles navigation and layout requests.
pub fn dispatch(app: &mut App, command: NavigationRequest) -> CommandOutcome {
    match command {
        NavigationRequest::SplitNew { vertical } => {
            let active_window = app.ui.focused_window_id();
            app.command_queue
                .push_back(AppCommand::Lifecycle(LifecycleRequest::Edit {
                    path: None,
                    force: true,
                }));
            crate::app::operations::SharedOperations::split_window(active_window, !vertical)
        }
        NavigationRequest::TabNew { path } => {
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
        NavigationRequest::TabNext { count } => {
            if let Err(error) = app.next_tab(count) {
                app.model.status = Some(error);
            }
            CommandOutcome::layout()
        }
        NavigationRequest::TabPrevious { count } => {
            if let Err(error) = app.previous_tab(count) {
                app.model.status = Some(error);
            }
            CommandOutcome::layout()
        }
        NavigationRequest::TabClose => {
            if let Err(error) = app.close_active_tab() {
                app.model.status = Some(error);
            }
            CommandOutcome::layout()
        }
        NavigationRequest::BufferNext { count } => {
            let active = app.ui.focused_window_id();
            crate::app::operations::SharedOperations::switch_buffer(
                &mut app.ui,
                &mut app.model,
                active,
                true,
                count,
            )
        }
        NavigationRequest::BufferPrevious { count } => {
            let active = app.ui.focused_window_id();
            crate::app::operations::SharedOperations::switch_buffer(
                &mut app.ui,
                &mut app.model,
                active,
                false,
                count,
            )
        }
    }
}
