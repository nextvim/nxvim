//! NxVim entry point.
//!
//! The previous implementation lives in `src_/` as reference material only
//! (see `src/RESCUE.md`). It is intentionally excluded from the build.

mod app;
mod kernel;
mod runtime;
mod script;
mod services;
mod terminal;
mod view;

use std::io;

#[macro_use]
extern crate nxvim_log;

fn main() -> io::Result<()> {
    let args = app::args::Args::parse();
    let mut session = terminal::TerminalSession::enter()?;
    let mut app = app::App::open(&args.paths);
    app.init(&args.pre_config_cmds, &args.post_config_cmds, &args.scripts);
    let result = runtime::run(&mut app, &session, &mut io::stdout());
    session.restore()?;
    result
}
