use std::path::Path;

use vim_input::Action;
use vim_ui::{Ui, WindowId};

use crate::model::EditorModel;

use super::command::CommandOutcome;
use super::shared_operations::SharedOperations;

/// Handles application lifecycle operations — quit, edit, and the write+quit
/// combinations (`:wq`, `:wqall`) — whether they originate from a resolved
/// key action (`Action::Quit`) or a script Ex command (`Command::Quit`,
/// `Command::Edit`, `Command::WriteQuit`, `Command::WriteQuitAll`). This is
/// the single call path for `SharedOperations::quit`/`SharedOperations::edit`,
/// so there is exactly one place that turns their `Result` into a
/// `CommandOutcome`/status message.
pub struct LifecycleHandler;

impl LifecycleHandler {
    pub fn handles(action: &Action) -> bool {
        matches!(action, Action::Quit)
    }

    pub fn execute(
        ui: &mut Ui,
        model: &mut EditorModel,
        active_window: WindowId,
        action: &Action,
    ) -> CommandOutcome {
        match action {
            Action::Quit => Self::quit(ui, model, active_window, false),
            _ => CommandOutcome::default(),
        }
    }

    pub fn quit(
        ui: &mut Ui,
        model: &mut EditorModel,
        active_window: WindowId,
        force: bool,
    ) -> CommandOutcome {
        let result = SharedOperations::quit(ui, model, active_window, force);
        Self::outcome_or_status(model, result)
    }

    pub fn edit(
        ui: &mut Ui,
        model: &mut EditorModel,
        active_window: WindowId,
        path: Option<&Path>,
        force: bool,
    ) -> CommandOutcome {
        let result = SharedOperations::edit(ui, model, active_window, path, force);
        Self::outcome_or_status(model, result)
    }

    /// Writes the active buffer and, only if that succeeds, quits exactly as
    /// `quit` would.
    pub fn write_and_quit(
        ui: &mut Ui,
        model: &mut EditorModel,
        active_window: WindowId,
        path: Option<&Path>,
        force: bool,
    ) -> CommandOutcome {
        match SharedOperations::write_result(ui, model, active_window, path, force) {
            Ok(mut outcome) => {
                outcome.merge(Self::quit(ui, model, active_window, force));
                outcome
            }
            Err(error) => {
                model.status = Some(format!("Save failed: {error}"));
                CommandOutcome::redraw()
            }
        }
    }

    /// `:wqall` should write and close every window; `SharedOperations::quit`
    /// does not yet distinguish closing one window from closing all of them
    /// (`SCRIPT.md` P0.4), so this mirrors the same simplification `:qall`
    /// already makes today and writes only the active buffer before quitting.
    pub fn write_and_quit_all(
        ui: &mut Ui,
        model: &mut EditorModel,
        active_window: WindowId,
        force: bool,
    ) -> CommandOutcome {
        Self::write_and_quit(ui, model, active_window, None, force)
    }

    pub fn clear_search_highlight(model: &mut EditorModel) -> CommandOutcome {
        model.search_pattern = None;
        model.search_regex = None;
        CommandOutcome::redraw()
    }

    fn outcome_or_status(
        model: &mut EditorModel,
        result: Result<CommandOutcome, vim_script::runtime::RuntimeError>,
    ) -> CommandOutcome {
        match result {
            Ok(outcome) => outcome,
            Err(error) => {
                model.status = Some(error.message);
                CommandOutcome::redraw()
            }
        }
    }
}
