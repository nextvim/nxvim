pub mod script;
pub mod buffer_manager;
pub mod input;
pub mod ui;
pub mod services;
pub mod controllers;
pub mod views;

use std::rc::Rc;
use std::cell::RefCell;
use crate::app::views::mainwindow::MainWindowState;

pub struct App {
    pub script: script::ScriptRuntime,
    pub buffer_manager: buffer_manager::BufferManager,
    pub controller: input::InputController,
    pub ui: ui::Ui,
    pub services: services::Services,
    pub main_window_state: Rc<RefCell<MainWindowState>>,
}

impl App {
    pub fn new() -> Self {
        let main_window_state = Rc::new(RefCell::new(MainWindowState::new()));
        let mut ui = ui::Ui::new(ui::Rect::new(0, 0, 80, 24));
        let _ = ui::setup_initial_layout(&mut ui, Rc::clone(&main_window_state));
        Self {
            script: script::ScriptRuntime::new(),
            buffer_manager: buffer_manager::BufferManager::new(),
            controller: input::InputController::new(vim_input::Mode::Normal),
            ui,
            services: services::Services::new(),
            main_window_state,
        }
    }
}

struct SimpleContext {
    text_models: std::collections::HashMap<vim_ui::WindowId, vim_ui::TextViewModel>,
    active_buffer_id: Option<vim_ui::BufferId>,
}

impl vim_ui::UIContext for SimpleContext {
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
}

fn build_context(app: &App, width: u16, height: u16) -> SimpleContext {
    let main_window_id = vim_ui::WindowId::new(3); // MAIN WINDOW
    let main_rect = app
        .ui
        .computed_layout()
        .get_rect(main_window_id)
        .unwrap_or(vim_ui::Rect::new(0, 0, width, height));

    let mut tab_layouts = Vec::new();
    let state = app.main_window_state.borrow();
    state.tree.compute_layout(main_rect, &mut tab_layouts);

    let mut text_models = std::collections::HashMap::new();
    let mut active_buffer_id = None;

    for (tab_id, tab_rect) in tab_layouts {
        if let Some(tab) = state.tree.find_tab(tab_id) {
            let buffer_id = tab.current_buffer_id;
            if tab_id == state.active_tab_id {
                active_buffer_id = Some(vim_ui::BufferId::new(buffer_id.get()));
            }

            let mut rows = Vec::new();
            if let Ok(buffer) = app.buffer_manager.get_buffer(buffer_id) {
                let snapshot = buffer.snapshot();
                let row_count = snapshot.row_count();

                for i in 0..row_count {
                    let line_len = snapshot.line_len(i).unwrap_or(0);
                    let start = snapshot
                        .point_to_offset(vim_buffer::Point::new(i, 0))
                        .unwrap()
                        .0;
                    let end = snapshot
                        .point_to_offset(vim_buffer::Point::new(i, line_len))
                        .unwrap()
                        .0;
                    let line: String = snapshot
                        .as_inner()
                        .as_rope()
                        .chunks_in_range(start..end)
                        .collect();

                    rows.push(vim_ui::model::DisplayRow {
                        buffer_row: Some(i),
                        kind: vim_ui::model::DisplayRowKind::Buffer,
                        gutter: Some(vim_ui::model::GutterCell {
                            text: format!(" {:2} ", i + 1),
                            style: vim_ui::Style::default(),
                        }),
                        spans: vec![vim_ui::model::TextSpan::new(
                            line,
                            vim_ui::Style::default(),
                        )],
                        fill_style: vim_ui::Style::default(),
                    });
                }
            }

            if rows.is_empty() {
                rows.push(vim_ui::model::DisplayRow {
                    buffer_row: Some(0),
                    kind: vim_ui::model::DisplayRowKind::Buffer,
                    gutter: Some(vim_ui::model::GutterCell {
                        text: "  1 ".to_string(),
                        style: vim_ui::Style::default(),
                    }),
                    spans: vec![vim_ui::model::TextSpan::new(
                        "".to_string(),
                        vim_ui::Style::default(),
                    )],
                    fill_style: vim_ui::Style::default(),
                });
            }

            let cursor = if tab_id == state.active_tab_id {
                Some(vim_ui::model::TextCursor {
                    position: vim_ui::model::DisplayPosition { row: 0, column: 4 }, // after 4-character gutter
                    shape: vim_ui::model::CursorShape::Block,
                    visible: true,
                })
            } else {
                None
            };

            let text_model = vim_ui::TextViewModel {
                viewport_width: tab_rect.width,
                viewport_height: tab_rect.height,
                rows,
                selections: vec![],
                cursor,
                scrollbar: None,
                default_style: vim_ui::Style::default(),
            };

            text_models.insert(vim_ui::WindowId::new(tab_id.0), text_model);
        }
    }

    SimpleContext {
        text_models,
        active_buffer_id,
    }
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    use crate::terminal::TerminalSession;
    use crossterm::event;
    use input::ControllerAction;
    use std::io::{stdout, Write};
    use vim_input::Action;
    use vim_ui::{BufferedRenderer, WindowId};
    use crate::app::buffer_manager::TabId;

    let mut app = App::new();
    let mut terminal = TerminalSession::enter()?;

    let rect = terminal.size().unwrap_or(vim_ui::Rect::new(0, 0, 80, 24));
    app.ui = ui::Ui::new(rect);
    let _ = ui::setup_initial_layout(&mut app.ui, Rc::clone(&app.main_window_state));

    let mut buffered_renderer = BufferedRenderer::new(rect.width, rect.height);
    let mut out = stdout();

    // Draw the initial layout
    let context = build_context(&app, rect.width, rect.height);
    app.ui.draw(&context, &mut buffered_renderer)?;
    buffered_renderer.flush(&mut out)?;
    out.flush()?;

    loop {
        let current_rect = app.ui.screen_rect();
        if let Ok(new_rect) = terminal.size() {
            if new_rect != current_rect {
                app.ui = ui::Ui::new(new_rect);
                let _ = ui::setup_initial_layout(&mut app.ui, Rc::clone(&app.main_window_state));
                buffered_renderer.resize(new_rect.width, new_rect.height);
            }
        }

        if event::poll(std::time::Duration::from_millis(50))? {
            let ev = event::read()?;

            if let event::Event::Resize(w, h) = ev {
                let new_rect = vim_ui::Rect::new(0, 0, w, h);
                app.ui = ui::Ui::new(new_rect);
                let _ = ui::setup_initial_layout(&mut app.ui, Rc::clone(&app.main_window_state));
                buffered_renderer.resize(w, h);
            }

            if let Some(resolved) = app.controller.feed_event(ev) {
                match resolved {
                    ControllerAction::Execute { action, register } => {
                        let mut msg = format!("[{:?}] Action: {:?}", app.controller.mode(), action);
                        if let Some(r) = register {
                            msg.push_str(&format!(" (reg: '{}')", r));
                        }

                        if let Some(w) = app.ui.window_mut(WindowId::new(5)) {
                            w.set_view(Box::new(vim_ui::views::statusline::StatusLineView::new(
                                msg,
                                "".to_string(),
                            )));
                        }

                        match action {
                            Action::NextTab { .. } => {
                                let buffers = app.buffer_manager.list();
                                let active_id = {
                                    let state = app.main_window_state.borrow();
                                    state.active_tab_id
                                };
                                let mut state = app.main_window_state.borrow_mut();
                                if let Some(tab) = state.tree.find_tab_mut(active_id) {
                                    tab.switch_next(&buffers);
                                }
                            }
                            Action::PreviousTab { .. } => {
                                let buffers = app.buffer_manager.list();
                                let active_id = {
                                    let state = app.main_window_state.borrow();
                                    state.active_tab_id
                                };
                                let mut state = app.main_window_state.borrow_mut();
                                if let Some(tab) = state.tree.find_tab_mut(active_id) {
                                    tab.switch_prev(&buffers);
                                }
                            }
                            Action::SplitHorizontal { .. } => {
                                let next_id = {
                                    let mut state = app.main_window_state.borrow_mut();
                                    state.next_tab_id += 1;
                                    TabId(state.next_tab_id)
                                };
                                let active_id = {
                                    let state = app.main_window_state.borrow();
                                    state.active_tab_id
                                };
                                let current_buf = {
                                    let mut state = app.main_window_state.borrow_mut();
                                    state
                                        .tree
                                        .find_tab_mut(active_id)
                                        .map(|t| t.current_buffer_id)
                                        .unwrap_or(vim_buffer::BufferId::new(1).unwrap())
                                };
                                let mut state = app.main_window_state.borrow_mut();
                                state.tree.split_tab(
                                    active_id,
                                    next_id,
                                    vim_ui::SplitAxis::Rows,
                                    current_buf,
                                );
                                state.active_tab_id = next_id;
                            }
                            Action::SplitVertical { .. } => {
                                let next_id = {
                                    let mut state = app.main_window_state.borrow_mut();
                                    state.next_tab_id += 1;
                                    TabId(state.next_tab_id)
                                };
                                let active_id = {
                                    let state = app.main_window_state.borrow();
                                    state.active_tab_id
                                };
                                let current_buf = {
                                    let mut state = app.main_window_state.borrow_mut();
                                    state
                                        .tree
                                        .find_tab_mut(active_id)
                                        .map(|t| t.current_buffer_id)
                                        .unwrap_or(vim_buffer::BufferId::new(1).unwrap())
                                };
                                let mut state = app.main_window_state.borrow_mut();
                                state.tree.split_tab(
                                    active_id,
                                    next_id,
                                    vim_ui::SplitAxis::Columns,
                                    current_buf,
                                );
                                state.active_tab_id = next_id;
                            }
                            Action::Quit => {
                                break;
                            }
                            _ => {}
                        }
                    }
                    ControllerAction::Pending => {
                        let msg = format!("Pending sequence: {}", app.controller.pending_display());
                        if let Some(w) = app.ui.window_mut(WindowId::new(5)) {
                            w.set_view(Box::new(vim_ui::views::statusline::StatusLineView::new(
                                msg,
                                "".to_string(),
                            )));
                        }
                    }
                    ControllerAction::Invalid => {
                        if let Some(w) = app.ui.window_mut(WindowId::new(5)) {
                            w.set_view(Box::new(vim_ui::views::statusline::StatusLineView::new(
                                "Invalid sequence".to_string(),
                                "".to_string(),
                            )));
                        }
                    }
                }
            }

            // Redraw layout with dynamic context
            let active_rect = app.ui.screen_rect();
            let context = build_context(&app, active_rect.width, active_rect.height);
            app.ui.draw(&context, &mut buffered_renderer)?;
            buffered_renderer.flush(&mut out)?;
            out.flush()?;
        }
    }

    terminal.restore()?;
    Ok(())
}
