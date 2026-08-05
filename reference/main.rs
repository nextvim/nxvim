use std::{env, error::Error, io, path::PathBuf, process::ExitCode};

use nxvim::{Application, CrosstermEventSource, Editor, TerminalPresenter, TerminalSession};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("nxvim: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let path = startup_path()?;
    let mut terminal = TerminalSession::enter()?;
    let size = terminal.size()?;
    let editor = match path {
        Some(path) => Editor::open(path, size)?,
        None => Editor::new(size)?,
    };
    let presenter = TerminalPresenter::new(size);
    let mut application = Application::new(editor, CrosstermEventSource, presenter);
    let run_result = application.run();
    let restore_result = terminal.restore();

    run_result?;
    restore_result?;
    Ok(())
}

fn startup_path() -> io::Result<Option<PathBuf>> {
    let mut arguments = env::args_os().skip(1);
    let path = arguments.next().map(PathBuf::from);
    if arguments.next().is_some() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "expected at most one file path",
        ));
    }
    Ok(path)
}
