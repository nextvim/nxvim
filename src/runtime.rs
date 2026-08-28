//! Event loop: poll a terminal event, translate it, hand it to `App`, render.
//!
//! Sequencing only — no semantics. `Editor::execute()` runs synchronously
//! inside `App::handle_action`; the redraw happens after it returns, never
//! mid-command (`RESCUE.md` Rule 4.7 / `docs/VIM.md` lesson #3).

use std::io::{self, Write};
use std::time::Duration;

use crossterm::event::{self, Event};

use crate::app::{App, input::InputTranslator};
use crate::view;

pub fn run(app: &mut App, session: &crate::terminal::TerminalSession, out: &mut impl Write) -> io::Result<()> {
    let mut screen = session.size().unwrap_or(vim_ui::Rect::new(0, 0, 80, 24));
    let mut input = InputTranslator::new();
    // Temporary debug status (mode + last resolved action), see `view::render`.
    let mut status = format!("-- {:?} -- last: (none)", app.editor().mode());
    let prompt_opt = if app.editor().mode() == crate::kernel::mode::Mode::Command {
        Some(app.prompt().text())
    } else {
        None
    };
    view::render(out, app.editor(), &status, prompt_opt, screen)?;

    loop {
        if !event::poll(Duration::from_millis(50))? {
            continue;
        }
        let ev = event::read()?;

        if let Event::Resize(columns, rows) = ev {
            screen = vim_ui::Rect::new(0, 0, columns, rows);
            let prompt_opt = if app.editor().mode() == crate::kernel::mode::Mode::Command {
                Some(app.prompt().text())
            } else {
                None
            };
            view::render(out, app.editor(), &status, prompt_opt, screen)?;
            continue;
        }

        let is_command_mode = app.editor().mode() == crate::kernel::mode::Mode::Command;
        if is_command_mode {
            if let Some(raw_key) = crate::app::input::translate_raw(&ev) {
                let outcome = app.handle_raw_key(raw_key);
                if let Some(crate::app::request::AppRequest::Quit) = app.take_request() {
                    return Ok(());
                }
                status = format!(
                    "-- {:?} -- mutated: {} invalidation: {:?} events: {}",
                    app.editor().mode(),
                    outcome.mutated,
                    outcome.invalidation,
                    outcome.events.len()
                );
            }
        } else {
            if let Some(resolved) = input.translate(ev) {
                let action_desc = format!("{:?}", resolved.action);
                let outcome = app.handle_action(resolved.action);
                status = format!(
                    "-- {:?} -- last: {action_desc} -- mutated: {} invalidation: {:?} events: {}",
                    app.editor().mode(),
                    outcome.mutated,
                    outcome.invalidation,
                    outcome.events.len()
                );
            } else {
                status = format!("-- {:?} -- last: (unresolved key)", app.editor().mode());
            }
        }

        let prompt_opt = if app.editor().mode() == crate::kernel::mode::Mode::Command {
            Some(app.prompt().text())
        } else {
            None
        };
        view::render(out, app.editor(), &status, prompt_opt, screen)?;
    }
}
