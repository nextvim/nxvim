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
    if args.headless {
        let mut app = app::App::open(&args.paths);
        app.init(
            &args.pre_config_cmds,
            &args.post_config_cmds,
            &args.scripts,
            args.skip_config,
        );
        let exit_code = runtime::run_headless(&mut app)?;
        std::process::exit(exit_code);
    } else {
        let mut session = terminal::TerminalSession::enter()?;
        let mut app = app::App::open(&args.paths);
        app.init(
            &args.pre_config_cmds,
            &args.post_config_cmds,
            &args.scripts,
            args.skip_config,
        );
        let result = runtime::run(&mut app, &session, &mut io::stdout());
        session.restore()?;
        result
    }
}
