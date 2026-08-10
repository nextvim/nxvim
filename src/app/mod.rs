pub mod script;
pub mod buffer_manager;
pub mod input;
pub mod ui;
pub mod services;

pub struct App {
    pub script: script::ScriptRuntime,
    pub buffer_manager: buffer_manager::BufferManager,
    pub controller: input::InputController,
    pub ui: ui::Ui,
    pub services: services::Services,
}

impl App {
    pub fn new() -> Self {
        let mut ui = ui::Ui::new(ui::Rect::new(0, 0, 80, 24));
        let _ = ui::setup_initial_layout(&mut ui);
        Self {
            script: script::ScriptRuntime::new(),
            buffer_manager: buffer_manager::BufferManager::new(),
            controller: input::InputController::new(vim_input::Mode::Normal),
            ui,
            services: services::Services::new(),
        }
    }
}

struct SimpleContext {
    lines: Vec<String>,
    text_model: vim_ui::TextViewModel,
}

impl vim_ui::UIContext for SimpleContext {
    fn get_buffer_model(&self, _id: vim_ui::BufferId) -> Option<vim_ui::BufferViewModel<'_>> {
        Some(vim_ui::BufferViewModel {
            lines: &self.lines,
            cursor: vim_ui::BufferPosition { row: 0, col: 0 },
            selections: &[],
            mode: vim_ui::EditorMode::Normal,
        })
    }
    fn get_active_buffer_id(&self) -> Option<vim_ui::BufferId> {
        Some(vim_ui::BufferId::new(1))
    }
    fn get_text_model(&self, _window_id: vim_ui::WindowId) -> Option<&vim_ui::TextViewModel> {
        Some(&self.text_model)
    }
    fn get_colorscheme(&self) -> Option<&vim_ui::ColorScheme> {
        None
    }
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    use std::io::{Write, stdout};
    use crate::terminal::TerminalSession;
    use crossterm::event;
    use input::ControllerAction;
    use vim_input::Action;
    use vim_ui::{BufferedRenderer, WindowId};

    let mut app = App::new();
    let mut terminal = TerminalSession::enter()?;
    
    let rect = terminal.size().unwrap_or(vim_ui::Rect::new(0, 0, 80, 24));
    app.ui = ui::Ui::new(rect);
    let _ = ui::setup_initial_layout(&mut app.ui);

    let mut buffered_renderer = BufferedRenderer::new(rect.width, rect.height);
    let mut out = stdout();

    let lines = vec!["hello world".to_string()];
    let mut rows = Vec::new();
    for (i, _line) in lines.iter().enumerate() {
        rows.push(vim_ui::model::DisplayRow {
            buffer_row: Some(i as u32),
            kind: vim_ui::model::DisplayRowKind::Buffer,
            gutter: Some(vim_ui::model::GutterCell {
                text: format!(" {:2} ", i + 1),
                style: vim_ui::Style::default(),
            }),
            spans: vec![
                vim_ui::model::TextSpan::new(
                    "hello ".to_string(),
                    vim_ui::Style::default(),
                ),
                vim_ui::model::TextSpan::new(
                    "world".to_string(),
                    vim_ui::Style::default().fg(vim_ui::Color::Red),
                ),
            ],
            fill_style: vim_ui::Style::default(),
        });
    }

    let text_model = vim_ui::TextViewModel {
        viewport_width: rect.width,
        viewport_height: rect.height,
        rows,
        selections: vec![],
        cursor: Some(vim_ui::model::TextCursor {
            position: vim_ui::model::DisplayPosition { row: 0, column: 15 }, // column offset inside the view (includes gutter width of 4 + 11 chars of "hello world")
            shape: vim_ui::model::CursorShape::Block,
            visible: true,
        }),
        scrollbar: None,
        default_style: vim_ui::Style::default(),
    };

    let context = SimpleContext {
        lines,
        text_model,
    };

    // Draw the initial layout
    app.ui.draw(&context, &mut buffered_renderer)?;
    buffered_renderer.flush(&mut out)?;
    out.flush()?;

    loop {
        if let Ok(new_rect) = terminal.size() {
            if new_rect != app.ui.screen_rect() {
                app.ui = ui::Ui::new(new_rect);
                let _ = ui::setup_initial_layout(&mut app.ui);
                buffered_renderer.resize(new_rect.width, new_rect.height);
            }
        }

        if event::poll(std::time::Duration::from_millis(50))? {
            let ev = event::read()?;
            
            if let event::Event::Resize(w, h) = ev {
                let new_rect = vim_ui::Rect::new(0, 0, w, h);
                app.ui = ui::Ui::new(new_rect);
                let _ = ui::setup_initial_layout(&mut app.ui);
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
                            w.set_view(Box::new(vim_ui::views::statusline::StatusLineView::new(msg, "".to_string())));
                        }

                        if action == Action::Quit {
                            break;
                        }
                    }
                    ControllerAction::Pending => {
                        let msg = format!("Pending sequence: {}", app.controller.pending_display());
                        if let Some(w) = app.ui.window_mut(WindowId::new(5)) {
                            w.set_view(Box::new(vim_ui::views::statusline::StatusLineView::new(msg, "".to_string())));
                        }
                    }
                    ControllerAction::Invalid => {
                        if let Some(w) = app.ui.window_mut(WindowId::new(5)) {
                            w.set_view(Box::new(vim_ui::views::statusline::StatusLineView::new("Invalid sequence".to_string(), "".to_string())));
                        }
                    }
                }
            }
            
            // Redraw layout
            app.ui.draw(&context, &mut buffered_renderer)?;
            buffered_renderer.flush(&mut out)?;
            out.flush()?;
        }
    }

    terminal.restore()?;
    Ok(())
}
