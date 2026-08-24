mod buffer;
mod registry;

use super::EditorState;
use std::sync::Mutex;
use vim_script::{
    host::{HostRequest, HostRuntime},
    runtime::{RuntimeError, RuntimeErrorKind, RuntimeResult, Value},
};

pub(super) fn register(host: &mut HostRuntime) {
    registry::register(host);
}

pub(super) fn call_sync(
    state: &Mutex<EditorState>,
    request: &HostRequest,
) -> Option<RuntimeResult<Value>> {
    let handler = registry::sync_handler(&request.function)?;
    Some(
        state
            .lock()
            .map_err(|_| lock_error())
            .and_then(|state| handler(&state, &request.arguments)),
    )
}

fn lock_error() -> RuntimeError {
    RuntimeError::coded(
        "E605",
        RuntimeErrorKind::HostError,
        "editor state lock is poisoned",
    )
}
