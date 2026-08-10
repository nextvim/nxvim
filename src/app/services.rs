use crate::app::buffer_manager::TabId;
use vim_buffer::BufferId;

pub use textmate as highlight;
pub use vim_clipboard as clipboard;
pub use vim_indexer as indexer;
pub use vim_macros as macros;
pub use vim_treesitter as treesitter;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TaskType {
    Highlight,
    DisplayMap,
    Indexer,
    Treesitter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OwnerId {
    pub buffer_id: Option<BufferId>,
    pub tab_id: Option<TabId>,
}

pub struct Services {
    pub background_workers: background_worker::WorkerManager,
    pub clipboard: clipboard::Clipboard,
    pub highlight: highlight::HighlightService,
    pub indexer: indexer::Indexer,
    pub macros: macros::MacroRecorder,
    pub treesitter: treesitter::TreeSitterService,
    pub results: Vec<background_worker::BackgroundResult>,
    pub task_metadata:
        std::sync::Mutex<std::collections::HashMap<background_worker::TaskId, (OwnerId, TaskType)>>,
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
            results: Vec::new(),
            task_metadata: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }

    pub fn poll(&mut self) -> bool {
        struct ResultsCollector<'a> {
            results: &'a mut Vec<background_worker::BackgroundResult>,
        }

        impl<'a> background_worker::WorkerResultHandler for ResultsCollector<'a> {
            fn handle_result(&mut self, result: background_worker::BackgroundResult) {
                self.results.push(result);
            }
        }

        let mut collector = ResultsCollector {
            results: &mut self.results,
        };
        let count = self.background_workers.poll(&mut collector);
        count > 0 || !self.results.is_empty()
    }

    pub fn spawn_task<T, F>(
        &self,
        worker_name: &str,
        sequence: std::sync::Arc<std::sync::atomic::AtomicU64>,
        owner_id: OwnerId,
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
            .insert(task_id, (owner_id, task_type));
        Some(task_id)
    }

    pub fn spawn_cancellable_task<T, F>(
        &self,
        worker_name: &str,
        sequence: std::sync::Arc<std::sync::atomic::AtomicU64>,
        owner_id: OwnerId,
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
        self.task_metadata
            .lock()
            .unwrap()
            .insert(task_id, (owner_id, task_type));
        Some(task_id)
    }
}

impl Default for Services {
    fn default() -> Self {
        Self::new()
    }
}
