use std::any::Any;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, mpsc};
use std::thread::{self, JoinHandle};

/// A monotonically increasing identifier used to discard obsolete work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TaskId(pub u64);

/// A value produced by a completed background task.
pub struct BackgroundResult {
    pub task_id: TaskId,
    pub worker_name: String,
    output: Box<dyn Any + Send>,
}

impl std::fmt::Debug for BackgroundResult {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("BackgroundResult")
            .field("task_id", &self.task_id)
            .field("worker_name", &self.worker_name)
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
        let task_id = self.task_id;
        let worker_name = self.worker_name.clone();
        match self.output.downcast::<T>() {
            Ok(output) => Ok(*output),
            Err(output) => Err(Self {
                task_id,
                worker_name,
                output,
            }),
        }
    }
}

type Job = Box<dyn FnOnce() -> Option<Box<dyn Any + Send>> + Send>;

#[derive(Clone)]
pub struct CancellationToken {
    task_id: TaskId,
    latest_task_id: Arc<AtomicU64>,
}

impl CancellationToken {
    pub fn is_cancelled(&self) -> bool {
        self.latest_task_id.load(Ordering::Acquire) > self.task_id.0
    }
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self {
            task_id: TaskId(0),
            latest_task_id: Arc::new(AtomicU64::new(0)),
        }
    }
}

struct BackgroundTask {
    task_id: TaskId,
    latest_task_id: Arc<AtomicU64>,
    job: Job,
}

enum WorkerMessage {
    Run(BackgroundTask),
    Shutdown,
}

/// A dedicated thread for CPU-bound work.
pub struct BackgroundWorker {
    name: String,
    task_tx: mpsc::Sender<WorkerMessage>,
    result_rx: mpsc::Receiver<BackgroundResult>,
    next_task_id: Arc<AtomicU64>,
    worker_thread: Option<JoinHandle<()>>,
}

impl BackgroundWorker {
    pub fn new(name: impl Into<String>, next_task_id: Arc<AtomicU64>) -> Self {
        let name = name.into();
        let (task_tx, task_rx) = mpsc::channel();
        let (result_tx, result_rx) = mpsc::channel();

        let thread_name = format!("worker-{}", name);
        let worker_name_clone = name.clone();
        let worker_thread = thread::Builder::new()
            .name(thread_name)
            .spawn(move || run_worker(worker_name_clone, task_rx, result_tx))
            .expect("failed to spawn background worker");

        Self {
            name,
            task_tx,
            result_rx,
            next_task_id,
            worker_thread: Some(worker_thread),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
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
        self.spawn_cancellable_task(latest_task_id, move |_| Some(job()))
    }

    /// Schedules cooperatively cancellable work in a related task sequence.
    pub fn spawn_cancellable_task<T, F>(&self, latest_task_id: Arc<AtomicU64>, job: F) -> TaskId
    where
        T: Any + Send + 'static,
        F: FnOnce(CancellationToken) -> Option<T> + Send + 'static,
    {
        let task_id = TaskId(self.next_task_id.fetch_add(1, Ordering::Relaxed));
        latest_task_id.store(task_id.0, Ordering::Release);
        let token = CancellationToken {
            task_id,
            latest_task_id: latest_task_id.clone(),
        };
        let task = BackgroundTask {
            task_id,
            latest_task_id,
            job: Box::new(move || job(token).map(|output| Box::new(output) as Box<dyn Any + Send>)),
        };
        let _ = self.task_tx.send(WorkerMessage::Run(task));
        task_id
    }

    /// Non-blockingly polls for completed work.
    pub fn try_recv(&self) -> Option<BackgroundResult> {
        self.result_rx.try_recv().ok()
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

fn run_worker(
    worker_name: String,
    task_rx: mpsc::Receiver<WorkerMessage>,
    result_tx: mpsc::Sender<BackgroundResult>,
) {
    while let Ok(message) = task_rx.recv() {
        let WorkerMessage::Run(task) = message else {
            break;
        };
        if is_obsolete(&task) {
            continue;
        }
        let Some(output) = (task.job)() else {
            continue;
        };
        if task.latest_task_id.load(Ordering::Acquire) > task.task_id.0 {
            continue;
        }
        if result_tx
            .send(BackgroundResult {
                task_id: task.task_id,
                worker_name: worker_name.clone(),
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

/// A trait for receiving completed task results.
pub trait WorkerResultHandler {
    fn handle_result(&mut self, result: BackgroundResult);
}

/// A manager that coordinates multiple named background workers and dispatches their results.
pub struct WorkerManager {
    workers: HashMap<String, BackgroundWorker>,
    next_task_id: Arc<AtomicU64>,
}

impl Default for WorkerManager {
    fn default() -> Self {
        Self {
            workers: HashMap::new(),
            next_task_id: Arc::new(AtomicU64::new(1)),
        }
    }
}

impl WorkerManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a new worker if it does not already exist.
    pub fn add_worker(&mut self, name: &str) {
        let next_id = self.next_task_id.clone();
        self.workers
            .entry(name.to_string())
            .or_insert_with(|| BackgroundWorker::new(name, next_id));
    }

    /// Gets a reference to a worker by name.
    pub fn worker(&self, name: &str) -> Option<&BackgroundWorker> {
        self.workers.get(name)
    }

    /// Spawns a task on a specific worker.
    pub fn spawn_task<T, F>(
        &self,
        worker_name: &str,
        sequence: Arc<AtomicU64>,
        job: F,
    ) -> Option<TaskId>
    where
        T: Any + Send + 'static,
        F: FnOnce() -> T + Send + 'static,
    {
        self.workers
            .get(worker_name)
            .map(|w| w.spawn_task(sequence, job))
    }

    /// Spawns a cancellable task on a specific worker.
    pub fn spawn_cancellable_task<T, F>(
        &self,
        worker_name: &str,
        sequence: Arc<AtomicU64>,
        job: F,
    ) -> Option<TaskId>
    where
        T: Any + Send + 'static,
        F: FnOnce(CancellationToken) -> Option<T> + Send + 'static,
    {
        self.workers
            .get(worker_name)
            .map(|w| w.spawn_cancellable_task(sequence, job))
    }

    /// Polls all managed workers for finished tasks, sending them to the handler.
    /// Returns the number of results handled in this poll cycle.
    pub fn poll(&self, handler: &mut dyn WorkerResultHandler) -> usize {
        let mut count = 0;
        for worker in self.workers.values() {
            while let Some(result) = worker.try_recv() {
                handler.handle_result(result);
                count += 1;
            }
        }
        count
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    struct TestHandler {
        results: Vec<BackgroundResult>,
    }

    impl WorkerResultHandler for TestHandler {
        fn handle_result(&mut self, result: BackgroundResult) {
            self.results.push(result);
        }
    }

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
        let worker = BackgroundWorker::new("test-worker", Arc::new(AtomicU64::new(1)));
        let sequence = worker.cancellation_sequence();
        let task_id = worker.spawn_task(sequence, || String::from("done"));
        let result = receive(&worker);

        assert_eq!(result.task_id, task_id);
        assert_eq!(result.worker_name, "test-worker");
        assert_eq!(result.downcast::<String>().unwrap(), "done");
    }

    #[test]
    fn cancels_running_cooperative_jobs() {
        let worker = BackgroundWorker::new("cancellation-worker", Arc::new(AtomicU64::new(1)));
        let sequence = worker.cancellation_sequence();
        let (started_tx, started_rx) = mpsc::channel();
        worker.spawn_cancellable_task(sequence.clone(), move |cancel| {
            started_tx.send(()).unwrap();
            while !cancel.is_cancelled() {
                thread::yield_now();
            }
            None::<u32>
        });
        started_rx.recv_timeout(Duration::from_secs(1)).unwrap();

        let latest = worker.spawn_task(sequence, || 2_u32);
        let result = receive(&worker);

        assert_eq!(result.task_id, latest);
        assert_eq!(result.downcast::<u32>().unwrap(), 2);
        assert!(worker.try_recv().is_none());
    }

    #[test]
    fn skips_queued_obsolete_jobs() {
        let worker = BackgroundWorker::new("obsolete-worker", Arc::new(AtomicU64::new(1)));
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

    #[test]
    fn manager_polls_multiple_workers_and_dispatches_to_handler() {
        let mut manager = WorkerManager::new();
        manager.add_worker("worker-1");
        manager.add_worker("worker-2");

        let seq_1 = manager.worker("worker-1").unwrap().cancellation_sequence();
        let seq_2 = manager.worker("worker-2").unwrap().cancellation_sequence();

        let t1 = manager.spawn_task("worker-1", seq_1, || 10_i32).unwrap();
        let t2 = manager
            .spawn_task("worker-2", seq_2, || String::from("hello"))
            .unwrap();

        // Wait a bit to ensure they run and complete
        thread::sleep(Duration::from_millis(50));

        let mut handler = TestHandler {
            results: Vec::new(),
        };
        let count = manager.poll(&mut handler);

        assert_eq!(count, 2);
        assert_eq!(handler.results.len(), 2);

        // Find results
        let r1 = handler
            .results
            .iter()
            .find(|r| r.task_id == t1 && r.worker_name == "worker-1")
            .unwrap();
        assert_eq!(r1.worker_name, "worker-1");
        assert_eq!(*r1.downcast_ref::<i32>().unwrap(), 10);

        let r2 = handler
            .results
            .iter()
            .find(|r| r.task_id == t2 && r.worker_name == "worker-2")
            .unwrap();
        assert_eq!(r2.worker_name, "worker-2");
        assert_eq!(r2.downcast_ref::<String>().unwrap(), "hello");
    }
}
