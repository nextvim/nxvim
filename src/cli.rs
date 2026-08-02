use std::{ffi::OsString, fmt, fs, path::PathBuf};

use crate::{EditorError, HeadlessEditor};

#[derive(Debug)]
pub enum CliError {
    Usage(String),
    Io {
        path: PathBuf,
        error: std::io::Error,
    },
    Editor {
        source: String,
        error: EditorError,
    },
}

impl fmt::Display for CliError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Usage(message) => formatter.write_str(message),
            Self::Io { path, error } => write!(formatter, "{}: {error}", path.display()),
            Self::Editor { source, error } => write!(formatter, "{source}: {error}"),
        }
    }
}

impl std::error::Error for CliError {}

#[derive(Debug, Default, PartialEq, Eq)]
struct CliPlan {
    clean: bool,
    before_files: Vec<String>,
    files: Vec<PathBuf>,
    source_files: Vec<PathBuf>,
    after_files: Vec<String>,
}

pub fn run_cli<I>(arguments: I) -> Result<u8, CliError>
where
    I: IntoIterator<Item = OsString>,
{
    let plan = parse_arguments(arguments)?;
    execute(plan)
}

fn parse_arguments<I>(arguments: I) -> Result<CliPlan, CliError>
where
    I: IntoIterator<Item = OsString>,
{
    let mut plan = CliPlan::default();
    let mut arguments = arguments.into_iter();
    let mut options = true;
    while let Some(argument) = arguments.next() {
        if options && argument == "--" {
            options = false;
        } else if options && argument == "--clean" {
            plan.clean = true;
        } else if options && (argument == "--cmd" || argument == "-c" || argument == "-S") {
            let value = arguments.next().ok_or_else(|| {
                CliError::Usage(format!(
                    "option {} requires an argument",
                    argument.to_string_lossy()
                ))
            })?;
            if argument == "--cmd" {
                plan.before_files.push(value.to_string_lossy().into_owned());
            } else if argument == "-c" {
                plan.after_files.push(value.to_string_lossy().into_owned());
            } else {
                plan.source_files.push(PathBuf::from(value));
            }
        } else if options && argument.to_string_lossy().starts_with('-') {
            return Err(CliError::Usage(format!(
                "unknown option: {}",
                argument.to_string_lossy()
            )));
        } else {
            plan.files.push(PathBuf::from(argument));
        }
    }
    Ok(plan)
}

fn execute(plan: CliPlan) -> Result<u8, CliError> {
    let mut editor = HeadlessEditor::new().map_err(|error| CliError::Editor {
        source: "startup".into(),
        error,
    })?;

    for (index, command) in plan.before_files.iter().enumerate() {
        evaluate(&mut editor, format!("--cmd #{}", index + 1), command)?;
        if editor.exit_requested().map_err(state_error)? {
            return requested_code(&editor);
        }
    }

    // Startup discovery is intentionally deferred; --clean already guarantees no startup files.
    let mut first_file_buffer = None;
    for path in &plan.files {
        evaluate(
            &mut editor,
            path.display().to_string(),
            &format!(":edit {}", path.display()),
        )?;
        first_file_buffer.get_or_insert(editor.current_buffer().map_err(state_error)?);
    }
    if let Some(buffer) = first_file_buffer {
        evaluate(
            &mut editor,
            "file argument selection".into(),
            &format!(":buffer {}", buffer.get()),
        )?;
    }

    for path in &plan.source_files {
        let source = fs::read_to_string(path).map_err(|error| CliError::Io {
            path: path.clone(),
            error,
        })?;
        evaluate(&mut editor, path.display().to_string(), &source)?;
        if editor.exit_requested().map_err(state_error)? {
            return requested_code(&editor);
        }
    }
    for (index, command) in plan.after_files.iter().enumerate() {
        evaluate(&mut editor, format!("-c #{}", index + 1), command)?;
        if editor.exit_requested().map_err(state_error)? {
            return requested_code(&editor);
        }
    }

    Ok(0)
}

fn evaluate(editor: &mut HeadlessEditor, source: String, script: &str) -> Result<(), CliError> {
    editor
        .eval(source.clone(), script)
        .map(|_| ())
        .map_err(|error| CliError::Editor { source, error })
}

fn state_error(error: EditorError) -> CliError {
    CliError::Editor {
        source: "editor state".into(),
        error,
    }
}

fn requested_code(editor: &HeadlessEditor) -> Result<u8, CliError> {
    editor
        .requested_exit_code()
        .map_err(state_error)?
        .ok_or_else(|| CliError::Usage("exit requested without a status".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    #[test]
    fn parser_preserves_order_within_cli_stages() {
        let plan = parse_arguments(args(&[
            "--clean",
            "--cmd",
            "let g:a = 1",
            "one",
            "-S",
            "setup.vim",
            "-c",
            "quit",
            "two",
            "-c",
            "cquit 7",
        ]))
        .unwrap();

        assert!(plan.clean);
        assert_eq!(plan.before_files, ["let g:a = 1"]);
        assert_eq!(plan.files, [PathBuf::from("one"), PathBuf::from("two")]);
        assert_eq!(plan.source_files, [PathBuf::from("setup.vim")]);
        assert_eq!(plan.after_files, ["quit", "cquit 7"]);
    }

    #[test]
    fn double_dash_allows_dash_prefixed_file_names() {
        let plan = parse_arguments(args(&["--", "-notes"])).unwrap();
        assert_eq!(plan.files, [PathBuf::from("-notes")]);
    }

    #[test]
    fn cquit_controls_process_status() {
        assert_eq!(run_cli(args(&["--clean", "-c", "cquit 23"])).unwrap(), 23);
    }

    #[test]
    fn malformed_options_are_errors_not_panics() {
        assert!(matches!(run_cli(args(&["--cmd"])), Err(CliError::Usage(_))));
        assert!(matches!(
            run_cli(args(&["--unknown"])),
            Err(CliError::Usage(_))
        ));
    }
}
