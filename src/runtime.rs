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
use crate::view::RenderState;

pub fn run(
    app: &mut App,
    session: &crate::terminal::TerminalSession,
    out: &mut impl Write,
) -> io::Result<()> {
    let mut screen = session.size().unwrap_or(vim_ui::Rect::new(0, 0, 80, 24));
    let mut input = InputTranslator::with_mappings(app.shared_keymaps());
    let mut render_state = RenderState::new();
    // Temporary debug status (mode + last resolved action), see `view::render`.
    let mut status = String::new();
    // Invalidations accumulated since the last frame was flushed, and
    // whether the next frame must repaint unconditionally (forced by a
    // resize). The very first frame always renders fully.
    let mut pending_invalidations: Vec<RedrawInvalidation> = Vec::new();
    let mut force_full = true;

    let mut last_command_time = std::time::Instant::now();
    let mut is_idle = false;
    let mut idle_since: Option<std::time::Instant> = None;

    let prompt_opt = if app.editor().mode().is_command() {
        Some(app.prompt().text().to_string())
    } else {
        None
    };
    app.render(
        out,
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
            let service_outcome = app.poll_services(&mut render_state);
            if service_outcome.invalidation != RedrawInvalidation::None {
                pending_invalidations.push(service_outcome.invalidation);
            }
            if !pending_invalidations.is_empty() || render_state.advance_idle() {
                let prompt_opt = if app.editor().mode().is_command() {
                    Some(app.prompt().text().to_string())
                } else {
                    None
                };
                app.render(
                    out,
                    &mut render_state,
                    &status,
                    prompt_opt.as_deref(),
                    screen,
                    &pending_invalidations,
                    false,
                )?;
                pending_invalidations.clear();
            }
            continue;
        }
        render_state.note_interaction();
        let ev = event::read()?;
        status = String::new();

        if let Event::Resize(columns, rows) = ev {
            screen = vim_ui::Rect::new(0, 0, columns, rows);
            force_full = true;
            let prompt_opt = if app.editor().mode().is_command() {
                Some(app.prompt().text().to_string())
            } else {
                None
            };
            app.render(
                out,
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
                if outcome.invalidation != RedrawInvalidation::None {
                    pending_invalidations.push(outcome.invalidation);
                }
            }
        } else {
            let buf_id = app.editor().current_context().buffer.get();
            if let Some(resolved) = input.translate_with_buffer(ev, Some(buf_id), app.digraphs()) {
                let outcome = app.handle_action(resolved.action, resolved.register);
                if outcome.invalidation != RedrawInvalidation::None {
                    pending_invalidations.push(outcome.invalidation);
                }
            }
        }

        let service_outcome = app.poll_services(&mut render_state);
        if service_outcome.invalidation != RedrawInvalidation::None {
            pending_invalidations.push(service_outcome.invalidation);
        }

        if let Some(request) = app.take_request() {
            match request {
                crate::app::request::AppRequest::Quit => return Ok(()),
                crate::app::request::AppRequest::ShowMessage(msg) => {
                    status = msg;
                }
                crate::app::request::AppRequest::ExecuteEx(cmd) => {
                    let outcome = app.execute_ex_command(cmd);
                    if outcome.invalidation != RedrawInvalidation::None {
                        pending_invalidations.push(outcome.invalidation);
                    }
                }
                crate::app::request::AppRequest::Source(path) => {
                    let outcome = app.execute_source(&path);
                    if outcome.invalidation != RedrawInvalidation::None {
                        pending_invalidations.push(outcome.invalidation);
                    }
                }
            }
        }

        let prompt_opt = if app.editor().mode().is_command() {
            Some(app.prompt().text().to_string())
        } else {
            None
        };

        app.render(
            out,
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
