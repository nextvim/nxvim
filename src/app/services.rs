use std::collections::HashMap;
use std::sync::Mutex;

pub use textmate as highlight;
pub use vim_clipboard as clipboard;
pub use vim_indexer as indexer;
pub use vim_macros as macros;
pub use vim_treesitter as treesitter;

use vim_buffer::BufferId;
use vim_ui::WindowId;

pub type TaskId = background_worker::TaskId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TaskType {
    Highlight,
    DisplayMap,
    Indexer,
    Treesitter,
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
        buffer_id: BufferId,
        revision: u64,
        result: Result<vim_treesitter::SyntaxTree, String>,
    },
    Index {
        task_id: TaskId,
        buffer_id: BufferId,
        revision: u64,
        result: Result<vim_indexer::IndexTaskResult, String>,
    },
    Highlight {
        task_id: TaskId,
        window_id: WindowId,
        buffer_id: BufferId,
        revision: u64,
        highlights: Vec<textmate::HighlightSpan>,
    },
    DisplayMapExpansion {
        task_id: TaskId,
        window_id: WindowId,
        buffer_id: BufferId,
        revision: u64,
        expansion: display_map::DisplayMapExpansion,
    },
}

pub(super) struct TaskMetadata {
    pub owner: TaskOwner,
    pub task_type: TaskType,
}

pub struct Services {
    background_workers: background_worker::WorkerManager,
    pub clipboard: clipboard::Clipboard,
    pub highlight: highlight::HighlightService,
    pub indexer: indexer::Indexer,
    pub macros: macros::MacroRecorder,
    pub treesitter: treesitter::TreeSitterService,
    raw_results: Vec<background_worker::BackgroundResult>,
    task_metadata: Mutex<HashMap<background_worker::TaskId, TaskMetadata>>,
}

impl Services {
    pub fn new() -> Self {
        let mut background_workers = background_worker::WorkerManager::new();
        background_workers.add_worker("display_map");
        Self {
            background_workers,
            clipboard: clipboard::Clipboard::new(),
            highlight: highlight::HighlightService::new(),
            indexer: indexer::Indexer::new(),
            macros: macros::MacroRecorder::new(),
            treesitter: treesitter::TreeSitterService::new(),
            raw_results: Vec::new(),
            task_metadata: Mutex::new(HashMap::new()),
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
                buffer_id: owner.buffer_id?,
                revision: owner.revision,
                result: result
                    .downcast::<Result<vim_treesitter::SyntaxTree, String>>()
                    .ok()?,
            }),
            TaskType::Indexer => Some(TaskResult::Index {
                task_id,
                buffer_id: owner.buffer_id?,
                revision: owner.revision,
                result: result
                    .downcast::<Result<vim_indexer::IndexTaskResult, String>>()
                    .ok()?,
            }),
            TaskType::Highlight => Some(TaskResult::Highlight {
                task_id,
                window_id: owner.window_id?,
                buffer_id: owner.buffer_id?,
                revision: owner.revision,
                highlights: result.downcast::<Vec<textmate::HighlightSpan>>().ok()?,
            }),
            TaskType::DisplayMap => Some(TaskResult::DisplayMapExpansion {
                task_id,
                window_id: owner.window_id?,
                buffer_id: owner.buffer_id?,
                revision: owner.revision,
                expansion: result.downcast::<display_map::DisplayMapExpansion>().ok()?,
            }),
        }
    }
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
    use vim_buffer::BufferId;
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
        let owner = TaskOwner {
            buffer_id: Some(buffer_id),
            window_id: Some(WindowId::new(8)),
            revision: 9,
        };
        services
            .spawn_task(
                "display_map",
                Arc::new(AtomicU64::new(0)),
                owner,
                TaskType::Highlight,
                Vec::<textmate::HighlightSpan>::new,
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
            TaskResult::Highlight {
                buffer_id: result_buffer_id,
                window_id: result_window_id,
                revision: 9,
                ..
            } if *result_buffer_id == buffer_id && *result_window_id == WindowId::new(8)
        ));
    }
}
