use std::path::Path;
use vim_ui::{NavigationDirection, SplitAxis, Ui, WindowId};

use crate::app::windows::WindowOps;
use crate::model::EditorModel;

use super::command::{CommandOutcome, ViewEffect};

pub struct SharedOperations;

impl SharedOperations {
    /// Write the current buffer to a path, folding any error into the status
    /// message. Callers that need to know whether the write actually
    /// succeeded (for example, `:wq`, which must not quit after a failed
    /// write) should use `write_result` instead.
    pub fn write(
        ui: &mut Ui,
        model: &mut EditorModel,
        active_window: WindowId,
        path: Option<&Path>,
        force: bool,
    ) -> CommandOutcome {
        match Self::write_result(ui, model, active_window, path, force) {
            Ok(outcome) => outcome,
            Err(error) => {
                model.status = Some(format!("Save failed: {error}"));
                CommandOutcome::redraw()
            }
        }
    }

    /// Write the current buffer to a path, reporting success or failure
    /// through the `Result` rather than folding it into the status message.
    pub fn write_result(
        ui: &mut Ui,
        model: &mut EditorModel,
        active_window: WindowId,
        path: Option<&Path>,
        force: bool,
    ) -> Result<CommandOutcome, vim_buffer::BufferError> {
        let result = match WindowOps::window_buffer(ui, active_window) {
            Some(buffer_id) => model.buffers_mut().save(buffer_id, path, force),
            None => Err(vim_buffer::BufferError::NotImplemented(
                "saving an unregistered window",
            )),
        };
        let saved = result?;
        model.status = Some(format!(
            "\"{}\" {} bytes written",
            saved.path.display(),
            saved.bytes_written
        ));
        Ok(CommandOutcome::redraw())
    }

    /// Edit a file or create a new buffer in the active window
    pub fn edit(
        ui: &mut Ui,
        model: &mut EditorModel,
        active_window: WindowId,
        path: Option<&Path>,
        force: bool,
    ) -> Result<CommandOutcome, vim_script::runtime::RuntimeError> {
        if !force {
            if let Some(buffer_id) = WindowOps::window_buffer(ui, active_window) {
                if let Ok(buffer) = model.get_buffer(buffer_id) {
                    if buffer.is_modified() {
                        return Err(vim_script::runtime::RuntimeError::coded(
                            "E37",
                            vim_script::runtime::RuntimeErrorKind::HostError,
                            "No write since last change (add ! to override)",
                        ));
                    }
                }
            }
        }

        let buffer_id = match path {
            Some(file_path) => model.open_path(file_path),
            None => model.create(""),
        };

        WindowOps::switch_to(ui, model, active_window, buffer_id);

        Ok(CommandOutcome::redraw())
    }

    /// Quit window or application
    pub fn quit(
        ui: &mut Ui,
        model: &mut EditorModel,
        active_window: WindowId,
        force: bool,
    ) -> Result<CommandOutcome, vim_script::runtime::RuntimeError> {
        if !force {
            if let Some(buffer_id) = WindowOps::window_buffer(ui, active_window) {
                if let Ok(buffer) = model.get_buffer(buffer_id) {
                    if buffer.is_modified() {
                        return Err(vim_script::runtime::RuntimeError::coded(
                            "E37",
                            vim_script::runtime::RuntimeErrorKind::HostError,
                            "No write since last change (add ! to override)",
                        ));
                    }
                }
            }
        }

        let non_cmd_windows: Vec<WindowId> = WindowOps::window_buffers(ui)
            .into_iter()
            .filter(|&(_, buf_id)| buf_id != model.commandline_buffer())
            .map(|(win_id, _)| win_id)
            .collect();

        if non_cmd_windows.len() <= 1 {
            Ok(CommandOutcome::quit())
        } else {
            let mut outcome = CommandOutcome::redraw();
            outcome.view_effects.push(ViewEffect::Close(active_window));
            if let Some(&remaining) = non_cmd_windows.iter().find(|&&win| win != active_window) {
                outcome.view_effects.push(ViewEffect::Focus(remaining));
            }
            Ok(outcome)
        }
    }

    /// Switch buffer in window (next/previous)
    pub fn switch_buffer(
        ui: &mut Ui,
        model: &EditorModel,
        active_window: WindowId,
        forward: bool,
        count: usize,
    ) -> CommandOutcome {
        for _ in 0..count {
            if forward {
                WindowOps::switch_next_buffer(ui, model, active_window);
            } else {
                WindowOps::switch_previous_buffer(ui, model, active_window);
            }
        }
        CommandOutcome::redraw()
    }

    /// Window split directional navigation
    pub fn split_window(active_window: WindowId, horizontal: bool) -> CommandOutcome {
        let axis = if horizontal {
            SplitAxis::Rows
        } else {
            SplitAxis::Columns
        };
        CommandOutcome::with_effect(ViewEffect::Split {
            source: active_window,
            axis,
        })
    }

    /// Window focus directional navigation
    pub fn focus_window(direction: NavigationDirection) -> CommandOutcome {
        CommandOutcome::with_effect(ViewEffect::FocusDirection(direction))
    }
}
