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
    DisplayMap {
        task_id: TaskId,
        window_id: WindowId,
        buffer_id: BufferId,
        revision: u64,
        map: display_map::DisplayMap,
        height: u32,
        layout_width: u32,
    },
}

pub(super) struct TaskMetadata {
    pub owner: TaskOwner,
    pub task_type: TaskType,
}
