use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use vim_script::ast::ExCommand;
use vim_script::compiler::Compiler;
use vim_script::host::{Arity, Capability, CommandDefinition, Host, HostContext, HostRuntime};
use vim_script::integration::{Event, SharedKeymapStore};
use vim_script::lexer::Lexer;
use vim_script::parser::Parser;
use vim_script::resolver::{Resolver, ResolverConfig};
use vim_script::runtime::{Scheduler, Value, Vm, builtins::BuiltinRegistry};
use vim_script::source::SourceMap;

pub mod commands;
mod ex;

#[derive(Clone, Default)]
pub struct EditorState {
    pub buffers: HashMap<text::BufferId, (text::BufferSnapshot, u64)>,
    pub names: HashMap<PathBuf, text::BufferId>,
    pub current_buffer_id: Option<text::BufferId>,
}

impl std::fmt::Debug for EditorState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EditorState")
            .field("buffers_count", &self.buffers.len())
            .field("names", &self.names)
            .field("current_buffer_id", &self.current_buffer_id)
            .finish()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AbbreviationMode {
    Insert,
    CommandLine,
}

#[derive(Clone, Debug)]
pub struct Abbreviation {
    pub lhs: String,
    pub rhs: String,
    pub non_recursive: bool,
    pub modes: Vec<AbbreviationMode>,
}

#[derive(Clone, Debug, Default)]
pub struct DigraphStore {
    custom: HashMap<(char, char), char>,
}

impl DigraphStore {
    pub fn new() -> Self {
        let mut custom = HashMap::new();
        custom.insert(('0', '0'), '∞');
        custom.insert(('C', 'o'), '©');
        custom.insert(('a', 'a'), 'あ');
        custom.insert(('e', '='), '€');
        custom.insert(('*', '*'), '★');
        custom.insert(('>', '='), '≥');
        custom.insert(('<', '='), '≤');
        Self { custom }
    }

    pub fn register(&mut self, c1: char, c2: char, target: char) {
        self.custom.insert((c1, c2), target);
    }

    pub fn lookup(&self, c1: char, c2: char) -> char {
        self.custom.get(&(c1, c2)).copied().unwrap_or('?')
    }
}

fn is_keyword(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

pub fn match_abbreviation(lhs: &str, left_text: &str) -> bool {
    if !left_text.ends_with(lhs) {
        return false;
    }
    let lhs_len = lhs.len();
    let pre_index = left_text.len() - lhs_len;
    let pre_char = if pre_index > 0 {
        left_text[..pre_index].chars().next_back()
    } else {
        None
    };

    let all_keyword = lhs.chars().all(is_keyword);
    let all_non_keyword = lhs.chars().all(|c| !is_keyword(c) && !c.is_whitespace());
    let starts_non_ends_keyword = lhs.chars().next().map_or(false, |c| !is_keyword(c))
        && lhs.chars().last().map_or(false, is_keyword);

    if all_keyword {
        pre_char.map_or(true, |c| !is_keyword(c))
    } else if all_non_keyword {
        pre_char.map_or(true, |c| is_keyword(c) || c.is_whitespace())
    } else if starts_non_ends_keyword {
        pre_char.map_or(true, |c| c.is_whitespace())
    } else {
        false
    }
}

pub struct ScriptHost {
    scheduler: Scheduler,
    keymaps: SharedKeymapStore,
    abbreviations: Vec<Abbreviation>,
    digraphs: DigraphStore,
    state: Arc<Mutex<EditorState>>,
    globals: HashMap<String, Value>,
    builtins: BuiltinRegistry,
    sources: SourceMap,
}

impl ScriptHost {
    pub fn new(
        host: Arc<dyn Host>,
        keymaps: SharedKeymapStore,
        state: Arc<Mutex<EditorState>>,
    ) -> Self {
        let mut runtime = HostRuntime::with_keymaps(host, keymaps.clone());
        runtime.capabilities.grant(Capability::Editor);
        runtime.capabilities.grant(Capability::BufferRead);
        runtime.capabilities.grant(Capability::BufferWrite);
        runtime.capabilities.grant(Capability::Window);
        runtime.capabilities.grant(Capability::UserInterface);
        runtime.capabilities.grant(Capability::Settings);

        runtime.register_function("echo", Arity::Exact(1), vec![Capability::Editor]);
        runtime.register_function("message", Arity::Exact(1), vec![Capability::Editor]);
        runtime.register_function("echomsg", Arity::Exact(1), vec![Capability::Editor]);
        runtime.register_function(
            "bufnr",
            Arity::Range { min: 0, max: 1 },
            vec![Capability::BufferRead],
        );
        runtime.register_function("bufexists", Arity::Exact(1), vec![Capability::BufferRead]);
        runtime.register_function(
            "getline",
            Arity::Range { min: 1, max: 2 },
            vec![Capability::BufferRead],
        );
        runtime.register_function(
            "getbufline",
            Arity::Range { min: 2, max: 3 },
            vec![Capability::BufferRead],
        );
        runtime.register_function(
            "getbufoneline",
            Arity::Exact(2),
            vec![Capability::BufferRead],
        );
        runtime.register_function(
            "feedkeys",
            Arity::Range { min: 1, max: 2 },
            vec![Capability::Editor],
        );

        for spec in commands::COMMAND_SPECS {
            runtime.register_command(CommandDefinition::from(spec));
        }

        let mut scheduler = Scheduler::default();
        scheduler.set_host(runtime);

        Self {
            scheduler,
            keymaps,
            abbreviations: Vec::new(),
            digraphs: DigraphStore::new(),
            state,
            globals: HashMap::new(),
            builtins: BuiltinRegistry::with_defaults(),
            sources: SourceMap::default(),
        }
    }

    pub fn execute(&mut self, source: &str) -> Result<Value, String> {
        self.execute_with_context(source, None)
    }

    pub fn canonicalize_command(&self, mut command: ExCommand) -> Result<ExCommand, String> {
        let host = self
            .scheduler
            .host()
            .ok_or_else(|| "script host is not installed".to_owned())?;
        let definition = host
            .commands
            .resolve(&command.name)
            .map_err(|error| error.message)?;
        command.name = definition.name.clone();
        Ok(command)
    }

    pub fn execute_with_context(
        &mut self,
        source: &str,
        current: Option<crate::kernel::command::CommandContext>,
    ) -> Result<Value, String> {
        let normalized = matches!(source.trim(), "&" | "~").then(|| "substitute".to_string());
        let mut source = normalized.as_deref().unwrap_or(source);
        if let Some(stripped) = source.strip_prefix(':') {
            if stripped
                .trim_start()
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            {
                source = stripped;
            }
        }

        let source_id = self.sources.add("command_line", source);
        let lexed = Lexer::new(source_id, source).lex();
        self.check_diagnostics(&lexed.diagnostics)?;

        let parsed = Parser::new_with_source(&lexed.tokens, source).parse();
        self.check_diagnostics(&parsed.diagnostics)?;
        let program = parsed
            .program
            .ok_or_else(|| "script produced no program".to_owned())?;

        let host = self.scheduler.host().expect("script host is installed");
        let mut config = ResolverConfig::default();
        config.unqualified_is_global = true;
        config
            .builtins
            .extend(host.functions.names().map(str::to_owned));

        let resolved = Resolver::new(config).resolve(program);
        self.check_diagnostics(&resolved.diagnostics)?;
        let resolved_program = resolved
            .program
            .ok_or_else(|| "script resolution produced no program".to_owned())?;

        let compiled = Compiler::new(&resolved_program).compile();
        self.check_diagnostics(&compiled.diagnostics)?;
        let module = compiled
            .module
            .ok_or_else(|| "script compilation produced no module".to_owned())?;

        let mut vm = Vm::with_globals(module, self.globals.clone()).map_err(|e| e.message)?;
        vm.builtins = self.builtins.clone();
        if let Some(current) = current {
            vm.host_context.current_tab = Some(current.tab.get());
            vm.host_context.current_window = Some(current.window.get());
            vm.host_context.current_buffer = Some(current.buffer.get());
        }

        let task_res = self.scheduler.spawn(vm).map_err(|e| e.message);
        let value = match task_res {
            Ok(task) => {
                let run_res = self
                    .scheduler
                    .run_until_complete(task)
                    .map_err(|e| e.message);
                if let Some(task_info) = self.scheduler.task(task) {
                    self.globals = task_info.vm.globals.clone();
                }
                run_res
            }
            Err(e) => Err(e),
        };

        value
    }

    fn check_diagnostics(&self, diagnostics: &[vim_script::Diagnostic]) -> Result<(), String> {
        if diagnostics.is_empty() {
            return Ok(());
        }
        Err(diagnostics
            .iter()
            .map(|diagnostic| self.sources.render(diagnostic))
            .collect::<Vec<_>>()
            .join("\n"))
    }

    pub fn update_state(&self, editor: &crate::kernel::Editor) -> Result<(), String> {
        let active_ids = editor.buffer_ids();
        let current_ctx = editor.current_context();
        let current_buffer_id = text::BufferId::new(current_ctx.buffer.get())
            .map_err(|_| format!("invalid current buffer id: {}", current_ctx.buffer.get()))?;

        let mut lock = self
            .state
            .lock()
            .map_err(|_| "Editor state lock is poisoned".to_owned())?;
        lock.current_buffer_id = Some(current_buffer_id);
        // Remove buffers that are no longer listed/active
        lock.buffers.retain(|id, _| {
            active_ids
                .iter()
                .any(|&active_id| active_id.get() == id.to_proto())
        });
        lock.names.clear();

        for id in active_ids {
            if let Some(buffer) = editor.buffer(id) {
                let text_id = text::BufferId::new(id.get()).unwrap();
                let current_tick = buffer.changedtick().get();

                let needs_update = match lock.buffers.get(&text_id) {
                    Some((_, existing_tick)) => *existing_tick != current_tick,
                    None => true,
                };

                if needs_update {
                    let snapshot = buffer.as_text_buffer().snapshot().clone();
                    lock.buffers.insert(text_id, (snapshot, current_tick));
                }

                if let Some(path) = buffer.path() {
                    lock.names.insert(path.to_path_buf(), text_id);
                }
            }
        }

        Ok(())
    }

    pub fn shared_keymaps(&self) -> SharedKeymapStore {
        self.keymaps.clone()
    }

    pub fn digraphs(&self) -> &DigraphStore {
        &self.digraphs
    }

    pub fn digraphs_mut(&mut self) -> &mut DigraphStore {
        &mut self.digraphs
    }

    pub fn lookup_abbreviation(
        &self,
        left_text: &str,
        mode: AbbreviationMode,
    ) -> Option<Abbreviation> {
        for abbr in &self.abbreviations {
            if abbr.modes.contains(&mode) && match_abbreviation(&abbr.lhs, left_text) {
                return Some(abbr.clone());
            }
        }
        None
    }

    pub fn fire_event(&mut self, name: &str, pattern: Option<&str>) -> Vec<ExCommand> {
        let event = Event {
            name: name.to_owned(),
            pattern: pattern.map(String::from),
            payload: HashMap::new(),
        };
        let requests = self
            .scheduler
            .host_mut()
            .unwrap()
            .event_commands(&event, HostContext::default());
        requests.into_iter().map(|req| req.command).collect()
    }
}
