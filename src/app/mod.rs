pub mod buffer_manager;
pub mod editor;
pub mod input;
pub mod script;
pub mod services;
pub mod ui;
pub mod views;

use text::{Point, ToPoint};

use crate::app::views::mainwindow::MainWindowState;
use std::cell::RefCell;
use std::rc::Rc;

pub struct App {
    pub script: script::ScriptRuntime,
    pub buffer_manager: buffer_manager::BufferManager,
    pub controller: input::InputController,
    pub ui: ui::Ui,
    pub services: services::Services,
    pub main_window_state: Rc<RefCell<MainWindowState>>,
    pub status_message: Option<String>,
    pub editor: editor::Editor,
    pub tabline_id: vim_ui::WindowId,
    pub status_id: vim_ui::WindowId,
}

impl App {
    pub fn new() -> Self {
        let main_window_state = Rc::new(RefCell::new(MainWindowState::new()));
        let mut ui = ui::Ui::new(ui::Rect::new(0, 0, 80, 24));
        let (tabline_id, status_id) = ui::setup_initial_layout(&mut ui, Rc::clone(&main_window_state)).unwrap();
        Self {
            script: script::ScriptRuntime::new(),
            buffer_manager: buffer_manager::BufferManager::new(),
            controller: input::InputController::new(vim_input::Mode::Normal),
            ui,
            services: services::Services::new(),
            main_window_state,
            status_message: None,
            editor: editor::Editor::new(),
            tabline_id,
            status_id,
        }
    }

    pub fn update(&mut self, width: u16, height: u16) {
        let window_buffers: Vec<(vim_ui::WindowId, vim_buffer::BufferId)> = self
            .main_window_state
            .borrow()
            .window_buffers
            .iter()
            .map(|(&k, &v)| (k, v))
            .collect();

        for (win_id, buffer_id) in window_buffers {
            let tab_id = crate::app::buffer_manager::TabId(win_id.get());
            let win_rect = self
                .ui
                .computed_layout()
                .get_rect(win_id)
                .unwrap_or(vim_ui::Rect::new(0, 0, width, height));

            let inner_rect = if let Some(win) = self.ui.window(win_id) {
                if win.draws_border() {
                    win_rect.inner(1)
                } else {
                    win_rect
                }
            } else {
                win_rect
            };

            let has_border = if let Some(win) = self.ui.window(win_id) {
                win.draws_border()
            } else {
                false
            };

            let snapshot = self.buffer_manager.get_buffer(buffer_id).ok().map(|buf| buf.snapshot().as_inner().clone());
            if let Some(snapshot) = snapshot {
                if let Some(display_context) = self
                    .buffer_manager
                    .get_buffer_display_context_mut(buffer_id, tab_id)
                {
                    display_context.update(
                        snapshot,
                        win_rect.width as u32,
                        inner_rect.height as u32,
                        has_border,
                    );
                } else {
                    let buffer_ref = self.buffer_manager.get_buffer(buffer_id).ok();
                    let display_context = crate::app::buffer_manager::BufferDisplayContext::new(
                        snapshot,
                        win_rect.width as u32,
                        inner_rect.height as u32,
                        has_border,
                        buffer_ref,
                    );
                    self.buffer_manager.set_buffer_display_context(
                        buffer_id,
                        tab_id,
                        display_context,
                    );
                }
            }
        }

        // Rebuild TabLineView
        let tabline_win_id = self.tabline_id;
        let current_tab_ids = self.buffer_manager.list();
        let active_win = self.ui.focused_window_id();
        let current_active_tab = self.main_window_state.borrow().window_buffers.get(&active_win).copied();
        
        let tabs: Vec<String> = current_tab_ids
            .iter()
            .map(|id| {
                if let Ok(buf) = self.buffer_manager.get_buffer(*id) {
                    buf.path()
                        .and_then(|p| p.file_name())
                        .and_then(|s| s.to_str())
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| format!("[No Name {}]", id.get()))
                } else {
                    format!("[No Name {}]", id.get())
                }
            })
            .collect();

        let active_index = current_active_tab
            .and_then(|active_id| current_tab_ids.iter().position(|&id| id == active_id))
            .unwrap_or(0);

        if let Some(w) = self.ui.window_mut(tabline_win_id) {
            w.set_view(Box::new(crate::app::views::TabLineView::new(tabs, active_index)));
        }

        // Rebuild StatusLineView
        let status_win_id = self.status_id;
        let mode = format!("{:?}", self.controller.mode()).to_uppercase();
        let buf_name = current_active_tab
            .and_then(|id| {
                if let Ok(buf) = self.buffer_manager.get_buffer(id) {
                    buf.path()
                        .and_then(|p| p.file_name())
                        .and_then(|s| s.to_str())
                        .map(|s| s.to_string())
                } else {
                    None
                }
            })
            .unwrap_or_else(|| "[No Name]".to_string());

        let left = format!(" {} [{}]", mode, buf_name);
        let cursor_str = "1:5".to_string(); // fallback/consistent display position row 1, column 5 (row 0, col 4 1-indexed)
        let right = format!("{} | utf-8 | rust ", cursor_str);

        if let Some(w) = self.ui.window_mut(status_win_id) {
            w.set_view(Box::new(crate::app::views::StatusLineView::new(left, right)));
        }
    }

    pub fn update_tasks(&mut self) {
        let results = std::mem::take(&mut self.services.results);
        for result in results {
            let metadata = self.services.task_metadata.lock().unwrap().remove(&result.task_id);
            if let Some((owner, task_type)) = metadata {
                use crate::app::services::TaskType;
                match task_type {
                    TaskType::Treesitter => {
                        if let Ok(data) = result.downcast::<Result<vim_treesitter::SyntaxTree, String>>() {
                            if let Some(bid) = owner.buffer_id {
                                if let Some(context) = self.buffer_manager.get_buffer_context_mut(bid) {
                                    context.treesitter = data;
                                }
                            }
                        }
                    }
                    TaskType::Indexer => {
                        if let Ok(data) = result.downcast::<Result<vim_indexer::IndexTaskResult, String>>() {
                            if let Some(bid) = owner.buffer_id {
                                if let Some(context) = self.buffer_manager.get_buffer_context_mut(bid) {
                                    context.index = data;
                                }
                            }
                        }
                    }
                    TaskType::Highlight => {
                        if let Ok(data) = result.downcast::<Vec<textmate::HighlightSpan>>() {
                            if let Some(tid) = owner.tab_id {
                                if let Some(bid) = owner.buffer_id {
                                    if let Some(display_context) = self.buffer_manager.get_buffer_display_context_mut(bid, tid) {
                                        display_context.highlights = data;
                                    }
                                } else {
                                    for (&(_bid, t), display_context) in self.buffer_manager.display_contexts_mut() {
                                        if t == tid {
                                            display_context.highlights = data.clone();
                                        }
                                    }
                                }
                            }
                        }
                    }
                    TaskType::DisplayMap => {
                        if let Ok((display_map, height, layout_width)) = result.downcast::<(display_map::DisplayMap, u32, u32)>() {
                            if let Some(tid) = owner.tab_id {
                                if let Some(bid) = owner.buffer_id {
                                    let current_snapshot = self.buffer_manager.get_buffer(bid).ok().map(|buf| buf.snapshot().as_inner().clone());
                                    if let Some(display_context) = self.buffer_manager.get_buffer_display_context_mut(bid, tid) {
                                        display_context.display_map = display_map;
                                        let cursor_anchor = display_context.selections.primary().head();
                                        let display_snapshot = display_context.display_map.snapshot();
                                        let original_buffer = display_snapshot.buffer_snapshot();
                                        let display_cursor = if let Some(ref snapshot) = current_snapshot {
                                            if original_buffer.version == snapshot.version {
                                                display_snapshot.anchor_to_display_point(cursor_anchor)
                                            } else {
                                                let point = cursor_anchor.to_point(snapshot);
                                                let max_row = original_buffer.row_count().saturating_sub(1);
                                                let row = point.row.min(max_row);
                                                let col = if row < original_buffer.row_count() {
                                                    point.column.min(original_buffer.line_len(row))
                                                } else {
                                                    0
                                                };
                                                let clipped_point = Point { row, column: col };
                                                display_snapshot.point_to_display_point(clipped_point)
                                            }
                                        } else {
                                            display_snapshot.anchor_to_display_point(cursor_anchor)
                                        };
                                        let wrap_width = display_context.display_map.wrap_width.unwrap_or(layout_width);
                                        display_context.display_map.scroll_to_cursor(
                                            display_cursor,
                                            height as i32,
                                            wrap_width as i32,
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        self.services.results.clear();
    }
}

struct AppContext {
    text_models: std::collections::HashMap<vim_ui::WindowId, vim_ui::TextViewModel>,
    active_buffer_id: Option<vim_ui::BufferId>,
    buffer_ids: Vec<vim_ui::BufferId>,
    buffer_names: std::collections::HashMap<vim_ui::BufferId, String>,
    active_cursor: Option<(u32, u32)>,
    mode_name: String,
    status_message: Option<String>,
}

impl vim_ui::UIContext for AppContext {
    fn get_buffer_model(&self, _id: vim_ui::BufferId) -> Option<vim_ui::BufferViewModel<'_>> {
        None
    }
    fn get_active_buffer_id(&self) -> Option<vim_ui::BufferId> {
        self.active_buffer_id
    }
    fn get_text_model(&self, window_id: vim_ui::WindowId) -> Option<&vim_ui::TextViewModel> {
        self.text_models.get(&window_id)
    }
    fn get_colorscheme(&self) -> Option<&vim_ui::ColorScheme> {
        None
    }
    fn get_buffer_ids(&self) -> Vec<vim_ui::BufferId> {
        self.buffer_ids.clone()
    }
    fn get_buffer_name(&self, id: vim_ui::BufferId) -> Option<String> {
        self.buffer_names.get(&id).cloned()
    }
    fn get_status_message(&self) -> Option<String> {
        self.status_message.clone()
    }
    fn get_mode_name(&self) -> String {
        self.mode_name.clone()
    }
    fn get_cursor_position(&self) -> Option<(u32, u32)> {
        self.active_cursor
    }
}

impl AppContext {
    pub fn new() -> Self {
        Self {
            text_models: std::collections::HashMap::new(),
            active_buffer_id: None,
            buffer_ids: Vec::new(),
            buffer_names: std::collections::HashMap::new(),
            active_cursor: None,
            mode_name: "NORMAL".to_string(),
            status_message: None,
        }
    }

    pub fn build(&mut self, app: &App, width: u16, height: u16) {
        self.text_models.clear();
        self.active_buffer_id = None;
        self.buffer_ids.clear();
        self.buffer_names.clear();
        self.active_cursor = None;
        self.mode_name = format!("{:?}", app.controller.mode()).to_uppercase();
        self.status_message = app.status_message.clone();

        // Track buffer IDs and their names
        for id in app.buffer_manager.list() {
            let ui_buf_id = vim_ui::BufferId::new(id.get());
            self.buffer_ids.push(ui_buf_id);

            let name = if let Ok(buf) = app.buffer_manager.get_buffer(id) {
                buf.path()
                    .and_then(|p| p.file_name())
                    .and_then(|s| s.to_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| format!("[No Name {}]", id.get()))
            } else {
                format!("[No Name {}]", id.get())
            };
            self.buffer_names.insert(ui_buf_id, name);
        }

        let active_id = app.ui.focused_window_id();
        let window_buffers: Vec<(vim_ui::WindowId, vim_buffer::BufferId)> = app
            .main_window_state
            .borrow()
            .window_buffers
            .iter()
            .map(|(&k, &v)| (k, v))
            .collect();

        for (win_id, buffer_id) in window_buffers {
            if win_id == active_id {
                self.active_buffer_id = Some(vim_ui::BufferId::new(buffer_id.get()));
            }

            let text_model = views::mainwindow::build_text(app, win_id, buffer_id, active_id, width, height);
            if win_id == active_id {
                if let Some(cursor) = text_model.cursor {
                    self.active_cursor = Some((cursor.position.row + 1, cursor.position.column + 1));
                }
            }
            self.text_models.insert(win_id, text_model);
        }
    }
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    use crate::terminal::TerminalSession;
    use crossterm::event;
    use input::ControllerAction;
    use std::io::{Write, stdout};
    use vim_input::Action;
    use vim_ui::BufferedRenderer;

    let mut app = App::new();
    let mut terminal = TerminalSession::enter()?;

    let rect = terminal.size().unwrap_or(vim_ui::Rect::new(0, 0, 80, 24));
    app.ui = ui::Ui::new(rect);
    let _ = ui::setup_initial_layout(&mut app.ui, Rc::clone(&app.main_window_state));

    let mut buffered_renderer = BufferedRenderer::new(rect.width, rect.height);
    let mut out = stdout();

    // Draw the initial layout
    app.update(rect.width, rect.height);
    let mut context = AppContext::new();
    context.build(&app, rect.width, rect.height);
    app.ui.draw(&context, &mut buffered_renderer)?;
    buffered_renderer.flush(&mut out)?;
    out.flush()?;

    loop {
        if app.services.poll() {
            app.update_tasks();
        }

        let current_rect = app.ui.screen_rect();
        if let Ok(new_rect) = terminal.size() {
            if new_rect != current_rect {
                app.ui.resize(new_rect);
                buffered_renderer.resize(new_rect.width, new_rect.height);
            }
        }

        if event::poll(std::time::Duration::from_millis(50))? {
            let ev = event::read()?;

            if let event::Event::Resize(w, h) = ev {
                let new_rect = vim_ui::Rect::new(0, 0, w, h);
                app.ui.resize(new_rect);
                buffered_renderer.resize(w, h);
            }

            if let Some(resolved) = app.controller.feed_event(ev) {
                match resolved {
                    ControllerAction::Execute { action, register } => {
                        let mut msg = format!("[{:?}] Action: {:?}", app.controller.mode(), action);
                        if let Some(r) = register {
                            msg.push_str(&format!(" (reg: '{}')", r));
                        }
                        app.status_message = Some(msg);

                        let active_id = app.ui.focused_window_id();
                        let tab_id = crate::app::buffer_manager::TabId(active_id.get());
                        let active_buf = app.main_window_state.borrow().window_buffers.get(&active_id).copied();
                        if let Some(buf_id) = active_buf {
                            let mut next_mode = None;
                            let _ = app.buffer_manager.with_mut(buf_id, tab_id, |buffer, context, display_context| {
                                if let Ok(mode) = app.editor.execute(app.controller.mode(), &action, buffer, context, display_context, &mut app.services) {
                                    next_mode = mode;
                                }
                            });
                            if let Some(m) = next_mode {
                                app.controller.set_mode(m);
                            }
                        }

                        match action {
                            Action::NextTab { .. } => {
                                let buffers = app.buffer_manager.list();
                                let active_id = app.ui.focused_window_id();
                                let mut state = app.main_window_state.borrow_mut();
                                if let Some(buf_id) = state.window_buffers.get_mut(&active_id) {
                                    if !buffers.is_empty() {
                                        if let Some(pos) =
                                            buffers.iter().position(|&id| id == *buf_id)
                                        {
                                            let next_pos = (pos + 1) % buffers.len();
                                            *buf_id = buffers[next_pos];
                                        } else {
                                            *buf_id = buffers[0];
                                        }
                                    }
                                }
                            }
                            Action::PreviousTab { .. } => {
                                let buffers = app.buffer_manager.list();
                                let active_id = app.ui.focused_window_id();
                                let mut state = app.main_window_state.borrow_mut();
                                if let Some(buf_id) = state.window_buffers.get_mut(&active_id) {
                                    if !buffers.is_empty() {
                                        if let Some(pos) =
                                            buffers.iter().position(|&id| id == *buf_id)
                                        {
                                            let prev_pos =
                                                if pos == 0 { buffers.len() - 1 } else { pos - 1 };
                                            *buf_id = buffers[prev_pos];
                                        } else {
                                            *buf_id = buffers[0];
                                        }
                                    }
                                }
                            }
                            Action::SplitHorizontal { .. } => {
                                let active_id = app.ui.focused_window_id();
                                let current_buf = app
                                    .main_window_state
                                    .borrow()
                                    .window_buffers
                                    .get(&active_id)
                                    .copied()
                                    .unwrap_or(vim_buffer::BufferId::new(1).unwrap());

                                if let Ok(new_win_id) =
                                    app.ui.split_focused(vim_ui::SplitAxis::Rows)
                                {
                                    if let Some(w) = app.ui.window_mut(new_win_id) {
                                        w.set_title("MAIN WINDOW".to_string());
                                        w.set_view(Box::new(
                                            crate::app::views::MainWindowView::new(new_win_id),
                                        ));
                                    }
                                    app.main_window_state
                                        .borrow_mut()
                                        .window_buffers
                                        .insert(new_win_id, current_buf);
                                    let _ = app.ui.focus(new_win_id);
                                }
                            }
                            Action::SplitVertical { .. } => {
                                let active_id = app.ui.focused_window_id();
                                let current_buf = app
                                    .main_window_state
                                    .borrow()
                                    .window_buffers
                                    .get(&active_id)
                                    .copied()
                                    .unwrap_or(vim_buffer::BufferId::new(1).unwrap());

                                if let Ok(new_win_id) =
                                    app.ui.split_focused(vim_ui::SplitAxis::Columns)
                                {
                                    if let Some(w) = app.ui.window_mut(new_win_id) {
                                        w.set_title("MAIN WINDOW".to_string());
                                        w.set_view(Box::new(
                                            crate::app::views::MainWindowView::new(new_win_id),
                                        ));
                                    }
                                    app.main_window_state
                                        .borrow_mut()
                                        .window_buffers
                                        .insert(new_win_id, current_buf);
                                    let _ = app.ui.focus(new_win_id);
                                }
                            }
                            Action::FocusLeftWindow => {
                                if let Some(neighbor) =
                                    app.ui.find_neighbor(vim_ui::NavigationDirection::Left)
                                {
                                    let _ = app.ui.focus(neighbor);
                                }
                            }
                            Action::FocusRightWindow => {
                                if let Some(neighbor) =
                                    app.ui.find_neighbor(vim_ui::NavigationDirection::Right)
                                {
                                    let _ = app.ui.focus(neighbor);
                                }
                            }
                            Action::FocusUpWindow => {
                                if let Some(neighbor) =
                                    app.ui.find_neighbor(vim_ui::NavigationDirection::Up)
                                {
                                    let _ = app.ui.focus(neighbor);
                                }
                            }
                            Action::FocusDownWindow => {
                                if let Some(neighbor) =
                                    app.ui.find_neighbor(vim_ui::NavigationDirection::Down)
                                {
                                    let _ = app.ui.focus(neighbor);
                                }
                            }
                            Action::Quit => {
                                break;
                            }
                            _ => {}
                        }
                    }
                    ControllerAction::Pending => {
                        let msg = format!("Pending sequence: {}", app.controller.pending_display());
                        app.status_message = Some(msg);
                    }
                    ControllerAction::Invalid => {
                        app.status_message = Some("Invalid sequence".to_string());
                    }
                }
            }

            // Redraw layout with dynamic context
            let active_rect = app.ui.screen_rect();
            app.update(active_rect.width, active_rect.height);
            context.build(&app, active_rect.width, active_rect.height);
            app.ui.draw(&context, &mut buffered_renderer)?;
            buffered_renderer.flush(&mut out)?;
            out.flush()?;
        }
    }

    terminal.restore()?;
    Ok(())
}
