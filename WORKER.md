# Background Worker Design Plan

This document outlines the architecture for the new `background_worker` crate, which is a generic, reusable library based on the current implementation in `src/services/background.rs`.

## Goals
1. **Generic Design**: Tasks and results should support any type, while still allowing a single worker thread to execute different tasks.
2. **Worker Manager**: A manager that tracks multiple workers or task sequences, coordinates execution, and manages lifecycle.
3. **Polling & Trait-Based Results**: An elegant way to poll completed tasks and automatically dispatch results to the owner via a trait interface.
4. **Cooperative Cancellation**: Retain and improve the cancellation sequence pattern from `background.rs` using `CancellationToken`.

---

## Architectural Components

### 1. `TaskId` and `CancellationToken`
* Monotonically increasing identifier to track tasks.
* Cancellation sequence tracking via `Arc<AtomicU64>` to invalidate obsolete tasks.

### 2. Task Definition
A task represents a unit of work that can be executed on a background thread:
```rust
pub type TaskJob = Box<dyn FnOnce(CancellationToken) -> Option<Box<dyn Any + Send>> + Send>;
```

### 3. `Worker` (Core Engine)
Wraps a background thread and channels for job submission and result collection.
* Uses standard Rust `std::thread` and `std::sync::mpsc`.
* Can execute any typed job by boxing it as `dyn Any + Send`.

### 4. `WorkerResult`
Encapsulates a completed task:
```rust
pub struct WorkerResult {
    pub task_id: TaskId,
    pub output: Box<dyn Any + Send>,
}
```

### 5. `WorkerResultHandler` Trait
An interface that owners implement to receive and handle typed results when polled:
```rust
pub trait WorkerResultHandler {
    /// Dispatches a type-erased result to the handler.
    fn handle_result(&mut self, task_id: TaskId, result: Box<dyn Any + Send>);
}
```

### 6. `WorkerManager`
Manages one or more workers, exposes APIs to spawn tasks in named sequences, and handles polling.
```rust
pub struct WorkerManager {
    workers: HashMap<String, Worker>,
}

impl WorkerManager {
    pub fn new() -> Self;
    pub fn add_worker(&mut self, name: &str);
    
    /// Spawns a task on a specific worker.
    pub fn spawn_task<T, F>(&self, worker_name: &str, sequence: Arc<AtomicU64>, job: F) -> TaskId
    where
        T: Any + Send + 'static,
        F: FnOnce(CancellationToken) -> Option<T> + Send + 'static;

    /// Polls all workers and dispatches results to the given handler.
    pub fn poll(&self, handler: &mut dyn WorkerResultHandler);
}
```

---

## Implementation Tasks

1. **Create Crate**: Initialize `crates/background_worker`.
2. **Add to Workspace**: Include `"crates/background_worker"` in the root `Cargo.toml`.
3. **Implement Core Engine**: Add `TaskId`, `CancellationToken`, `WorkerResult`, and `Worker` implementation.
4. **Implement WorkerManager**: Add named worker pools and the `poll` method with `WorkerResultHandler` dispatch.
5. **Write Unit Tests**: Cover worker execution, cancellation, obsolete task skipping, and manager polling.
6. **Migrate existing code**: If applicable, explain how to integrate it.
