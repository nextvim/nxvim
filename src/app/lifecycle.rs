//! App-owned lifecycle request routing.

use crate::app::App;
use crate::app::command::LifecycleRequest;
use crate::app::outcome::AppCommandOutcome;

fn save_async(
    app: &mut App,
    active_window: vim_ui::WindowId,
    path: Option<std::path::PathBuf>,
    force: bool,
) -> AppCommandOutcome {
    let Some(buffer_id) = crate::app::windows::WindowOps::window_buffer(&app.ui, active_window)
    else {
        return AppCommandOutcome::statusline();
    };
    let Ok(buffer) = app.model.get_buffer(buffer_id) else {
        return AppCommandOutcome::statusline();
    };
    if buffer.options().readonly && !force {
        app.model.status = Some(format!(
            "Save failed: ReadOnly (buffer {})",
            buffer_id.get()
        ));
        return AppCommandOutcome::statusline();
    }
    let path = match path.or_else(|| buffer.path().map(std::path::Path::to_path_buf)) {
        Some(path) => path,
        None => {
            app.model.status = Some(format!(
                "Save failed: No file name (buffer {})",
                buffer_id.get()
            ));
            return AppCommandOutcome::statusline();
        }
    };
    let snapshot = buffer.snapshot();
    let options = buffer.options().clone();
    let revision = app
        .model
        .buffer_state(buffer_id)
        .map(|state| state.revision)
        .unwrap_or(0);
    let sequence = app
        .services
        .files
        .begin_save(buffer_id, snapshot.changedtick());
    let owner = crate::app::services::TaskOwner {
        buffer_id: Some(buffer_id),
        window_id: Some(active_window),
        revision,
    };
    if let Some(task_id) = app.services.spawn_cancellable_task(
        "files",
        sequence,
        owner,
        crate::app::services::TaskType::Files,
        move |token| {
            Some(files::save_file_cancellable(
                snapshot,
                path,
                options,
                move || token.is_cancelled(),
            )?)
        },
    ) {
        app.services.files.set_pending_task(buffer_id, task_id);
        app.model.status = Some("Saving file in background...".to_string());
    }
    AppCommandOutcome::redraw()
}

pub fn dispatch(app: &mut App, command: LifecycleRequest) -> AppCommandOutcome {
    let active_window = app.ui.focused_window_id();
    match command {
        LifecycleRequest::Save { path, force } => save_async(app, active_window, path, force),
        LifecycleRequest::Quit { force } => {
            LifecycleHandler::quit(&mut app.ui, &mut app.model, active_window, force)
        }
        LifecycleRequest::QuitAll { force } => LifecycleHandler::quit_all(&mut app.model, force),
        LifecycleRequest::Edit { path, force } => LifecycleHandler::edit(
            &mut app.ui,
            &mut app.model,
            active_window,
            path.as_deref(),
            force,
        ),
        LifecycleRequest::WriteQuit { path, force } => LifecycleHandler::write_and_quit(
            &mut app.ui,
            &mut app.model,
            active_window,
            path.as_deref(),
            force,
        ),
        LifecycleRequest::WriteQuitAll { force } => {
            LifecycleHandler::write_and_quit_all(&mut app.ui, &mut app.model, active_window, force)
        }
    }
}

use crate::app::ui::ViewEffect;
use crate::app::windows::WindowOps;
use crate::model::EditorModel;
use std::path::Path;
use vim_input::Action;
use vim_ui::{NavigationDirection, SplitAxis, Ui, WindowId};

pub struct LifecycleOperations;

impl LifecycleOperations {
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
    ) -> AppCommandOutcome {
        match Self::write_result(ui, model, active_window, path, force) {
            Ok(outcome) => outcome,
            Err(error) => {
                model.status = Some(format!("Save failed: {error}"));
                AppCommandOutcome::redraw()
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
    ) -> Result<AppCommandOutcome, vim_buffer::BufferError> {
        let result = match WindowOps::window_buffer(ui, active_window) {
            Some(buffer_id) => model.save(buffer_id, path, force),
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
        Ok(AppCommandOutcome::redraw())
    }

    /// Edit a file or create a new buffer in the active window
    pub fn edit(
        ui: &mut Ui,
        model: &mut EditorModel,
        active_window: WindowId,
        path: Option<&Path>,
        force: bool,
    ) -> Result<AppCommandOutcome, vim_script::runtime::RuntimeError> {
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

        if WindowOps::switch_to(ui, model, active_window, buffer_id) {
            let _ = model
                .kernel_mut()
                .set_window_buffer(active_window, buffer_id);
        }

        Ok(AppCommandOutcome::redraw())
    }

    /// Quit window or application
    pub fn quit(
        ui: &mut Ui,
        model: &mut EditorModel,
        active_window: WindowId,
        force: bool,
    ) -> Result<AppCommandOutcome, vim_script::runtime::RuntimeError> {
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

        if non_cmd_windows.len() > 1 {
            let mut outcome = AppCommandOutcome::redraw();
            outcome.view_effects.push(ViewEffect::Close(active_window));
            if let Some(&remaining) = non_cmd_windows.iter().find(|&&win| win != active_window) {
                outcome.view_effects.push(ViewEffect::Focus(remaining));
            }
            Ok(outcome)
        } else {
            let Some(active_buffer) = WindowOps::window_buffer(ui, active_window) else {
                return Ok(AppCommandOutcome::quit());
            };
            let remaining_buffer = model
                .list()
                .into_iter()
                .find(|&buffer_id| buffer_id != active_buffer);

            if let Some(remaining_buffer) = remaining_buffer {
                model.wipe(active_buffer, force).map_err(|error| {
                    vim_script::runtime::RuntimeError::coded(
                        "E89",
                        vim_script::runtime::RuntimeErrorKind::HostError,
                        error.to_string(),
                    )
                })?;
                WindowOps::switch_to(ui, model, active_window, remaining_buffer);
                Ok(AppCommandOutcome::redraw())
            } else {
                Ok(AppCommandOutcome::quit())
            }
        }
    }

    /// Switch buffer in window (next/previous)
    pub fn switch_buffer(
        ui: &mut Ui,
        model: &mut EditorModel,
        active_window: WindowId,
        forward: bool,
        count: usize,
    ) -> AppCommandOutcome {
        for _ in 0..count {
            if forward {
                WindowOps::switch_next_buffer(ui, model, active_window);
            } else {
                WindowOps::switch_previous_buffer(ui, model, active_window);
            }
        }
        if let Some(buffer) = WindowOps::window_buffer(ui, active_window) {
            let _ = model.kernel_mut().set_window_buffer(active_window, buffer);
        }
        AppCommandOutcome::redraw()
    }

    /// Quit the application after verifying that no editor buffer has unsaved changes.
    pub fn quit_all(
        model: &mut EditorModel,
        force: bool,
    ) -> Result<AppCommandOutcome, vim_script::runtime::RuntimeError> {
        if !force
            && let Some(buffer) = model
                .buffers()
                .list()
                .into_iter()
                .filter(|&buffer_id| buffer_id != model.commandline_buffer())
                .filter_map(|buffer_id| model.get_buffer(buffer_id).ok())
                .find(|buffer| buffer.is_modified())
        {
            return Err(vim_script::runtime::RuntimeError::coded(
                "E37",
                vim_script::runtime::RuntimeErrorKind::HostError,
                format!(
                    "No write since last change in {} (add ! to override)",
                    buffer.path().map_or_else(
                        || "[No Name]".to_string(),
                        |path| path.display().to_string()
                    )
                ),
            ));
        }

        Ok(AppCommandOutcome::quit())
    }

    /// Window split directional navigation
    pub fn split_window(active_window: WindowId, horizontal: bool) -> AppCommandOutcome {
        let axis = if horizontal {
            SplitAxis::Rows
        } else {
            SplitAxis::Columns
        };
        AppCommandOutcome::with_effect(ViewEffect::Split {
            source: active_window,
            axis,
        })
    }

    /// Window focus directional navigation
    pub fn focus_window(direction: NavigationDirection) -> AppCommandOutcome {
        AppCommandOutcome::with_effect(ViewEffect::FocusDirection(direction))
    }
}

/// Handles application lifecycle operations — quit, edit, and the write+quit
/// combinations (`:wq`, `:wqall`) — whether they originate from a resolved
/// key action (`Action::Quit`) or a script Ex command (`Command::Quit`,
/// `Command::QuitAll`, `Command::Edit`, `Command::WriteQuit`,
/// `Command::WriteQuitAll`). This is
/// the single call path for `LifecycleOperations::quit`/`LifecycleOperations::edit`,
/// so there is exactly one place that turns their `Result` into a
/// `AppCommandOutcome`/status message.
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
    ) -> AppCommandOutcome {
        match action {
            Action::Quit => Self::quit(ui, model, active_window, false),
            _ => AppCommandOutcome::default(),
        }
    }

    pub fn quit(
        ui: &mut Ui,
        model: &mut EditorModel,
        active_window: WindowId,
        force: bool,
    ) -> AppCommandOutcome {
        let result = LifecycleOperations::quit(ui, model, active_window, force);
        Self::outcome_or_status(model, result)
    }

    pub fn quit_all(model: &mut EditorModel, force: bool) -> AppCommandOutcome {
        let result = LifecycleOperations::quit_all(model, force);
        Self::outcome_or_status(model, result)
    }

    pub fn edit(
        ui: &mut Ui,
        model: &mut EditorModel,
        active_window: WindowId,
        path: Option<&Path>,
        force: bool,
    ) -> AppCommandOutcome {
        let result = LifecycleOperations::edit(ui, model, active_window, path, force);
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
    ) -> AppCommandOutcome {
        match LifecycleOperations::write_result(ui, model, active_window, path, force) {
            Ok(mut outcome) => {
                outcome.merge(Self::quit(ui, model, active_window, force));
                outcome
            }
            Err(error) => {
                model.status = Some(format!("Save failed: {error}"));
                AppCommandOutcome::redraw()
            }
        }
    }

    /// `:wqall` writes the active buffer before quitting. Unlike `:qall`, it
    /// retains the write step and currently writes only the active buffer.
    pub fn write_and_quit_all(
        ui: &mut Ui,
        model: &mut EditorModel,
        active_window: WindowId,
        force: bool,
    ) -> AppCommandOutcome {
        Self::write_and_quit(ui, model, active_window, None, force)
    }

    pub fn clear_search_highlight(model: &mut EditorModel) -> AppCommandOutcome {
        model.kernel_mut().search_mut().clear();
        AppCommandOutcome::redraw()
    }

    fn outcome_or_status(
        model: &mut EditorModel,
        result: Result<AppCommandOutcome, vim_script::runtime::RuntimeError>,
    ) -> AppCommandOutcome {
        match result {
            Ok(outcome) => outcome,
            Err(error) => {
                model.status = Some(error.message);
                AppCommandOutcome::redraw()
            }
        }
    }

    pub fn colorscheme(
        ui: &mut Ui,
        model: &mut EditorModel,
        app_colorscheme: &mut Option<vim_colorscheme::ColorScheme>,
        app_highlighter: &mut Option<textmate::Highlighter<'static>>,
        name: Option<&str>,
    ) -> AppCommandOutcome {
        if let Some(name) = name {
            if let Some(cs) = vim_colorscheme::ColorScheme::get_by_name(name) {
                let highlighter = textmate::load_colorscheme(&cs);
                *app_colorscheme = Some(cs.clone());
                *app_highlighter = Some(highlighter);
                ui.set_colorscheme(Some(cs));
                model.invalidate_all_highlights();
                model.status = None;
                AppCommandOutcome::redraw()
            } else {
                model.status = Some(format!("E185: Cannot find color scheme '{name}'"));
                AppCommandOutcome::redraw()
            }
        } else {
            if let Some(cs) = app_colorscheme {
                model.status = Some(cs.metadata.name.clone());
            } else {
                model.status = Some("default".to_string());
            }
            AppCommandOutcome::redraw()
        }
    }
}
