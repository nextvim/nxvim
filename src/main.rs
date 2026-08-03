mod controller;
mod editor;
mod services;
mod ui;

use std::{
    io::{Write, stdout},
    time::{Duration, Instant},
};

use crossterm::{
    cursor::MoveTo,
    event::{self, Event, KeyCode, KeyEvent, MouseEventKind},
    execute,
    terminal::{Clear, ClearType},
};

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let file_paths = if args.len() > 1 {
        args[1..].to_vec()
    } else {
        Vec::new()
    };

    let mut stdout = stdout();

    let mut vim_buffers = editor::buffers::VimBuffers::new();
    let mut ui = ui::Ui::new();
    let mut editor = editor::Editor::new()?;
    let mut controller = controller::Controller::new();

    for path in &file_paths {
        vim_buffers.add_buffer_for_path(path)?;
    }

    if vim_buffers.list().is_empty() {
        vim_buffers.add_buffer_for_path("")?;
    }

    let active_vim_buffer_id = vim_buffers.list().first().copied().unwrap();
    ui.set_vim_window_buffer(
        ui::WindowId::MainWindow as usize,
        active_vim_buffer_id,
        &vim_buffers,
    );

    let cmd_vim_buffer_id = vim_buffers.add_buffer_for_path("#command")?.id;
    ui.set_vim_window_buffer(
        ui::WindowId::CommandLine as usize,
        cmd_vim_buffer_id,
        &vim_buffers,
    );

    crossterm::terminal::enable_raw_mode().unwrap();
    execute!(
        stdout,
        crossterm::event::EnableBracketedPaste,
        Clear(ClearType::All),
        MoveTo(0, 0)
    )
    .unwrap();

    execute!(
        stdout,
        crossterm::event::EnableMouseCapture,
        Clear(ClearType::All),
        MoveTo(0, 0)
    )?;

    execute!(stdout, crossterm::cursor::Hide).unwrap();

    loop {
        ui.update(&mut editor, &mut vim_buffers)?;
        if editor.should_redraw {
            ui.draw(&mut stdout, &mut editor, &mut vim_buffers)?;
            editor.should_redraw = false;
        }

        if event::poll(Duration::from_millis(50))? {
            let event = event::read()?;
            match controller.handle_event(event, &mut editor)? {
                controller::ControllerResult::Exit => {
                    break;
                }
                _ => {}
            }
        }

        match controller.dispatch_actions(&mut editor, &mut vim_buffers, &mut ui)? {
            controller::ControllerResult::Exit => {
                break;
            }
            _ => {}
        }

        services::poll(&mut editor, &mut vim_buffers, &mut ui)?;
    }

    crossterm::terminal::disable_raw_mode().unwrap();
    execute!(
        stdout,
        crossterm::event::DisableBracketedPaste,
        Clear(ClearType::All),
        MoveTo(0, 0)
    )
    .unwrap();
    execute!(
        stdout,
        crossterm::event::DisableMouseCapture,
        Clear(ClearType::All),
        MoveTo(0, 0)
    )
    .unwrap();
    Ok(())
}
