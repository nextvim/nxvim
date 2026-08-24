use std::collections::HashMap;
use std::sync::Mutex;

pub use files;
pub use vim_clipboard as clipboard;
pub use vim_indexer as indexer;
pub use vim_macros as macros;
pub use vim_treesitter as treesitter;

use crate::app::App;
use crate::app::windows::WindowOps;
use text::ToPoint;
use vim_buffer::BufferId;
use vim_ui::{Window, WindowId};

pub type TaskId = background_worker::TaskId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TaskType {
    DisplayMap,
    Indexer,
    Treesitter,
    Files,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TaskOwner {
    pub buffer_id: Option<BufferId>,
    pub window_id: Option<WindowId>,
    pub revision: u64,
}

pub enum TaskResult {
    Treesitter {
        task_id: TaskId,
        revision: u64,
        completed: vim_treesitter::ParseTaskResult,
    },
    Index {
        task_id: TaskId,
        buffer_id: BufferId,
        revision: u64,
        result: Result<vim_indexer::IndexTaskResult, String>,
    },
    DisplayMapExpansion {
        task_id: TaskId,
        window_id: WindowId,
        buffer_id: BufferId,
        revision: u64,
        expansion: display_map::DisplayMapExpansion,
    },
    SaveFile {
        task_id: TaskId,
        buffer_id: BufferId,
        revision: u64,
        result: files::SaveTaskResult,
    },
}

pub(super) struct TaskMetadata {
    pub owner: TaskOwner,
    pub task_type: TaskType,
}

pub struct Services {
    background_workers: background_worker::WorkerManager,
    pub clipboard: clipboard::Clipboard,
    pub indexer: indexer::Indexer,
    pub files: files::FilesService,
    pub macros: macros::MacroRecorder,
    pub treesitter: treesitter::TreeSitterService,
    raw_results: Vec<background_worker::BackgroundResult>,
    task_metadata: Mutex<HashMap<background_worker::TaskId, TaskMetadata>>,
    pub repeat_actions: Option<Vec<vim_input::Action>>,
    pub recording_repeat: Option<Vec<vim_input::Action>>,
}

impl Services {
    pub fn new() -> Self {
        let mut background_workers = background_worker::WorkerManager::new();
        background_workers.add_worker("display_map");
        background_workers.add_worker("highlight");
        background_workers.add_worker("treesitter");
        background_workers.add_worker("indexer");
        background_workers.add_worker("files");

        let mut macros = macros::MacroRecorder::new();
        macros.begin("*");
        macros.record(vim_input::Action::SetToVisual, None);
        macros.record(
            vim_input::Action::MoveWithinCharacter { count: 1, ch: 'w' },
            None,
        );
        macros.record(vim_input::Action::SetToCommandSearchForward, None);
        macros.end();
        macros.begin("#");
        macros.record(vim_input::Action::SetToVisual, None);
        macros.record(
            vim_input::Action::MoveWithinCharacter { count: 1, ch: 'w' },
            None,
        );
        macros.record(vim_input::Action::SetToCommandSearchBackward, None);
        macros.end();

        Self {
            background_workers,
            clipboard: clipboard::Clipboard::new(),
            indexer: indexer::Indexer::new(),
            files: files::FilesService::new(),
            macros,
            treesitter: treesitter::TreeSitterService::new(),
            raw_results: Vec::new(),
            task_metadata: Mutex::new(HashMap::new()),
            repeat_actions: None,
            recording_repeat: None,
        }
    }

    pub fn poll(&mut self) -> bool {
        struct ResultsCollector<'a> {
            results: &'a mut Vec<background_worker::BackgroundResult>,
        }

        impl background_worker::WorkerResultHandler for ResultsCollector<'_> {
            fn handle_result(&mut self, result: background_worker::BackgroundResult) {
                self.results.push(result);
            }
        }

        let mut collector = ResultsCollector {
            results: &mut self.raw_results,
        };
        let count = self.background_workers.poll(&mut collector);
        count > 0 || !self.raw_results.is_empty()
    }

    pub fn drain_results(&mut self) -> Vec<TaskResult> {
        let raw_results = std::mem::take(&mut self.raw_results);
        let mut metadata = self.task_metadata.lock().unwrap();
        raw_results
            .into_iter()
            .filter_map(|result| {
                let task_id = result.task_id;
                let metadata = metadata.remove(&task_id)?;
                Self::decode_result(result, metadata)
            })
            .collect()
    }

    pub fn spawn_task<T, F>(
        &self,
        worker_name: &str,
        sequence: std::sync::Arc<std::sync::atomic::AtomicU64>,
        owner: TaskOwner,
        task_type: TaskType,
        job: F,
    ) -> Option<background_worker::TaskId>
    where
        T: std::any::Any + Send + 'static,
        F: FnOnce() -> T + Send + 'static,
    {
        let task_id = self
            .background_workers
            .spawn_task(worker_name, sequence, job)?;
        self.task_metadata
            .lock()
            .unwrap()
            .insert(task_id, TaskMetadata { owner, task_type });
        Some(task_id)
    }

    pub fn spawn_cancellable_task<T, F>(
        &self,
        worker_name: &str,
        sequence: std::sync::Arc<std::sync::atomic::AtomicU64>,
        owner: TaskOwner,
        task_type: TaskType,
        job: F,
    ) -> Option<background_worker::TaskId>
    where
        T: std::any::Any + Send + 'static,
        F: FnOnce(background_worker::CancellationToken) -> Option<T> + Send + 'static,
    {
        let task_id = self
            .background_workers
            .spawn_cancellable_task(worker_name, sequence, job)?;
        let mut metadata = self.task_metadata.lock().unwrap();
        if task_type == TaskType::DisplayMap {
            metadata.retain(|_, existing| {
                existing.task_type != TaskType::DisplayMap
                    || existing.owner.window_id != owner.window_id
            });
        }
        metadata.insert(task_id, TaskMetadata { owner, task_type });
        Some(task_id)
    }

    fn decode_result(
        result: background_worker::BackgroundResult,
        metadata: TaskMetadata,
    ) -> Option<TaskResult> {
        let task_id = result.task_id;
        let owner = metadata.owner;
        match metadata.task_type {
            TaskType::Treesitter => Some(TaskResult::Treesitter {
                task_id,
                revision: owner.revision,
                completed: result.downcast::<vim_treesitter::ParseTaskResult>().ok()?,
            }),
            TaskType::Indexer => Some(TaskResult::Index {
                task_id,
                buffer_id: owner.buffer_id?,
                revision: owner.revision,
                result: result
                    .downcast::<Result<vim_indexer::IndexTaskResult, String>>()
                    .ok()?,
            }),

            TaskType::DisplayMap => Some(TaskResult::DisplayMapExpansion {
                task_id,
                window_id: owner.window_id?,
                buffer_id: owner.buffer_id?,
                revision: owner.revision,
                expansion: result.downcast::<display_map::DisplayMapExpansion>().ok()?,
            }),
            TaskType::Files => Some(TaskResult::SaveFile {
                task_id,
                buffer_id: owner.buffer_id?,
                revision: owner.revision,
                result: result.downcast::<files::SaveTaskResult>().ok()?,
            }),
        }
    }

    pub fn has_pending_saves(&self) -> bool {
        let metadata = self.task_metadata.lock().unwrap();
        metadata
            .values()
            .any(|meta| meta.task_type == TaskType::Files)
    }
}

/// Speculative idle prefetch grows the highlighted margin around the
/// viewport gradually across repeated idle ticks instead of requesting
/// the full margin in one shot. `highlight_run` parses any newly
/// requested, not-yet-cached rows synchronously, so a large one-shot
/// margin (previously up to 1000 rows before + 500 after) could stall
/// the main loop for a visible amount of time. Ramping the margin by a
/// bounded step every ~50ms tick keeps each synchronous parse small
/// while still reaching full prefetch coverage within about half a
/// second of going idle.
pub fn schedule_state_updates(app: &mut App, idle_elapsed: Option<std::time::Duration>) {
    const IDLE_EXPAND_STEP_BEFORE: u32 = 100;
    const IDLE_EXPAND_STEP_AFTER: u32 = 50;
    const IDLE_EXPAND_MAX_BEFORE: u32 = 1000;
    const IDLE_EXPAND_MAX_AFTER: u32 = 500;

    let (expand_before, expand_after) = match idle_elapsed {
        Some(elapsed) => {
            let ticks = elapsed.as_millis() as u32 / 50;
            (
                ticks
                    .saturating_mul(IDLE_EXPAND_STEP_BEFORE)
                    .min(IDLE_EXPAND_MAX_BEFORE),
                ticks
                    .saturating_mul(IDLE_EXPAND_STEP_AFTER)
                    .min(IDLE_EXPAND_MAX_AFTER),
            )
        }
        None => (0, 0),
    };

    let window_ids: Vec<_> = WindowOps::window_buffers(&app.ui)
        .into_iter()
        .map(|(window_id, _)| window_id)
        .collect();

    for window_id in window_ids {
        schedule_window_display_map(app, window_id);
        schedule_window_highlight(app, window_id, expand_before, expand_after);
        schedule_window_treesitter(app, window_id);
        schedule_window_indexer(app, window_id);
    }
}

fn schedule_window_display_map(app: &mut App, window_id: vim_ui::WindowId) -> Option<()> {
    const CHUNK_ROWS: u32 = 4_096;

    let buffer_id = WindowOps::window_buffer(&app.ui, window_id)?;
    let revision = app.model.buffer_state_mut(buffer_id)?.revision;
    let buffer = app.model.get_buffer(buffer_id).ok()?;
    let snapshot = buffer.snapshot().as_inner().clone();
    let window = app.ui.window(window_id).and_then(Window::window_state)?;

    if window.pending_display_map.is_some() {
        return None;
    }

    let cursor_row = if window.selections.selections.is_empty() {
        0
    } else {
        window.selections.primary().head().to_point(&snapshot).row
    };

    let requested_rows = window
        .display_map
        .nearest_missing_range(cursor_row, CHUNK_ROWS)?;
    let input = window.display_map.expansion_input(requested_rows.clone())?;
    let generation = input.generation.clone();
    let sequence = window.sequence.clone();
    let owner = crate::app::services::TaskOwner {
        buffer_id: Some(buffer_id),
        window_id: Some(window_id),
        revision,
    };

    let task = app.services.spawn_cancellable_task(
        "display_map",
        sequence,
        owner,
        crate::app::services::TaskType::DisplayMap,
        move |token| display_map::build_expansion(input, &token),
    );

    if task.is_some() {
        let window_mut = app
            .ui
            .window_mut(window_id)
            .and_then(Window::window_state_mut)?;
        window_mut.pending_display_map = Some((generation, requested_rows));
    }

    Some(())
}

pub fn schedule_window_highlight(
    app: &mut App,
    window_id: vim_ui::WindowId,
    expand_before: u32,
    expand_after: u32,
) -> Option<()> {
    if !app.syntax_highlight {
        return Some(());
    }
    let buffer_id = WindowOps::window_buffer(&app.ui, window_id)?;
    let buffer = app.model.get_buffer(buffer_id).ok()?;
    let snapshot = buffer.snapshot().as_inner().clone();
    let file_path = buffer.path().and_then(|p| p.to_str()).map(str::to_owned);
    let window = app.ui.window(window_id).and_then(Window::window_state)?;

    let display_map_snapshot = window.display_map.snapshot();
    let scroll_y = display_map_snapshot.scroll_y;
    let viewport_height = window.viewport.height as u32;
    let start_row = display_map_snapshot
        .try_buffer_row_for_display_row(scroll_y)
        .unwrap_or(0);
    let end_row = display_map_snapshot
        .try_buffer_row_for_display_row(
            (scroll_y + viewport_height).min(display_map_snapshot.row_count()),
        )
        .unwrap_or_else(|| display_map_snapshot.buffer_snapshot().max_point().row);

    let colorscheme = app.colorscheme.as_ref();
    let fallback_colorscheme;
    let cs_ref = match colorscheme {
        Some(cs) => cs,
        None => {
            fallback_colorscheme = vim_colorscheme::ColorScheme::load_default();
            &fallback_colorscheme
        }
    };

    let highlights = &mut app.model.buffer_state_mut(buffer_id)?.highlights;
    textmate::highlight_run(
        highlights,
        &snapshot,
        file_path.as_deref(),
        start_row,
        end_row,
        expand_before,
        expand_after,
        app.highlighter.as_ref(),
        cs_ref,
    );

    Some(())
}

fn schedule_window_treesitter(app: &mut App, window_id: vim_ui::WindowId) -> Option<()> {
    if !app.treesitter_enabled {
        return Some(());
    }
    let buffer_id = WindowOps::window_buffer(&app.ui, window_id)?;
    let revision = app.model.buffer_state_mut(buffer_id)?.revision;
    let buffer = app.model.get_buffer(buffer_id).ok()?;
    let path = buffer.path()?.to_str()?;
    let grammar = treesitter::Grammar::from_path(path)?;
    let changedtick = buffer.changedtick();

    if !app.services.treesitter.is_parsing(buffer_id)
        && app.services.treesitter.syntax_tree(buffer_id).is_none()
    {
        if let Some(state) = app.model.buffer_state(buffer_id) {
            if let Ok(syntax_tree) = &state.treesitter {
                if syntax_tree.grammar() == grammar {
                    app.services.treesitter.initialize_from_parsed(
                        buffer_id,
                        changedtick,
                        grammar,
                        syntax_tree.clone(),
                    );
                }
            }
        }
    }

    if !app
        .services
        .treesitter
        .should_parse(buffer_id, changedtick, grammar)
    {
        return None;
    }

    let sequence = app
        .services
        .treesitter
        .begin_parse(buffer_id, changedtick, grammar);
    let snapshot = buffer.snapshot().clone();
    let owner = crate::app::services::TaskOwner {
        buffer_id: Some(buffer_id),
        window_id: Some(window_id),
        revision,
    };

    let old_tree = app.services.treesitter.syntax_tree(buffer_id).cloned();
    let task_id = app.services.spawn_cancellable_task(
        "treesitter",
        sequence,
        owner,
        crate::app::services::TaskType::Treesitter,
        move |token| {
            let cancelled = move || token.is_cancelled();
            let res =
                treesitter::parse_snapshot_cancellable(snapshot, grammar, old_tree, cancelled);
            Some(res)
        },
    )?;

    app.services.treesitter.set_pending_task(buffer_id, task_id);
    Some(())
}

fn schedule_window_indexer(app: &mut App, window_id: vim_ui::WindowId) -> Option<()> {
    if !app.indexer_enabled {
        return Some(());
    }
    let buffer_id = WindowOps::window_buffer(&app.ui, window_id)?;
    let revision = app.model.buffer_state_mut(buffer_id)?.revision;
    let buffer = app.model.get_buffer(buffer_id).ok()?;
    let changedtick = buffer.changedtick();

    if app.services.indexer.should_index(buffer_id, changedtick) {
        if let Some(state) = app.model.buffer_state(buffer_id) {
            if let Ok(index_result) = &state.index {
                if index_result.changedtick == changedtick {
                    app.services.indexer.initialize_from_indexed(
                        buffer_id,
                        changedtick,
                        index_result.source_key.clone(),
                        index_result.keywords.clone(),
                    );
                }
            }
        }
    }

    if !app.services.indexer.should_index(buffer_id, changedtick) {
        return None;
    }

    let sequence = app.services.indexer.begin_index(buffer_id, changedtick);
    let snapshot = buffer.snapshot().clone();
    let source_key = buffer.path()?.to_string_lossy().into_owned();
    let owner = crate::app::services::TaskOwner {
        buffer_id: Some(buffer_id),
        window_id: Some(window_id),
        revision,
    };

    let task_id = app.services.spawn_cancellable_task(
        "indexer",
        sequence,
        owner,
        crate::app::services::TaskType::Indexer,
        move |token| {
            let cancelled = move || token.is_cancelled();
            indexer::index_buffer_cancellable(source_key, snapshot, cancelled)
        },
    )?;

    app.services.indexer.set_pending_task(buffer_id, task_id);
    Some(())
}

impl Default for Services {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::AtomicU64;
    use std::time::{Duration, Instant};
    use vim_ui::WindowId;

    #[test]
    fn display_map_expansion_is_decoded_with_owner_metadata() {
        let mut services = Services::new();
        let buffer_id = BufferId::new(7).unwrap();
        let window_id = WindowId::new(8);
        let buffer = text::Buffer::new(
            clock::ReplicaId::LOCAL,
            text::BufferId::new(7).unwrap(),
            "one\ntwo\nthree",
        );
        let map = display_map::DisplayMap::new_windowed(buffer.snapshot().clone(), Some(80), 0..1);
        let input = map.expansion_input(1..3).unwrap();
        services
            .spawn_cancellable_task(
                "display_map",
                Arc::new(AtomicU64::new(0)),
                TaskOwner {
                    buffer_id: Some(buffer_id),
                    window_id: Some(window_id),
                    revision: 9,
                },
                TaskType::DisplayMap,
                move |token| display_map::build_expansion(input, &token),
            )
            .unwrap();

        let deadline = Instant::now() + Duration::from_secs(2);
        while !services.poll() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        let results = services.drain_results();

        assert!(matches!(
            results.as_slice(),
            [TaskResult::DisplayMapExpansion {
                buffer_id: result_buffer_id,
                window_id: result_window_id,
                revision: 9,
                ..
            }] if *result_buffer_id == buffer_id && *result_window_id == window_id
        ));
    }

    #[test]
    fn drain_results_decodes_owner_and_revision() {
        let mut services = Services::new();
        let buffer_id = BufferId::new(7).unwrap();
        let window_id = WindowId::new(8);
        let owner = TaskOwner {
            buffer_id: Some(buffer_id),
            window_id: Some(window_id),
            revision: 9,
        };
        let buffer = text::Buffer::new(
            clock::ReplicaId::LOCAL,
            text::BufferId::new(7).unwrap(),
            "one\ntwo\nthree",
        );
        let map = display_map::DisplayMap::new_windowed(buffer.snapshot().clone(), Some(80), 0..1);
        let input = map.expansion_input(1..3).unwrap();
        services
            .spawn_task(
                "display_map",
                Arc::new(AtomicU64::new(0)),
                owner,
                TaskType::DisplayMap,
                move || {
                    display_map::build_expansion(
                        input,
                        &background_worker::CancellationToken::default(),
                    )
                    .unwrap()
                },
            )
            .unwrap();

        let deadline = Instant::now() + Duration::from_secs(2);
        while !services.poll() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        let results = services.drain_results();

        assert_eq!(results.len(), 1);
        assert!(matches!(
            &results[0],
            TaskResult::DisplayMapExpansion {
                buffer_id: b,
                window_id: w,
                revision: 9,
                ..
            } if *b == buffer_id && *w == window_id
        ));
    }
}
