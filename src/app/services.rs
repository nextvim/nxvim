//! Application-owned background work orchestration.
//!
//! Services carry stable kernel ownership and the captured buffer revision next
//! to every task. The kernel never sees workers; the application validates a
//! result before handing it to the view, filesystem, or script host.

use crate::kernel::{ids::BufferId, ids::WindowId};
use background_worker::{BackgroundResult, CancellationToken, TaskId, WorkerManager};
use std::{
    collections::HashMap,
    sync::{Arc, atomic::AtomicU64},
};
use vim_buffer::Revision;

use crate::app::request::AppRequest;
use crate::kernel::outcome::Effect;

pub const DISPLAY_MAP_WORKER: &str = "display-map";
pub const FILE_WORKER: &str = "file";
pub const TREESITTER_WORKER: &str = "tree-sitter";
pub const INDEXER_WORKER: &str = "indexer";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TaskKind {
    DisplayMap,
    File,
    TreeSitter,
    Indexer,
}

impl TaskKind {
    fn worker_name(self) -> &'static str {
        match self {
            Self::DisplayMap => DISPLAY_MAP_WORKER,
            Self::File => FILE_WORKER,
            Self::TreeSitter => TREESITTER_WORKER,
            Self::Indexer => INDEXER_WORKER,
        }
    }
}

#[derive(Clone, Debug)]
pub struct TaskMetadata {
    pub id: TaskId,
    pub kind: TaskKind,
    pub buffer: Option<BufferId>,
    pub window: Option<WindowId>,
    pub revision: Option<Revision>,
}

pub enum ServiceOutput {
    DisplayMap(display_map::DisplayMapExpansion),
    File(files::SaveTaskResult),
    TreeSitter(vim_treesitter::ParseTaskResult),
    Indexer(vim_indexer::IndexTaskResult),
}

pub struct ServiceResult {
    pub metadata: TaskMetadata,
    pub output: ServiceOutput,
}

#[derive(Default)]
pub struct Services {
    workers: WorkerManager,
    sequences: HashMap<TaskKind, Arc<AtomicU64>>,
    pending: HashMap<TaskId, TaskMetadata>,
}

impl Services {
    pub fn new() -> Self {
        let mut services = Self::default();
        for kind in [
            TaskKind::DisplayMap,
            TaskKind::File,
            TaskKind::TreeSitter,
            TaskKind::Indexer,
        ] {
            services.workers.add_worker(kind.worker_name());
            let sequence = services
                .workers
                .worker(kind.worker_name())
                .expect("worker was registered")
                .cancellation_sequence();
            services.sequences.insert(kind, sequence);
        }
        services
    }

    fn track(
        &mut self,
        id: TaskId,
        kind: TaskKind,
        buffer: Option<BufferId>,
        window: Option<WindowId>,
        revision: Option<Revision>,
    ) -> TaskId {
        self.pending.insert(
            id,
            TaskMetadata {
                id,
                kind,
                buffer,
                window,
                revision,
            },
        );
        id
    }

    pub fn spawn_display_map(
        &mut self,
        buffer: BufferId,
        window: WindowId,
        input: display_map::DisplayMapExpansionInput,
    ) -> Option<TaskId> {
        let kind = TaskKind::DisplayMap;
        let revision = input.generation.buffer_version.clone();
        let sequence = self.sequences.get(&kind)?.clone();
        let id =
            self.workers
                .spawn_cancellable_task(kind.worker_name(), sequence, move |cancel| {
                    display_map::build_expansion(input, &cancel)
                })?;
        Some(self.track(id, kind, Some(buffer), Some(window), Some(revision)))
    }

    pub fn spawn_file_save(
        &mut self,
        snapshot: vim_buffer::BufferSnapshot,
        path: std::path::PathBuf,
        options: vim_buffer::BufferOptions,
    ) -> Option<TaskId> {
        let kind = TaskKind::File;
        let buffer = snapshot.id();
        let revision = snapshot.as_inner().version.clone();
        let sequence = self.sequences.get(&kind)?.clone();
        let id =
            self.workers
                .spawn_cancellable_task(kind.worker_name(), sequence, move |cancel| {
                    files::save_file_cancellable(snapshot, path, options, || cancel.is_cancelled())
                })?;
        Some(self.track(id, kind, Some(buffer), None, Some(revision)))
    }

    pub fn spawn_tree_sitter(
        &mut self,
        snapshot: vim_buffer::BufferSnapshot,
        grammar: vim_treesitter::Grammar,
    ) -> Option<TaskId> {
        let kind = TaskKind::TreeSitter;
        let buffer = snapshot.id();
        let revision = snapshot.as_inner().version.clone();
        let sequence = self.sequences.get(&kind)?.clone();
        let id =
            self.workers
                .spawn_cancellable_task(kind.worker_name(), sequence, move |cancel| {
                    Some(vim_treesitter::parse_snapshot_cancellable(
                        snapshot,
                        grammar,
                        None,
                        || cancel.is_cancelled(),
                    ))
                })?;
        Some(self.track(id, kind, Some(buffer), None, Some(revision)))
    }

    pub fn spawn_indexer<F>(
        &mut self,
        buffer: BufferId,
        revision: Revision,
        job: F,
    ) -> Option<TaskId>
    where
        F: FnOnce(CancellationToken) -> Option<vim_indexer::IndexTaskResult> + Send + 'static,
    {
        let kind = TaskKind::Indexer;
        let sequence = self.sequences.get(&kind)?.clone();
        let id = self
            .workers
            .spawn_cancellable_task(kind.worker_name(), sequence, job)?;
        Some(self.track(id, kind, Some(buffer), None, Some(revision)))
    }

    pub fn cancel(&mut self, kind: TaskKind) {
        if let Some(sequence) = self.sequences.get(&kind).cloned() {
            // The worker crate cancels cooperatively when a newer task advances
            // the sequence. A no-op replacement also covers tasks that are
            // already queued but have not started running yet.
            let _ =
                self.workers
                    .spawn_cancellable_task(kind.worker_name(), sequence, |_| None::<()>);
            self.pending.retain(|_, task| task.kind != kind);
        }
    }

    pub fn registered_workers(&self) -> [&'static str; 4] {
        [
            DISPLAY_MAP_WORKER,
            FILE_WORKER,
            TREESITTER_WORKER,
            INDEXER_WORKER,
        ]
    }

    pub fn pending(&self) -> impl Iterator<Item = &TaskMetadata> {
        self.pending.values()
    }

    pub fn has_pending(&self, kind: TaskKind, window: Option<WindowId>) -> bool {
        self.pending
            .values()
            .any(|task| task.kind == kind && task.window == window)
    }

    /// Acknowledges a result only after the application has admitted and
    /// applied it. Polling deliberately leaves metadata pending so a stale or
    /// otherwise rejected result cannot clear newer application state.
    pub fn finish(&mut self, id: TaskId) -> Option<TaskMetadata> {
        self.pending.remove(&id)
    }

    pub fn poll(&mut self) -> Vec<ServiceResult> {
        let mut raw = Vec::new();
        for worker_name in [
            DISPLAY_MAP_WORKER,
            FILE_WORKER,
            TREESITTER_WORKER,
            INDEXER_WORKER,
        ] {
            if let Some(worker) = self.workers.worker(worker_name) {
                while let Some(result) = worker.try_recv() {
                    raw.push(result);
                }
            }
        }
        raw.into_iter()
            .filter_map(|result| self.decode(result))
            .collect()
    }

    fn decode(&mut self, result: BackgroundResult) -> Option<ServiceResult> {
        let metadata = self.pending.get(&result.task_id)?.clone();
        let output = match metadata.kind {
            TaskKind::DisplayMap => ServiceOutput::DisplayMap(result.downcast().ok()?),
            TaskKind::File => ServiceOutput::File(result.downcast().ok()?),
            TaskKind::TreeSitter => ServiceOutput::TreeSitter(result.downcast().ok()?),
            TaskKind::Indexer => ServiceOutput::Indexer(result.downcast().ok()?),
        };
        Some(ServiceResult { metadata, output })
    }
}

/// Translates kernel-side effects into application-level requests.
pub fn describe_effect(effect: &Effect) -> Option<AppRequest> {
    match effect {
        Effect::FileSaved {
            path,
            bytes_written,
        } => Some(AppRequest::ShowMessage(format!(
            "\"{}\" {}B written",
            path.display(),
            bytes_written
        ))),
        Effect::FileSaveFailed { message } => Some(AppRequest::ShowMessage(message.clone())),
        Effect::OptionMessage { message } => Some(AppRequest::ShowMessage(message.clone())),
        Effect::ClipboardWrite { text, primary } => {
            let reg_name = if *primary {
                vim_clipboard::RegisterName::Selection
            } else {
                vim_clipboard::RegisterName::System
            };
            vim_clipboard::write_system_clipboard(reg_name, text);
            None
        }
        Effect::ConfirmSubstitute { replacement, .. } => Some(AppRequest::ShowMessage(format!(
            "replace with {} (y/n/a/q/l)?",
            replacement
        ))),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{thread, time::Duration};

    #[test]
    fn service_result_keeps_ownership_and_decodes_output() {
        let buffer_id = BufferId::new(1).unwrap();
        let buffer = vim_buffer::Buffer::new(buffer_id, clock::ReplicaId::LOCAL, "word");
        let revision = buffer.revision();
        let changedtick = buffer.changedtick();
        let mut services = Services::new();
        let id = services
            .spawn_indexer(buffer_id, revision.clone(), move |_| {
                Some(vim_indexer::IndexTaskResult {
                    buffer_id,
                    changedtick,
                    source_key: "memory".into(),
                    keywords: HashMap::new(),
                })
            })
            .unwrap();
        for _ in 0..100 {
            if let Some(result) = services.poll().into_iter().next() {
                assert_eq!(result.metadata.id, id);
                assert_eq!(result.metadata.buffer, Some(buffer_id));
                assert_eq!(result.metadata.revision, Some(revision));
                assert!(
                    matches!(result.output, ServiceOutput::Indexer(output) if output.source_key == "memory")
                );
                assert_eq!(services.pending().count(), 1);
                assert_eq!(services.finish(id).map(|task| task.id), Some(id));
                assert_eq!(services.pending().count(), 0);
                return;
            }
            thread::sleep(Duration::from_millis(2));
        }
        panic!("service task did not complete");
    }

    #[test]
    fn cancellation_removes_pending_work_and_suppresses_result() {
        let buffer_id = BufferId::new(2).unwrap();
        let buffer = vim_buffer::Buffer::new(buffer_id, clock::ReplicaId::LOCAL, "word");
        let revision = buffer.revision();
        let changedtick = buffer.changedtick();
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let mut services = Services::new();
        services
            .spawn_indexer(buffer_id, revision, move |cancel| {
                started_tx.send(()).unwrap();
                while !cancel.is_cancelled() {
                    std::thread::yield_now();
                }
                Some(vim_indexer::IndexTaskResult {
                    buffer_id,
                    changedtick,
                    source_key: "cancelled".into(),
                    keywords: HashMap::new(),
                })
            })
            .unwrap();
        started_rx.recv_timeout(Duration::from_secs(1)).unwrap();

        services.cancel(TaskKind::Indexer);
        assert_eq!(services.pending().count(), 0);
        thread::sleep(Duration::from_millis(10));
        assert!(services.poll().is_empty());
    }
}
