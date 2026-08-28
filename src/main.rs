//! NxVim entry point.
//!
//! The previous implementation lives in `src_/` as reference material only
//! (see `src/RESCUE.md`). It is intentionally excluded from the build.

mod app;
mod kernel;
mod runtime;
mod terminal;
mod view;

use std::io;

/// No file loading yet in this milestone, so the editor starts on an
/// in-memory placeholder buffer with enough lines/columns to make `h/j/k/l`
/// visibly testable (an empty buffer has nowhere for a motion to go).
const PLACEHOLDER_TEXT: &str = "NxVim skeleton\n\nh/j/k/l move the cursor.\ni enters Insert mode, Esc returns to Normal.\nCtrl-C quits (no :q yet).\n";

fn main() -> io::Result<()> {
    let mut session = terminal::TerminalSession::enter()?;
    let mut app = app::App::new(PLACEHOLDER_TEXT);
    let result = runtime::run(&mut app, &session, &mut io::stdout());
    session.restore()?;
    result
}
