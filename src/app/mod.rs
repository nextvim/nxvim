//! Composition root for the terminal application.
//!
//! `App` owns exactly one `kernel::Editor` and turns translated input into
//! `Editor::execute()` calls. No queues, no services, no script host yet —
//! those arrive with the milestones that need them.

pub mod args;
pub mod input;
pub mod lifecycle;
pub mod persistence;
pub mod prompt;
pub mod request;
pub mod script_host;
pub mod task_dispatcher;
pub mod view_sync;

use crate::kernel::outcome::RedrawInvalidation;
use crate::kernel::{Editor, outcome::Outcome};
use crate::services;

use prompt::CommandPrompt;
use request::AppRequest;
use vim_buffer::BufferText;
use vim_input::Action;

fn merge_outcomes(combined: &mut Outcome, next: Outcome) {
    use crate::kernel::outcome::RedrawInvalidation;

    combined.mutated |= next.mutated;
    combined.mode_changed |= next.mode_changed;
    combined.invalidation = match (combined.invalidation, next.invalidation) {
        (RedrawInvalidation::None, invalidation) => invalidation,
        (invalidation, RedrawInvalidation::None) => invalidation,
        (RedrawInvalidation::All, _) | (_, RedrawInvalidation::All) => RedrawInvalidation::All,
        (left, right) if left == right => left,
        _ => RedrawInvalidation::All,
    };
    combined.effects.extend(next.effects);
    combined.events.extend(next.events);
}

pub fn parse_feedkeys_keys(input: &str) -> Vec<vim_input::Key> {
    use vim_input::{Key, KeyCode, KeyPattern, KeySequence, Modifiers};

    let mut keys = Vec::new();
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let ch = chars[i];
        if ch == '\\' && i + 1 < chars.len() && chars[i + 1] == '<' {
            i += 1;
            continue;
        }
        if ch == '\x1b' {
            keys.push(Key::new(KeyCode::Escape, Modifiers::NONE));
            i += 1;
        } else if ch == '\r' || ch == '\n' {
            keys.push(Key::new(KeyCode::Enter, Modifiers::NONE));
            i += 1;
        } else if ch == '\t' {
            keys.push(Key::new(KeyCode::Tab, Modifiers::NONE));
            i += 1;
        } else if ch == '\x08' || ch == '\x7f' {
            keys.push(Key::new(KeyCode::Backspace, Modifiers::NONE));
            i += 1;
        } else if ch == '<' {
            let rest: String = chars[i..].iter().collect();
            if let Ok(seq) = KeySequence::parse(&rest) {
                if let Some(KeyPattern::Exact(k)) = seq.items.first() {
                    keys.push(*k);
                    if let Some(close_idx) = chars[i + 1..].iter().position(|c| *c == '>') {
                        i += close_idx + 2;
                        continue;
                    }
                }
            }
            keys.push(Key::char('<'));
            i += 1;
        } else {
            keys.push(Key::char(ch));
            i += 1;
        }
    }

    keys
}

pub struct App {
    editor: Editor,
    prompt: CommandPrompt,
    pending_request: Option<AppRequest>,
    script: crate::script::ScriptHost,
    colorscheme: vim_ui::ColorScheme,
    script_rx: std::sync::mpsc::Receiver<AppRequest>,
    services: services::Services,
    source_depth: usize,
    command_history: Vec<String>,
    search_history: Vec<String>,
    history_index: Option<usize>,
    history_temp: String,
}

impl App {
    pub fn new(initial_text: impl Into<String>) -> Self {
        Self::from_editor(Editor::new(initial_text))
    }

    /// Creates an app by loading `paths` from disk as the initial editor
    /// state (see `kernel::Editor::open`) — what `main.rs` uses for real
    /// command-line file arguments. `App::new` (seeded in-memory text)
    /// stays the constructor used by tests and the no-args placeholder.
    pub fn open(paths: &[std::path::PathBuf]) -> Self {
        Self::from_editor(Editor::open(paths))
    }

    fn from_editor(editor: Editor) -> Self {
        let prompt = CommandPrompt::new();
        let keymaps =
            std::sync::Arc::new(std::sync::RwLock::new(vim_input::MappingStore::default()));
        let (tx, rx) = std::sync::mpsc::channel();
        let state =
            std::sync::Arc::new(std::sync::Mutex::new(crate::script::EditorState::default()));
        let host = std::sync::Arc::new(script_host::ActiveHost::new(tx, state.clone()));
        let script = crate::script::ScriptHost::new(host, keymaps, state);

        Self {
            editor,
            prompt,
            pending_request: None,
            script,
            colorscheme: vim_ui::ColorScheme::load_default(),
            script_rx: rx,
            services: services::Services::new(),
            source_depth: 0,
            command_history: Vec::new(),
            search_history: Vec::new(),
            history_index: None,
            history_temp: String::new(),
        }
    }

    pub fn init(
        &mut self,
        pre_config_cmds: &[String],
        post_config_cmds: &[String],
        scripts: &[std::path::PathBuf],
        skip_config: bool,
    ) {
        for cmd in pre_config_cmds {
            self.execute_line(cmd);
        }

        if !skip_config {
            let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
            if let Some(home) = home {
                let paths = [
                    home.join(".config/nxvim/init.vim"),
                    home.join(".nxvimrc"),
                    home.join(".nxvim/nxvimrc"),
                    home.join(".config/nxvim/nxvimrc"),
                ];

                for path in &paths {
                    if path.exists() {
                        self.execute_source(path);
                        break;
                    }
                }
            }
        }

        for cmd in post_config_cmds {
            self.execute_line(cmd);
        }

        for path in scripts {
            self.execute_source(path);
        }
    }

    pub fn script_mut(&mut self) -> &mut crate::script::ScriptHost {
        &mut self.script
    }

    pub fn execute_source(&mut self, path: &std::path::Path) -> Outcome {
        const MAX_SOURCE_DEPTH: usize = 100;

        if self.source_depth >= MAX_SOURCE_DEPTH {
            self.pending_request = Some(AppRequest::ShowMessage(format!(
                "E169: Command too recursive while sourcing {}",
                path.display()
            )));
            return Outcome::default();
        }

        let content = match std::fs::read_to_string(path) {
            Ok(content) => content,
            Err(_) => {
                self.pending_request = Some(AppRequest::ShowMessage(format!(
                    "E484: Can't open file {}",
                    path.display()
                )));
                return Outcome::default();
            }
        };

        self.source_depth += 1;
        let outcome = self.execute_script(&content);
        self.source_depth -= 1;
        outcome
    }

    pub fn execute_line(&mut self, line: &str) -> Outcome {
        self.execute_script(line)
    }

    pub fn execute_script(&mut self, content: &str) -> Outcome {
        let _ = self.script.update_state(&self.editor);
        let ctx = self.editor.current_context();
        if let Err(e) = self.script.execute_with_context(content, Some(ctx)) {
            self.pending_request = Some(AppRequest::ShowMessage(format!("Error: {e}")));
        }
        let outcome = self.dispatch_script_requests();
        let _ = self.script.sync_state_to_editor(&mut self.editor);
        let _ = self.script.update_state(&self.editor);
        outcome
    }

    /// Drains commands emitted by the scripting host. Kernel mutations stay on
    /// the application thread; only terminal-facing requests are retained for
    /// the outer runtime loop.
    pub fn dispatch_script_requests(&mut self) -> Outcome {
        let mut combined = Outcome::default();
        while let Ok(request) = self.script_rx.try_recv() {
            let outcome = match request {
                AppRequest::Quit => {
                    self.pending_request = Some(AppRequest::Quit);
                    Outcome::default()
                }
                AppRequest::ShowMessage(message) => {
                    self.pending_request = Some(AppRequest::ShowMessage(message));
                    Outcome::default()
                }
                AppRequest::ExecuteEx(command) => self.execute_ex_command(command),
                AppRequest::ExecuteExString(cmd_str) => self.execute_script(&cmd_str),
                AppRequest::Source(path) => self.execute_source(&path),
                AppRequest::FeedKeys { keys, mode } => self.execute_feedkeys(&keys, &mode),
                AppRequest::PopupCreate { lines, options } => self.execute_popup_create(lines, options),
                AppRequest::PopupClose { id } => self.execute_popup_close(id),
                AppRequest::PopupSetText { id, lines } => self.execute_popup_settext(id, lines),
            };
            merge_outcomes(&mut combined, outcome);
        }
        combined
    }

    pub fn execute_feedkeys(&mut self, keys_str: &str, mode_flags: &str) -> Outcome {
        let keys = parse_feedkeys_keys(keys_str);
        let remap = !mode_flags.contains('n');
        let mut translator = input::InputTranslator::with_mappings(self.shared_keymaps());
        let mut combined_outcome = Outcome::default();

        for key in keys {
            translator.sync_mode(self.editor().mode());
            translator.sync_recording(self.editor().macro_recorder.is_recording());

            if self.editor().mode().is_command() {
                let raw_key = match key.code {
                    vim_input::KeyCode::Char(c) => crate::app::input::RawKey::Char(c),
                    vim_input::KeyCode::Enter => crate::app::input::RawKey::Enter,
                    vim_input::KeyCode::Escape => crate::app::input::RawKey::Escape,
                    vim_input::KeyCode::Backspace => crate::app::input::RawKey::Backspace,
                    vim_input::KeyCode::Tab => crate::app::input::RawKey::Char('\t'),
                    vim_input::KeyCode::Up => crate::app::input::RawKey::Up,
                    vim_input::KeyCode::Down => crate::app::input::RawKey::Down,
                    vim_input::KeyCode::Left => crate::app::input::RawKey::Left { select: false },
                    vim_input::KeyCode::Right => crate::app::input::RawKey::Right { select: false },
                    _ => crate::app::input::RawKey::Char(' '),
                };
                let outcome = self.handle_raw_key(raw_key);
                merge_outcomes(&mut combined_outcome, outcome);
            } else {
                let buf_id = self.editor().current_context().buffer.get();
                let resolved_opt = if remap {
                    translator.feed_key_with_buffer_public(key, Some(buf_id))
                } else {
                    translator.feed_key_noremap(key, Some(buf_id))
                };

                if let Some(resolved) = resolved_opt {
                    let outcome = self.handle_action(resolved.action, resolved.register);
                    merge_outcomes(&mut combined_outcome, outcome);
                }
            }
        }

        combined_outcome
    }

    pub fn execute_popup_create(
        &mut self,
        lines: Vec<String>,
        options: std::collections::BTreeMap<String, vim_script::runtime::Value>,
    ) -> Outcome {
        let text = lines.join("\n");
        let buf_id = self.editor.buffers_mut().insert(text);

        let popup_id = self.editor.global_popups_mut().insert(|id| {
            let mut popup = crate::kernel::window::popup::PopupWindow::new(id, buf_id);
            if let Some(vim_script::runtime::Value::Integer(t)) = options.get("time") {
                if *t > 0 {
                    popup.behavior.time_limit_ms = Some(*t as u64);
                }
            }
            if let Some(vim_script::runtime::Value::String(title)) = options.get("title") {
                popup.style.title = Some(title.to_string());
            }
            if let Some(vim_script::runtime::Value::List(b)) = options.get("border") {
                if !b.is_empty() {
                    popup.style.border = crate::kernel::window::popup::PopupBorder {
                        top: true,
                        right: true,
                        bottom: true,
                        left: true,
                    };
                }
            }
            if let Some(vim_script::runtime::Value::Integer(line)) = options.get("line") {
                popup.layout.line = *line as i32;
            }
            if let Some(vim_script::runtime::Value::Integer(col)) = options.get("col") {
                popup.layout.col = *col as i32;
            }
            if let Some(filter_val) = options.get("filter") {
                let filter_str = match filter_val {
                    vim_script::runtime::Value::String(s) => s.as_ref(),
                    vim_script::runtime::Value::Builtin(s) => s.as_ref(),
                    vim_script::runtime::Value::HostFunction(s) => s.as_ref(),
                    other => "",
                };
                match filter_str {
                    "popup_filter_yesno" => {
                        popup.behavior.filter = crate::kernel::window::popup::PopupFilter::BuiltinYesNo;
                    }
                    "popup_filter_menu" => {
                        popup.behavior.filter = crate::kernel::window::popup::PopupFilter::BuiltinMenu { selected_index: 1 };
                    }
                    func if !func.is_empty() => {
                        popup.behavior.filter = crate::kernel::window::popup::PopupFilter::ScriptFunction(func.to_string());
                    }
                    _ => {}
                }
            }
            popup
        });

        let _ = popup_id;
        Outcome {
            invalidation: RedrawInvalidation::Popup,
            ..Default::default()
        }
    }

    pub fn execute_popup_close(&mut self, id: u64) -> Outcome {
        if id == 0 {
            if let Some((_, popup_id, _)) = self.editor.find_active_filter_popup() {
                self.editor.close_popup(popup_id, 0);
            }
        } else {
            let popup_id = crate::kernel::ids::PopupWindowId::new(id);
            self.editor.close_popup(popup_id, 0);
        }
        Outcome {
            invalidation: RedrawInvalidation::Popup,
            ..Default::default()
        }
    }

    pub fn execute_popup_settext(&mut self, id: u64, lines: Vec<String>) -> Outcome {
        let popup_id = crate::kernel::ids::PopupWindowId::new(id);
        self.editor.set_popup_text(popup_id, &lines);
        Outcome {
            invalidation: RedrawInvalidation::Popup,
            ..Default::default()
        }
    }

    pub fn editor(&self) -> &Editor {
        &self.editor
    }

    pub fn editor_mut(&mut self) -> &mut Editor {
        &mut self.editor
    }

    pub fn colorscheme(&self) -> &vim_ui::ColorScheme {
        &self.colorscheme
    }

    pub fn shared_keymaps(&self) -> vim_input::SharedMappingStore {
        self.script.shared_keymaps()
    }

    pub fn digraphs(&self) -> &crate::script::DigraphStore {
        self.script.digraphs()
    }

    /// Queues a save through the application service boundary. The caller does
    /// not receive worker/task internals; completion is sequenced by runtime.
    pub fn save_current_buffer_in_background(
        &mut self,
        path: Option<std::path::PathBuf>,
    ) -> Result<(), String> {
        let buffer = self.editor.current_context().buffer;
        lifecycle::start_background_save(&mut self.services, &self.editor, buffer, path).map(|_| ())
    }

    pub(crate) fn poll_services(&mut self, render_state: &mut crate::view::RenderState) -> Outcome {
        let mut outcome = Outcome::default();
        for result in self.services.poll() {
            match result.metadata.kind {
                services::TaskKind::DisplayMap => {
                    let application = view_sync::apply_display_map_result(
                        &self.editor,
                        &mut self.services,
                        render_state,
                        result,
                    );
                    if application != crate::view::ExpansionApplication::Discarded {
                        outcome.invalidation =
                            crate::kernel::outcome::RedrawInvalidation::CurrentWindow;
                    }
                }
                services::TaskKind::File => {
                    if let Ok(Some(effect)) = lifecycle::apply_background_save(
                        &mut self.services,
                        &mut self.editor,
                        result,
                    ) {
                        if let Some(request) = services::describe_effect(&effect) {
                            self.pending_request = Some(request);
                        }
                        outcome.effects.push(effect);
                    }
                }
                services::TaskKind::TreeSitter => {
                    view_sync::apply_treesitter_result(&self.editor, &mut self.services, result);
                }
                services::TaskKind::Indexer => {}
            }
        }
        view_sync::schedule_treesitter_parses(&self.editor, &mut self.services);
        outcome
    }

    pub fn render(
        &mut self,
        out: &mut impl std::io::Write,
        render_state: &mut crate::view::RenderState,
        status: &str,
        prompt: Option<&str>,
        screen: vim_ui::Rect,
        pending: &[crate::kernel::outcome::RedrawInvalidation],
        force_full: bool,
    ) -> std::io::Result<()> {
        crate::view::render_with_scheme(
            out,
            &mut self.editor,
            render_state,
            status,
            prompt,
            prompt.map(|_| self.prompt.cursor()),
            screen,
            pending,
            force_full,
            &self.colorscheme,
        )?;

        Ok(())
    }

    /// Spawn display-map expansion tasks for windows that have missing rows.
    /// Intended to be called from the runtime idle poll loop, gated by
    /// `RenderState::advance_idle()`.
    pub(crate) fn schedule_display_map_expansions(
        &mut self,
        render_state: &mut crate::view::RenderState,
    ) {
        view_sync::schedule_display_map_expansions(&mut self.services, render_state);
    }

    pub fn prompt(&self) -> &CommandPrompt {
        &self.prompt
    }

    pub fn handle_action(&mut self, action: Action, register: Option<char>) -> Outcome {
        let _ = self.script.update_state(&self.editor);
        if let Some((_, popup_id, _)) = self.editor.find_active_filter_popup() {
            let script_filter = self.editor.global_popups().get(popup_id).and_then(|p| {
                if let crate::kernel::window::popup::PopupFilter::ScriptFunction(ref func) = p.behavior.filter {
                    Some(func.clone())
                } else {
                    None
                }
            });
            if let Some(func_name) = script_filter {
                let key_str = match &action {
                    Action::InsertText(s) => s.clone(),
                    Action::CarriageReturn | Action::InsertNewLine { .. } => "\r".to_string(),
                    Action::MoveUp { .. } => "k".to_string(),
                    Action::MoveDown { .. } => "j".to_string(),
                    Action::MoveLeft { .. } => "h".to_string(),
                    Action::MoveRight { .. } => "l".to_string(),
                    Action::Quit => "\x1b".to_string(),
                    Action::DeleteCharBefore { .. } => "\x08".to_string(),
                    _ => "".to_string(),
                };
                let escaped_key = format!("\"{}\"", key_str.escape_default());
                let script_cmd = format!("let g:__popup_filter_ret = {func_name}({}, {escaped_key})", popup_id.get());
                self.execute_script(&script_cmd);
            }
        }
        if matches!(
            action,
            Action::SetToCommand
                | Action::SetToCommandSearchForward
                | Action::SetToCommandSearchBackward
        ) {
            self.history_index = None;
            self.history_temp.clear();
        }
        if register == Some('+') || register == Some('*') {
            let reg_name = if register == Some('*') {
                vim_clipboard::RegisterName::Selection
            } else {
                vim_clipboard::RegisterName::System
            };
            if let Some(text) = vim_clipboard::read_system_clipboard(reg_name) {
                self.editor.prime_clipboard_register(text);
            }
        }

        let mut prefix_outcome = None;
        if self.editor.mode().is_insert() {
            let is_keyword = |c: char| c.is_alphanumeric() || c == '_';
            match &action {
                Action::InsertText(text) => {
                    if let Some(first_char) = text.chars().next() {
                        if !is_keyword(first_char) {
                            prefix_outcome = self.check_abbreviation_expansion(Some(first_char));
                        }
                    }
                }
                Action::InsertNewLine { .. } | Action::Clear => {
                    prefix_outcome = self.check_abbreviation_expansion(None);
                }
                _ => {}
            }
        }
        let visual_prompt_prefix = if self.editor.mode().is_visual() {
            match action {
                Action::SetToCommand => Some("'<,'>".to_string()),
                Action::SetToCommandSearchForward | Action::SetToCommandSearchBackward => {
                    use text::{ToOffset, ToPoint};
                    use vim_buffer::{BufferText, TextSearch};
                    let ctx = self.editor.current_context();
                    let (win, buffer) = self.editor.window_and_buffer_mut(ctx.window);
                    let text_buf = buffer.as_text_buffer();
                    let primary = win.selections().primary();
                    let start_off = primary.start.to_offset(text_buf);
                    let end_off = primary.end.to_offset(text_buf);

                    let text = if start_off != end_off {
                        let low = start_off.min(end_off);
                        let high = start_off.max(end_off);
                        text_buf
                            .as_rope()
                            .chunks_in_range(low..high)
                            .collect::<String>()
                    } else {
                        let point = primary.head().to_point(text_buf);
                        let row_text = text_buf.row_text(point.row);
                        row_text
                            .find_word(point.column as usize)
                            .map(|(_, _, w)| w.to_string())
                            .unwrap_or_default()
                    };

                    if text.is_empty() {
                        None
                    } else {
                        let is_word = text.chars().all(|c| c.is_alphanumeric() || c == '_');
                        let escaped = crate::kernel::command::search::regex_escape(&text);
                        let query = if is_word {
                            format!("\\<{}\\>", escaped)
                        } else {
                            escaped
                        };
                        Some(query)
                    }
                }
                _ => None,
            }
        } else {
            None
        };

        let mut outcome = if view_sync::handle_treesitter_motion(&mut self.editor, &self.services, &action) {
            crate::kernel::outcome::Outcome {
                invalidation: crate::kernel::outcome::RedrawInvalidation::CurrentWindow,
                ..Default::default()
            }
        } else {
            self.editor.execute_with_register(action, register)
        };
        if let Some(prefix) = visual_prompt_prefix {
            self.prompt.set_text(prefix);
        }
        if let Some(prefix) = prefix_outcome {
            outcome.mode_changed |= prefix.mode_changed;
            use crate::kernel::outcome::RedrawInvalidation;
            match (prefix.invalidation, outcome.invalidation) {
                (RedrawInvalidation::All, _) | (_, RedrawInvalidation::All) => {
                    outcome.invalidation = RedrawInvalidation::All;
                }
                (RedrawInvalidation::Range { buffer, range }, _) => {
                    if outcome.invalidation == RedrawInvalidation::None
                        || outcome.invalidation == RedrawInvalidation::CurrentWindow
                    {
                        outcome.invalidation = RedrawInvalidation::Range { buffer, range };
                    }
                }
                (RedrawInvalidation::CurrentWindow, _) => {
                    if outcome.invalidation == RedrawInvalidation::None {
                        outcome.invalidation = RedrawInvalidation::CurrentWindow;
                    }
                }
                _ => {}
            }
            outcome.effects.extend(prefix.effects);
            outcome.events.extend(prefix.events);
        }

        if outcome
            .effects
            .contains(&crate::kernel::outcome::Effect::Quit)
        {
            self.pending_request = Some(AppRequest::Quit);
        }
        for effect in &outcome.effects {
            if let Some(req) = services::describe_effect(effect) {
                self.pending_request = Some(req);
            }
        }
        self.process_autocommands(&outcome);
        outcome
    }

    fn active_history(&self) -> &[String] {
        match self.editor.mode() {
            crate::kernel::mode::Mode::Command(
                crate::kernel::mode::CommandKind::SearchForward
                | crate::kernel::mode::CommandKind::SearchBackward,
            ) => &self.search_history,
            _ => &self.command_history,
        }
    }

    fn history_previous(&mut self) {
        let len = self.active_history().len();
        if len == 0 {
            return;
        }
        let index = match self.history_index {
            None => {
                self.history_temp = self.prompt.text().to_owned();
                len - 1
            }
            Some(index) => index.saturating_sub(1),
        };
        let text = self.active_history()[index].clone();
        self.history_index = Some(index);
        self.prompt.set_text(text);
    }

    fn history_next(&mut self) {
        let Some(index) = self.history_index else {
            return;
        };
        let len = self.active_history().len();
        if index + 1 < len {
            let next = index + 1;
            let text = self.active_history()[next].clone();
            self.history_index = Some(next);
            self.prompt.set_text(text);
        } else {
            self.history_index = None;
            self.prompt.set_text(self.history_temp.clone());
        }
    }

    fn record_history(&mut self, mode: crate::kernel::mode::Mode, line: &str) {
        if line.is_empty() {
            return;
        }
        let history = match mode {
            crate::kernel::mode::Mode::Command(
                crate::kernel::mode::CommandKind::SearchForward
                | crate::kernel::mode::CommandKind::SearchBackward,
            ) => &mut self.search_history,
            crate::kernel::mode::Mode::Command(crate::kernel::mode::CommandKind::Ex) => {
                &mut self.command_history
            }
            _ => return,
        };
        if history.last().is_none_or(|previous| previous != line) {
            history.push(line.to_owned());
        }
    }

    pub fn handle_raw_key(&mut self, raw_key: input::RawKey) -> Outcome {
        let _ = self.script.update_state(&self.editor);
        if self.editor.find_active_filter_popup().is_some() {
            let action = match raw_key {
                input::RawKey::Char(c) => vim_input::Action::InsertText(c.to_string()),
                input::RawKey::Escape => vim_input::Action::InsertText("\x1b".to_string()),
                input::RawKey::Enter => vim_input::Action::CarriageReturn,
                input::RawKey::Up => vim_input::Action::MoveUp { count: 1, select: false },
                input::RawKey::Down => vim_input::Action::MoveDown { count: 1, select: false },
                input::RawKey::Left { .. } => vim_input::Action::MoveLeft { count: 1, select: false },
                input::RawKey::Right { .. } => vim_input::Action::MoveRight { count: 1, select: false },
                input::RawKey::Backspace => vim_input::Action::DeleteCharBefore { count: 1 },
                _ => vim_input::Action::NoOp,
            };
            return self.handle_action(action, None);
        }

        if self.editor.has_pending_substitute() {
            let outcome = match raw_key {
                input::RawKey::Char(ch) => {
                    let ch = ch.to_ascii_lowercase();
                    if "ynaql".contains(ch) {
                        self.editor.handle_substitute_confirm(ch)
                    } else {
                        Outcome::default()
                    }
                }
                input::RawKey::Escape => self.editor.handle_substitute_confirm('q'),
                _ => Outcome::default(),
            };
            for effect in &outcome.effects {
                if let Some(req) = services::describe_effect(effect) {
                    self.pending_request = Some(req);
                }
            }
            return outcome;
        }

        let mut outcome = match raw_key {
            input::RawKey::Char(ch) => {
                let is_keyword = |c: char| c.is_alphanumeric() || c == '_';
                if !is_keyword(ch) {
                    let left_text = self.prompt.text();
                    if let Some(abbr) = self.script.lookup_abbreviation(
                        left_text,
                        crate::script::AbbreviationMode::CommandLine,
                    ) {
                        let lhs_len = abbr.lhs.len();
                        let new_len = left_text.len() - lhs_len;
                        let mut new_text = left_text[..new_len].to_string();
                        new_text.push_str(&abbr.rhs);
                        self.prompt.set_text(new_text);
                    }
                }
                self.prompt.push(ch);
                Outcome::default()
            }
            input::RawKey::Backspace => {
                self.prompt.backspace();
                Outcome::default()
            }
            input::RawKey::Delete => {
                self.prompt.delete();
                Outcome::default()
            }
            input::RawKey::Left { select } => {
                self.prompt.move_left(select);
                Outcome::default()
            }
            input::RawKey::Right { select } => {
                self.prompt.move_right(select);
                Outcome::default()
            }
            input::RawKey::Home { select } => {
                self.prompt.move_home(select);
                Outcome::default()
            }
            input::RawKey::End { select } => {
                self.prompt.move_end(select);
                Outcome::default()
            }
            input::RawKey::Up => {
                self.history_previous();
                Outcome::default()
            }
            input::RawKey::Down => {
                self.history_next();
                Outcome::default()
            }
            // Command-line buffers are deliberately single-line: Vim's
            // InsertNewLine equivalent submits instead of mutating the buffer.
            input::RawKey::Enter => {
                let mode = self.editor.mode();
                let line = self.prompt.take();
                self.record_history(mode, &line);
                self.history_index = None;
                self.history_temp.clear();

                // Clear command mode and return to Normal mode before executing submitted line
                self.editor.execute(Action::Clear);

                let mut outcome = match mode {
                    crate::kernel::mode::Mode::Command(crate::kernel::mode::CommandKind::SearchForward) => {
                        let (pattern, offset) =
                            crate::kernel::command::search::parse_search_query(&line, '/');
                        crate::kernel::command::search::search(&mut self.editor, &pattern, true, 1, offset)
                    }
                    crate::kernel::mode::Mode::Command(
                        crate::kernel::mode::CommandKind::SearchBackward,
                    ) => {
                        let (pattern, offset) =
                            crate::kernel::command::search::parse_search_query(&line, '?');
                        crate::kernel::command::search::search(&mut self.editor, &pattern, false, 1, offset)
                    }
                    _ => self.execute_line(&line),
                };
                outcome.mode_changed = true;
                if outcome.invalidation == crate::kernel::outcome::RedrawInvalidation::None {
                    outcome.invalidation =
                        crate::kernel::outcome::RedrawInvalidation::CurrentWindow;
                } else {
                    outcome.invalidation = outcome.invalidation.combine(crate::kernel::outcome::RedrawInvalidation::Popup);
                }

                self.process_autocommands(&outcome);
                outcome
            }
            input::RawKey::Escape => {
                self.prompt.clear();
                self.history_index = None;
                self.history_temp.clear();
                // Esc back to Normal mode via Clear
                self.editor.execute(Action::Clear)
            }
        };

        // Command peeking: update search highlights in real-time as the user types
        let mode = self.editor.mode();
        if matches!(
            mode,
            crate::kernel::mode::Mode::Command(crate::kernel::mode::CommandKind::SearchForward)
                | crate::kernel::mode::Mode::Command(
                    crate::kernel::mode::CommandKind::SearchBackward
                )
        ) {
            let delimiter = if matches!(
                mode,
                crate::kernel::mode::Mode::Command(crate::kernel::mode::CommandKind::SearchForward)
            ) {
                '/'
            } else {
                '?'
            };
            let (pattern, _) =
                crate::kernel::command::search::parse_search_query(self.prompt.text(), delimiter);
            self.editor.registers_mut().set(
                crate::kernel::buffer::registers::RegisterName::Search,
                crate::kernel::buffer::registers::Register {
                    text: pattern,
                    kind: crate::kernel::buffer::registers::RegisterKind::Character,
                },
            );
            self.editor.set_peeked_search_range(None);
            self.editor.set_peeked_substitute_text(None);
            outcome.invalidation = crate::kernel::outcome::RedrawInvalidation::CurrentWindow;
        } else if matches!(
            mode,
            crate::kernel::mode::Mode::Command(crate::kernel::mode::CommandKind::Ex)
        ) {
            let mut range_set = false;
            if let Some(cmd) = crate::kernel::command::ex::parse(self.prompt.text()) {
                if cmd.name == "s" || cmd.name == "substitute" {
                    if let Ok(args) =
                        crate::kernel::command::substitute::parse_substitute(&cmd.arguments)
                    {
                        self.editor.registers_mut().set(
                            crate::kernel::buffer::registers::RegisterName::Search,
                            crate::kernel::buffer::registers::Register {
                                text: args.pattern,
                                kind: crate::kernel::buffer::registers::RegisterKind::Character,
                            },
                        );
                        if let Some(r) = cmd.range {
                            self.editor.set_peeked_search_range(Some(r));
                        } else {
                            self.editor.set_peeked_search_range(Some(
                                vim_script::ast::CommandRange {
                                    start: vim_script::ast::Address::Current,
                                    end: None,
                                    separator: None,
                                },
                            ));
                        }
                        if args.replacement_resolved {
                            self.editor
                                .set_peeked_substitute_text(Some(args.replacement));
                        } else {
                            self.editor.set_peeked_substitute_text(None);
                        }
                        range_set = true;
                        outcome.invalidation =
                            crate::kernel::outcome::RedrawInvalidation::CurrentWindow;
                    }
                }
            }
            if !range_set {
                self.editor.set_peeked_search_range(None);
                self.editor.set_peeked_substitute_text(None);
            }
        } else {
            self.editor.set_peeked_search_range(None);
            self.editor.set_peeked_substitute_text(None);
        }

        outcome
    }

    fn handle_submitted_line(&mut self, line: &str) -> Outcome {
        let mode = self.editor.mode();
        match mode {
            crate::kernel::mode::Mode::Command(crate::kernel::mode::CommandKind::SearchForward) => {
                let (pattern, offset) =
                    crate::kernel::command::search::parse_search_query(line, '/');
                crate::kernel::command::search::search(&mut self.editor, &pattern, true, 1, offset)
            }
            crate::kernel::mode::Mode::Command(
                crate::kernel::mode::CommandKind::SearchBackward,
            ) => {
                let (pattern, offset) =
                    crate::kernel::command::search::parse_search_query(line, '?');
                crate::kernel::command::search::search(&mut self.editor, &pattern, false, 1, offset)
            }
            _ => self.execute_line(line),
        }
    }

    pub(crate) fn execute_ex_command(&mut self, command: vim_script::ast::ExCommand) -> Outcome {
        let command = self
            .script
            .canonicalize_command(command.clone())
            .unwrap_or(command);
        if let Some(reg_result) = self.script.try_handle_registration(&command) {
            let _ = reg_result;
            return Outcome::default();
        }

        if command.name == "echo" || command.name == "echomsg" {
            let message = command.arguments.trim().to_string();
            self.pending_request = Some(AppRequest::ShowMessage(message));
            return Outcome::default();
        }

        if command.name == "call" {
            let expr = command.arguments.trim();
            if !expr.is_empty() {
                if let Err(e) = self.script.execute(expr) {
                    self.pending_request = Some(AppRequest::ShowMessage(e));
                }
            }
            return Outcome::default();
        }

        if command.name == "colorscheme" || command.name == "colo" {
            let name = command.arguments.trim();
            if name.is_empty() {
                self.pending_request = Some(AppRequest::ShowMessage(format!(
                    "{}",
                    self.colorscheme.metadata.name
                )));
                return Outcome::default();
            }
            return match vim_ui::ColorScheme::get_by_name(name) {
                Some(colorscheme) => {
                    self.colorscheme = colorscheme;
                    self.editor.buffers_mut().invalidate_all_highlights();
                    self.pending_request = Some(AppRequest::ShowMessage(format!(
                        "colorscheme {}",
                        self.colorscheme.metadata.name
                    )));
                    Outcome {
                        invalidation: crate::kernel::outcome::RedrawInvalidation::All,
                        ..Outcome::default()
                    }
                }
                None => {
                    self.pending_request = Some(AppRequest::ShowMessage(format!(
                        "E185: Cannot find color scheme '{name}'"
                    )));
                    Outcome::default()
                }
            };
        }

        if command.name == "treesitter" || command.name == "tre" {
            let args = command.arguments.trim();
            if args == "on" || args == "enable" || args.is_empty() {
                self.editor.global_options_mut().treesitter = true;
                view_sync::schedule_treesitter_parses(&self.editor, &mut self.services);
                self.pending_request = Some(AppRequest::ShowMessage("Tree-sitter enabled".to_string()));
            } else if args == "off" || args == "disable" {
                self.editor.global_options_mut().treesitter = false;
                self.pending_request = Some(AppRequest::ShowMessage("Tree-sitter disabled".to_string()));
            } else if args == "status" {
                let status = if self.editor.global_options().treesitter { "enabled" } else { "disabled" };
                self.pending_request = Some(AppRequest::ShowMessage(format!("Tree-sitter parsing is {status}")));
            } else {
                self.pending_request = Some(AppRequest::ShowMessage(format!("E475: Invalid argument: {args}")));
            }
            return Outcome {
                invalidation: crate::kernel::outcome::RedrawInvalidation::CurrentWindow,
                ..Outcome::default()
            };
        }

        let expanded = match self.script.expand_user_command(command) {
            Ok(cmd) => cmd,
            Err(_) => return Outcome::default(),
        };

        let ctx = self.editor.current_context();
        let outcome = crate::kernel::command::ex::admit_command(&mut self.editor, ctx, expanded);
        if outcome
            .effects
            .contains(&crate::kernel::outcome::Effect::Quit)
        {
            self.pending_request = Some(AppRequest::Quit);
        }
        for effect in &outcome.effects {
            if let Some(req) = services::describe_effect(effect) {
                self.pending_request = Some(req);
            }
        }
        self.process_autocommands(&outcome);
        outcome
    }

    fn process_autocommands(&mut self, outcome: &Outcome) {
        if outcome.events.is_empty() {
            return;
        }

        let mut autocmds_to_run = Vec::new();
        for event in &outcome.events {
            match event {
                crate::kernel::events::EditorEvent::TextChanged { .. } => {
                    let commands = self.script.fire_event("TextChanged", None);
                    autocmds_to_run.extend(commands);
                }
                crate::kernel::events::EditorEvent::OptionSet { name } => {
                    let commands = self.script.fire_event("OptionSet", Some(name));
                    autocmds_to_run.extend(commands);
                }
            }
        }

        for command in autocmds_to_run {
            self.execute_ex_command(command);
        }
    }

    fn check_abbreviation_expansion(&mut self, _trigger_char: Option<char>) -> Option<Outcome> {
        let mode = self.editor.mode();
        if !matches!(
            mode,
            crate::kernel::mode::Mode::Insert
                | crate::kernel::mode::Mode::Replace
                | crate::kernel::mode::Mode::VirtualReplace
        ) {
            return None;
        }

        let ctx = self.editor.current_context();
        let buffer = self.editor.buffer(ctx.buffer)?;
        let text_buffer = buffer.as_text_buffer();
        let head = self
            .editor
            .window(ctx.window)?
            .selections()
            .primary()
            .head();
        let offset = text_buffer.offset_for_anchor(&head);

        use text::ToPoint;
        let point = offset.to_point(text_buffer);
        let row_text = BufferText::row_text(text_buffer, point.row);
        if (point.column as usize) > row_text.len() {
            return None;
        }
        let left_text = &row_text[..point.column as usize];

        if let Some(abbr) = self
            .script
            .lookup_abbreviation(left_text, crate::script::AbbreviationMode::Insert)
        {
            let del_outcome = self.editor.execute(Action::DeleteCharBefore {
                count: abbr.lhs.chars().count() as u32,
            });
            let _ins_outcome = self.editor.execute(Action::InsertText(abbr.rhs.clone()));
            let mut combined = del_outcome;
            combined.invalidation = crate::kernel::outcome::RedrawInvalidation::CurrentWindow;
            return Some(combined);
        }
        None
    }

    pub fn take_request(&mut self) -> Option<AppRequest> {
        let _ = self.dispatch_script_requests();
        self.pending_request.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::input::InputTranslator;
    use crossterm::event::{Event, KeyCode as CKey, KeyEvent, KeyModifiers as CMod};
    use text::Point;

    fn key_event(ch: char) -> Event {
        Event::Key(KeyEvent::new(CKey::Char(ch), CMod::NONE))
    }

    fn cursor(app: &App) -> Point {
        let head = app.editor().current_window().selections().primary().head();
        app.editor()
            .current_buffer()
            .as_text_buffer()
            .summary_for_anchor(&head)
    }

    fn text_of(app: &App) -> String {
        app.editor().current_buffer().snapshot().chunks().collect()
    }

    /// End-to-end: exactly the `InputTranslator -> App::handle_action` wiring
    /// `runtime::run` uses, with no terminal involved.
    #[test]
    fn real_key_events_move_the_cursor_through_the_full_app_pipeline() {
        let mut app = App::new("ab\ncd\n");
        let mut input = InputTranslator::new();
        assert_eq!(cursor(&app), Point::new(0, 0));

        let resolved = input.translate(key_event('l')).expect("l resolves");
        app.handle_action(resolved.action, resolved.register);
        assert_eq!(cursor(&app), Point::new(0, 1));

        let resolved = input.translate(key_event('j')).expect("j resolves");
        app.handle_action(resolved.action, resolved.register);
        assert_eq!(cursor(&app), Point::new(1, 1));

        let resolved = input.translate(key_event('h')).expect("h resolves");
        app.handle_action(resolved.action, resolved.register);
        assert_eq!(cursor(&app), Point::new(1, 0));

        let resolved = input.translate(key_event('k')).expect("k resolves");
        app.handle_action(resolved.action, resolved.register);
        assert_eq!(cursor(&app), Point::new(0, 0));
    }

    /// Regression test for a mode desync bug: `Esc` in Insert mode resolves
    /// to `Action::Clear`, not `Action::SetToNormal`
    /// (`vim_input::Keymap::vim_defaults`'s `insert_actions` table). If the
    /// kernel only treats `SetToNormal` as "leave Insert", `vim_input::
    /// Resolver`'s own mode flips back to Normal (so it starts decoding
    /// keys as Normal-mode commands again) while `kernel::Mode` stays stuck
    /// on `Insert`, silently dropping every motion afterwards.
    #[test]
    fn esc_via_real_key_event_leaves_insert_mode_and_motions_resume() {
        use crate::kernel::mode::Mode;

        let mut app = App::new("ab\ncd\n");
        let mut input = InputTranslator::new();

        let resolved = input.translate(key_event('i')).expect("i resolves");
        app.handle_action(resolved.action, resolved.register);
        assert_eq!(app.editor().mode(), Mode::Insert);

        let resolved = input
            .translate(key_event('X'))
            .expect("typed char resolves");
        app.handle_action(resolved.action, resolved.register);

        let esc = Event::Key(KeyEvent::new(CKey::Esc, CMod::NONE));
        let resolved = input.translate(esc).expect("Esc resolves");
        assert_eq!(resolved.action, Action::Clear);
        app.handle_action(resolved.action, resolved.register);
        assert_eq!(
            app.editor().mode(),
            Mode::Normal,
            "kernel mode must leave Insert on Action::Clear, matching the \
             resolver's own mode transition"
        );

        let before = cursor(&app);
        let resolved = input.translate(key_event('l')).expect("l resolves");
        app.handle_action(resolved.action, resolved.register);
        assert_ne!(
            cursor(&app),
            before,
            "motions must work again once back in Normal mode"
        );
    }

    #[test]
    fn visual_block_delete_exits_to_normal_mode_and_allows_inserting_text() {
        use crate::kernel::mode::Mode;
        let mut app = App::new("hello\nworld\n");
        let mut input = InputTranslator::new();

        // Enter Visual Block mode: Ctrl-v, j, l
        input.sync_mode(app.editor().mode());
        let ctrl_v = Event::Key(KeyEvent::new(CKey::Char('v'), CMod::CONTROL));
        let resolved = input.translate(ctrl_v).expect("Ctrl-v resolves");
        app.handle_action(resolved.action, resolved.register);
        input.sync_mode(app.editor().mode());
        assert!(app.editor().mode().is_visual());

        let resolved = input.translate(key_event('j')).expect("j resolves");
        app.handle_action(resolved.action, resolved.register);
        input.sync_mode(app.editor().mode());

        let resolved = input.translate(key_event('x')).expect("x resolves");
        app.handle_action(resolved.action, resolved.register);
        input.sync_mode(app.editor().mode());

        // Deleting visual block must land in Normal mode
        assert_eq!(app.editor().mode(), Mode::Normal);
        assert_eq!(input.resolver.mode(), vim_input::Mode::Normal);

        // Press 'i' to enter insert mode
        let resolved = input.translate(key_event('i')).expect("i resolves");
        app.handle_action(resolved.action, resolved.register);
        input.sync_mode(app.editor().mode());
        assert_eq!(app.editor().mode(), Mode::Insert);
        assert_eq!(input.resolver.mode(), vim_input::Mode::Insert);

        // Type 'a'
        let resolved = input.translate(key_event('a')).expect("a resolves");
        assert_eq!(resolved.action, Action::InsertText("a".to_string()));
        app.handle_action(resolved.action, resolved.register);
        input.sync_mode(app.editor().mode());

        assert_eq!(text_of(&app), "aello\naorld\n");
    }

    #[test]
    fn visual_search_populates_prompt_query() {
        use crate::kernel::mode::{CommandKind, Mode};
        let mut app = App::new("hello world\n");
        let mut input = InputTranslator::new();

        // Enter Visual mode: 'v', 'w'
        input.sync_mode(app.editor().mode());
        let resolved = input.translate(key_event('v')).expect("v resolves");
        app.handle_action(resolved.action, resolved.register);
        input.sync_mode(app.editor().mode());

        let resolved = input.translate(key_event('e')).expect("e resolves");
        app.handle_action(resolved.action, resolved.register);
        input.sync_mode(app.editor().mode());

        // Press '/' to search
        let resolved = input.translate(key_event('/')).expect("/ resolves");
        app.handle_action(resolved.action, resolved.register);
        input.sync_mode(app.editor().mode());

        assert_eq!(app.editor().mode(), Mode::Command(CommandKind::SearchForward));
        assert_eq!(app.prompt().text(), "\\<hello\\>");
    }

    #[test]
    fn visual_command_populates_range_prompt() {
        use crate::kernel::mode::{CommandKind, Mode};
        let mut app = App::new("hello world\n");
        let mut input = InputTranslator::new();

        // Enter Visual mode: 'v'
        input.sync_mode(app.editor().mode());
        let resolved = input.translate(key_event('v')).expect("v resolves");
        app.handle_action(resolved.action, resolved.register);
        input.sync_mode(app.editor().mode());

        // Press ':' to enter command mode
        let resolved = input.translate(key_event(':')).expect(": resolves");
        app.handle_action(resolved.action, resolved.register);
        input.sync_mode(app.editor().mode());

        assert_eq!(app.editor().mode(), Mode::Command(CommandKind::Ex));
        assert_eq!(app.prompt().text(), "'<,'>");
    }


    #[test]
    fn visual_block_insert_multiple_chars_inserts_sequentially_on_all_cursors() {
        use crate::kernel::mode::Mode;
        let mut app = App::new("hello\nworld\n");
        let mut input = InputTranslator::new();

        // Enter Visual Block mode: Ctrl-v, j
        input.sync_mode(app.editor().mode());
        let ctrl_v = Event::Key(KeyEvent::new(CKey::Char('v'), CMod::CONTROL));
        let resolved = input.translate(ctrl_v).expect("Ctrl-v resolves");
        app.handle_action(resolved.action, resolved.register);
        input.sync_mode(app.editor().mode());

        let resolved = input.translate(key_event('j')).expect("j resolves");
        app.handle_action(resolved.action, resolved.register);
        input.sync_mode(app.editor().mode());

        // Press 'I' to enter insert at start of block
        let resolved = input.translate(key_event('I')).expect("I resolves");
        app.handle_action(resolved.action, resolved.register);
        input.sync_mode(app.editor().mode());
        assert_eq!(app.editor().mode(), Mode::Insert);
        println!("SELECTIONS AFTER I: {:?}", app.editor().window(app.editor().current_context().window).unwrap().selections().selections());

        // Type 'x' and 'y'
        let resolved = input.translate(key_event('x')).expect("x resolves");
        app.handle_action(resolved.action, resolved.register);
        input.sync_mode(app.editor().mode());
        let resolved = input.translate(key_event('y')).expect("y resolves");
        app.handle_action(resolved.action, resolved.register);

        println!("RESULT TEXT:\n{:?}", text_of(&app));
        assert_eq!(text_of(&app), "xyhello\nxyworld\n");
    }

    fn submit_line(app: &mut App, line: &str) {
        for ch in line.chars() {
            app.handle_raw_key(input::RawKey::Char(ch));
        }
        app.handle_raw_key(input::RawKey::Enter);
    }

    fn temporary_script(contents: &str) -> std::path::PathBuf {
        use std::time::{SystemTime, UNIX_EPOCH};

        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("nxvim-source-{}-{nonce}.vim", std::process::id()));
        std::fs::write(&path, contents).unwrap();
        path
    }

    #[test]
    fn source_command_executes_a_script_file() {
        let path = temporary_script("delete");
        let mut app = App::new("line1\nline2");

        let outcome = app.execute_line(&format!("source {}", path.display()));

        let text: String = app.editor().current_buffer().snapshot().chunks().collect();
        assert_eq!(text, "line2");
        assert!(outcome.mutated);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn feedkeys_simulates_keystrokes_synchronously() {
        let mut app = App::new("hello\nworld");
        app.execute_line("feedkeys('iTesting\\<Esc>', 'xt')");
        assert_eq!(text_of(&app), "Testinghello\nworld");
    }

    #[test]
    fn source_command_reports_a_missing_file() {
        let path =
            std::env::temp_dir().join(format!("nxvim-missing-source-{}.vim", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let mut app = App::new("text");

        app.execute_line(&format!("source {}", path.display()));

        assert_eq!(
            app.take_request(),
            Some(AppRequest::ShowMessage(format!(
                "E484: Can't open file {}",
                path.display()
            )))
        );
    }

    #[test]
    fn command_line_alias_is_canonicalized_before_kernel_admission() {
        let mut app = App::new("line1\nline2");

        submit_line(&mut app, "d");

        let text: String = app.editor().current_buffer().snapshot().chunks().collect();
        assert_eq!(text, "line2");
    }

    #[test]
    fn command_line_resolves_registered_simple_command_and_executes_it() {
        let mut app = App::new("charlie\nalice\nbob");

        submit_line(&mut app, "sort");

        let text: String = app.editor().current_buffer().snapshot().chunks().collect();
        assert_eq!(text, "alice\nbob\ncharlie");
    }

    #[test]
    fn script_channel_dispatches_ex_commands_to_the_kernel() {
        let mut app = App::new("line1\nline2");

        let outcome = app.execute_line("delete");

        let text: String = app.editor().current_buffer().snapshot().chunks().collect();
        assert_eq!(text, "line2");
        assert!(outcome.mutated);
    }

    #[test]
    fn script_channel_retains_terminal_requests_without_requeueing() {
        let mut app = App::new("text");

        app.execute_line("quit");

        assert_eq!(app.take_request(), Some(AppRequest::Quit));
        assert_eq!(app.take_request(), None);
    }

    #[test]
    fn mapping_smoke_test() {
        let mut app = App::new("hello world");
        let mut input = InputTranslator::with_mappings(app.shared_keymaps());

        submit_line(&mut app, "nnoremap x dw");

        let resolved = input.translate(key_event('x')).expect("x should resolve");
        assert_eq!(
            resolved.action,
            Action::DeleteMotion {
                count: 1,
                motion: Box::new(Action::MoveToWord {
                    count: 1,
                    select: false
                })
            }
        );

        app.handle_action(resolved.action, resolved.register);
        let text: String = app.editor().current_buffer().snapshot().chunks().collect();
        assert_eq!(text, "world");
    }

    #[test]
    fn user_command_smoke_test() {
        let mut app = App::new("line1\nline2\nline3");

        submit_line(&mut app, "command Del d");
        submit_line(&mut app, "Del");

        let text: String = app.editor().current_buffer().snapshot().chunks().collect();
        assert_eq!(text, "line2\nline3");
    }

    #[test]
    fn autocommand_smoke_test() {
        let mut app = App::new("word1 word2");

        submit_line(&mut app, "autocmd TextChanged * q!");

        let resolved = Action::DeleteMotion {
            count: 1,
            motion: Box::new(Action::MoveToWord {
                count: 1,
                select: false,
            }),
        };
        app.handle_action(resolved, None);

        assert_eq!(app.take_request(), Some(AppRequest::Quit));
    }

    #[test]
    fn colorscheme_command_replaces_app_theme_and_invalidates_highlights() {
        let mut app = App::new("fn main() {}\n");
        let buffer = app.editor.current_context().buffer;
        app.editor
            .buffers_mut()
            .analysis_mut(buffer)
            .unwrap()
            .highlights_mut()
            .rows
            .insert(0, Vec::new());
        let command = crate::kernel::command::ex::parse("colorscheme dracula").unwrap();

        let outcome = app.execute_ex_command(command);

        assert_eq!(
            outcome.invalidation,
            crate::kernel::outcome::RedrawInvalidation::All
        );
        assert!(
            app.colorscheme
                .metadata
                .name
                .to_lowercase()
                .contains("dracula")
        );
        assert!(
            app.editor
                .buffers_mut()
                .analysis(buffer)
                .unwrap()
                .highlights()
                .rows
                .is_empty()
        );
        assert!(matches!(
            app.take_request(),
            Some(AppRequest::ShowMessage(_))
        ));
    }

    #[test]
    fn unknown_colorscheme_preserves_current_theme() {
        let mut app = App::new("text");
        let original = app.colorscheme.clone();
        let command = crate::kernel::command::ex::parse("colo definitely-missing").unwrap();

        let outcome = app.execute_ex_command(command);

        assert_eq!(
            outcome.invalidation,
            crate::kernel::outcome::RedrawInvalidation::None
        );
        assert_eq!(app.colorscheme, original);
        assert_eq!(
            app.take_request(),
            Some(AppRequest::ShowMessage(
                "E185: Cannot find color scheme 'definitely-missing'".to_string()
            ))
        );
    }

    #[test]
    fn echo_smoke_test() {
        let mut app = App::new("line1");

        submit_line(&mut app, "echo \"hello\"");

        assert_eq!(
            app.take_request(),
            Some(AppRequest::ShowMessage("hello".to_string()))
        );

        let text: String = app.editor().current_buffer().snapshot().chunks().collect();
        assert_eq!(text, "line1");
    }

    #[test]
    fn mode_eval_execute_builtins_test() {
        use vim_script::runtime::Value;

        let mut app = App::new("hello world");

        // Test eval() & mode() in VimScript
        app.execute_script("call assert_equal('n', mode())");
        app.execute_script("call assert_equal(10, eval('5 + 5'))");

        let errs = app.script_mut().globals.get("v:errors").cloned();
        assert_eq!(errs, Some(Value::List(vec![])));

        // Test execute() in VimScript
        app.execute_script("call execute('echo \"test execute\"')");
        let errs = app.script_mut().globals.get("v:errors").cloned();
        assert_eq!(errs, Some(Value::List(vec![])));
    }

    #[test]
    fn buffer_script_builtins_test() {
        use vim_script::runtime::Value;

        let mut app = App::new("line 1\nline 2\nline 3");

        // bufnr, bufexists, bufname, getbufinfo
        app.execute_script("let g:nr = bufnr('%')");
        app.execute_script("let g:ex = bufexists(g:nr)");
        app.execute_script("let g:name = bufname(g:nr)");
        app.execute_script("let g:info = getbufinfo(g:nr)");

        assert_eq!(app.script_mut().globals.get("g:ex"), Some(&Value::Integer(1)));

        // getbufline
        app.execute_script("let g:lines = getbufline(g:nr, 1, '$')");
        let lines = app.script_mut().globals.get("g:lines").cloned();
        assert_eq!(
            lines,
            Some(Value::List(vec![
                Value::String(std::sync::Arc::from("line 1")),
                Value::String(std::sync::Arc::from("line 2")),
                Value::String(std::sync::Arc::from("line 3")),
            ]))
        );

        // setbufline
        app.execute_script("let g:set_res = setbufline(g:nr, 2, 'replaced line 2')");
        println!("g:nr = {:?}", app.script_mut().globals.get("g:nr"));
        println!("g:set_res = {:?}", app.script_mut().globals.get("g:set_res"));
        app.execute_script("let g:lines_after_set = getbufline(g:nr, 1, '$')");
        let lines_after_set = app.script_mut().globals.get("g:lines_after_set").cloned();
        assert_eq!(
            lines_after_set,
            Some(Value::List(vec![
                Value::String(std::sync::Arc::from("line 1")),
                Value::String(std::sync::Arc::from("replaced line 2")),
                Value::String(std::sync::Arc::from("line 3")),
            ]))
        );

        // append
        app.execute_script("let g:app_res = append(3, 'appended line 4')");
        println!("g:app_res = {:?}", app.script_mut().globals.get("g:app_res"));
        app.execute_script("let g:lines_after_append = getbufline(g:nr, 1, '$')");
        let lines_after_append = app.script_mut().globals.get("g:lines_after_append").cloned();
        println!("lines_after_append = {:?}", lines_after_append);
        assert_eq!(
            lines_after_append,
            Some(Value::List(vec![
                Value::String(std::sync::Arc::from("line 1")),
                Value::String(std::sync::Arc::from("replaced line 2")),
                Value::String(std::sync::Arc::from("line 3")),
                Value::String(std::sync::Arc::from("appended line 4")),
            ]))
        );

        // deletebufline
        app.execute_script("let g:del_res = deletebufline(g:nr, 2, 3)");
        println!("g:del_res = {:?}", app.script_mut().globals.get("g:del_res"));
        app.execute_script("let g:lines_after_delete = getbufline(g:nr, 1, '$')");
        let lines_after_delete = app.script_mut().globals.get("g:lines_after_delete").cloned();
        assert_eq!(
            lines_after_delete,
            Some(Value::List(vec![
                Value::String(std::sync::Arc::from("line 1")),
                Value::String(std::sync::Arc::from("appended line 4")),
            ]))
        );
    }

    #[test]
    fn abbreviations_smoke_test() {
        let mut app = App::new("");
        submit_line(&mut app, "iabbrev ad advertisement");

        let mut input = InputTranslator::new();
        let resolved = input.translate(key_event('i')).expect("i resolves");
        app.handle_action(resolved.action, resolved.register);

        // Type 'a', 'd', ' '
        app.handle_action(Action::InsertText("a".to_string()), None);
        app.handle_action(Action::InsertText("d".to_string()), None);
        app.handle_action(Action::InsertText(" ".to_string()), None);

        let text: String = app.editor().current_buffer().snapshot().chunks().collect();
        assert_eq!(text, "advertisement ");
    }

    #[test]
    fn digraphs_smoke_test() {
        let mut app = App::new("");
        let mut input = InputTranslator::with_mappings(app.shared_keymaps());
        app.editor.set_mode(crate::kernel::mode::Mode::Insert);
        input.resolver = vim_input::Resolver::new(vim_input::Mode::Insert);

        // Feed Ctrl-K, 0, 0
        let r1 = input.translate(Event::Key(KeyEvent::new(CKey::Char('k'), CMod::CONTROL)));
        assert!(r1.is_none());
        let r2 = input.translate(Event::Key(KeyEvent::new(CKey::Char('0'), CMod::NONE)));
        assert!(r2.is_none());
        let r3 = input.translate(Event::Key(KeyEvent::new(CKey::Char('0'), CMod::NONE)));
        assert!(r3.is_some());

        let resolved = r3.unwrap();
        assert_eq!(resolved.action, Action::InsertText("∞".to_string()));
    }

    #[test]
    fn command_line_history_is_separate_and_restores_draft() {
        let mut app = App::new("");
        use crate::kernel::mode::{CommandKind, Mode};

        app.record_history(Mode::Command(CommandKind::Ex), "write");
        app.record_history(Mode::Command(CommandKind::Ex), "write");
        app.record_history(Mode::Command(CommandKind::SearchForward), "needle");
        assert_eq!(app.command_history, ["write"]);
        assert_eq!(app.search_history, ["needle"]);

        app.editor.set_mode(Mode::Command(CommandKind::Ex));
        app.prompt.set_text("draft".into());
        app.history_previous();
        assert_eq!(app.prompt.text(), "write");
        app.history_next();
        assert_eq!(app.prompt.text(), "draft");

        app.editor
            .set_mode(Mode::Command(CommandKind::SearchBackward));
        app.history_previous();
        assert_eq!(app.prompt.text(), "needle");
    }

    #[test]
    fn app_boundary_owns_background_save_submission_and_completion() {
        let path = std::env::temp_dir().join(format!(
            "nxvim-app-background-save-{}-{}",
            std::process::id(),
            rand::random::<u64>()
        ));
        let mut app = App::new("saved asynchronously");
        app.save_current_buffer_in_background(Some(path.clone()))
            .unwrap();
        let mut render_state = crate::view::RenderState::new();

        for _ in 0..100 {
            let outcome = app.poll_services(&mut render_state);
            if outcome.effects.iter().any(|effect| {
                matches!(
                    effect,
                    crate::kernel::outcome::Effect::FileSaved { path: saved, .. } if saved == &path
                )
            }) {
                assert_eq!(
                    std::fs::read_to_string(&path).unwrap(),
                    "saved asynchronously\n"
                );
                let _ = std::fs::remove_file(path);
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        panic!("background save did not complete");
    }

    #[test]
    fn mapping_recursion_limit_smoke_test() {
        let mut app = App::new("");
        let mut input = InputTranslator::with_mappings(app.shared_keymaps());
        submit_line(&mut app, "map a b");
        submit_line(&mut app, "map b a");

        // This would trigger an infinite loop if no limit exists
        let resolved = input.translate(key_event('a'));
        assert!(resolved.is_some());
        assert_eq!(resolved.unwrap().action, Action::NoOp);
    }

    #[test]
    fn custom_script_filter_invokes_function_updates_text_and_closes() {
        let mut app = App::new("initial content");
        let _ = app.script.execute("function! TestFilter(id, key)\ncall popup_settext(a:id, ['Key: ' . a:key])\nreturn popup_filter_yesno(a:id, a:key)\nendfunction");
        let _ = app.script.execute("let g:pop_id = popup_create(['Prompt'], { 'filter': 'TestFilter' })");
        app.dispatch_script_requests();

        assert!(app.editor().find_active_filter_popup().is_some());

        // Press 'y'
        let _ = app.handle_action(Action::InsertText("y".to_string()), None);

        // Popup should be closed by popup_filter_yesno inside TestFilter
        assert!(app.editor().find_active_filter_popup().is_none());
        // Buffer below should remain unmutated ("initial content")
        let buf_id = app.editor().current_context().buffer;
        use vim_buffer::BufferText;
        let text = app.editor().buffer(buf_id).unwrap().as_text_buffer().row_text(0).to_string();
        assert_eq!(text, "initial content");
    }

    #[test]
    fn call_command_executes_function() {
        let mut app = App::new("test");
        app.execute_script("function! Hello()\necho 'hello'\nendfunction");
        app.execute_script("call Hello()");
        assert_eq!(app.pending_request, Some(AppRequest::ShowMessage("hello".to_string())));
    }

    #[test]
    fn popup_input_filter_key_routing_test() {
        let mut app = App::new("initial buffer content");
        let script = "let g:pop_id = popup_create(['Confirm?', '(y/n)'], { 'line': 5, 'col': 15, 'filter': 'popup_filter_yesno' })";

        app.execute_script(&script);

        // Verify popup was created and is active with a filter
        assert!(app.editor().find_active_filter_popup().is_some());
        let popup_id = app.editor().find_active_filter_popup().unwrap().1;

        // Key 'y' routed to popup filter -> popup closes without mutating document
        let outcome = app.handle_action(Action::InsertText("y".to_string()), None);
        assert_eq!(outcome.invalidation, RedrawInvalidation::Popup);

        // Popup should now be closed and removed
        assert!(app.editor().find_active_filter_popup().is_none());
        assert!(app.editor().global_popups().get(popup_id).is_none());

        // Underlying buffer text remains unaffected
        assert_eq!(text_of(&app), "initial buffer content");
    }
}
