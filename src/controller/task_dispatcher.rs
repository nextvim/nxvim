use text::ToPoint;

use crate::app::services::TaskResult;
use crate::model::EditorModel;

use super::command::CommandOutcome;

pub struct TaskDispatcher;

impl TaskDispatcher {
    pub fn dispatch(model: &mut EditorModel, result: TaskResult) -> CommandOutcome {
        let accepted = match result {
            TaskResult::Treesitter {
                buffer_id,
                revision,
                result,
                ..
            } => {
                let Some(state) = Self::current_buffer_state(model, buffer_id, revision) else {
                    return CommandOutcome::default();
                };
                state.treesitter = result;
                true
            }
            TaskResult::Index {
                buffer_id,
                revision,
                result,
                ..
            } => {
                let Some(state) = Self::current_buffer_state(model, buffer_id, revision) else {
                    return CommandOutcome::default();
                };
                state.index = result;
                true
            }
            TaskResult::Highlight {
                window_id,
                buffer_id,
                revision,
                highlights,
                ..
            } => {
                if !Self::window_is_current(model, window_id, buffer_id, revision) {
                    return CommandOutcome::default();
                }
                let Some(window) = model.window_state_mut(window_id) else {
                    return CommandOutcome::default();
                };
                window.highlights = highlights;
                true
            }
            TaskResult::DisplayMap {
                window_id,
                buffer_id,
                revision,
                map,
                height,
                layout_width,
                ..
            } => {
                if !Self::window_is_current(model, window_id, buffer_id, revision) {
                    return CommandOutcome::default();
                }
                let current_snapshot = model
                    .get_buffer(buffer_id)
                    .ok()
                    .map(|buffer| buffer.snapshot().as_inner().clone());
                let Some(window) = model.window_state_mut(window_id) else {
                    return CommandOutcome::default();
                };
                window.display_map = map;
                if window.selections.selections.is_empty() {
                    return CommandOutcome::redraw();
                }
                let cursor_anchor = window.selections.primary().head();
                let display_snapshot = window.display_map.snapshot();
                let original_buffer = display_snapshot.buffer_snapshot();
                let display_cursor = if let Some(snapshot) = current_snapshot.as_ref() {
                    if original_buffer.version == snapshot.version {
                        display_snapshot.anchor_to_display_point(cursor_anchor)
                    } else {
                        let point = cursor_anchor.to_point(snapshot);
                        let max_row = original_buffer.row_count().saturating_sub(1);
                        let row = point.row.min(max_row);
                        let column = if row < original_buffer.row_count() {
                            point.column.min(original_buffer.line_len(row))
                        } else {
                            0
                        };
                        display_snapshot.point_to_display_point(text::Point { row, column })
                    }
                } else {
                    display_snapshot.anchor_to_display_point(cursor_anchor)
                };
                let wrap_width = window.display_map.wrap_width.unwrap_or(layout_width);
                window.display_map.scroll_to_cursor(
                    display_cursor,
                    height as i32,
                    wrap_width as i32,
                );
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
        model: &EditorModel,
        window_id: vim_ui::WindowId,
        buffer_id: vim_buffer::BufferId,
        revision: u64,
    ) -> bool {
        model.window_buffer(window_id) == Some(buffer_id)
            && model
                .buffer_state(buffer_id)
                .is_some_and(|state| state.revision == revision)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use background_worker::TaskId;
    use vim_ui::WindowId;

    fn model() -> (EditorModel, WindowId, WindowId) {
        let main = WindowId::new(10);
        let commandline = WindowId::new(11);
        (
            EditorModel::new(Vec::new(), main, commandline),
            main,
            commandline,
        )
    }

    fn highlight(
        window_id: WindowId,
        buffer_id: vim_buffer::BufferId,
        revision: u64,
    ) -> TaskResult {
        TaskResult::Highlight {
            task_id: TaskId(1),
            window_id,
            buffer_id,
            revision,
            highlights: Vec::new(),
        }
    }

    #[test]
    fn current_revision_is_applied_and_requests_redraw() {
        let (mut model, main, _) = model();
        let buffer = model.window_buffer(main).unwrap();
        let revision = model.buffer_state_mut(buffer).unwrap().revision;

        let outcome = TaskDispatcher::dispatch(&mut model, highlight(main, buffer, revision));

        assert!(outcome.redraw);
    }

    #[test]
    fn stale_revision_is_discarded() {
        let (mut model, main, _) = model();
        let buffer = model.window_buffer(main).unwrap();
        model.buffer_state_mut(buffer).unwrap().revision = 2;

        let outcome = TaskDispatcher::dispatch(&mut model, highlight(main, buffer, 1));

        assert!(!outcome.redraw);
    }

    #[test]
    fn deleted_buffer_result_is_discarded() {
        let (mut model, main, _) = model();
        let removed = model.window_buffer(main).unwrap();
        model.create("fallback");
        model.wipe(removed, true).unwrap();

        let outcome = TaskDispatcher::dispatch(&mut model, highlight(main, removed, 0));

        assert!(!outcome.redraw);
    }

    #[test]
    fn result_for_window_that_switched_buffers_is_discarded() {
        let (mut model, main, _) = model();
        let original = model.window_buffer(main).unwrap();
        model.create("second");
        assert!(model.switch_next_buffer(main));
        assert_ne!(model.window_buffer(main), Some(original));

        let outcome = TaskDispatcher::dispatch(&mut model, highlight(main, original, 0));

        assert!(!outcome.redraw);
    }
}
