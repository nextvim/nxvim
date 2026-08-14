use std::path::Path;
use vim_ui::{NavigationDirection, SplitAxis, WindowId};

use crate::model::EditorModel;

use super::command::{CommandOutcome, ViewEffect};

pub struct SharedOperations;

impl SharedOperations {
    /// Write the current buffer to a path
    pub fn write(
        model: &mut EditorModel,
        active_window: WindowId,
        path: Option<&Path>,
        force: bool,
    ) -> CommandOutcome {
        match model.save_window(active_window, path, force) {
            Ok(saved) => {
                model.status = Some(format!(
                    "\"{}\" {} bytes written",
                    saved.path.display(),
                    saved.bytes_written
                ));
            }
            Err(error) => {
                model.status = Some(format!("Save failed: {error}"));
            }
        }
        CommandOutcome::redraw()
    }

    /// Edit a file or create a new buffer in the active window
    pub fn edit(
        model: &mut EditorModel,
        active_window: WindowId,
        path: Option<&Path>,
        force: bool,
    ) -> Result<CommandOutcome, vim_script::runtime::RuntimeError> {
        if !force {
            if let Some(buffer_id) = model.window_buffer(active_window) {
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

        model.switch_to(active_window, buffer_id);

        Ok(CommandOutcome::redraw())
    }

    /// Quit window or application
    pub fn quit(
        model: &mut EditorModel,
        active_window: WindowId,
        force: bool,
    ) -> Result<CommandOutcome, vim_script::runtime::RuntimeError> {
        if !force {
            if let Some(buffer_id) = model.window_buffer(active_window) {
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

        let non_cmd_windows: Vec<WindowId> = model.window_buffers()
            .filter(|(_, buf_id)| *buf_id != model.commandline_buffer())
            .map(|(win_id, _)| win_id)
            .collect();

        if non_cmd_windows.len() <= 1 {
            Ok(CommandOutcome::quit())
        } else {
            model.remove_window(active_window);
            let mut outcome = CommandOutcome::redraw();
            outcome.view_effects.push(ViewEffect::Close(active_window));
            if let Some(&remaining) = non_cmd_windows.iter().find(|&&win| win != active_window) {
                model.focus_window(remaining);
                outcome.view_effects.push(ViewEffect::Focus(remaining));
            }
            Ok(outcome)
        }
    }

    /// Switch buffer in window (next/previous)
    pub fn switch_buffer(
        model: &mut EditorModel,
        active_window: WindowId,
        forward: bool,
        count: usize,
    ) -> CommandOutcome {
        for _ in 0..count {
            if forward {
                model.switch_next_buffer(active_window);
            } else {
                model.switch_previous_buffer(active_window);
            }
        }
        CommandOutcome::redraw()
    }

    /// Window split directional navigation
    pub fn split_window(
        active_window: WindowId,
        horizontal: bool,
    ) -> CommandOutcome {
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
    pub fn focus_window(
        direction: NavigationDirection,
    ) -> CommandOutcome {
        CommandOutcome::with_effect(ViewEffect::FocusDirection(direction))
    }
}
