use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use vim_buffer::{
    Action, ActionOutcome, BufferId, BufferManager, ByteOffset, Edit, EditOrigin, Mutator,
    PlannedEdit, Point, TextRange,
};
use vim_script::{
    compiler::Compiler,
    host::{
        Arity, Capability, CapabilitySet, CommandDefinition, CommandRequest, Host, HostContext,
        HostFuture, HostRequest, HostRuntime,
    },
    lexer::Lexer,
    parser::Parser,
    resolver::{Resolver, ResolverConfig},
    runtime::{RuntimeError, RuntimeErrorKind, Scheduler, Value, Vm},
    source::SourceMap,
};

use crate::EditorError;

struct EditorState {
    buffers: BufferManager,
    mutator: Mutator,
    exit_requested: bool,
}

#[derive(Clone)]
struct EditorHost {
    state: Arc<Mutex<EditorState>>,
}

pub struct HeadlessEditor {
    state: Arc<Mutex<EditorState>>,
    host: HostRuntime,
    sources: SourceMap,
    globals: HashMap<String, Value>,
}

impl HeadlessEditor {
    pub fn new() -> Result<Self, EditorError> {
        let mut state = EditorState {
            buffers: BufferManager::new(),
            mutator: Mutator::default(),
            exit_requested: false,
        };
        let created = state.mutator.execute(
            &mut state.buffers,
            Action::Create {
                initial_text: String::new(),
            },
        )?;
        let ActionOutcome::Manager(vim_buffer::ManagerOutcome::Added(buffer)) = created else {
            return Err(EditorError::State(
                "initial buffer creation returned an invalid outcome",
            ));
        };
        state
            .mutator
            .execute(&mut state.buffers, Action::SetCurrent { buffer })?;

        let state = Arc::new(Mutex::new(state));
        let mut host = HostRuntime::new(Arc::new(EditorHost {
            state: Arc::clone(&state),
        }));
        host.capabilities = CapabilitySet::from([Capability::BufferRead, Capability::BufferWrite]);
        host.register_function(
            "bufnr",
            Arity::Range { min: 0, max: 1 },
            vec![Capability::BufferRead],
        );
        host.register_function("getline", Arity::Exact(1), vec![Capability::BufferRead]);
        host.register_function("setline", Arity::Exact(2), vec![Capability::BufferWrite]);
        register_commands(&mut host);

        Ok(Self {
            state,
            host,
            sources: SourceMap::default(),
            globals: HashMap::from([
                ("v:version".into(), Value::Integer(902)),
                ("v:true".into(), Value::Bool(true)),
                ("v:false".into(), Value::Bool(false)),
                ("v:null".into(), Value::Null),
            ]),
        })
    }

    pub fn current_buffer(&self) -> Result<BufferId, EditorError> {
        self.with_state(|state| {
            state
                .buffers
                .current()
                .ok_or(EditorError::State("headless editor has no current buffer"))
        })
    }

    pub fn buffer_text(&self, buffer: BufferId) -> Result<String, EditorError> {
        self.with_state(|state| Ok(state.buffers.get(buffer)?.snapshot().chunks().collect()))
    }

    pub fn current_text(&self) -> Result<String, EditorError> {
        self.buffer_text(self.current_buffer()?)
    }

    pub fn changedtick(&self, buffer: BufferId) -> Result<u64, EditorError> {
        self.with_state(|state| Ok(state.buffers.get(buffer)?.changedtick().get()))
    }

    pub fn listed_buffers(&self) -> Result<Vec<BufferId>, EditorError> {
        self.with_state(|state| Ok(state.buffers.listed()))
    }

    pub fn buffer_exists(&self, buffer: BufferId) -> Result<bool, EditorError> {
        self.with_state(|state| Ok(state.buffers.get(buffer).is_ok()))
    }

    pub fn buffer_loaded(&self, buffer: BufferId) -> Result<bool, EditorError> {
        self.with_state(|state| Ok(state.buffers.get(buffer)?.is_loaded()))
    }

    pub fn alternate_buffer(&self) -> Result<Option<BufferId>, EditorError> {
        self.with_state(|state| Ok(state.buffers.alternate()))
    }

    pub fn exit_requested(&self) -> Result<bool, EditorError> {
        self.with_state(|state| Ok(state.exit_requested))
    }

    pub fn undo(&self) -> Result<bool, EditorError> {
        self.with_state(|state| {
            let buffer = state
                .buffers
                .current()
                .ok_or(EditorError::State("headless editor has no current buffer"))?;
            let EditorState {
                buffers, mutator, ..
            } = &mut *state;
            let outcome = mutator.execute(buffers, Action::Undo { buffer, count: 1 })?;
            Ok(matches!(outcome, ActionOutcome::Mutation(Some(_))))
        })
    }

    pub fn eval(
        &mut self,
        source_name: impl Into<String>,
        source: &str,
    ) -> Result<Value, EditorError> {
        let source_name = source_name.into();
        let source_id = self.sources.add(source_name.clone(), source);
        let lexed = Lexer::new(source_id, source).lex();
        if !lexed.diagnostics.is_empty() {
            return Err(EditorError::Diagnostics {
                stage: "lexing",
                diagnostics: lexed.diagnostics,
            });
        }
        let parsed = Parser::new_with_source(&lexed.tokens, source).parse();
        if !parsed.diagnostics.is_empty() {
            return Err(EditorError::Diagnostics {
                stage: "parsing",
                diagnostics: parsed.diagnostics,
            });
        }

        let mut resolver_config = ResolverConfig::default();
        resolver_config
            .builtins
            .extend(self.host.functions.names().map(str::to_owned));
        let resolved = Resolver::new(resolver_config)
            .resolve(parsed.program.expect("parser always returns a program"));
        if !resolved.diagnostics.is_empty() {
            return Err(EditorError::Diagnostics {
                stage: "resolution",
                diagnostics: resolved.diagnostics,
            });
        }
        let compiled = Compiler::new(
            &resolved
                .program
                .expect("resolver always returns a resolved program"),
        )
        .compile();
        if !compiled.diagnostics.is_empty() {
            return Err(EditorError::Diagnostics {
                stage: "compilation",
                diagnostics: compiled.diagnostics,
            });
        }

        let buffer = self.current_buffer()?;
        self.globals.insert(
            "b:changedtick".into(),
            Value::Integer(self.changedtick(buffer)? as i64),
        );
        let mut vm = Vm::with_globals(
            compiled.module.expect("compiler always returns a module"),
            self.globals.clone(),
        )?;
        vm.host_context = HostContext {
            script_name: Some(source_name),
            current_buffer: Some(buffer.get()),
            ..HostContext::default()
        };

        let mut scheduler = Scheduler::default();
        scheduler.set_host(self.host.clone());
        let task = scheduler.spawn(vm)?;
        let result = scheduler.run_until_complete(task)?;
        let completed = scheduler
            .task(task)
            .ok_or(EditorError::State("completed Vimscript task disappeared"))?;
        self.globals = completed.vm.globals.clone();
        self.globals.insert(
            "b:changedtick".into(),
            Value::Integer(self.changedtick(buffer)? as i64),
        );
        if let Some(host) = scheduler.host().cloned() {
            self.host = host;
        }
        Ok(result)
    }

    pub fn global(&self, name: &str) -> Option<&Value> {
        self.globals.get(name)
    }

    pub fn render_diagnostics(&self, error: &EditorError) -> Vec<String> {
        match error {
            EditorError::Diagnostics { diagnostics, .. } => diagnostics
                .iter()
                .map(|diagnostic| self.sources.render(diagnostic))
                .collect(),
            _ => Vec::new(),
        }
    }

    fn with_state<T>(
        &self,
        operation: impl FnOnce(&mut EditorState) -> Result<T, EditorError>,
    ) -> Result<T, EditorError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| EditorError::State("headless editor state lock is poisoned"))?;
        operation(&mut state)
    }
}

impl Host for EditorHost {
    fn call(&self, request: HostRequest) -> HostFuture {
        let state = Arc::clone(&self.state);
        Box::pin(async move {
            let mut state = state
                .lock()
                .map_err(|_| host_error("editor state lock is poisoned"))?;
            match request.function.as_str() {
                "bufnr" => {
                    let id = resolve_buffer(&state, request.arguments.first())?;
                    Ok(Value::Integer(id.map_or(-1, |id| id.get() as i64)))
                }
                "getline" => {
                    let line = integer_argument(&request, 0)?;
                    let buffer = current_buffer(&state)?;
                    let snapshot = state.buffers.get(buffer).map_err(buffer_error)?.snapshot();
                    let range = line_range(&snapshot, line)?;
                    let text = snapshot
                        .chunks_for_range(range)
                        .map_err(buffer_error)?
                        .collect::<String>();
                    Ok(Value::String(Arc::from(text)))
                }
                "setline" => {
                    let line = integer_argument(&request, 0)?;
                    let replacement = string_argument(&request, 1)?;
                    let buffer = current_buffer(&state)?;
                    let snapshot = state.buffers.get(buffer).map_err(buffer_error)?.snapshot();
                    let range = line_range(&snapshot, line)?;
                    let EditorState {
                        buffers, mutator, ..
                    } = &mut *state;
                    mutator
                        .apply_edits(
                            buffers,
                            buffer,
                            EditOrigin::VimScript,
                            [PlannedEdit {
                                selection: None,
                                edit: Edit::replace(range, replacement),
                            }],
                            None,
                            false,
                        )
                        .map_err(buffer_error)?;
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

    fn execute_command(&self, request: CommandRequest) -> HostFuture {
        let state = Arc::clone(&self.state);
        Box::pin(async move {
            let mut state = state
                .lock()
                .map_err(|_| host_error("editor state lock is poisoned"))?;
            execute_editor_command(&mut state, request)
        })
    }
}

fn register_commands(host: &mut HostRuntime) {
    for (name, abbreviation, bang, capability) in [
        ("enew", 1, true, Capability::BufferWrite),
        ("edit", 1, true, Capability::FileSystemRead),
        ("buffer", 1, true, Capability::BufferRead),
        ("bdelete", 2, true, Capability::BufferWrite),
        ("bwipeout", 2, true, Capability::BufferWrite),
        ("bunload", 3, true, Capability::BufferWrite),
        ("write", 1, true, Capability::FileSystemWrite),
        ("undo", 1, false, Capability::BufferWrite),
        ("redo", 3, false, Capability::BufferWrite),
        ("quit", 1, true, Capability::Editor),
    ] {
        host.capabilities.grant(capability.clone());
        host.register_command(CommandDefinition {
            name: name.into(),
            minimum_abbreviation: abbreviation,
            accepts_bang: bang,
            accepts_range: false,
            accepts_count: matches!(name, "undo" | "redo"),
            accepts_register: false,
            required_capabilities: vec![capability],
        });
    }
}

fn execute_editor_command(
    state: &mut EditorState,
    request: CommandRequest,
) -> Result<Value, RuntimeError> {
    let command = request.command;
    let current = current_buffer(state)?;
    let EditorState {
        buffers,
        mutator,
        exit_requested,
    } = state;
    match command.name.as_str() {
        "enew" => {
            let current_buffer = buffers.get(current).map_err(buffer_error)?;
            let pristine_unnamed = current_buffer.path().is_none()
                && !current_buffer.is_modified()
                && current_buffer.snapshot().is_empty();
            if pristine_unnamed {
                return Ok(Value::Null);
            }
            if current_buffer.is_modified() && !command.bang {
                return Err(buffer_error(vim_buffer::BufferError::ModifiedBuffer(
                    current,
                )));
            }
            if command.bang && current_buffer.path().is_none() {
                mutator
                    .execute(
                        buffers,
                        Action::Delete {
                            buffer: current,
                            force: true,
                        },
                    )
                    .map_err(buffer_error)?;
            } else {
                let created = mutator
                    .execute(
                        buffers,
                        Action::Create {
                            initial_text: String::new(),
                        },
                    )
                    .map_err(buffer_error)?;
                let ActionOutcome::Manager(vim_buffer::ManagerOutcome::Added(buffer)) = created
                else {
                    return Err(host_error("invalid create outcome"));
                };
                mutator
                    .execute(buffers, Action::SetCurrent { buffer })
                    .map_err(buffer_error)?;
            }
        }
        "edit" => {
            let path = command.arguments.trim();
            if path.is_empty() {
                return Err(RuntimeError::coded(
                    "E32",
                    RuntimeErrorKind::InvalidCommand,
                    "no file name",
                ));
            }
            let loaded = mutator
                .execute(buffers, Action::Load { path: path.into() })
                .map_err(buffer_error)?;
            let buffer = match loaded {
                ActionOutcome::Manager(
                    vim_buffer::ManagerOutcome::Loaded(id)
                    | vim_buffer::ManagerOutcome::Existing(id),
                ) => id,
                _ => return Err(host_error("invalid load outcome")),
            };
            mutator
                .execute(buffers, Action::SetCurrent { buffer })
                .map_err(buffer_error)?;
        }
        "buffer" => {
            let buffer = command_buffer(buffers, command.arguments.trim())?;
            mutator
                .execute(buffers, Action::SetCurrent { buffer })
                .map_err(buffer_error)?;
        }
        "bdelete" | "bwipeout" | "bunload" => {
            let buffer = if command.arguments.trim().is_empty() {
                current
            } else {
                command_buffer(buffers, command.arguments.trim())?
            };
            let action = match command.name.as_str() {
                "bdelete" => Action::Delete {
                    buffer,
                    force: command.bang,
                },
                "bwipeout" => Action::Wipe {
                    buffer,
                    force: command.bang,
                },
                _ => Action::Unload {
                    buffer,
                    force: command.bang,
                },
            };
            mutator.execute(buffers, action).map_err(buffer_error)?;
        }
        "write" => {
            let path =
                (!command.arguments.trim().is_empty()).then(|| command.arguments.trim().into());
            mutator
                .execute(
                    buffers,
                    Action::Save {
                        buffer: current,
                        path,
                        force: command.bang,
                    },
                )
                .map_err(buffer_error)?;
        }
        "undo" | "redo" => {
            let count = command.count.unwrap_or(1).try_into().unwrap_or(u32::MAX);
            let action = if command.name == "undo" {
                Action::Undo {
                    buffer: current,
                    count,
                }
            } else {
                Action::Redo {
                    buffer: current,
                    count,
                }
            };
            mutator.execute(buffers, action).map_err(buffer_error)?;
        }
        "quit" => {
            if buffers.get(current).map_err(buffer_error)?.is_modified() && !command.bang {
                return Err(buffer_error(vim_buffer::BufferError::ModifiedBuffer(
                    current,
                )));
            }
            *exit_requested = true;
        }
        _ => {
            return Err(RuntimeError::coded(
                "E492",
                RuntimeErrorKind::InvalidCommand,
                "unsupported command",
            ));
        }
    }
    Ok(Value::Null)
}

fn command_buffer(buffers: &BufferManager, argument: &str) -> Result<BufferId, RuntimeError> {
    let id = match argument {
        "%" => buffers.current(),
        "#" => buffers.alternate(),
        value => value.parse::<u64>().ok().and_then(BufferId::new),
    }
    .ok_or_else(|| {
        RuntimeError::coded(
            "E86",
            RuntimeErrorKind::InvalidCommand,
            "buffer does not exist",
        )
    })?;
    buffers.get(id).map_err(buffer_error)?;
    Ok(id)
}

fn current_buffer(state: &EditorState) -> Result<BufferId, RuntimeError> {
    state
        .buffers
        .current()
        .ok_or_else(|| host_error("editor has no current buffer"))
}

fn resolve_buffer(
    state: &EditorState,
    argument: Option<&Value>,
) -> Result<Option<BufferId>, RuntimeError> {
    match argument {
        None => Ok(state.buffers.current()),
        Some(Value::String(value)) if value.as_ref() == "%" => Ok(state.buffers.current()),
        Some(Value::String(value)) if value.as_ref() == "#" => Ok(state.buffers.alternate()),
        Some(Value::Integer(number)) if *number > 0 => {
            let id = BufferId::new(*number as u64);
            Ok(id.filter(|id| state.buffers.get(*id).is_ok()))
        }
        Some(value) => Err(RuntimeError::coded(
            "E745",
            RuntimeErrorKind::TypeError,
            format!(
                "bufnr() argument must be a number, '%' or '#', got {}",
                value.type_name()
            ),
        )),
    }
}

fn line_range(snapshot: &vim_buffer::BufferSnapshot, line: i64) -> Result<TextRange, RuntimeError> {
    let row = u32::try_from(
        line.checked_sub(1)
            .ok_or_else(|| range_error(line, snapshot.row_count()))?,
    )
    .map_err(|_| range_error(line, snapshot.row_count()))?;
    if row >= snapshot.row_count() {
        return Err(range_error(line, snapshot.row_count()));
    }
    let start = snapshot
        .point_to_offset(Point::new(row, 0))
        .map_err(buffer_error)?;
    let end = snapshot
        .point_to_offset(Point::new(
            row,
            snapshot.line_len(row).map_err(buffer_error)?,
        ))
        .map_err(buffer_error)?;
    TextRange::new(ByteOffset(start.0), ByteOffset(end.0))
        .ok_or_else(|| host_error("invalid line range"))
}

fn integer_argument(request: &HostRequest, index: usize) -> Result<i64, RuntimeError> {
    match request.arguments.get(index) {
        Some(Value::Integer(value)) => Ok(*value),
        Some(value) => Err(RuntimeError::coded(
            "E745",
            RuntimeErrorKind::TypeError,
            format!(
                "argument {} to {} must be a number, got {}",
                index + 1,
                request.function,
                value.type_name()
            ),
        )),
        None => Err(RuntimeError::coded(
            "E119",
            RuntimeErrorKind::ArityError,
            "missing argument",
        )),
    }
}

fn string_argument(request: &HostRequest, index: usize) -> Result<String, RuntimeError> {
    match request.arguments.get(index) {
        Some(Value::String(value)) => Ok(value.to_string()),
        Some(value) => Err(RuntimeError::coded(
            "E730",
            RuntimeErrorKind::TypeError,
            format!(
                "argument {} to {} must be a string, got {}",
                index + 1,
                request.function,
                value.type_name()
            ),
        )),
        None => Err(RuntimeError::coded(
            "E119",
            RuntimeErrorKind::ArityError,
            "missing argument",
        )),
    }
}

fn range_error(line: i64, line_count: u32) -> RuntimeError {
    RuntimeError::coded(
        "E16",
        RuntimeErrorKind::IndexError,
        format!("line {line} is outside the valid range 1..={line_count}"),
    )
}

fn buffer_error(error: vim_buffer::BufferError) -> RuntimeError {
    RuntimeError::coded("E_BUFFER", RuntimeErrorKind::HostError, error.to_string())
}

fn host_error(message: impl Into<String>) -> RuntimeError {
    RuntimeError::coded("E605", RuntimeErrorKind::HostError, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boots_with_one_current_unnamed_buffer() {
        let editor = HeadlessEditor::new().unwrap();
        let buffer = editor.current_buffer().unwrap();

        assert_eq!(buffer.get(), 1);
        assert_eq!(editor.current_text().unwrap(), "");
        assert_eq!(editor.changedtick(buffer).unwrap(), 0);
    }

    #[test]
    fn script_can_read_and_atomically_edit_current_buffer() {
        let mut editor = HeadlessEditor::new().unwrap();
        let buffer = editor.current_buffer().unwrap();

        editor
            .eval(
                "edit.vim",
                "let g:before = await getline(1)\nlet g:id = await bufnr()\nlet g:status = await setline(1, 'hello')",
            )
            .unwrap();

        assert_eq!(editor.current_text().unwrap(), "hello");
        assert_eq!(editor.changedtick(buffer).unwrap(), 1);
        assert_eq!(
            editor.global("g:before"),
            Some(&Value::String(Arc::from("")))
        );
        assert_eq!(editor.global("g:id"), Some(&Value::Integer(1)));
        assert_eq!(editor.global("g:status"), Some(&Value::Integer(0)));
        assert_eq!(editor.global("b:changedtick"), Some(&Value::Integer(1)));
    }

    #[test]
    fn failed_script_edit_does_not_mutate_the_buffer() {
        let mut editor = HeadlessEditor::new().unwrap();
        let buffer = editor.current_buffer().unwrap();

        let error = editor
            .eval("bad.vim", "let g:status = await setline(2, 'no')")
            .unwrap_err();

        assert!(matches!(error, EditorError::Runtime(_)));
        assert_eq!(editor.current_text().unwrap(), "");
        assert_eq!(editor.changedtick(buffer).unwrap(), 0);
    }

    #[test]
    fn script_edits_are_one_undo_step() {
        let mut editor = HeadlessEditor::new().unwrap();
        let buffer = editor.current_buffer().unwrap();
        editor
            .eval("edit.vim", "let g:status = await setline(1, 'hello')")
            .unwrap();

        assert!(editor.undo().unwrap());
        assert_eq!(editor.current_text().unwrap(), "");
        assert_eq!(editor.changedtick(buffer).unwrap(), 2);
        assert!(!editor.undo().unwrap());
    }

    #[test]
    fn ex_lifecycle_commands_follow_pristine_and_forced_enew_rules() {
        let mut editor = HeadlessEditor::new().unwrap();
        let first = editor.current_buffer().unwrap();

        editor.eval("lifecycle.vim", "enew").unwrap();
        assert_eq!(editor.current_buffer().unwrap(), first);

        editor
            .eval("change.vim", "let g:status = await setline(1, 'changed')")
            .unwrap();
        let error = editor.eval("protected.vim", "enew").unwrap_err();
        assert!(matches!(error, EditorError::Runtime(_)));

        editor.eval("forced.vim", "enew!").unwrap();
        let second = editor.current_buffer().unwrap();
        assert_ne!(second, first);
        assert_eq!(editor.alternate_buffer().unwrap(), Some(first));
        assert!(editor.buffer_exists(first).unwrap());
        assert!(!editor.buffer_loaded(first).unwrap());
        assert!(!editor.listed_buffers().unwrap().contains(&first));

        editor
            .eval("wipe.vim", &format!("bwipeout! {}", first.get()))
            .unwrap();
        assert!(!editor.buffer_exists(first).unwrap());
        assert_eq!(editor.current_buffer().unwrap(), second);
    }

    #[test]
    fn write_command_saves_through_the_mutator() {
        let directory =
            std::env::temp_dir().join(format!("nxvim-headless-write-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("written.txt");
        let mut editor = HeadlessEditor::new().unwrap();
        editor
            .eval("change.vim", "let g:status = await setline(1, 'saved')")
            .unwrap();
        editor
            .eval("write.vim", &format!(":write {}", path.display()))
            .unwrap();

        assert_eq!(std::fs::read_to_string(&path).unwrap(), "saved\n");
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    #[ignore = "requires the pinned Vim 9.2.0843 oracle executable"]
    fn lifecycle_state_matches_pinned_vim_oracle() {
        use std::process::Command;

        let output_path =
            std::env::temp_dir().join(format!("nxvim-headless-oracle-{}.txt", std::process::id()));
        let script =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("oracle/headless-lifecycle.vim");
        let status = Command::new("vim")
            .args([
                "--clean",
                "--not-a-term",
                "-N",
                "-es",
                "-X",
                "-i",
                "NONE",
                "-u",
                "NONE",
                "-U",
                "NONE",
                "-n",
                "-S",
            ])
            .arg(script)
            .env("NXVIM_ORACLE_OUTPUT", &output_path)
            .status()
            .unwrap();
        assert!(status.success(), "pinned Vim oracle failed with {status}");
        let oracle = std::fs::read_to_string(&output_path).unwrap();

        let mut editor = HeadlessEditor::new().unwrap();
        let first = editor.current_buffer().unwrap();
        editor.eval("pristine.vim", "enew").unwrap();
        let after_pristine = editor.current_buffer().unwrap();
        editor
            .eval("change.vim", "let g:status = await setline(1, 'changed')")
            .unwrap();
        editor.eval("forced.vim", "enew!").unwrap();
        let second = editor.current_buffer().unwrap();
        let alternate = editor.alternate_buffer().unwrap().unwrap();
        let first_line = format!(
            "{},{},{},{},{},{},{}",
            first.get(),
            after_pristine.get(),
            second.get(),
            alternate.get(),
            u8::from(editor.buffer_exists(first).unwrap()),
            u8::from(editor.buffer_loaded(first).unwrap()),
            u8::from(editor.listed_buffers().unwrap().contains(&first))
        );
        editor
            .eval("wipe.vim", &format!("bwipeout! {}", first.get()))
            .unwrap();
        let second_line = format!(
            "{},{},{}",
            editor.current_buffer().unwrap().get(),
            u8::from(editor.buffer_exists(first).unwrap()),
            u8::from(editor.listed_buffers().unwrap().contains(&second))
        );
        let actual = format!("{first_line}\n{second_line}\n");

        assert_eq!(actual, oracle);
        let _ = std::fs::remove_file(output_path);
    }
}
