use crate::app::services::TaskResult;
use crate::app::windows::WindowOps;
use crate::model::EditorModel;
use vim_ui::Ui;

use super::command::CommandOutcome;

pub struct TaskDispatcher;

impl TaskDispatcher {
    pub fn dispatch(
        ui: &mut Ui,
        model: &mut EditorModel,
        services: &mut crate::app::services::Services,
        result: TaskResult,
    ) -> CommandOutcome {
        let accepted = match result {
            TaskResult::Treesitter {
                task_id,
                revision,
                completed,
            } => {
                let buffer_id = completed.buffer_id;
                let Some(state) = Self::current_buffer_state(model, buffer_id, revision) else {
                    return CommandOutcome::default();
                };
                state.treesitter = completed.result.clone();
                services.treesitter.apply_task_result(task_id, completed);
                true
            }
            TaskResult::Index {
                task_id,
                buffer_id,
                revision,
                result,
            } => {
                let Some(state) = Self::current_buffer_state(model, buffer_id, revision) else {
                    return CommandOutcome::default();
                };
                state.index = result.clone();
                if let Ok(completed) = result {
                    services.indexer.apply_task_result(task_id, completed);
                }
                true
            }

            TaskResult::DisplayMapExpansion {
                window_id,
                buffer_id,
                revision,
                expansion,
                ..
            } => {
                if !Self::window_is_current(ui, model, window_id, buffer_id, revision) {
                    return CommandOutcome::default();
                }
                let Some(window) = ui
                    .window_mut(window_id)
                    .and_then(vim_ui::Window::window_state_mut)
                else {
                    return CommandOutcome::default();
                };
                let generation = expansion.generation.clone();
                let exact_rows = expansion.exact_rows.clone();
                if window.pending_display_map.as_ref()
                    == Some(&(generation.clone(), expansion.requested_rows.clone()))
                {
                    window.pending_display_map = None;
                }
                if window.display_map.apply_expansion(expansion).is_err() {
                    return CommandOutcome::default();
                }

                let hot_window = window.display_map.hot_window();
                let affects_viewport = exact_rows.start < hot_window.end;
                if affects_viewport && !window.selections.selections.is_empty() {
                    window.scroll_to_cursor();
                }
                affects_viewport
            }
            TaskResult::SaveFile {
                task_id,
                buffer_id,
                revision: _,
                result,
            } => {
                if !services.files.apply_task_result(task_id, &result) {
                    return CommandOutcome::default();
                }
                match result.result {
                    Ok(outcome) => {
                        if let Ok(buffer) = model.get_buffer_mut(buffer_id) {
                            if buffer.options().fixeol
                                && !buffer.options().binary
                                && !buffer.options().endofline
                            {
                                let mut options = buffer.options().clone();
                                options.endofline = true;
                                let _ = buffer.set_options(options);
                            }
                            let metadata = std::fs::metadata(&outcome.path);
                            buffer.set_file_metadata(vim_buffer::FileMetadata {
                                path: Some(outcome.path.clone()),
                                source: vim_buffer::LoadSource::File,
                                modified: metadata.as_ref().ok().and_then(|m| m.modified().ok()),
                                size: metadata.as_ref().ok().map(|m| m.len()),
                            });
                            buffer.mark_saved();
                            model.status = Some(format!(
                                "\"{}\" {} bytes written (background)",
                                outcome.path.display(),
                                outcome.bytes_written
                            ));
                        }
                    }
                    Err(err) => {
                        model.status = Some(format!("Save failed in background: {}", err));
                    }
                }
                true
            }
        };

        if accepted {
            CommandOutcome::redraw()
        } else {
            CommandOutcome::default()
        }
    }

    fn current_buffer_state(
        model: &mut EditorModel,
        buffer_id: vim_buffer::BufferId,
        revision: u64,
    ) -> Option<&mut crate::model::BufferState> {
        if model.get_buffer(buffer_id).is_err() {
            return None;
        }
        let state = model.buffer_state_mut(buffer_id)?;
        (state.revision == revision).then_some(state)
    }

    fn window_is_current(
        ui: &Ui,
        model: &EditorModel,
        window_id: vim_ui::WindowId,
        buffer_id: vim_buffer::BufferId,
        revision: u64,
    ) -> bool {
        WindowOps::window_buffer(ui, window_id) == Some(buffer_id)
            && model
                .buffer_state(buffer_id)
                .is_some_and(|state| state.revision == revision)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use background_worker::TaskId;
    use vim_ui::{Rect, WindowId};

    fn model() -> (Ui, EditorModel, WindowId, WindowId) {
        let mut ui = Ui::new(Rect::new(0, 0, 80, 24));
        let main = ui.focused_window_id();
        let commandline = ui.create_window("COMMAND LINE".to_string());
        let model = EditorModel::new(Vec::new());
        WindowOps::register(
            &mut ui,
            main,
            model.get_buffer(model.initial_buffer()).unwrap(),
            vim_ui::Viewport::default(),
        );
        WindowOps::register(
            &mut ui,
            commandline,
            model.get_buffer(model.commandline_buffer()).unwrap(),
            vim_ui::Viewport::default(),
        );
        (ui, model, main, commandline)
    }

    fn display_map_expansion(
        window_id: WindowId,
        buffer_id: vim_buffer::BufferId,
        revision: u64,
        expansion: display_map::DisplayMapExpansion,
    ) -> TaskResult {
        TaskResult::DisplayMapExpansion {
            task_id: TaskId(2),
            window_id,
            buffer_id,
            revision,
            expansion,
        }
    }

    #[test]
    fn offscreen_display_map_expansion_merges_without_redraw() {
        let (mut ui, mut model, main, _) = model();
        let text = (0..1_000)
            .map(|row| format!("row {row}\n"))
            .collect::<String>();
        let buffer_id = model.create(text);
        assert!(WindowOps::switch_next_buffer(&mut ui, &model, main));
        assert_eq!(WindowOps::window_buffer(&ui, main), Some(buffer_id));
        let revision = model.buffer_state_mut(buffer_id).unwrap().revision;
        let window = ui.window_mut(main).unwrap().window_state_mut().unwrap();
        let requested = 200..300;
        let expansion = display_map::build_expansion(
            window
                .display_map
                .expansion_input(requested.clone())
                .unwrap(),
            &background_worker::CancellationToken::default(),
        )
        .unwrap();
        window.pending_display_map = Some((window.display_map.generation(), requested));

        let mut services = crate::app::services::Services::new();
        let outcome = TaskDispatcher::dispatch(
            &mut ui,
            &mut model,
            &mut services,
            display_map_expansion(main, buffer_id, revision, expansion),
        );

        assert!(!outcome.redraw);
        let window = ui.window(main).unwrap().window_state().unwrap();
        assert!(window.display_map.covers_exactly(200..300));
        assert!(window.pending_display_map.is_none());
    }

    #[test]
    fn stale_display_map_generation_is_discarded() {
        let (mut ui, mut model, main, _) = model();
        let buffer_id = WindowOps::window_buffer(&ui, main).unwrap();
        let revision = model.buffer_state_mut(buffer_id).unwrap().revision;
        let window = ui.window_mut(main).unwrap().window_state_mut().unwrap();
        let expansion = display_map::build_expansion(
            window.display_map.expansion_input(0..1).unwrap(),
            &background_worker::CancellationToken::default(),
        )
        .unwrap();
        window.display_map.set_wrap_width(Some(10));

        let mut services = crate::app::services::Services::new();
        let outcome = TaskDispatcher::dispatch(
            &mut ui,
            &mut model,
            &mut services,
            display_map_expansion(main, buffer_id, revision, expansion),
        );

        assert!(!outcome.redraw);
    }
}
