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
                buffer_id,
                revision,
                highlights,
                ..
            } => {
                let Some(state) = Self::current_buffer_state(model, buffer_id, revision) else {
                    return CommandOutcome::default();
                };
                state.highlights = highlights;
                model.window_buffers().any(|(_, b_id)| b_id == buffer_id)
            }
            TaskResult::DisplayMapExpansion {
                window_id,
                buffer_id,
                revision,
                expansion,
                ..
            } => {
                if !Self::window_is_current(model, window_id, buffer_id, revision) {
                    return CommandOutcome::default();
                }
                let Some(window) = model.window_state_mut(window_id) else {
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
    fn offscreen_display_map_expansion_merges_without_redraw() {
        let (mut model, main, _) = model();
        let text = (0..1_000)
            .map(|row| format!("row {row}\n"))
            .collect::<String>();
        let buffer_id = model.create(text);
        assert!(model.switch_next_buffer(main));
        assert_eq!(model.window_buffer(main), Some(buffer_id));
        let revision = model.buffer_state_mut(buffer_id).unwrap().revision;
        let window = model.window_state_mut(main).unwrap();
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

        let outcome = TaskDispatcher::dispatch(
            &mut model,
            display_map_expansion(main, buffer_id, revision, expansion),
        );

        assert!(!outcome.redraw);
        let window = model.window_state(main).unwrap();
        assert!(window.display_map.covers_exactly(200..300));
        assert!(window.pending_display_map.is_none());
    }

    #[test]
    fn stale_display_map_generation_is_discarded() {
        let (mut model, main, _) = model();
        let buffer_id = model.window_buffer(main).unwrap();
        let revision = model.buffer_state_mut(buffer_id).unwrap().revision;
        let window = model.window_state_mut(main).unwrap();
        let expansion = display_map::build_expansion(
            window.display_map.expansion_input(0..1).unwrap(),
            &background_worker::CancellationToken::default(),
        )
        .unwrap();
        window.display_map.set_wrap_width(Some(10));

        let outcome = TaskDispatcher::dispatch(
            &mut model,
            display_map_expansion(main, buffer_id, revision, expansion),
        );

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
