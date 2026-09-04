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

fn find_buffer_id(state: &EditorState, buf_expr: &Value) -> Option<text::BufferId> {
    match buf_expr {
        Value::Null => state.current_buffer_id,
        Value::Integer(id) => {
            if *id == 0 {
                state.current_buffer_id
            } else if *id > 0 {
                let text_id = text::BufferId::new(*id as u64).ok()?;
                if state.buffers.contains_key(&text_id) {
                    Some(text_id)
                } else {
                    None
                }
            } else {
                None
            }
        }
        Value::String(name) => {
            let s = name.as_ref();
            if s == "%" || s == "" || s == "#" {
                state.current_buffer_id
            } else if s == "$" {
                state.buffers.keys().max().cloned()
            } else {
                let path = PathBuf::from(s);
                if let Some(id) = state.names.get(&path) {
                    Some(*id)
                } else {
                    for (buf_path, id) in &state.names {
                        if buf_path.ends_with(&path) {
                            return Some(*id);
                        }
                    }
                    None
                }
            }
        }
        _ => None,
    }
}

fn resolve_lnum(val: &Value, line_count: usize) -> Option<usize> {
    match val {
        Value::Integer(n) => {
            if *n < 0 {
                None
            } else {
                usize::try_from(*n).ok()
            }
        }
        Value::String(s) => {
            if s.as_ref() == "$" {
                Some(line_count)
            } else {
                s.parse::<usize>().ok()
            }
        }
        _ => None,
    }
}

fn snapshot_lines(snapshot: &text::BufferSnapshot) -> Vec<String> {
    let row_count = snapshot.row_count();
    let mut lines = Vec::with_capacity(row_count as usize);
    for r in 0..row_count {
        let start = snapshot.point_to_offset(text::Point::new(r, 0));
        let end = snapshot.point_to_offset(text::Point::new(r, snapshot.line_len(r)));
        let line_text: String = snapshot.text_for_range(start..end).collect();
        lines.push(line_text);
    }
    lines
}

fn create_snapshot_from_lines(id: text::BufferId, lines: &[String]) -> text::BufferSnapshot {
    let full_text = lines.join("\n");
    let text_buf = text::Buffer::new(clock::ReplicaId::default(), id, full_text);
    text_buf.snapshot().clone()
}

// Synchronous function handlers
fn bufnr(state: &EditorState, args: &[Value]) -> RuntimeResult<Value> {
    if args.is_empty() {
        return match state.current_buffer_id {
            Some(id) => Ok(Value::Integer(id.to_proto() as i64)),
            None => Ok(Value::Integer(-1)),
        };
    }
    let first_arg = &args[0];
    if let Some(id) = find_buffer_id(state, first_arg) {
        return Ok(Value::Integer(id.to_proto() as i64));
    }
    let create = args.get(1).map_or(false, |v| match v {
        Value::Bool(b) => *b,
        Value::Integer(i) => *i != 0,
        _ => false,
    });
    if create {
        if let Value::Integer(id) = first_arg {
            if *id > 0 {
                return Ok(Value::Integer(*id));
            }
        }
    }
    Ok(Value::Integer(-1))
}

fn bufname(state: &EditorState, args: &[Value]) -> RuntimeResult<Value> {
    let default_target = Value::String(Arc::from("%"));
    let target_val = args.get(0).unwrap_or(&default_target);
    let id = match find_buffer_id(state, target_val) {
        Some(id) => id,
        None => return Ok(Value::String(Arc::from(""))),
    };
    for (path, buf_id) in &state.names {
        if *buf_id == id {
            return Ok(Value::String(Arc::from(path.to_string_lossy().as_ref())));
        }
    }
    Ok(Value::String(Arc::from("")))
}

fn bufexists(state: &EditorState, args: &[Value]) -> RuntimeResult<Value> {
    let first_arg = args.get(0).ok_or_else(|| {
        RuntimeError::coded("E119", RuntimeErrorKind::ArityError, "missing argument")
    })?;
    let exists = find_buffer_id(state, first_arg).is_some();
    Ok(Value::Integer(if exists { 1 } else { 0 }))
}

fn getbufinfo(state: &EditorState, args: &[Value]) -> RuntimeResult<Value> {
    let mut target_ids: Vec<text::BufferId> = Vec::new();
    let mut filter_listed: Option<bool> = None;
    let mut filter_loaded: Option<bool> = None;
    let mut filter_modified: Option<bool> = None;

    if let Some(arg) = args.get(0) {
        if let Value::Dictionary(dict) = arg {
            if let Some(val) = dict.get("buflisted") {
                filter_listed = Some(val.to_string() != "0" && val != &Value::Bool(false));
            }
            if let Some(val) = dict.get("bufloaded") {
                filter_loaded = Some(val.to_string() != "0" && val != &Value::Bool(false));
            }
            if let Some(val) = dict.get("bufmodified") {
                filter_modified = Some(val.to_string() != "0" && val != &Value::Bool(false));
            }
            target_ids = state.buffers.keys().cloned().collect();
        } else {
            if let Some(id) = find_buffer_id(state, arg) {
                target_ids.push(id);
            } else {
                return Ok(Value::List(Vec::new()));
            }
        }
    } else {
        target_ids = state.buffers.keys().cloned().collect();
    }

    target_ids.sort_by_key(|id| id.to_proto());
    let mut res = Vec::new();

    for id in target_ids {
        let (snapshot, tick) = match state.buffers.get(&id) {
            Some(pair) => pair,
            None => continue,
        };

        if let Some(fl) = filter_loaded {
            if !fl { continue; }
        }
        if let Some(fm) = filter_modified {
            let is_modified = *tick > 0;
            if fm != is_modified { continue; }
        }
        if let Some(flist) = filter_listed {
            if !flist { continue; }
        }

        let name = state.names.iter().find(|(_, b_id)| **b_id == id)
            .map(|(path, _)| path.to_string_lossy().to_string())
            .unwrap_or_default();

        let mut dict = std::collections::BTreeMap::new();
        dict.insert("bufnr".to_string(), Value::Integer(id.to_proto() as i64));
        dict.insert("name".to_string(), Value::String(Arc::from(name.as_str())));
        dict.insert("lnum".to_string(), Value::Integer(1));
        dict.insert("linecount".to_string(), Value::Integer(snapshot.row_count() as i64));
        dict.insert("loaded".to_string(), Value::Integer(1));
        dict.insert("listed".to_string(), Value::Integer(1));
        dict.insert("changed".to_string(), Value::Integer(if *tick > 0 { 1 } else { 0 }));
        dict.insert("changedtick".to_string(), Value::Integer(*tick as i64));
        dict.insert("hidden".to_string(), Value::Integer(if state.current_buffer_id == Some(id) { 0 } else { 1 }));
        dict.insert("variables".to_string(), Value::Dictionary(std::collections::BTreeMap::new()));
        dict.insert("windows".to_string(), Value::List(Vec::new()));
        dict.insert("popups".to_string(), Value::List(Vec::new()));
        dict.insert("signs".to_string(), Value::List(Vec::new()));

        res.push(Value::Dictionary(dict));
    }

    Ok(Value::List(res))
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
    let row_count = snapshot.row_count() as usize;
    let start_lnum = match resolve_lnum(first_arg, row_count) {
        Some(l) => l,
        None => return Ok(Value::String(std::sync::Arc::from(""))),
    };

    if args.len() == 1 {
        if start_lnum == 0 || start_lnum > row_count {
            Ok(Value::String(std::sync::Arc::from("")))
        } else {
            let row = (start_lnum - 1) as u32;
            let start = snapshot.point_to_offset(text::Point::new(row, 0));
            let end = snapshot.point_to_offset(text::Point::new(row, snapshot.line_len(row)));
            let line_text: String = snapshot.text_for_range(start..end).collect();
            Ok(Value::String(std::sync::Arc::from(line_text)))
        }
    } else {
        let second_arg = args.get(1).ok_or_else(|| {
            RuntimeError::coded("E119", RuntimeErrorKind::ArityError, "missing argument")
        })?;
        let end_lnum = match resolve_lnum(second_arg, row_count) {
            Some(l) => l,
            None => return Ok(Value::List(Vec::new())),
        };

        if start_lnum == 0 || start_lnum > row_count || end_lnum < start_lnum {
            return Ok(Value::List(Vec::new()));
        }

        let start_row = start_lnum - 1;
        let end_row = (end_lnum.min(row_count)) - 1;

        let mut lines = Vec::new();
        for r in start_row..=end_row {
            let u_r = r as u32;
            let start = snapshot.point_to_offset(text::Point::new(u_r, 0));
            let end = snapshot.point_to_offset(text::Point::new(u_r, snapshot.line_len(u_r)));
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
    let id = match find_buffer_id(state, first_arg) {
        Some(id) => id,
        None => return Ok(Value::List(Vec::new())),
    };
    let snapshot = match state.buffers.get(&id) {
        Some((snap, _)) => snap,
        None => return Ok(Value::List(Vec::new())),
    };

    let second_arg = args.get(1).ok_or_else(|| {
        RuntimeError::coded("E119", RuntimeErrorKind::ArityError, "missing argument")
    })?;
    let row_count = snapshot.row_count() as usize;
    let start_lnum = match resolve_lnum(second_arg, row_count) {
        Some(l) => l,
        None => return Ok(Value::List(Vec::new())),
    };

    let end_lnum = if args.len() > 2 {
        match resolve_lnum(&args[2], row_count) {
            Some(l) => l,
            None => return Ok(Value::List(Vec::new())),
        }
    } else {
        start_lnum
    };

    if start_lnum == 0 || start_lnum > row_count || end_lnum < start_lnum {
        return Ok(Value::List(Vec::new()));
    }

    let start_row = start_lnum - 1;
    let end_row = (end_lnum.min(row_count)) - 1;

    let mut lines = Vec::new();
    for r in start_row..=end_row {
        let u_r = r as u32;
        let start = snapshot.point_to_offset(text::Point::new(u_r, 0));
        let end = snapshot.point_to_offset(text::Point::new(u_r, snapshot.line_len(u_r)));
        let line_text: String = snapshot.text_for_range(start..end).collect();
        lines.push(Value::String(Arc::from(line_text)));
    }
    Ok(Value::List(lines))
}

fn getbufoneline(state: &EditorState, args: &[Value]) -> RuntimeResult<Value> {
    let first_arg = args.get(0).ok_or_else(|| {
        RuntimeError::coded("E119", RuntimeErrorKind::ArityError, "missing argument")
    })?;
    let id = match find_buffer_id(state, first_arg) {
        Some(id) => id,
        None => return Ok(Value::String(std::sync::Arc::from(""))),
    };
    let snapshot = match state.buffers.get(&id) {
        Some((snap, _)) => snap,
        None => return Ok(Value::String(std::sync::Arc::from(""))),
    };

    let second_arg = args.get(1).ok_or_else(|| {
        RuntimeError::coded("E119", RuntimeErrorKind::ArityError, "missing argument")
    })?;
    let row_count = snapshot.row_count() as usize;
    let lnum = match resolve_lnum(second_arg, row_count) {
        Some(l) => l,
        None => return Ok(Value::String(std::sync::Arc::from(""))),
    };

    if lnum == 0 || lnum > row_count {
        Ok(Value::String(std::sync::Arc::from("")))
    } else {
        let u_r = (lnum - 1) as u32;
        let start = snapshot.point_to_offset(text::Point::new(u_r, 0));
        let end = snapshot.point_to_offset(text::Point::new(u_r, snapshot.line_len(u_r)));
        let line_text: String = snapshot.text_for_range(start..end).collect();
        Ok(Value::String(std::sync::Arc::from(line_text)))
    }
}

fn setbufline(state: &mut EditorState, args: &[Value]) -> RuntimeResult<Value> {
    let first_arg = args.get(0).ok_or_else(|| {
        RuntimeError::coded("E119", RuntimeErrorKind::ArityError, "missing argument")
    })?;
    let id = match find_buffer_id(state, first_arg) {
        Some(id) => id,
        None => return Ok(Value::Integer(1)),
    };
    let (snapshot, tick) = match state.buffers.get(&id) {
        Some((snap, t)) => (snap.clone(), *t),
        None => return Ok(Value::Integer(1)),
    };

    let second_arg = args.get(1).ok_or_else(|| {
        RuntimeError::coded("E119", RuntimeErrorKind::ArityError, "missing argument")
    })?;
    let mut lines = snapshot_lines(&snapshot);
    let line_count = lines.len();
    let lnum = match resolve_lnum(second_arg, line_count) {
        Some(l) => l,
        None => return Ok(Value::Integer(1)),
    };

    if lnum < 1 || lnum > line_count + 1 {
        return Ok(Value::Integer(1));
    }

    let text_arg = args.get(2).ok_or_else(|| {
        RuntimeError::coded("E119", RuntimeErrorKind::ArityError, "missing argument")
    })?;

    let to_insert: Vec<String> = match text_arg {
        Value::List(items) => {
            if items.is_empty() {
                return Ok(Value::Integer(0));
            }
            items.iter().map(|item| item.to_string()).collect()
        }
        other => vec![other.to_string()],
    };

    let start_idx = lnum - 1;
    for (i, new_line) in to_insert.into_iter().enumerate() {
        let idx = start_idx + i;
        if idx < lines.len() {
            lines[idx] = new_line;
        } else {
            lines.push(new_line);
        }
    }

    let new_snap = create_snapshot_from_lines(id, &lines);
    state.buffers.insert(id, (new_snap, tick + 1));
    Ok(Value::Integer(0))
}

fn deletebufline(state: &mut EditorState, args: &[Value]) -> RuntimeResult<Value> {
    let first_arg = args.get(0).ok_or_else(|| {
        RuntimeError::coded("E119", RuntimeErrorKind::ArityError, "missing argument")
    })?;
    let id = match find_buffer_id(state, first_arg) {
        Some(id) => id,
        None => return Ok(Value::Integer(1)),
    };
    let (snapshot, tick) = match state.buffers.get(&id) {
        Some((snap, t)) => (snap.clone(), *t),
        None => return Ok(Value::Integer(1)),
    };

    let mut lines = snapshot_lines(&snapshot);
    let line_count = lines.len();

    let second_arg = args.get(1).ok_or_else(|| {
        RuntimeError::coded("E119", RuntimeErrorKind::ArityError, "missing argument")
    })?;
    let first_lnum = match resolve_lnum(second_arg, line_count) {
        Some(l) => l,
        None => return Ok(Value::Integer(1)),
    };

    let last_lnum = if args.len() > 2 {
        match resolve_lnum(&args[2], line_count) {
            Some(l) => l,
            None => return Ok(Value::Integer(1)),
        }
    } else {
        first_lnum
    };

    if first_lnum < 1 || first_lnum > line_count || last_lnum < first_lnum {
        return Ok(Value::Integer(1));
    }

    let end_lnum = last_lnum.min(line_count);
    lines.drain((first_lnum - 1)..end_lnum);
    if lines.is_empty() {
        lines.push(String::new());
    }

    let new_snap = create_snapshot_from_lines(id, &lines);
    state.buffers.insert(id, (new_snap, tick + 1));
    Ok(Value::Integer(0))
}

fn append(state: &mut EditorState, args: &[Value]) -> RuntimeResult<Value> {
    let current_id = match state.current_buffer_id {
        Some(id) => id,
        None => return Ok(Value::Integer(1)),
    };
    let (snapshot, tick) = match state.buffers.get(&current_id) {
        Some((snap, t)) => (snap.clone(), *t),
        None => return Ok(Value::Integer(1)),
    };

    let mut lines = snapshot_lines(&snapshot);
    let line_count = lines.len();

    let first_arg = args.get(0).ok_or_else(|| {
        RuntimeError::coded("E119", RuntimeErrorKind::ArityError, "missing argument")
    })?;
    let lnum = match resolve_lnum(first_arg, line_count) {
        Some(l) => l,
        None => return Ok(Value::Integer(1)),
    };

    if lnum > line_count {
        return Ok(Value::Integer(1));
    }

    let second_arg = args.get(1).ok_or_else(|| {
        RuntimeError::coded("E119", RuntimeErrorKind::ArityError, "missing argument")
    })?;

    let to_insert: Vec<String> = match second_arg {
        Value::List(items) => {
            if items.is_empty() {
                return Ok(Value::Integer(0));
            }
            items.iter().map(|item| item.to_string()).collect()
        }
        other => vec![other.to_string()],
    };

    lines.splice(lnum..lnum, to_insert);

    let new_snap = create_snapshot_from_lines(current_id, &lines);
    state.buffers.insert(current_id, (new_snap, tick + 1));
    Ok(Value::Integer(0))
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
        let mut state = match self.state.lock() {
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
            "bufname" => Some(bufname(&state, &request.arguments)),
            "bufexists" => Some(bufexists(&state, &request.arguments)),
            "getbufinfo" => Some(getbufinfo(&state, &request.arguments)),
            "getline" => Some(getline(&state, &request.arguments)),
            "getbufline" => Some(getbufline(&state, &request.arguments)),
            "getbufoneline" => Some(getbufoneline(&state, &request.arguments)),
            "setbufline" => Some(setbufline(&mut state, &request.arguments)),
            "deletebufline" => Some(deletebufline(&mut state, &request.arguments)),
            "append" => Some(append(&mut state, &request.arguments)),
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
