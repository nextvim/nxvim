//! NxVim entry point.
//!
//! The previous implementation lives in `src_/` as reference material only
//! (see `src/RESCUE.md`). It is intentionally excluded from the build.

mod app;
mod kernel;
mod runtime;
mod script;
mod terminal;
mod view;

use std::io;

fn main() -> io::Result<()> {
    let args = app::args::Args::parse();
    let mut session = terminal::TerminalSession::enter()?;
    let mut app = app::App::open(&args.paths);
    let result = runtime::run(&mut app, &session, &mut io::stdout());
    session.restore()?;
    result
}
