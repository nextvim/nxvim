use std::any::Any;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, mpsc};
use std::thread::{self, JoinHandle};

/// A monotonically increasing identifier used to discard obsolete work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TaskId(pub u64);

/// A value produced by a completed background task.
pub struct BackgroundResult {
    pub task_id: TaskId,
    output: Box<dyn Any + Send>,
}

impl std::fmt::Debug for BackgroundResult {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("BackgroundResult")
            .field("task_id", &self.task_id)
            .field("output_type_id", &self.output.as_ref().type_id())
            .finish()
    }
}

impl BackgroundResult {
    /// Borrows the result when its concrete type is `T`.
    pub fn downcast_ref<T: Any>(&self) -> Option<&T> {
        self.output.downcast_ref()
    }

    /// Extracts the result when its concrete type is `T`.
    pub fn downcast<T: Any + Send>(self) -> Result<T, Self> {
        match self.output.downcast::<T>() {
            Ok(output) => Ok(*output),
            Err(output) => Err(Self {
                task_id: self.task_id,
                output,
            }),
        }
    }
}

type Job = Box<dyn FnOnce() -> Box<dyn Any + Send> + Send>;

struct BackgroundTask {
    task_id: TaskId,
    latest_task_id: Arc<AtomicU64>,
    job: Job,
}

enum WorkerMessage {
    Run(BackgroundTask),
    Shutdown,
}

/// A dedicated thread for CPU-bound work that must not block the UI thread.
pub struct BackgroundWorker {
    task_tx: mpsc::Sender<WorkerMessage>,
    result_rx: mpsc::Receiver<BackgroundResult>,
    next_task_id: AtomicU64,
    worker_thread: Option<JoinHandle<()>>,
}

impl BackgroundWorker {
    pub fn new() -> Self {
        let (task_tx, task_rx) = mpsc::channel();
        let (result_tx, result_rx) = mpsc::channel();
        let worker_thread = thread::Builder::new()
            .name("nxvim-background".into())
            .spawn(move || run_worker(task_rx, result_tx))
            .expect("failed to spawn background worker");

        Self {
            task_tx,
            result_rx,
            next_task_id: AtomicU64::new(1),
            worker_thread: Some(worker_thread),
        }
    }

    /// Creates a cancellation sequence shared by a related stream of tasks.
    pub fn cancellation_sequence(&self) -> Arc<AtomicU64> {
        Arc::new(AtomicU64::new(0))
    }

    /// Schedules work and marks earlier tasks in the same sequence obsolete.
    pub fn spawn_task<T, F>(&self, latest_task_id: Arc<AtomicU64>, job: F) -> TaskId
    where
        T: Any + Send + 'static,
        F: FnOnce() -> T + Send + 'static,
    {
        let task_id = TaskId(self.next_task_id.fetch_add(1, Ordering::Relaxed));
        latest_task_id.store(task_id.0, Ordering::Release);
        let task = BackgroundTask {
            task_id,
            latest_task_id,
            job: Box::new(move || Box::new(job())),
        };
        let _ = self.task_tx.send(WorkerMessage::Run(task));
        task_id
    }

    /// Non-blockingly polls for completed work.
    pub fn try_recv(&self) -> Option<BackgroundResult> {
        self.result_rx.try_recv().ok()
    }
}

impl Default for BackgroundWorker {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for BackgroundWorker {
    fn drop(&mut self) {
        let _ = self.task_tx.send(WorkerMessage::Shutdown);
        if let Some(worker_thread) = self.worker_thread.take() {
            let _ = worker_thread.join();
        }
    }
}

fn run_worker(task_rx: mpsc::Receiver<WorkerMessage>, result_tx: mpsc::Sender<BackgroundResult>) {
    while let Ok(message) = task_rx.recv() {
        let WorkerMessage::Run(task) = message else {
            break;
        };
        if is_obsolete(&task) {
            continue;
        }
        let output = (task.job)();
        if task.latest_task_id.load(Ordering::Acquire) > task.task_id.0 {
            continue;
        }
        if result_tx
            .send(BackgroundResult {
                task_id: task.task_id,
                output,
            })
            .is_err()
        {
            break;
        }
    }
}

fn is_obsolete(task: &BackgroundTask) -> bool {
    task.latest_task_id.load(Ordering::Acquire) > task.task_id.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn receive(worker: &BackgroundWorker) -> BackgroundResult {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(result) = worker.try_recv() {
                return result;
            }
            assert!(Instant::now() < deadline, "background task timed out");
            thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn runs_jobs_and_returns_typed_results() {
        let worker = BackgroundWorker::new();
        let sequence = worker.cancellation_sequence();
        let task_id = worker.spawn_task(sequence, || String::from("done"));
        let result = receive(&worker);

        assert_eq!(result.task_id, task_id);
        assert_eq!(result.downcast::<String>().unwrap(), "done");
    }

    #[test]
    fn skips_queued_obsolete_jobs() {
        let worker = BackgroundWorker::new();
        let blocker = worker.cancellation_sequence();
        worker.spawn_task(blocker, || thread::sleep(Duration::from_millis(30)));

        let sequence = worker.cancellation_sequence();
        worker.spawn_task(sequence.clone(), || 1_u32);
        let latest = worker.spawn_task(sequence, || 2_u32);

        let first = receive(&worker);
        let result = if first.task_id == latest {
            first
        } else {
            receive(&worker)
        };
        assert_eq!(result.task_id, latest);
        assert_eq!(result.downcast::<u32>().unwrap(), 2);
        assert!(worker.try_recv().is_none());
    }
}
