use crate::app::App;
use crate::app::services::{indexer, treesitter};
use crate::app::windows::WindowOps;
use crate::controller::{Command, CommandOutcome, Dispatcher, ViewEffect};
use crate::terminal::TerminalSession;
use crate::view::{
    CommandLineView, LayoutSnapshot, RenderGlobals, StatusLineView, TabLineView, TextView,
    WindowLayout, globals::buffer_display_name,
};
use crossterm::event;
use std::io::{Write, stdout};
use text::ToPoint;
use vim_ui::{BufferedRenderer, Window};

// Owns terminal lifecycle, source polling, command dispatch, and rendering.
fn prompt_choice(event: &event::Event) -> Option<crate::controller::PromptChoice> {
    use crate::controller::PromptChoice as Choice;
    use event::{KeyCode, KeyEventKind, KeyModifiers};

    let event::Event::Key(key) = event else {
        return None;
    };
    if key.kind == KeyEventKind::Release {
        return None;
    }
    match key.code {
        KeyCode::Char('y' | 'Y') => Some(Choice::Yes),
        KeyCode::Char('n' | 'N') => Some(Choice::No),
        KeyCode::Char('a' | 'A') => Some(Choice::All),
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => Some(Choice::Quit),
        KeyCode::Char('q' | 'Q') | KeyCode::Esc => Some(Choice::Quit),
        KeyCode::Char('l' | 'L') => Some(Choice::Last),
        _ => None,
    }
}

pub struct Runtime {
    terminal: TerminalSession,
    app: App,
    buffered_renderer: BufferedRenderer,
    script: crate::script::ScriptRuntime,
}

impl Runtime {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let terminal = TerminalSession::enter()?;
        let rect = terminal.size().unwrap_or(vim_ui::Rect::new(0, 0, 80, 24));

        let args = crate::app::args::Args::parse();
        let pre_config_cmds = args.pre_config_cmds.clone();
        let post_config_cmds = args.post_config_cmds.clone();
        let scripts = args.scripts.clone();

        let mut app = App::new(rect, args);
        let mut script = crate::script::ScriptRuntime::new();
        app.init(&mut script, pre_config_cmds, post_config_cmds, scripts);

        Ok(Self {
            terminal,
            app,
            buffered_renderer: BufferedRenderer::new(rect.width, rect.height),
            script,
        })
    }

    fn update_script_state(&self) -> Result<(), String> {
        let focused_window = self.app.ui.focused_window_id();
        let current_buffer = WindowOps::window_buffer(&self.app.ui, focused_window)
            .filter(|id| *id != self.app.model.commandline_buffer())
            .or_else(|| WindowOps::window_buffer(&self.app.ui, self.app.view_ids.main))
            .unwrap_or_else(|| self.app.model.buffers().current());

        self.script.update_state(&self.app.model, current_buffer)
    }

    pub fn run(mut self) -> Result<(), Box<dyn std::error::Error>> {
        let mut out = stdout();
        let rect = self.app.ui.screen_rect();
        self.schedule_state_updates(None);

        let mut should_redraw = true;
        let mut last_command_time = std::time::Instant::now();
        let mut is_idle = false;
        let mut idle_since: Option<std::time::Instant> = None;

        'main_loop: loop {
            let current_rect = self.app.ui.screen_rect();
            if let Ok(new_rect) = self.terminal.size() {
                if new_rect != current_rect {
                    self.resize(new_rect);
                    should_redraw = true;
                }
            }

            let mut commands = Vec::new();

            while let Some(cmd) = self.app.command_queue.pop_front() {
                commands.push(cmd);
            }

            if commands.is_empty() && self.app.services.poll() {
                commands.extend(
                    self.app
                        .services
                        .drain_results()
                        .into_iter()
                        .map(Command::Task),
                );
            }

            commands.extend(std::iter::from_fn(|| self.script.try_next_command()));

            if commands.is_empty() {
                if event::poll(std::time::Duration::from_millis(50))? {
                    let terminal_event = event::read()?;
                    if let event::Event::Resize(width, height) = terminal_event {
                        self.resize(vim_ui::Rect::new(0, 0, width, height));
                        should_redraw = true;
                    } else if self.app.prompt.is_some() {
                        if let Some(choice) = prompt_choice(&terminal_event) {
                            if let Some(handler) =
                                self.app.prompt.as_ref().map(|prompt| prompt.handler)
                            {
                                commands.push(Command::PromptChoice { handler, choice });
                            }
                        }
                    } else if let Some(command) = self.app.controller.feed_event(terminal_event) {
                        commands.push(command);
                    }
                }
            }

            if commands.is_empty() {
                self.schedule_state_updates(
                    idle_since.map(|since: std::time::Instant| since.elapsed()),
                );
            }

            if !commands.is_empty() {
                last_command_time = std::time::Instant::now();
                if is_idle {
                    is_idle = false;
                    idle_since = None;
                    if self.app.model.status.as_deref() == Some("idle") {
                        self.app.model.status = None;
                        should_redraw = true;
                    }
                }
            } else if !is_idle && last_command_time.elapsed() >= std::time::Duration::from_secs(2) {
                is_idle = true;
                idle_since = Some(std::time::Instant::now());
                self.app.model.status = Some("idle".to_string());
                should_redraw = true;
            }

            let processed_any = !commands.is_empty();
            for command in commands {
                if let Command::ExecuteScript(ref script_str) = command {
                    let _ = self.update_script_state();
                    if let Err(err) = self.script.execute(script_str) {
                        self.app.model.status = Some(err);
                    }
                    should_redraw = true;
                    continue;
                }
                let outcome = Dispatcher::dispatch(&mut self.app, command);
                should_redraw |= outcome.redraw;
                self.apply_outcome(&outcome);
                if outcome.quit {
                    break 'main_loop;
                }
            }
            if processed_any {
                let _ = self.update_script_state();
            }

            if should_redraw {
                let active_rect = self.app.ui.screen_rect();
                self.redraw(active_rect, &mut out)?;
                should_redraw = false;
            }
        }

        while self.app.services.has_pending_saves() {
            if self.app.services.poll() {
                for task_res in self.app.services.drain_results() {
                    let _ = Dispatcher::dispatch(&mut self.app, Command::Task(task_res));
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }

        self.terminal.restore()?;

        Ok(())
    }

    fn apply_outcome(&mut self, outcome: &CommandOutcome) {
        for effect in &outcome.view_effects {
            self.apply_view_effect(*effect);
        }
    }

    fn apply_view_effect(&mut self, effect: ViewEffect) {
        crate::app::ui::ViewSynchronizer::apply(
            &mut self.app.ui,
            &mut self.app.model,
            self.app.view_ids,
            effect,
        );
    }

    /// Speculative idle prefetch grows the highlighted margin around the
    /// viewport gradually across repeated idle ticks instead of requesting
    /// the full margin in one shot. `highlight_run` parses any newly
    /// requested, not-yet-cached rows synchronously, so a large one-shot
    /// margin (previously up to 1000 rows before + 500 after) could stall
    /// the main loop for a visible amount of time. Ramping the margin by a
    /// bounded step every ~50ms tick keeps each synchronous parse small
    /// while still reaching full prefetch coverage within about half a
    /// second of going idle.
    fn schedule_state_updates(&mut self, idle_elapsed: Option<std::time::Duration>) {
        const IDLE_EXPAND_STEP_BEFORE: u32 = 100;
        const IDLE_EXPAND_STEP_AFTER: u32 = 50;
        const IDLE_EXPAND_MAX_BEFORE: u32 = 1000;
        const IDLE_EXPAND_MAX_AFTER: u32 = 500;

        let (expand_before, expand_after) = match idle_elapsed {
            Some(elapsed) => {
                let ticks = elapsed.as_millis() as u32 / 50;
                (
                    ticks
                        .saturating_mul(IDLE_EXPAND_STEP_BEFORE)
                        .min(IDLE_EXPAND_MAX_BEFORE),
                    ticks
                        .saturating_mul(IDLE_EXPAND_STEP_AFTER)
                        .min(IDLE_EXPAND_MAX_AFTER),
                )
            }
            None => (0, 0),
        };

        let window_ids: Vec<_> = WindowOps::window_buffers(&self.app.ui)
            .into_iter()
            .map(|(window_id, _)| window_id)
            .collect();

        for window_id in window_ids {
            self.schedule_window_display_map(window_id);
            self.schedule_window_highlight(window_id, expand_before, expand_after);
            self.schedule_window_treesitter(window_id);
            self.schedule_window_indexer(window_id);
        }
    }

    fn schedule_window_display_map(&mut self, window_id: vim_ui::WindowId) -> Option<()> {
        const CHUNK_ROWS: u32 = 4_096;

        let buffer_id = WindowOps::window_buffer(&self.app.ui, window_id)?;
        let revision = self.app.model.buffer_state_mut(buffer_id)?.revision;
        let buffer = self.app.model.get_buffer(buffer_id).ok()?;
        let snapshot = buffer.snapshot().as_inner().clone();
        let window = self
            .app
            .ui
            .window(window_id)
            .and_then(Window::window_state)?;

        if window.pending_display_map.is_some() {
            return None;
        }

        let cursor_row = if window.selections.selections.is_empty() {
            0
        } else {
            window.selections.primary().head().to_point(&snapshot).row
        };

        let requested_rows = window
            .display_map
            .nearest_missing_range(cursor_row, CHUNK_ROWS)?;
        let input = window.display_map.expansion_input(requested_rows.clone())?;
        let generation = input.generation.clone();
        let sequence = window.sequence.clone();
        let owner = crate::app::services::TaskOwner {
            buffer_id: Some(buffer_id),
            window_id: Some(window_id),
            revision,
        };

        let task = self.app.services.spawn_cancellable_task(
            "display_map",
            sequence,
            owner,
            crate::app::services::TaskType::DisplayMap,
            move |token| display_map::build_expansion(input, &token),
        );

        if task.is_some() {
            let window_mut = self
                .app
                .ui
                .window_mut(window_id)
                .and_then(Window::window_state_mut)?;
            window_mut.pending_display_map = Some((generation, requested_rows));
        }

        Some(())
    }

    fn schedule_window_highlight(
        &mut self,
        window_id: vim_ui::WindowId,
        expand_before: u32,
        expand_after: u32,
    ) -> Option<()> {
        if !self.app.syntax_highlight {
            return Some(());
        }
        let buffer_id = WindowOps::window_buffer(&self.app.ui, window_id)?;
        let buffer = self.app.model.get_buffer(buffer_id).ok()?;
        let snapshot = buffer.snapshot().as_inner().clone();
        let file_path = buffer.path().and_then(|p| p.to_str()).map(str::to_owned);
        let window = self
            .app
            .ui
            .window(window_id)
            .and_then(Window::window_state)?;

        let display_map_snapshot = window.display_map.snapshot();
        let scroll_y = display_map_snapshot.scroll_y;
        let viewport_height = window.viewport.height as u32;
        let start_row = display_map_snapshot
            .try_buffer_row_for_display_row(scroll_y)
            .unwrap_or(0);
        let end_row = display_map_snapshot
            .try_buffer_row_for_display_row(
                (scroll_y + viewport_height).min(display_map_snapshot.row_count()),
            )
            .unwrap_or_else(|| display_map_snapshot.buffer_snapshot().max_point().row);

        let colorscheme = self.app.colorscheme.as_ref();
        let fallback_colorscheme;
        let cs_ref = match colorscheme {
            Some(cs) => cs,
            None => {
                fallback_colorscheme = vim_colorscheme::ColorScheme::load_default();
                &fallback_colorscheme
            }
        };

        let highlights = &mut self.app.model.buffer_state_mut(buffer_id)?.highlights;
        textmate::highlight_run(
            highlights,
            &snapshot,
            file_path.as_deref(),
            start_row,
            end_row,
            expand_before,
            expand_after,
            self.app.highlighter.as_ref(),
            cs_ref,
        );

        Some(())
    }

    fn schedule_window_treesitter(&mut self, window_id: vim_ui::WindowId) -> Option<()> {
        if !self.app.treesitter_enabled {
            return Some(());
        }
        let buffer_id = WindowOps::window_buffer(&self.app.ui, window_id)?;
        let revision = self.app.model.buffer_state_mut(buffer_id)?.revision;
        let buffer = self.app.model.get_buffer(buffer_id).ok()?;
        let path = buffer.path()?.to_str()?;
        let grammar = treesitter::Grammar::from_path(path)?;
        let changedtick = buffer.changedtick();

        if !self.app.services.treesitter.is_parsing(buffer_id)
            && self
                .app
                .services
                .treesitter
                .syntax_tree(buffer_id)
                .is_none()
        {
            if let Some(state) = self.app.model.buffer_state(buffer_id) {
                if let Ok(syntax_tree) = &state.treesitter {
                    if syntax_tree.grammar() == grammar {
                        self.app.services.treesitter.initialize_from_parsed(
                            buffer_id,
                            changedtick,
                            grammar,
                            syntax_tree.clone(),
                        );
                    }
                }
            }
        }

        if !self
            .app
            .services
            .treesitter
            .should_parse(buffer_id, changedtick, grammar)
        {
            return None;
        }

        let sequence = self
            .app
            .services
            .treesitter
            .begin_parse(buffer_id, changedtick, grammar);
        let snapshot = buffer.snapshot().clone();
        let owner = crate::app::services::TaskOwner {
            buffer_id: Some(buffer_id),
            window_id: Some(window_id),
            revision,
        };

        let old_tree = self.app.services.treesitter.syntax_tree(buffer_id).cloned();
        let task_id = self.app.services.spawn_cancellable_task(
            "treesitter",
            sequence,
            owner,
            crate::app::services::TaskType::Treesitter,
            move |token| {
                let cancelled = move || token.is_cancelled();
                let res =
                    treesitter::parse_snapshot_cancellable(snapshot, grammar, old_tree, cancelled);
                Some(res)
            },
        )?;

        self.app
            .services
            .treesitter
            .set_pending_task(buffer_id, task_id);
        Some(())
    }

    fn schedule_window_indexer(&mut self, window_id: vim_ui::WindowId) -> Option<()> {
        if !self.app.indexer_enabled {
            return Some(());
        }
        let buffer_id = WindowOps::window_buffer(&self.app.ui, window_id)?;
        let revision = self.app.model.buffer_state_mut(buffer_id)?.revision;
        let buffer = self.app.model.get_buffer(buffer_id).ok()?;
        let changedtick = buffer.changedtick();

        if self
            .app
            .services
            .indexer
            .should_index(buffer_id, changedtick)
        {
            if let Some(state) = self.app.model.buffer_state(buffer_id) {
                if let Ok(index_result) = &state.index {
                    if index_result.changedtick == changedtick {
                        self.app.services.indexer.initialize_from_indexed(
                            buffer_id,
                            changedtick,
                            index_result.source_key.clone(),
                            index_result.keywords.clone(),
                        );
                    }
                }
            }
        }

        if !self
            .app
            .services
            .indexer
            .should_index(buffer_id, changedtick)
        {
            return None;
        }

        let sequence = self
            .app
            .services
            .indexer
            .begin_index(buffer_id, changedtick);
        let snapshot = buffer.snapshot().clone();
        let source_key = buffer.path()?.to_string_lossy().into_owned();
        let owner = crate::app::services::TaskOwner {
            buffer_id: Some(buffer_id),
            window_id: Some(window_id),
            revision,
        };

        let task_id = self.app.services.spawn_cancellable_task(
            "indexer",
            sequence,
            owner,
            crate::app::services::TaskType::Indexer,
            move |token| {
                let cancelled = move || token.is_cancelled();
                indexer::index_buffer_cancellable(source_key, snapshot, cancelled)
            },
        )?;

        self.app
            .services
            .indexer
            .set_pending_task(buffer_id, task_id);
        Some(())
    }

    fn resize(&mut self, rect: vim_ui::Rect) {
        self.apply_view_effect(ViewEffect::Resize {
            width: rect.width,
            height: rect.height,
        });
        self.buffered_renderer.resize(rect.width, rect.height);
    }

    fn redraw(
        &mut self,
        rect: vim_ui::Rect,
        out: &mut impl Write,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let status_height = if self.app.inspect { 2 } else { 1 };
        self.app.ui.set_window_constraint(
            self.app.view_ids.statusline,
            vim_ui::SizeConstraint::Fixed(status_height),
        );

        let layout = self.layout_snapshot(rect);
        crate::app::ui::ViewSynchronizer::synchronize_viewports(
            &mut self.app.ui,
            &self.app.model,
            &layout,
        );

        let window_ids: Vec<_> = WindowOps::window_buffers(&self.app.ui)
            .into_iter()
            .map(|(window_id, _)| window_id)
            .collect();
        for window_id in window_ids {
            self.schedule_window_highlight(window_id, 0, 0);
        }

        self.refresh_views(&layout);
        self.app.ui.draw(&mut self.buffered_renderer)?;
        self.buffered_renderer.flush(out)?;
        out.flush()?;
        Ok(())
    }

    /// Rebuilds every window's owned rendering model from window state,
    /// buffer state, and `RenderGlobals`, immediately before the draw pass.
    fn refresh_views(&mut self, layout: &LayoutSnapshot) {
        self.app.ui.set_colorscheme(self.app.colorscheme.clone());
        let colorscheme = self.app.ui.colorscheme().cloned();
        let globals = RenderGlobals {
            mode: self.app.controller.mode(),
            status_message: self.app.model.status.as_deref(),
            search_pattern: self.app.model.search_pattern.as_deref(),
            search_regex: self.app.model.search_regex.as_ref(),
            search_range: self.app.model.search_range.as_ref(),
            substitute_text: self.app.model.substitute_text.as_deref(),
            colorscheme: colorscheme.as_ref(),
        };

        let active_window = self.app.ui.focused_window_id();
        let commandline_id = self.app.view_ids.commandline;

        for (window_id, buffer_id) in WindowOps::window_buffers(&self.app.ui) {
            let Ok(buffer) = self.app.model.get_buffer(buffer_id) else {
                continue;
            };
            let Some(buffer_state) = self.app.model.buffer_state(buffer_id) else {
                continue;
            };
            let fallback_viewport = self
                .app
                .ui
                .window(window_id)
                .and_then(Window::window_state)
                .map(|state| state.viewport)
                .unwrap_or_default();
            let window_layout = layout.get(window_id).unwrap_or(WindowLayout {
                rect: vim_ui::Rect::new(
                    0,
                    0,
                    fallback_viewport.width as u16,
                    fallback_viewport.height as u16,
                ),
                draws_border: fallback_viewport.has_border,
            });
            let inner_rect = if window_layout.draws_border {
                window_layout.rect.inner(1)
            } else {
                window_layout.rect
            };
            let active = window_id == active_window;

            let Some(window) = self.app.ui.window_mut(window_id) else {
                continue;
            };
            if window_id == commandline_id {
                let (window_state, view) = window.refresh_parts::<CommandLineView>();
                if let (Some(window_state), Some(view)) = (window_state, view) {
                    view.refresh(
                        buffer,
                        window_state,
                        buffer_state,
                        inner_rect,
                        active,
                        &globals,
                    );
                }
            } else {
                let show_number = self
                    .app
                    .config
                    .get("number", Some(buffer_id), Some(window_id))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let show_cursorline = self
                    .app
                    .config
                    .get("cursorline", Some(buffer_id), Some(window_id))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let wrap_text = self
                    .app
                    .config
                    .get("wrap", Some(buffer_id), Some(window_id))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                if let Some(state) = window.window_state_mut() {
                    state.set_show_gutter(show_number);
                    state.set_show_cursorline(show_cursorline);
                    state.set_wrap_text(wrap_text);
                }
                let (window_state, view) = window.refresh_parts::<TextView>();
                if let (Some(window_state), Some(view)) = (window_state, view) {
                    view.refresh(
                        buffer,
                        window_state,
                        buffer_state,
                        inner_rect,
                        active,
                        &globals,
                    );
                }
            }
        }

        let buffer_ids = self.app.model.list();
        let tabs: Vec<String> = buffer_ids
            .iter()
            .map(|&id| buffer_display_name(&self.app.model, id))
            .collect();
        // The command-line buffer is intentionally unlisted, so use the
        // editor window that was focused before entering command-line mode.
        let tab_window = if active_window == commandline_id {
            self.app
                .ui
                .focus_manager()
                .previous_id()
                .filter(|&id| id != commandline_id)
                .unwrap_or(self.app.view_ids.main)
        } else {
            active_window
        };
        let active_index = WindowOps::window_buffer(&self.app.ui, tab_window)
            .and_then(|id| buffer_ids.iter().position(|&candidate| candidate == id))
            .unwrap_or(0);
        if let Some(view) = self
            .app
            .ui
            .window_mut(self.app.view_ids.tabline)
            .and_then(Window::view_as_mut::<TabLineView>)
        {
            view.refresh(&tabs, active_index, &globals);
        }

        let (buffer_name, modified, cursor, scope_path, inspect_label) =
            self.status_line_data(active_window);
        if let Some(view) = self
            .app
            .ui
            .window_mut(self.app.view_ids.statusline)
            .and_then(Window::view_as_mut::<StatusLineView>)
        {
            view.refresh(
                &globals,
                buffer_name,
                modified,
                cursor,
                scope_path,
                inspect_label,
            );
        }
    }

    fn status_line_data(
        &self,
        active_window: vim_ui::WindowId,
    ) -> (String, bool, Option<(u32, u32)>, Vec<String>, String) {
        let Some(buffer_id) = WindowOps::window_buffer(&self.app.ui, active_window) else {
            return (String::new(), false, None, Vec::new(), "Scope".to_string());
        };
        let buffer_name = buffer_display_name(&self.app.model, buffer_id);
        let Ok(buffer) = self.app.model.get_buffer(buffer_id) else {
            return (buffer_name, false, None, Vec::new(), "Scope".to_string());
        };
        let modified = buffer.is_modified();
        let Some(window_state) = self
            .app
            .ui
            .window(active_window)
            .and_then(Window::window_state)
        else {
            return (buffer_name, modified, None, Vec::new(), "Scope".to_string());
        };
        let point = if window_state.selections.selections.is_empty() {
            text::Point::new(0, 0)
        } else {
            window_state
                .selections
                .primary()
                .head()
                .to_point(buffer.snapshot().as_inner())
        };
        let cursor = Some((point.row + 1, point.column + 1));

        let mut scope_path = Vec::new();
        let mut inspect_label = "Scope".to_string();
        if self.app.inspect {
            inspect_label = match self.app.inspect_what {
                crate::app::InspectKind::TreeSitter => "[treesitter]".to_string(),
                crate::app::InspectKind::Textmate => "[textmate]".to_string(),
                crate::app::InspectKind::Indexer => "[indexer]".to_string(),
                crate::app::InspectKind::None => "Scope".to_string(),
            };
            if let Some(state) = self.app.model.buffer_state(buffer_id) {
                match self.app.inspect_what {
                    crate::app::InspectKind::TreeSitter => {
                        if !self.app.treesitter_enabled {
                            scope_path = vec!["treesitter is not enabled".to_string()];
                        } else if let Ok(tree) = &state.treesitter {
                            if let Ok(offset) = buffer
                                .snapshot()
                                .point_to_offset(vim_buffer::Point::new(point.row, point.column))
                            {
                                scope_path = tree
                                    .scope_path_at_byte(offset.0)
                                    .into_iter()
                                    .filter(|node| node.named && !node.kind.is_empty())
                                    .map(|node| node.kind)
                                    .collect();
                            }
                        }
                    }
                    crate::app::InspectKind::Textmate => {
                        if !self.app.syntax_highlight {
                            scope_path = vec!["syntax highlight is not enabled".to_string()];
                        } else {
                            let file_path = buffer.path().and_then(|p| p.to_str());
                            scope_path = state.highlights.scope_path_at_position(
                                buffer.snapshot().as_inner(),
                                file_path,
                                point.row,
                                point.column,
                            );
                        }
                    }
                    crate::app::InspectKind::Indexer => {
                        if !self.app.indexer_enabled {
                            scope_path = vec!["indexer is not enabled".to_string()];
                        } else {
                            let files_count = self.app.services.indexer.buffer_keywords.len();
                            let keys_count: usize = self
                                .app
                                .services
                                .indexer
                                .buffer_keywords
                                .values()
                                .map(|row_map| row_map.values().map(|set| set.len()).sum::<usize>())
                                .sum();
                            scope_path =
                                vec![format!("files: {}, keys: {}", files_count, keys_count)];
                            if let Ok(offset) = buffer
                                .snapshot()
                                .point_to_offset(vim_buffer::Point::new(point.row, point.column))
                            {
                                let text: String = buffer.snapshot().chunks().collect();
                                use vim_buffer::TextSearch;
                                if let Some((_, _, word)) = text.find_word(offset.0) {
                                    let results = self.app.services.indexer.query(word, None);
                                    scope_path.extend(
                                        results.iter().map(|entry| entry.keyword.clone()).take(5),
                                    );
                                }
                            }
                        }
                    }
                    crate::app::InspectKind::None => {}
                }
            }
        }

        (buffer_name, modified, cursor, scope_path, inspect_label)
    }

    fn layout_snapshot(&self, fallback: vim_ui::Rect) -> LayoutSnapshot {
        let mut snapshot = LayoutSnapshot::default();
        for (window_id, _) in WindowOps::window_buffers(&self.app.ui) {
            let rect = self
                .app
                .ui
                .computed_layout()
                .get_rect(window_id)
                .unwrap_or(fallback);
            let draws_border = self
                .app
                .ui
                .window(window_id)
                .is_some_and(|window| window.draws_border());
            snapshot.insert(window_id, rect, draws_border);
        }
        snapshot
    }
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    Runtime::new()?.run()
}
