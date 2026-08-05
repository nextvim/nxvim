use std::io::{Write, stdout};
use std::time::Duration;

use crossterm::event::{self, Event, KeyEventKind};
use vim_buffer::BufferManager;
use vim_input::Mode;
use vim_ui::{Anchor, BufferedRenderer, FloatingConfig, RelativeTo, Ui};

use crate::{
    controller::InputController,
    editor,
    event::handle_key_event,
    state::{AppState, PopupWindows, TabPage, editor_rect},
    terminal::TerminalSession,
};

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut terminal = TerminalSession::enter()?;
    let screen = terminal.size()?;
    let mut state = initial_state(screen)?;
    let mut renderer = BufferedRenderer::new(screen.width, screen.height);
    let mut output = stdout();

    while state.running {
        let current_size = terminal.size()?;
        state.resize_ui(current_size);
        editor::draw(&mut state, current_size, &mut renderer)?;
        renderer.flush(&mut output)?;
        output.flush()?;

        if event::poll(Duration::from_millis(100))?
            && let Event::Key(key) = event::read()?
            && matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat)
        {
            handle_key_event(&mut state, key, current_size.height as usize - 3)?;
        }
    }

    terminal.restore()?;
    Ok(())
}

fn initial_state(screen: vim_ui::Rect) -> Result<AppState, Box<dyn std::error::Error>> {
    let mut buffers = BufferManager::new();
    let first = buffers.create("Welcome to nxvim!\nThis is a complete rebuild of the editor from the ground up.\nNow powered by vim-ui and vim-buffer.\n\nEnjoy editing!\n").id();
    let second = buffers.create("This is buffer 2.\nYou can cycle through tabs using Tab / Shift-Tab.\nEditing characters is supported in INSERT mode.").id();
    let third = buffers
        .create("This is buffer 3.\nIt represents another open file.")
        .id();
    let command_buffer_id = buffers.create("").id();
    let mut command_selections = vim_buffer::SelectionSet::new();
    command_selections.add(buffers.get(command_buffer_id)?.as_text_buffer(), 0);
    let tabs = vec![
        TabPage::new("nxvim_welcome", first, buffers.get(first)?),
        TabPage::new("buffer_two", second, buffers.get(second)?),
        TabPage::new("buffer_three", third, buffers.get(third)?),
    ];

    let mut ui = Ui::new(editor_rect(screen));
    let editor_id = ui.focused_window_id();
    ui.window_mut(editor_id)
        .expect("initial editor window")
        .set_draw_border(false);

    let autocomplete = ui.create_floating_window(
        "Completions",
        FloatingConfig {
            relative_to: RelativeTo::Cursor,
            anchor: Anchor::TopLeft,
            row: 1,
            col: 0,
            width: 20,
            height: 5,
            zindex: 110,
            border: true,
        },
    );
    let dialog = ui.create_floating_window(
        "Message",
        FloatingConfig {
            relative_to: RelativeTo::Editor,
            anchor: Anchor::TopLeft,
            row: (screen.height / 2).saturating_sub(2) as i16,
            col: (screen.width / 2).saturating_sub(20) as i16,
            width: screen.width.min(40),
            height: 3,
            zindex: 200,
            border: true,
        },
    );
    ui.set_window_visible(autocomplete, false)?;
    ui.set_window_visible(dialog, false)?;

    Ok(AppState {
        buffers,
        tabs,
        active_tab_index: 0,
        mode: Mode::Normal,
        running: true,
        command_buffer_id,
        command_selections,
        command_return_focus: editor_id,
        command_line_focused: false,
        controller: InputController::new(Mode::Normal),
        ui,
        window_tabs: [(editor_id, 0)].into_iter().collect(),
        popups: PopupWindows {
            autocomplete,
            dialog,
        },
        dialog_message: None,
    })
}
