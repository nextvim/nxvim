mod controller;
mod editor;
mod scripting;
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
    let mut script = scripting::script::ScriptRuntime::new();

    for path in file_paths {
        buffer_manager.add_buffer_for_path(&path)?;
    }

    if buffer_manager.buffers.is_empty() {
        buffer_manager.add_buffer_for_path("")?;
    }

    let active_buffer_id = buffer_manager.buffers.first().unwrap().id;
    ui.set_window_buffer(
        ui::WindowId::MainWindow as usize,
        active_buffer_id,
        &buffer_manager,
    );

    let cmd_buf = buffer_manager.add_buffer_for_path("#command")?;
    let cmd_id = cmd_buf.id;

    ui.set_window_buffer(ui::WindowId::CommandLine as usize, cmd_id, &buffer_manager);

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

    'main_loop: loop {
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

        let cmds: Vec<controller::ex::Ex> =
            std::iter::from_fn(|| script.try_next_command()).collect();
        for cmd in cmds {
            match cmd {
                controller::ex::Ex::Quit => {
                    break 'main_loop;
                }
                controller::ex::Ex::Delete => {
                    break 'main_loop;
                    // controller.queue_action(vim_input::Action::DeleteLine { count: 1 });
                }
                _ => {}
            }
        }

        match controller.dispatch_actions(
            &mut editor,
            &mut buffer_manager,
            &mut ui,
            Some(&mut script),
        )? {
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
