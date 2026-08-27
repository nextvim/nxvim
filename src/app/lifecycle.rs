//! App-owned lifecycle request routing.

use crate::app::App;
use crate::app::command::LifecycleRequest;
use crate::app::outcome::CommandOutcome;

fn save_async(
    app: &mut App,
    active_window: vim_ui::WindowId,
    path: Option<std::path::PathBuf>,
    force: bool,
) -> CommandOutcome {
    let Some(buffer_id) = crate::app::windows::WindowOps::window_buffer(&app.ui, active_window)
    else {
        return CommandOutcome::statusline();
    };
    let Ok(buffer) = app.model.get_buffer(buffer_id) else {
        return CommandOutcome::statusline();
    };
    if buffer.options().readonly && !force {
        app.model.status = Some(format!(
            "Save failed: ReadOnly (buffer {})",
            buffer_id.get()
        ));
        return CommandOutcome::statusline();
    }
    let path = match path.or_else(|| buffer.path().map(std::path::Path::to_path_buf)) {
        Some(path) => path,
        None => {
            app.model.status = Some(format!(
                "Save failed: No file name (buffer {})",
                buffer_id.get()
            ));
            return CommandOutcome::statusline();
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
    CommandOutcome::redraw()
}

pub fn dispatch(app: &mut App, command: LifecycleRequest) -> CommandOutcome {
    let active_window = app.ui.focused_window_id();
    match command {
        LifecycleRequest::Save { path, force } => save_async(app, active_window, path, force),
        LifecycleRequest::Quit { force } => crate::app::lifecycle_ops::LifecycleHandler::quit(
            &mut app.ui,
            &mut app.model,
            active_window,
            force,
        ),
        LifecycleRequest::QuitAll { force } => {
            crate::app::lifecycle_ops::LifecycleHandler::quit_all(&mut app.model, force)
        }
        LifecycleRequest::Edit { path, force } => {
            crate::app::lifecycle_ops::LifecycleHandler::edit(
                &mut app.ui,
                &mut app.model,
                active_window,
                path.as_deref(),
                force,
            )
        }
        LifecycleRequest::WriteQuit { path, force } => {
            crate::app::lifecycle_ops::LifecycleHandler::write_and_quit(
                &mut app.ui,
                &mut app.model,
                active_window,
                path.as_deref(),
                force,
            )
        }
        LifecycleRequest::WriteQuitAll { force } => {
            crate::app::lifecycle_ops::LifecycleHandler::write_and_quit_all(
                &mut app.ui,
                &mut app.model,
                active_window,
                force,
            )
        }
    }
}
