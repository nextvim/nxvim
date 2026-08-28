//! Event loop: poll a terminal event, translate it, hand it to `App`, render.
//!
//! Sequencing only — no semantics. `Editor::execute()` runs synchronously
//! inside `App::handle_action`; the redraw happens after it returns, never
//! mid-command (`RESCUE.md` Rule 4.7 / `docs/VIM.md` lesson #3).

use std::io::{self, Write};
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyModifiers};

use crate::app::{App, input::InputTranslator};
use crate::view;

pub fn run(app: &mut App, out: &mut impl Write) -> io::Result<()> {
    let mut input = InputTranslator::new();
    // Temporary debug status (mode + last resolved action), see `view::render`.
    let mut status = format!("-- {:?} -- last: (none)", app.editor().mode());
    view::render(out, app.editor(), &status)?;

    loop {
        if !event::poll(Duration::from_millis(50))? {
            continue;
        }
        let ev = event::read()?;

        // Temporary escape hatch until `:q` exists (no Ex commands in this
        // milestone) — not a Vim command, just how a manual smoke test exits.
        if let Event::Key(key) = &ev
            && key.modifiers.contains(KeyModifiers::CONTROL)
            && key.code == KeyCode::Char('c')
        {
            return Ok(());
        }

        if matches!(ev, Event::Resize(_, _)) {
            view::render(out, app.editor(), &status)?;
            continue;
        }

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
        view::render(out, app.editor(), &status)?;
    }
}
