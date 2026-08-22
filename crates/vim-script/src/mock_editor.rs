use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::host::{CommandRequest, Host, HostFuture, HostRequest};
use crate::runtime::{RuntimeError, RuntimeErrorKind, RuntimeResult, Value};

#[derive(Clone, Debug, PartialEq)]
pub struct MockBuffer {
    pub id: u64,
    pub name: String,
    pub lines: Vec<String>,
}

impl MockBuffer {
    pub fn new(id: u64, name: impl Into<String>, lines: Vec<String>) -> Self {
        Self {
            id,
            name: name.into(),
            lines,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct HighlightStyle {
    pub attributes: HashMap<String, String>,
}

#[derive(Clone, Debug, Default)]
pub struct HighlightRegistry {
    pub groups: HashMap<String, HighlightStyle>,
    pub links: HashMap<String, String>,
    pub clear_count: usize,
}

#[derive(Clone, Debug)]
pub struct MockEditorState {
    pub buffers: HashMap<u64, MockBuffer>,
    pub current_buffer: u64,
    pub cursor: (usize, usize),
    pub options: HashMap<String, Value>,
    pub messages: Vec<String>,
    pub command_log: Vec<CommandRequest>,
    pub write_count: usize,
    pub highlights: HighlightRegistry,
    pub syntax_reset_count: usize,
}

impl Default for MockEditorState {
    fn default() -> Self {
        let buffer = MockBuffer::new(1, "", vec![String::new()]);
        Self {
            buffers: HashMap::from([(buffer.id, buffer)]),
            current_buffer: 1,
            cursor: (1, 0),
            options: HashMap::new(),
            messages: Vec::new(),
            command_log: Vec::new(),
            write_count: 0,
            highlights: HighlightRegistry::default(),
            syntax_reset_count: 0,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct MockEditor {
    pub state: Arc<Mutex<MockEditorState>>,
}

impl MockEditor {
    pub fn new(state: MockEditorState) -> Self {
        Self {
            state: Arc::new(Mutex::new(state)),
        }
    }

    pub fn snapshot(&self) -> RuntimeResult<MockEditorState> {
        self.state
            .lock()
            .map(|state| state.clone())
            .map_err(|_| lock_error())
    }
}

impl Host for MockEditor {
    fn call(&self, request: HostRequest) -> HostFuture {
        let state = Arc::clone(&self.state);
        Box::pin(async move {
            match request.function.as_str() {
                "getline" => {
                    expect_arity(&request, 1)?;
                    let line = integer_argument(&request, 0)?;
                    let state = state.lock().map_err(|_| lock_error())?;
                    let buffer = current_buffer(&state)?;
                    let index = line_index(line, buffer.lines.len(), false)?;
                    Ok(Value::String(Arc::from(buffer.lines[index].as_str())))
                }
                "setline" => {
                    expect_arity(&request, 2)?;
                    let line = integer_argument(&request, 0)?;
                    let text = string_argument(&request, 1)?;
                    let mut state = state.lock().map_err(|_| lock_error())?;
                    let buffer = current_buffer_mut(&mut state)?;
                    let index = line_index(line, buffer.lines.len(), false)?;
                    buffer.lines[index] = text;
                    Ok(Value::Integer(0))
                }
                "append" => {
                    expect_arity(&request, 2)?;
                    let line = integer_argument(&request, 0)?;
                    let text = string_argument(&request, 1)?;
                    let mut state = state.lock().map_err(|_| lock_error())?;
                    let buffer = current_buffer_mut(&mut state)?;
                    let index = line_index(line, buffer.lines.len(), true)?;
                    buffer.lines.insert(index, text);
                    Ok(Value::Integer(0))
                }
                "cursor" => {
                    expect_arity(&request, 2)?;
                    let line = integer_argument(&request, 0)?;
                    let column = integer_argument(&request, 1)?;
                    let mut state = state.lock().map_err(|_| lock_error())?;
                    let line_count = current_buffer(&state)?.lines.len();
                    let line = line_index(line, line_count, false)? + 1;
                    let column = usize::try_from(column).map_err(|_| {
                        range_error(format!("cursor column must be non-negative: {column}"))
                    })?;
                    state.cursor = (line, column);
                    Ok(Value::Integer(0))
                }
                "echo" | "message" | "echomsg" => {
                    expect_arity(&request, 1)?;
                    let message = request.arguments[0].to_string();
                    state
                        .lock()
                        .map_err(|_| lock_error())?
                        .messages
                        .push(message);
                    Ok(Value::Null)
                }
                name => Err(RuntimeError::coded(
                    "E117",
                    RuntimeErrorKind::NameError,
                    format!("unknown host function: {name}"),
                )),
            }
        })
    }

    fn execute_command(&self, request: CommandRequest) -> HostFuture {
        let state = Arc::clone(&self.state);
        Box::pin(async move {
            let mut state = state.lock().map_err(|_| lock_error())?;
            state.command_log.push(request.clone());
            match request.command.name.as_str() {
                "write" | "w" => {
                    state.write_count += 1;
                    Ok(Value::Null)
                }
                "set" | "se" => {
                    apply_set(&mut state.options, &request.command.arguments)?;
                    Ok(Value::Null)
                }
                "highlight" | "hi" => {
                    apply_highlight(&mut state.highlights, &request.command.arguments)?;
                    Ok(Value::Null)
                }
                "syntax" | "syn" if request.command.arguments.trim() == "reset" => {
                    state.syntax_reset_count += 1;
                    Ok(Value::Null)
                }
                "DeleteLines" => {
                    let cursor_line = state.cursor.0;
                    let buffer = current_buffer_mut(&mut state)?;
                    let line_count = buffer.lines.len();

                    let (start, end) = if let Some(range) = &request.command.range {
                        resolve_range(range, cursor_line, line_count)
                    } else {
                        (cursor_line, cursor_line)
                    };

                    // Clamp to valid line numbers and convert to 0-based indices
                    let start_idx = start.clamp(1, line_count) - 1;
                    let end_idx = end.clamp(1, line_count) - 1;

                    let (low, high) = if start_idx <= end_idx {
                        (start_idx, end_idx)
                    } else {
                        (end_idx, start_idx)
                    };

                    // Delete the lines in the resolved range
                    buffer.lines.drain(low..=high);

                    // Ensure buffer is never completely empty
                    if buffer.lines.is_empty() {
                        buffer.lines.push(String::new());
                    }

                    // Adjust the cursor if it has been invalidated by the deletion
                    let new_count = buffer.lines.len();
                    if state.cursor.0 > new_count {
                        state.cursor.0 = new_count;
                    }

                    Ok(Value::Null)
                }
                "LogRange" => {
                    let cursor_line = state.cursor.0;
                    let (low, high) = {
                        let buffer = current_buffer(&state)?;
                        let line_count = buffer.lines.len();
                        let (start, end) = if let Some(range) = &request.command.range {
                            resolve_range(range, cursor_line, line_count)
                        } else {
                            (cursor_line, cursor_line)
                        };
                        let start_idx = start.clamp(1, line_count) - 1;
                        let end_idx = end.clamp(1, line_count) - 1;
                        if start_idx <= end_idx {
                            (start_idx, end_idx)
                        } else {
                            (end_idx, start_idx)
                        }
                    };

                    let mut logged = Vec::new();
                    {
                        let buffer = current_buffer(&state)?;
                        for i in low..=high {
                            if let Some(line) = buffer.lines.get(i) {
                                logged.push((i + 1, line.clone()));
                            }
                        }
                    }

                    for (line_num, line) in logged {
                        state
                            .messages
                            .push(format!("Logged Line {line_num}: {line}"));
                    }

                    Ok(Value::Null)
                }
                name => Err(RuntimeError::coded(
                    "E492",
                    RuntimeErrorKind::InvalidCommand,
                    format!("not an editor command: {name}"),
                )),
            }
        })
    }
}

fn current_buffer(state: &MockEditorState) -> RuntimeResult<&MockBuffer> {
    state.buffers.get(&state.current_buffer).ok_or_else(|| {
        RuntimeError::coded(
            "E86",
            RuntimeErrorKind::HostError,
            format!("buffer {} does not exist", state.current_buffer),
        )
    })
}

fn current_buffer_mut(state: &mut MockEditorState) -> RuntimeResult<&mut MockBuffer> {
    state.buffers.get_mut(&state.current_buffer).ok_or_else(|| {
        RuntimeError::coded(
            "E86",
            RuntimeErrorKind::HostError,
            format!("buffer {} does not exist", state.current_buffer),
        )
    })
}

fn expect_arity(request: &HostRequest, expected: usize) -> RuntimeResult<()> {
    if request.arguments.len() == expected {
        Ok(())
    } else {
        Err(RuntimeError::coded(
            "E119",
            RuntimeErrorKind::ArityError,
            format!(
                "{} expects {expected} argument(s), got {}",
                request.function,
                request.arguments.len()
            ),
        ))
    }
}

fn integer_argument(request: &HostRequest, index: usize) -> RuntimeResult<i64> {
    match &request.arguments[index] {
        Value::Integer(value) => Ok(*value),
        value => Err(RuntimeError::coded(
            "E745",
            RuntimeErrorKind::TypeError,
            format!(
                "argument {} to {} must be a number, got {}",
                index + 1,
                request.function,
                value.type_name()
            ),
        )),
    }
}

fn string_argument(request: &HostRequest, index: usize) -> RuntimeResult<String> {
    match &request.arguments[index] {
        Value::String(value) => Ok(value.to_string()),
        value => Err(RuntimeError::coded(
            "E730",
            RuntimeErrorKind::TypeError,
            format!(
                "argument {} to {} must be a string, got {}",
                index + 1,
                request.function,
                value.type_name()
            ),
        )),
    }
}

fn resolve_address(
    address: &crate::ast::Address,
    current_cursor_line: usize,
    last_line: usize,
) -> usize {
    use crate::ast::Address;
    match address {
        Address::Current => current_cursor_line,
        Address::Last => last_line,
        Address::Line(line) => *line as usize,
        Address::WholeFile => 1,
        Address::Offset { base, amount } => {
            let base_val = resolve_address(base, current_cursor_line, last_line);
            if *amount >= 0 {
                base_val.saturating_add(*amount as usize)
            } else {
                base_val.saturating_sub((-*amount) as usize)
            }
        }
        _ => 1, // Fallback for patterns, marks, etc.
    }
}

fn resolve_range(
    range: &crate::ast::CommandRange,
    current_cursor_line: usize,
    last_line: usize,
) -> (usize, usize) {
    use crate::ast::Address;
    let start = resolve_address(&range.start, current_cursor_line, last_line);
    let end = if let Some(end_addr) = &range.end {
        resolve_address(end_addr, current_cursor_line, last_line)
    } else {
        match &range.start {
            Address::WholeFile => last_line,
            _ => start,
        }
    };
    (start, end)
}

fn line_index(line: i64, line_count: usize, allow_zero: bool) -> RuntimeResult<usize> {
    let minimum = if allow_zero { 0 } else { 1 };
    if line < minimum || usize::try_from(line).map_or(true, |line| line > line_count) {
        return Err(range_error(format!(
            "line {line} is outside the valid range {minimum}..={line_count}"
        )));
    }
    if allow_zero {
        Ok(line as usize)
    } else {
        Ok(line as usize - 1)
    }
}

fn apply_set(options: &mut HashMap<String, Value>, arguments: &str) -> RuntimeResult<()> {
    for option in arguments.split_whitespace() {
        if let Some((name, value)) = option.split_once('=') {
            if name.is_empty() {
                return Err(invalid_argument("set option name cannot be empty"));
            }
            options.insert(name.to_string(), Value::String(Arc::from(value)));
        } else if let Some(name) = option.strip_prefix("no") {
            if name.is_empty() {
                return Err(invalid_argument("set option name cannot be empty"));
            }
            options.insert(name.to_string(), Value::Bool(false));
        } else {
            options.insert(option.to_string(), Value::Bool(true));
        }
    }
    Ok(())
}

fn apply_highlight(registry: &mut HighlightRegistry, arguments: &str) -> RuntimeResult<()> {
    let mut parts = arguments.split_whitespace().peekable();
    let Some(first) = parts.next() else {
        return Ok(());
    };
    if first.eq_ignore_ascii_case("clear") {
        registry.clear_count += 1;
        if let Some(group) = parts.next() {
            registry.groups.remove(group);
            registry.links.remove(group);
        } else {
            registry.groups.clear();
            registry.links.clear();
        }
        return Ok(());
    }
    let first = if first.eq_ignore_ascii_case("default") || first.eq_ignore_ascii_case("def") {
        parts
            .next()
            .ok_or_else(|| invalid_argument("highlight default requires a group or link"))?
    } else {
        first
    };
    if first.eq_ignore_ascii_case("link") {
        let from = parts
            .next()
            .ok_or_else(|| invalid_argument("highlight link requires a source group"))?;
        let to = parts
            .next()
            .ok_or_else(|| invalid_argument("highlight link requires a target group"))?;
        registry.links.insert(from.to_owned(), to.to_owned());
        return Ok(());
    }
    let style = registry.groups.entry(first.to_owned()).or_default();
    for attribute in parts {
        let Some((name, value)) = attribute.split_once('=') else {
            return Err(invalid_argument(format!(
                "invalid highlight attribute: {attribute}"
            )));
        };
        style
            .attributes
            .insert(name.to_ascii_lowercase(), value.to_owned());
    }
    registry.links.remove(first);
    Ok(())
}

fn range_error(message: impl Into<String>) -> RuntimeError {
    RuntimeError::coded("E16", RuntimeErrorKind::IndexError, message)
}

fn invalid_argument(message: impl Into<String>) -> RuntimeError {
    RuntimeError::coded("E474", RuntimeErrorKind::InvalidCommand, message)
}

fn lock_error() -> RuntimeError {
    RuntimeError::coded(
        "E605",
        RuntimeErrorKind::HostError,
        "mock editor state lock is poisoned",
    )
}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::task::{Context, Poll, Waker};

    use super::*;
    use crate::ast::ExCommand;
    use crate::host::{HostContext, HostTarget};

    fn run(future: HostFuture) -> RuntimeResult<Value> {
        let mut context = Context::from_waker(Waker::noop());
        let mut future = future;
        match Future::poll(future.as_mut(), &mut context) {
            Poll::Ready(result) => result,
            Poll::Pending => panic!("mock editor futures must complete immediately"),
        }
    }

    fn call(editor: &MockEditor, function: &str, arguments: Vec<Value>) -> RuntimeResult<Value> {
        run(editor.call(HostRequest {
            target: HostTarget::Global,
            function: function.to_string(),
            arguments,
            context: HostContext::default(),
        }))
    }

    #[test]
    fn edits_current_buffer_and_moves_cursor() {
        let editor = MockEditor::default();
        call(
            &editor,
            "setline",
            vec![Value::Integer(1), Value::String(Arc::from("first"))],
        )
        .unwrap();
        call(
            &editor,
            "append",
            vec![Value::Integer(1), Value::String(Arc::from("second"))],
        )
        .unwrap();
        call(
            &editor,
            "cursor",
            vec![Value::Integer(2), Value::Integer(3)],
        )
        .unwrap();

        let state = editor.snapshot().unwrap();
        assert_eq!(state.buffers[&1].lines, ["first", "second"]);
        assert_eq!(state.cursor, (2, 3));
    }

    #[test]
    fn commands_are_logged_and_applied() {
        let editor = MockEditor::default();
        for (name, arguments) in [("write", ""), ("set", "number filetype=rust")] {
            run(editor.execute_command(CommandRequest {
                command: ExCommand {
                    modifiers: Vec::new(),
                    range: None,
                    name: name.to_string(),
                    bang: false,
                    count: None,
                    register: None,
                    arguments: arguments.to_string(),
                },
                context: HostContext::default(),
            }))
            .unwrap();
        }

        let state = editor.snapshot().unwrap();
        assert_eq!(state.command_log.len(), 2);
        assert_eq!(state.write_count, 1);
        assert_eq!(state.options["number"], Value::Bool(true));
        assert_eq!(state.options["filetype"], Value::String(Arc::from("rust")));
    }

    #[test]
    fn bad_line_returns_a_structured_error() {
        let editor = MockEditor::default();
        let error = call(&editor, "getline", vec![Value::Integer(2)]).unwrap_err();
        assert_eq!(error.code.as_deref(), Some("E16"));
        assert!(matches!(error.kind, RuntimeErrorKind::IndexError));
    }

    #[test]
    fn handles_range_commands() {
        use crate::ast::{Address, CommandRange};

        let editor = MockEditor::default();
        // Setup initial buffers
        {
            let mut state = editor.state.lock().unwrap();
            let buf = state.buffers.get_mut(&1).unwrap();
            buf.lines = vec![
                "line 1".to_string(),
                "line 2".to_string(),
                "line 3".to_string(),
            ];
        }

        // Run LogRange on whole file range
        run(editor.execute_command(CommandRequest {
            command: ExCommand {
                modifiers: Vec::new(),
                range: Some(CommandRange {
                    start: Address::WholeFile,
                    end: None,
                    separator: None,
                }),
                name: "LogRange".to_string(),
                bang: false,
                count: None,
                register: None,
                arguments: "".to_string(),
            },
            context: HostContext::default(),
        }))
        .unwrap();

        // Run DeleteLines on 1,2
        run(editor.execute_command(CommandRequest {
            command: ExCommand {
                modifiers: Vec::new(),
                range: Some(CommandRange {
                    start: Address::Line(1),
                    end: Some(Address::Line(2)),
                    separator: None,
                }),
                name: "DeleteLines".to_string(),
                bang: false,
                count: None,
                register: None,
                arguments: "".to_string(),
            },
            context: HostContext::default(),
        }))
        .unwrap();

        let state = editor.snapshot().unwrap();
        assert_eq!(
            state.messages,
            vec![
                "Logged Line 1: line 1".to_string(),
                "Logged Line 2: line 2".to_string(),
                "Logged Line 3: line 3".to_string(),
            ]
        );
        assert_eq!(state.buffers[&1].lines, vec!["line 3".to_string()]);
    }
}
