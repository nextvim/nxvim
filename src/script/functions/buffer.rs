use super::super::EditorState;
use std::path::PathBuf;
use vim_script::runtime::{RuntimeError, RuntimeErrorKind, RuntimeResult, Value};

fn type_error(function: &str, expected: &str, actual: &Value) -> RuntimeError {
    RuntimeError::coded(
        "E745",
        RuntimeErrorKind::TypeError,
        format!("{function} expected {expected}, got {}", actual.type_name()),
    )
}

pub fn bufnr(state: &EditorState, args: &[Value]) -> RuntimeResult<Value> {
    {
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
}

pub fn bufexists(state: &EditorState, args: &[Value]) -> RuntimeResult<Value> {
    {
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

pub fn getline(state: &EditorState, args: &[Value]) -> RuntimeResult<Value> {
    {
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
                let end = snapshot
                    .point_to_offset(text::Point::new(start_row, snapshot.line_len(start_row)));
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

pub fn getbufline(state: &EditorState, args: &[Value]) -> RuntimeResult<Value> {
    {
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
}

pub fn getbufoneline(state: &EditorState, args: &[Value]) -> RuntimeResult<Value> {
    {
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
}
