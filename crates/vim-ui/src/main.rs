use crossterm::{
    cursor::{Hide, Show},
    event, execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use std::collections::HashMap;
use std::io::{Write, stdout};
use vim_ui::{
    colorscheme::{ColorScheme, Metadata, Style},
    *,
};

fn create_tokyonight_scheme() -> ColorScheme {
    let metadata = Metadata {
        name: "tokyonight".to_string(),
        description: Some("TokyoNight theme".to_string()),
        author: Some("folke".to_string()),
        r#type: Some("dark".to_string()),
        github: Some("https://github.com/folke/tokyonight.nvim".to_string()),
    };

    let mut cs = ColorScheme::new(metadata);

    let bg = Color::Rgb(26, 27, 38);
    let fg = Color::Rgb(192, 202, 245);
    let dark_bg = Color::Rgb(22, 22, 30);
    let status_bg = Color::Rgb(30, 32, 48);
    let comment = Color::Rgb(86, 95, 137);
    let magenta = Color::Rgb(187, 154, 247);
    let blue = Color::Rgb(122, 162, 247);

    // Set basic colors
    cs.background = Some(bg);
    cs.foreground = Some(fg);
    cs.cursor = Some(Color::Rgb(255, 0, 124));
    cs.selection = Some(Color::Rgb(47, 51, 76));

    // Populating highlight groups
    cs.insert_style("Normal", Style::default().fg(fg).bg(bg));
    cs.insert_style("LineNr", Style::default().fg(comment).bg(bg));
    cs.insert_style("WinSeparator", Style::default().fg(magenta));
    cs.insert_style("StatusLine", Style::default().fg(fg).bg(status_bg).bold());
    cs.insert_style("StatusLineNC", Style::default().fg(comment).bg(status_bg));
    cs.insert_style("TabLine", Style::default().fg(comment).bg(dark_bg));
    cs.insert_style("TabLineSel", Style::default().fg(fg).bg(bg).bold());
    cs.insert_style("TabLineFill", Style::default().bg(dark_bg));
    cs.insert_style("Title", Style::default().fg(blue).bold());

    cs
}

struct SimpleContext {
    buffers: HashMap<BufferId, Vec<String>>,
    cursor: BufferPosition,
    selections: Vec<Selection>,
    mode: EditorMode,
    colorscheme: ColorScheme,
}

impl UIContext for SimpleContext {
    fn get_buffer_model(&self, id: BufferId) -> Option<BufferViewModel<'_>> {
        let lines = self.buffers.get(&id)?;
        let cursor = if id == BufferId::new(2) {
            self.cursor
        } else {
            BufferPosition { row: 0, col: 0 }
        };
        Some(BufferViewModel {
            lines,
            cursor,
            selections: &self.selections,
            mode: self.mode,
        })
    }

    fn get_active_buffer_id(&self) -> Option<BufferId> {
        Some(BufferId::new(2))
    }

    fn get_colorscheme(&self) -> Option<&ColorScheme> {
        Some(&self.colorscheme)
    }
}

fn main() -> anyhow::Result<()> {
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen, Hide)?;

    let mut ui = Ui::new(Rect::new(0, 0, 100, 30));

    let sidebar_buf_id = BufferId::new(1);
    let editor_buf_id = BufferId::new(2);

    let mut buffers = HashMap::new();
    buffers.insert(
        sidebar_buf_id,
        vec![
            " 📁 src".into(),
            "   lib.rs".into(),
            "   main.rs".into(),
            " 📄 Cargo.toml".into(),
            " 📄 README.md".into(),
        ],
    );
    buffers.insert(
        editor_buf_id,
        vec![
            "fn main() {".into(),
            "    println!(\"Hello, Vim-UI!\");".into(),
            "}".into(),
        ],
    );

    let mut context = SimpleContext {
        buffers,
        cursor: BufferPosition { row: 1, col: 4 },
        selections: vec![],
        mode: EditorMode::Normal,
        colorscheme: create_tokyonight_scheme(),
    };

    // Initial setup
    setup_initial_layout(&mut ui, sidebar_buf_id, editor_buf_id)?;

    let (w, h) = crossterm::terminal::size()?;
    let mut buffered_renderer = BufferedRenderer::new(w, h);

    loop {
        // Draw to buffer
        ui.draw(&context, &mut buffered_renderer)?;

        // Flush buffer to stdout
        buffered_renderer.flush(&mut stdout)?;
        stdout.flush()?;

        if event::poll(std::time::Duration::from_millis(10))? {
            let cross_event = event::read()?;
            if let Ok(ui_event) = UiEvent::try_from(cross_event) {
                if let UiEvent::Key(ref key) = ui_event {
                    if key.modifiers.control && key.code == KeyCode::Char('c') {
                        break;
                    }
                }

                let result = ui.dispatch_event(&ui_event, &mut context);

                if result == EventResult::Ignored {
                    if let UiEvent::Key(key) = ui_event {
                        if key.modifiers.control {
                            match key.code {
                                KeyCode::Char('q') => break,

                                // Focus navigation
                                KeyCode::Char('h') => {
                                    if let Some(id) = ui.find_neighbor(NavigationDirection::Left) {
                                        ui.focus(id)?;
                                    }
                                }
                                KeyCode::Char('j') => {
                                    if let Some(id) = ui.find_neighbor(NavigationDirection::Down) {
                                        ui.focus(id)?;
                                    }
                                }
                                KeyCode::Char('k') => {
                                    if let Some(id) = ui.find_neighbor(NavigationDirection::Up) {
                                        ui.focus(id)?;
                                    }
                                }
                                KeyCode::Char('l') => {
                                    if let Some(id) = ui.find_neighbor(NavigationDirection::Right) {
                                        ui.focus(id)?;
                                    }
                                }

                                // Splitting (Vim-style names)
                                KeyCode::Char('v') => {
                                    ui.split_focused(SplitAxis::Columns)?;
                                }
                                KeyCode::Char('s') => {
                                    ui.split_focused(SplitAxis::Rows)?;
                                }

                                // Closing
                                KeyCode::Char('x') => {
                                    let id = ui.focused_window_id();
                                    if ui.close_window(id)
                                        == Err(UiError::CannotCloseFinalEditorWindow)
                                    {
                                        // Vim-style behavior: keep the final editor window open.
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
        }

        // Update screen size if it changed
        if let Ok((w, h)) = crossterm::terminal::size() {
            if w != ui.screen_rect().width || h != ui.screen_rect().height {
                ui.resize(Rect::new(0, 0, w, h));
                buffered_renderer.resize(w, h);
            }
        }
    }

    disable_raw_mode()?;
    execute!(std::io::stdout(), LeaveAlternateScreen, Show)?;
    Ok(())
}

fn setup_initial_layout(
    ui: &mut Ui,
    sidebar_buf_id: BufferId,
    editor_buf_id: BufferId,
) -> anyhow::Result<()> {
    let editor_id = ui.focused_window_id();
    let tabline_id = ui.create_window("Tabs".to_string());
    {
        let tabline_win = ui.window_mut(tabline_id).unwrap();
        tabline_win.set_draw_border(false);
        tabline_win.set_view(Box::new(TabLineView::new(
            vec!["main.rs".into(), "lib.rs".into(), "Cargo.toml".into()],
            0,
        )));
    }

    let sidebar_id = ui.create_window("Files".to_string());
    {
        let sidebar_win = ui.window_mut(sidebar_id).unwrap();
        sidebar_win.set_view(Box::new(BufferView::new(sidebar_buf_id, false)));
    }

    {
        let editor_win = ui.window_mut(editor_id).unwrap();
        editor_win.set_title("main.rs");
        editor_win.set_view(Box::new(BufferView::new(editor_buf_id, true)));
    }

    let status_id = ui.create_window("Status".to_string());
    {
        let status_win = ui.window_mut(status_id).unwrap();
        status_win.set_draw_border(false);
        status_win.set_view(Box::new(StatusLineView::new(
            " NORMAL ".into(),
            " utf-8 | rust | 1:1 ".into(),
        )));
    }

    ui.set_layout(SlotLayout {
        top_bar: Some((tabline_id, SizeConstraint::Fixed(1))),
        left_sidebar: Some((sidebar_id, SizeConstraint::Fixed(25))),
        right_sidebar: None,
        bottom_bar: None,
        status_bar: Some((status_id, SizeConstraint::Fixed(1))),
        center: editor_id,
    }.build())?;
    Ok(())
}
