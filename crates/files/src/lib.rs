use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, atomic::AtomicU64},
};

use background_worker::TaskId;
use vim_buffer::{BufferId, BufferOptions, BufferSnapshot, ChangedTick, SaveOutcome};

#[derive(Debug, Clone)]
pub struct SaveTaskResult {
    pub buffer_id: BufferId,
    pub changedtick: ChangedTick,
    pub path: PathBuf,
    pub result: Result<SaveOutcome, String>,
}

struct SaveRequest {
    changedtick: ChangedTick,
    pending_task_id: Option<TaskId>,
    latest_task_id: Arc<AtomicU64>,
}

pub struct FilesService {
    requests: HashMap<BufferId, SaveRequest>,
}

impl FilesService {
    pub fn new() -> Self {
        Self {
            requests: HashMap::new(),
        }
    }

    pub fn should_save(&self, buffer_id: BufferId, changedtick: ChangedTick) -> bool {
        self.requests
            .get(&buffer_id)
            .is_none_or(|request| request.changedtick != changedtick)
    }

    pub fn begin_save(&mut self, buffer_id: BufferId, changedtick: ChangedTick) -> Arc<AtomicU64> {
        let request = self
            .requests
            .entry(buffer_id)
            .or_insert_with(|| SaveRequest {
                changedtick,
                pending_task_id: None,
                latest_task_id: Arc::new(AtomicU64::new(0)),
            });
        request.changedtick = changedtick;
        request.latest_task_id.clone()
    }

    pub fn set_pending_task(&mut self, buffer_id: BufferId, task_id: TaskId) {
        if let Some(request) = self.requests.get_mut(&buffer_id) {
            request.pending_task_id = Some(task_id);
        }
    }

    pub fn apply_task_result(&mut self, task_id: TaskId, result: &SaveTaskResult) -> bool {
        let Some(request) = self.requests.get_mut(&result.buffer_id) else {
            return false;
        };
        if request.changedtick != result.changedtick || request.pending_task_id != Some(task_id) {
            return false;
        }
        request.pending_task_id = None;
        true
    }
}

impl Default for FilesService {
    fn default() -> Self {
        Self::new()
    }
}

pub fn save_file(
    snapshot: BufferSnapshot,
    path: PathBuf,
    options: BufferOptions,
) -> SaveTaskResult {
    save_file_cancellable(snapshot, path, options, || false)
        .expect("non-cancellable saving cannot be cancelled")
}

pub fn save_file_cancellable(
    snapshot: BufferSnapshot,
    path: PathBuf,
    options: BufferOptions,
    mut is_cancelled: impl FnMut() -> bool,
) -> Option<SaveTaskResult> {
    if is_cancelled() {
        return None;
    }
    let buffer_id = snapshot.id();
    let changedtick = snapshot.changedtick();
    let path_clone = path.clone();

    let text_ref = snapshot.as_inner().text();
    if is_cancelled() {
        return None;
    }

    let bytes = match vim_buffer::encode_utf8(text_ref.as_ref(), &options) {
        Ok(bytes) => bytes,
        Err(e) => {
            return Some(SaveTaskResult {
                buffer_id,
                changedtick,
                path: path_clone,
                result: Err(format!("Failed to encode buffer: {:?}", e)),
            });
        }
    };

    if is_cancelled() {
        return None;
    }

    match vim_buffer::atomic_write(&path, &bytes) {
        Ok(()) => Some(SaveTaskResult {
            buffer_id,
            changedtick,
            path: path_clone,
            result: Ok(SaveOutcome {
                buffer: buffer_id,
                path,
                bytes_written: bytes.len() as u64,
            }),
        }),
        Err(e) => Some(SaveTaskResult {
            buffer_id,
            changedtick,
            path: path_clone,
            result: Err(format!("Failed to write file atomically: {:?}", e)),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clock::ReplicaId;
    use std::fs;
    use vim_buffer::{Buffer, BufferId};

    #[test]
    fn test_files_service_changedtick_gate() {
        let mut service = FilesService::new();
        let buf_id = BufferId::new(1).unwrap();
        let tick1 = ChangedTick::new(1);
        let tick2 = ChangedTick::new(2);

        assert!(service.should_save(buf_id, tick1));

        let _seq = service.begin_save(buf_id, tick1);
        service.set_pending_task(buf_id, TaskId(1));

        // Since tick1 save is in progress, should_save for tick1 is false
        assert!(!service.should_save(buf_id, tick1));
        // But for tick2, it is true
        assert!(service.should_save(buf_id, tick2));

        // Completing with correct parameters succeeds
        let res = SaveTaskResult {
            buffer_id: buf_id,
            changedtick: tick1,
            path: PathBuf::from("test.txt"),
            result: Ok(SaveOutcome {
                buffer: buf_id,
                path: PathBuf::from("test.txt"),
                bytes_written: 10,
            }),
        };
        assert!(service.apply_task_result(TaskId(1), &res));
    }

    #[test]
    fn test_files_service_rejects_stale() {
        let mut service = FilesService::new();
        let buf_id = BufferId::new(1).unwrap();
        let tick1 = ChangedTick::new(1);
        let tick2 = ChangedTick::new(2);

        service.begin_save(buf_id, tick1);
        service.set_pending_task(buf_id, TaskId(1));

        service.begin_save(buf_id, tick2);
        service.set_pending_task(buf_id, TaskId(2));

        let res_stale = SaveTaskResult {
            buffer_id: buf_id,
            changedtick: tick1,
            path: PathBuf::from("test.txt"),
            result: Ok(SaveOutcome {
                buffer: buf_id,
                path: PathBuf::from("test.txt"),
                bytes_written: 10,
            }),
        };

        // Applying task 1 with stale tick1 returns false
        assert!(!service.apply_task_result(TaskId(1), &res_stale));
    }

    #[test]
    fn test_save_file_writes_to_disk() {
        let temp_dir = std::env::temp_dir();
        let test_path = temp_dir.join(format!("nxvim_test_save_{}.txt", std::process::id()));

        let text = "hello background save\n";
        let buffer = Buffer::new(BufferId::new(1).unwrap(), ReplicaId::LOCAL, text);
        let snapshot = buffer.snapshot();
        let options = buffer.options().clone();

        let task_res = save_file(snapshot, test_path.clone(), options);
        assert!(task_res.result.is_ok());
        let outcome = task_res.result.unwrap();
        assert_eq!(outcome.bytes_written, text.len() as u64);

        let read_back = fs::read_to_string(&test_path).unwrap();
        assert_eq!(read_back, text);

        let _ = fs::remove_file(test_path);
    }
}
