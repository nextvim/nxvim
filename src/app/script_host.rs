use crate::app::request::AppRequest;
use crate::script::EditorState;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, mpsc};
use vim_script::host::{CommandRequest, Host, HostFuture, HostRequest};
use vim_script::runtime::{RuntimeError, RuntimeErrorKind, RuntimeResult, Value};

pub struct ActiveHost {
    pub sender: mpsc::Sender<AppRequest>,
    pub state: Arc<Mutex<EditorState>>,
}

impl ActiveHost {
    pub fn new(sender: mpsc::Sender<AppRequest>, state: Arc<Mutex<EditorState>>) -> Self {
        Self { sender, state }
    }
}

fn expect_arity(request: &HostRequest, expected: usize) -> Result<(), RuntimeError> {
    if request.arguments.len() != expected {
        return Err(RuntimeError::coded(
            "E119",
            RuntimeErrorKind::ArityError,
            format!(
                "expected {} arguments, got {}",
                expected,
                request.arguments.len()
            ),
        ));
    }
    Ok(())
}

fn type_error(function: &str, expected: &str, actual: &Value) -> RuntimeError {
    RuntimeError::coded(
        "E745",
        RuntimeErrorKind::TypeError,
        format!("{function} expected {expected}, got {}", actual.type_name()),
    )
}

fn parse_lnum(val: &Value) -> Option<u32> {
    let num = match val {
        Value::Integer(n) => Some(*n),
        Value::String(s) => s.parse::<i64>().ok(),
        _ => None,
    }?;
    let positive = num.max(1);
    u32::try_from(positive - 1).ok()
}

fn find_buffer_snapshot<'a>(
    state: &'a EditorState,
    buf_expr: &Value,
) -> Option<&'a text::BufferSnapshot> {
    match buf_expr {
        Value::String(name) => {
            let path = PathBuf::from(name.as_ref());
            if let Some(id) = state.names.get(&path) {
                state.buffers.get(id).map(|(snap, _)| snap)
            } else {
                for (buf_path, id) in &state.names {
                    if buf_path.ends_with(&path) {
                        return state.buffers.get(id).map(|(snap, _)| snap);
                    }
                }
                None
            }
        }
        Value::Integer(id) => {
            if *id >= 0 {
                let text_id = text::BufferId::new(*id as u64).ok()?;
                state.buffers.get(&text_id).map(|(snap, _)| snap)
            } else {
                None
            }
        }
        _ => None,
    }
}

// Synchronous function handlers
fn bufnr(state: &EditorState, args: &[Value]) -> RuntimeResult<Value> {
    if args.is_empty() {
        let first_id = state.buffers.keys().next().cloned();
        match first_id {
            Some(id) => Ok(Value::Integer(id.to_proto() as i64)),
            None => Ok(Value::Integer(-1)),
        }
    } else {
        let first_arg = args.get(0).ok_or_else(|| {
            RuntimeError::coded("E119", RuntimeErrorKind::ArityError, "missing argument")
        })?;
        match first_arg {
            Value::String(name) => {
                let path = PathBuf::from(name.as_ref());
                if let Some(id) = state.names.get(&path) {
                    Ok(Value::Integer(id.to_proto() as i64))
                } else {
                    for (buf_path, id) in &state.names {
                        if buf_path.ends_with(&path) {
                            return Ok(Value::Integer(id.to_proto() as i64));
                        }
                    }
                    Ok(Value::Integer(-1))
                }
            }
            Value::Integer(id) => {
                if *id >= 0 {
                    if let Ok(text_id) = text::BufferId::new(*id as u64) {
                        if state.buffers.contains_key(&text_id) {
                            return Ok(Value::Integer(*id));
                        }
                    }
                }
                Ok(Value::Integer(-1))
            }
            other => Err(type_error("bufnr", "String or Number", other)),
        }
    }
}

fn bufexists(state: &EditorState, args: &[Value]) -> RuntimeResult<Value> {
    let first_arg = args.get(0).ok_or_else(|| {
        RuntimeError::coded("E119", RuntimeErrorKind::ArityError, "missing argument")
    })?;
    match first_arg {
        Value::String(name) => {
            let path = PathBuf::from(name.as_ref());
            let exists = state.names.contains_key(&path)
                || state.names.keys().any(|buf_path| buf_path.ends_with(&path));
            Ok(Value::Integer(if exists { 1 } else { 0 }))
        }
        Value::Integer(id) => {
            let exists = if *id >= 0 {
                if let Ok(text_id) = text::BufferId::new(*id as u64) {
                    state.buffers.contains_key(&text_id)
                } else {
                    false
                }
            } else {
                false
            };
            Ok(Value::Integer(if exists { 1 } else { 0 }))
        }
        other => Err(type_error("bufexists", "String or Number", other)),
    }
}

fn getline(state: &EditorState, args: &[Value]) -> RuntimeResult<Value> {
    let current_id = match state.current_buffer_id {
        Some(id) => id,
        None => return Ok(Value::String(std::sync::Arc::from(""))),
    };
    let snapshot = match state.buffers.get(&current_id) {
        Some((snap, _)) => snap,
        None => return Ok(Value::String(std::sync::Arc::from(""))),
    };

    let first_arg = args.get(0).ok_or_else(|| {
        RuntimeError::coded("E119", RuntimeErrorKind::ArityError, "missing argument")
    })?;
    let start_row = match parse_lnum(first_arg) {
        Some(row) => row,
        None => return Ok(Value::String(std::sync::Arc::from(""))),
    };

    if args.len() == 1 {
        if start_row >= snapshot.row_count() {
            Ok(Value::String(std::sync::Arc::from("")))
        } else {
            let start = snapshot.point_to_offset(text::Point::new(start_row, 0));
            let end =
                snapshot.point_to_offset(text::Point::new(start_row, snapshot.line_len(start_row)));
            let line_text: String = snapshot.text_for_range(start..end).collect();
            Ok(Value::String(std::sync::Arc::from(line_text)))
        }
    } else {
        let second_arg = args.get(1).ok_or_else(|| {
            RuntimeError::coded("E119", RuntimeErrorKind::ArityError, "missing argument")
        })?;
        let end_row = match parse_lnum(second_arg) {
            Some(row) => row,
            None => return Ok(Value::List(Vec::new())),
        };

        let row_count = snapshot.row_count();
        let mut lines = Vec::new();
        for r in start_row..=end_row {
            if r >= row_count {
                break;
            }
            let start = snapshot.point_to_offset(text::Point::new(r, 0));
            let end = snapshot.point_to_offset(text::Point::new(r, snapshot.line_len(r)));
            let line_text: String = snapshot.text_for_range(start..end).collect();
            lines.push(Value::String(std::sync::Arc::from(line_text)));
        }
        Ok(Value::List(lines))
    }
}

fn getbufline(state: &EditorState, args: &[Value]) -> RuntimeResult<Value> {
    let first_arg = args.get(0).ok_or_else(|| {
        RuntimeError::coded("E119", RuntimeErrorKind::ArityError, "missing argument")
    })?;
    let snapshot = match find_buffer_snapshot(state, first_arg) {
        Some(snap) => snap,
        None => return Ok(Value::List(Vec::new())),
    };

    let second_arg = args.get(1).ok_or_else(|| {
        RuntimeError::coded("E119", RuntimeErrorKind::ArityError, "missing argument")
    })?;
    let start_row = match parse_lnum(second_arg) {
        Some(row) => row,
        None => return Ok(Value::List(Vec::new())),
    };

    let end_row = if args.len() > 2 {
        let third_arg = args.get(2).ok_or_else(|| {
            RuntimeError::coded("E119", RuntimeErrorKind::ArityError, "missing argument")
        })?;
        match parse_lnum(third_arg) {
            Some(row) => row,
            None => return Ok(Value::List(Vec::new())),
        }
    } else {
        start_row
    };

    let row_count = snapshot.row_count();
    let mut lines = Vec::new();
    for r in start_row..=end_row {
        if r >= row_count {
            break;
        }
        let start = snapshot.point_to_offset(text::Point::new(r, 0));
        let end = snapshot.point_to_offset(text::Point::new(r, snapshot.line_len(r)));
        let line_text: String = snapshot.text_for_range(start..end).collect();
        lines.push(Value::String(std::sync::Arc::from(line_text)));
    }
    Ok(Value::List(lines))
}

fn getbufoneline(state: &EditorState, args: &[Value]) -> RuntimeResult<Value> {
    let first_arg = args.get(0).ok_or_else(|| {
        RuntimeError::coded("E119", RuntimeErrorKind::ArityError, "missing argument")
    })?;
    let snapshot = match find_buffer_snapshot(state, first_arg) {
        Some(snap) => snap,
        None => return Ok(Value::String(std::sync::Arc::from(""))),
    };

    let second_arg = args.get(1).ok_or_else(|| {
        RuntimeError::coded("E119", RuntimeErrorKind::ArityError, "missing argument")
    })?;
    let row = match parse_lnum(second_arg) {
        Some(r) => r,
        None => return Ok(Value::String(std::sync::Arc::from(""))),
    };

    if row >= snapshot.row_count() {
        Ok(Value::String(std::sync::Arc::from("")))
    } else {
        let start = snapshot.point_to_offset(text::Point::new(row, 0));
        let end = snapshot.point_to_offset(text::Point::new(row, snapshot.line_len(row)));
        let line_text: String = snapshot.text_for_range(start..end).collect();
        Ok(Value::String(std::sync::Arc::from(line_text)))
    }
}

impl Host for ActiveHost {
    fn call(&self, request: HostRequest) -> HostFuture {
        let sender = self.sender.clone();
        Box::pin(async move {
            match request.function.as_str() {
                "echo" | "message" | "echomsg" => {
                    expect_arity(&request, 1)?;
                    let message = request.arguments[0].to_string();
                    sender.send(AppRequest::ShowMessage(message)).map_err(|_| {
                        RuntimeError::coded(
                            "E_HOST",
                            RuntimeErrorKind::HostError,
                            "editor command queue is closed",
                        )
                    })?;
                    Ok(Value::Null)
                }
                "execute" => {
                    if request.arguments.is_empty() {
                        return Err(RuntimeError::coded(
                            "E119",
                            RuntimeErrorKind::ArityError,
                            "execute expects at least 1 argument",
                        ));
                    }
                    let cmd_str = match &request.arguments[0] {
                        Value::String(s) => s.to_string(),
                        Value::List(l) => {
                            let mut lines = Vec::new();
                            for item in l.iter() {
                                lines.push(item.to_string());
                            }
                            lines.join("\n")
                        }
                        other => other.to_string(),
                    };
                    sender
                        .send(AppRequest::ExecuteExString(cmd_str))
                        .map_err(|_| {
                            RuntimeError::coded(
                                "E_HOST",
                                RuntimeErrorKind::HostError,
                                "editor command queue is closed",
                            )
                        })?;
                    Ok(Value::String(Arc::from("")))
                }
                "feedkeys" => {
                    if request.arguments.is_empty() {
                        return Err(RuntimeError::coded(
                            "E119",
                            RuntimeErrorKind::ArityError,
                            "feedkeys expects at least 1 argument",
                        ));
                    }
                    let keys = request.arguments[0].to_string();
                    let mode = if request.arguments.len() > 1 {
                        request.arguments[1].to_string()
                    } else {
                        "m".to_string()
                    };
                    sender
                        .send(AppRequest::FeedKeys { keys, mode })
                        .map_err(|_| {
                            RuntimeError::coded(
                                "E_HOST",
                                RuntimeErrorKind::HostError,
                                "editor command queue is closed",
                            )
                        })?;
                    Ok(Value::Integer(0))
                }
                name => Err(RuntimeError::coded(
                    "E117",
                    RuntimeErrorKind::NameError,
                    format!("unknown host function: {name}"),
                )),
            }
        })
    }

    fn call_sync(&self, request: HostRequest) -> Option<RuntimeResult<Value>> {
        let state = match self.state.lock() {
            Ok(s) => s,
            Err(_) => {
                return Some(Err(RuntimeError::coded(
                    "E605",
                    RuntimeErrorKind::HostError,
                    "editor state lock is poisoned",
                )));
            }
        };

        match request.function.as_str() {
            "mode" => Some(Ok(Value::String(Arc::from(state.current_mode.as_str())))),
            "bufnr" => Some(bufnr(&state, &request.arguments)),
            "bufexists" => Some(bufexists(&state, &request.arguments)),
            "getline" => Some(getline(&state, &request.arguments)),
            "getbufline" => Some(getbufline(&state, &request.arguments)),
            "getbufoneline" => Some(getbufoneline(&state, &request.arguments)),
            _ => None,
        }
    }

    fn execute_command(&self, request: CommandRequest) -> HostFuture {
        let sender = self.sender.clone();
        Box::pin(async move {
            match request.command.name.as_str() {
                "quit" => {
                    sender.send(AppRequest::Quit).map_err(|_| {
                        RuntimeError::coded(
                            "E_HOST",
                            RuntimeErrorKind::HostError,
                            "editor command queue is closed",
                        )
                    })?;
                    Ok(Value::Null)
                }
                "source" => {
                    let path_str = request.command.arguments.trim();
                    if path_str.is_empty() {
                        return Err(RuntimeError::coded(
                            "E471",
                            RuntimeErrorKind::InvalidCommand,
                            "Argument required",
                        ));
                    }
                    sender
                        .send(AppRequest::Source(PathBuf::from(path_str)))
                        .map_err(|_| {
                            RuntimeError::coded(
                                "E_HOST",
                                RuntimeErrorKind::HostError,
                                "editor command queue is closed",
                            )
                        })?;
                    Ok(Value::Null)
                }
                _ => {
                    sender
                        .send(AppRequest::ExecuteEx(request.command))
                        .map_err(|_| {
                            RuntimeError::coded(
                                "E_HOST",
                                RuntimeErrorKind::HostError,
                                "editor command queue is closed",
                            )
                        })?;
                    Ok(Value::Null)
                }
            }
        })
    }
}
