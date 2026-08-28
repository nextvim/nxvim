//! Event loop: poll a terminal event, translate it, hand it to `App`, render.
//!
//! Sequencing only — no semantics. `Editor::execute()` runs synchronously
//! inside `App::handle_action`; the redraw happens after it returns, never
//! mid-command (`RESCUE.md` Rule 4.7 / `docs/VIM.md` lesson #3).

use std::io::{self, Write};
use std::time::Duration;

use crossterm::event::{self, Event};

use crate::app::{App, input::InputTranslator};
use crate::kernel::outcome::RedrawInvalidation;
use crate::view;

pub fn run(
    app: &mut App,
    session: &crate::terminal::TerminalSession,
    out: &mut impl Write,
) -> io::Result<()> {
    let mut screen = session.size().unwrap_or(vim_ui::Rect::new(0, 0, 80, 24));
    let mut input = InputTranslator::with_mappings(app.shared_keymaps());
    let mut render_state = view::RenderState::new();
    // Temporary debug status (mode + last resolved action), see `view::render`.
    let mut status = format!("-- {:?} -- last: (none)", app.editor().mode());
    // Invalidations accumulated since the last frame was flushed, and
    // whether the next frame must repaint unconditionally (forced by a
    // resize). The very first frame always renders fully.
    let mut pending_invalidations: Vec<RedrawInvalidation> = Vec::new();
    let mut force_full = true;

    let prompt_opt = if app.editor().mode().is_command() {
        Some(app.prompt().text().to_string())
    } else {
        None
    };
    view::render(
        out,
        app.editor_mut(),
        &mut render_state,
        &status,
        prompt_opt.as_deref(),
        screen,
        &pending_invalidations,
        force_full,
    )?;
    pending_invalidations.clear();
    force_full = false;

    loop {
        if !event::poll(Duration::from_millis(50))? {
            continue;
        }
        let ev = event::read()?;

        if let Event::Resize(columns, rows) = ev {
            screen = vim_ui::Rect::new(0, 0, columns, rows);
            force_full = true;
            let prompt_opt = if app.editor().mode().is_command() {
                Some(app.prompt().text().to_string())
            } else {
                None
            };
            view::render(
                out,
                app.editor_mut(),
                &mut render_state,
                &status,
                prompt_opt.as_deref(),
                screen,
                &pending_invalidations,
                force_full,
            )?;
            pending_invalidations.clear();
            force_full = false;
            continue;
        }

        let is_command_mode = app.editor().mode().is_command();
        if is_command_mode {
            if let Some(raw_key) = crate::app::input::translate_raw(&ev) {
                let outcome = app.handle_raw_key(raw_key);
                status = format!(
                    "-- {:?} -- mutated: {} invalidation: {:?} events: {}",
                    app.editor().mode(),
                    outcome.mutated,
                    outcome.invalidation,
                    outcome.events.len()
                );
                if outcome.invalidation != RedrawInvalidation::None {
                    pending_invalidations.push(outcome.invalidation);
                }
            }
        } else {
            let buf_id = app.editor().current_context().buffer.get();
            if let Some(resolved) = input.translate_with_buffer(ev, Some(buf_id)) {
                let action_desc = format!("{:?}", resolved.action);
                let outcome = app.handle_action(resolved.action, resolved.register);
                if outcome.invalidation != RedrawInvalidation::None {
                    pending_invalidations.push(outcome.invalidation);
                }
                // status = format!(
                //     "-- {:?} -- last: {action_desc} -- mutated: {} invalidation: {:?} events: {}",
                //     app.editor().mode(),
                //     outcome.mutated,
                //     outcome.invalidation,
                //     outcome.events.len()
                // );
            }
            // else {
            status = format!("-- {:?} -- last: (unresolved key)", app.editor().mode());
            // }
            status = format!("-- {:?} -- last: (unresolved key)", app.editor().mode());
        }

        if let Some(request) = app.take_request() {
            match request {
                crate::app::request::AppRequest::Quit => return Ok(()),
                crate::app::request::AppRequest::ShowMessage(msg) => {
                    status = msg;
                }
            }
        }

        let prompt_opt = if app.editor().mode().is_command() {
            Some(app.prompt().text().to_string())
        } else {
            None
        };
        view::render(
            out,
            app.editor_mut(),
            &mut render_state,
            &status,
            prompt_opt.as_deref(),
            screen,
            &pending_invalidations,
            force_full,
        )?;
        pending_invalidations.clear();
        force_full = false;
    }
}
