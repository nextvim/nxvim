mod controller;
mod editor;
mod services;
mod ui;

use std::{
    collections::HashMap,
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

    let mut buffer_manager = editor::buffers::BufferManager::new();
    let mut ui = ui::Ui::new();
    let mut editor = editor::Editor::new()?;
    let mut controller = controller::Controller::new();

    for path in file_paths {
        buffer_manager.add_buffer_for_path(&path)?;
    }

    if buffer_manager.buffers.is_empty() {
        buffer_manager.add_buffer_for_path("")?;
    }
    
    if let Some(win) = ui.windows.get_mut(&(ui::WindowId::MainWindow as usize)) {
        let active_buf = buffer_manager.buffers.first().unwrap();
        win.buffer_id = Some(active_buf.id);
        win.doc = Some(editor::document::Document::new_with_buffer(
            active_buf.id,
            &active_buf.buffer,
            &active_buf.file_path,
        ));
    }

    let cmd_buf = buffer_manager.add_buffer_for_path("#command")?;
    let cmd_id = cmd_buf.id;
    let cmd_file_path = cmd_buf.file_path.clone();
    if let Some(win) = ui.windows.get_mut(&(ui::WindowId::CommandLine as usize)) {
        let cmd_buffer = buffer_manager.find_by_path(&cmd_file_path).unwrap();
        win.buffer_id = Some(cmd_id);
        win.doc = Some(editor::document::Document::new_with_buffer(
            cmd_id,
            &cmd_buffer.buffer,
            &cmd_buffer.file_path,
        ));
    }

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
        ui.update(&mut editor, &mut buffer_manager)?;
        if editor.should_redraw {
            ui.draw(&mut stdout, &mut editor, &mut buffer_manager)?;
            editor.should_redraw = false;
        }

        if event::poll(Duration::from_millis(50))? {
            let event = event::read()?;
            match controller.handle_event(event, &mut editor, &mut buffer_manager)? {
                controller::ControllerResult::Exit => {
                    break;
                }
                _ => {}
            }
        }

        match controller.dispatch_actions(&mut editor, &mut buffer_manager, &mut ui)?{
                controller::ControllerResult::Exit => {
                    break;
                }
                _ => {}
            }

        services::poll(&mut editor, &mut buffer_manager, &mut ui)?;
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
