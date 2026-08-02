use std::{
    borrow::Cow,
    io::{self, IsTerminal, Write},
    path::Path,
    process::ExitCode,
};

use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute, queue,
    style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor},
    terminal::{
        self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode,
        enable_raw_mode,
    },
};
use nxvim::HeadlessEditor;
use vim_formatter::{
    CompiledFormat, ExprId, FormatDialect, FormatResolver, RenderItem, StyleId, parse,
};

const STATUS_FORMAT: &str =
    "%#Mode# EX %#File#%f %#Modified#%m%#StatusLine#%=%#Message#%{message}%#Metadata# [%n] ";

fn main() -> ExitCode {
    let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
    if arguments.is_empty() && io::stdin().is_terminal() && io::stdout().is_terminal() {
        return match InteractiveEditor::run() {
            Ok(code) => ExitCode::from(code),
            Err(error) => {
                eprintln!("nxvim: {error}");
                ExitCode::FAILURE
            }
        };
    }

    match nxvim::run_cli(arguments) {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("nxvim: {error}");
            ExitCode::FAILURE
        }
    }
}

struct TerminalGuard;

impl TerminalGuard {
    fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        if let Err(error) = execute!(io::stdout(), EnterAlternateScreen, Hide) {
            let _ = disable_raw_mode();
            return Err(error);
        }
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = execute!(io::stdout(), Show, LeaveAlternateScreen);
        let _ = disable_raw_mode();
    }
}

struct InteractiveEditor {
    editor: HeadlessEditor,
    command: String,
    message: String,
    history: Vec<String>,
    history_index: Option<usize>,
    status_format: CompiledFormat,
}

impl InteractiveEditor {
    fn run() -> Result<u8, Box<dyn std::error::Error>> {
        let _terminal = TerminalGuard::enter()?;
        let mut application = Self {
            editor: HeadlessEditor::new()?,
            command: String::new(),
            message: "nxvim — type :quit to exit".into(),
            history: Vec::new(),
            history_index: None,
            status_format: compile_status_format()?,
        };

        loop {
            application.draw()?;
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    application.handle_key(key)?
                }
                Event::Resize(_, _) => {}
                _ => continue,
            }
            if let Some(code) = application.editor.requested_exit_code()? {
                return Ok(code);
            }
        }
    }

    fn handle_key(&mut self, key: KeyEvent) -> Result<(), Box<dyn std::error::Error>> {
        match (key.code, key.modifiers) {
            (KeyCode::Char('c'), modifiers) if modifiers.contains(KeyModifiers::CONTROL) => {
                self.editor.eval("<keyboard>", ":cquit 130")?;
            }
            (KeyCode::Enter, _) => self.execute_command(),
            (KeyCode::Backspace, _) => {
                self.command.pop();
                self.history_index = None;
            }
            (KeyCode::Esc, _) => {
                self.command.clear();
                self.history_index = None;
                self.message.clear();
            }
            (KeyCode::Up, _) => self.older_command(),
            (KeyCode::Down, _) => self.newer_command(),
            (KeyCode::Char(character), modifiers)
                if !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.command.push(character);
                self.history_index = None;
            }
            _ => {}
        }
        Ok(())
    }

    fn execute_command(&mut self) {
        let command = self.command.trim().trim_start_matches(':').to_owned();
        self.command.clear();
        self.history_index = None;
        if command.is_empty() {
            return;
        }
        if self.history.last() != Some(&command) {
            self.history.push(command.clone());
        }
        match self.editor.eval("<command-line>", &command) {
            Ok(_) => self.message.clear(),
            Err(error) => self.message = format!("{error}"),
        }
    }

    fn older_command(&mut self) {
        if self.history.is_empty() {
            return;
        }
        let index = self
            .history_index
            .map_or(self.history.len() - 1, |index| index.saturating_sub(1));
        self.history_index = Some(index);
        self.command.clone_from(&self.history[index]);
    }

    fn newer_command(&mut self) {
        let Some(index) = self.history_index else {
            return;
        };
        if index + 1 < self.history.len() {
            self.history_index = Some(index + 1);
            self.command.clone_from(&self.history[index + 1]);
        } else {
            self.history_index = None;
            self.command.clear();
        }
    }

    fn draw(&self) -> io::Result<()> {
        let (width, height) = terminal::size()?;
        let mut stdout = io::stdout().lock();
        queue!(stdout, Hide, MoveTo(0, 0), Clear(ClearType::All))?;

        let content_rows = height.saturating_sub(2);
        let text = self.editor.current_text().map_err(io::Error::other)?;
        for (row, line) in text.split('\n').take(content_rows as usize).enumerate() {
            queue!(stdout, MoveTo(0, row as u16), Print(clipped(line, width)))?;
        }
        let line_count = text.split('\n').count().min(content_rows as usize);
        for row in line_count..content_rows as usize {
            queue!(stdout, MoveTo(0, row as u16), Print("~"))?;
        }

        if height >= 2 {
            let context = StatusContext {
                file_name: self
                    .editor
                    .current_buffer_name()
                    .map_err(io::Error::other)?
                    .unwrap_or_else(|| "[No Name]".into()),
                message: &self.message,
                total_lines: text.split('\n').count(),
                buffer_number: self
                    .editor
                    .current_buffer()
                    .map_err(io::Error::other)?
                    .get() as usize,
                modified: self
                    .editor
                    .current_buffer_modified()
                    .map_err(io::Error::other)?,
            };
            let status = self
                .status_format
                .render(&context, width as usize)
                .map_err(io::Error::other)?;
            queue!(stdout, MoveTo(0, height - 2))?;
            render_statusline(&mut stdout, &status)?;
            queue!(stdout, ResetColor)?;
        }

        if height > 0 {
            let prompt = format!(":{}", self.command);
            let visible = clipped_from_end(&prompt, width);
            let cursor_column = visible.chars().count().min(width as usize) as u16;
            queue!(
                stdout,
                MoveTo(0, height - 1),
                Print(padded(&visible, width)),
                MoveTo(cursor_column.min(width.saturating_sub(1)), height - 1),
                Show
            )?;
        }
        stdout.flush()
    }
}

struct StatusContext<'a> {
    file_name: String,
    message: &'a str,
    total_lines: usize,
    buffer_number: usize,
    modified: bool,
}

impl FormatResolver for StatusContext<'_> {
    fn file_name(&self) -> Cow<'_, str> {
        Path::new(&self.file_name)
            .file_name()
            .map_or_else(|| Cow::Borrowed("[No Name]"), |name| name.to_string_lossy())
    }

    fn total_lines(&self) -> usize {
        self.total_lines
    }

    fn buffer_number(&self) -> usize {
        self.buffer_number
    }

    fn is_modified(&self) -> bool {
        self.modified
    }

    fn resolve_highlight(&self, name: &str) -> Option<StyleId> {
        match name {
            "StatusLine" => Some(StyleId(1)),
            "Mode" => Some(StyleId(2)),
            "File" => Some(StyleId(3)),
            "Modified" => Some(StyleId(4)),
            "Message" => Some(StyleId(5)),
            "Metadata" => Some(StyleId(6)),
            _ => None,
        }
    }

    fn eval_expression(&self, _id: ExprId, source: &str) -> Cow<'_, str> {
        match source {
            "message" => Cow::Borrowed(self.message),
            _ => Cow::Borrowed(""),
        }
    }
}

fn compile_status_format() -> Result<CompiledFormat, Box<dyn std::error::Error>> {
    let ast = parse(STATUS_FORMAT, FormatDialect::StatusLine)?;
    Ok(CompiledFormat::compile(&ast)?)
}

fn render_statusline(output: &mut impl Write, items: &[RenderItem<'_>]) -> io::Result<()> {
    for item in items {
        if let RenderItem::Text { text, style } = item {
            let (foreground, background) = status_colors(*style);
            queue!(
                output,
                SetForegroundColor(foreground),
                SetBackgroundColor(background),
                Print(text)
            )?;
        }
    }
    Ok(())
}

fn status_colors(style: Option<StyleId>) -> (Color, Color) {
    match style {
        Some(StyleId(2)) => (Color::Black, Color::Cyan),
        Some(StyleId(3)) => (Color::White, Color::DarkGrey),
        Some(StyleId(4)) => (Color::Yellow, Color::DarkGrey),
        Some(StyleId(5)) => (Color::Red, Color::DarkGrey),
        Some(StyleId(6)) => (Color::Grey, Color::DarkGrey),
        _ => (Color::White, Color::DarkGrey),
    }
}

fn clipped(value: &str, width: u16) -> String {
    value.chars().take(width as usize).collect()
}

fn clipped_from_end(value: &str, width: u16) -> String {
    let characters = value.chars().collect::<Vec<_>>();
    let start = characters.len().saturating_sub(width as usize);
    characters[start..].iter().collect()
}

fn padded(value: &str, width: u16) -> String {
    let mut value = clipped(value, width);
    let used = value.chars().count();
    value.extend(std::iter::repeat_n(' ', width as usize - used));
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screen_text_is_clipped_without_splitting_unicode() {
        assert_eq!(clipped("aé🙂z", 3), "aé🙂");
        assert_eq!(clipped_from_end(":abcdef", 4), "cdef");
        assert_eq!(padded("é", 3), "é  ");
    }

    #[test]
    fn statusline_is_laid_out_by_vim_formatter() {
        let format = compile_status_format().unwrap();
        let context = StatusContext {
            file_name: "/tmp/example.txt".into(),
            message: "ready",
            total_lines: 12,
            buffer_number: 3,
            modified: true,
        };
        let rendered = format.render(&context, 40).unwrap();
        let text = rendered
            .iter()
            .filter_map(|item| match item {
                RenderItem::Text { text, .. } => Some(text.as_ref()),
                _ => None,
            })
            .collect::<String>();

        assert_eq!(text.chars().count(), 40);
        assert!(
            text.contains("EX example.txt [+]"),
            "rendered statusline: {text:?}"
        );
        assert!(text.contains("ready [3]"), "rendered statusline: {text:?}");
    }

    #[test]
    fn interactive_command_executes_script_statements_without_forcing_ex_syntax() {
        let mut application = InteractiveEditor {
            editor: HeadlessEditor::new().unwrap(),
            command: "let g:status = await setline(1, 'hello')".into(),
            message: String::new(),
            history: Vec::new(),
            history_index: None,
            status_format: compile_status_format().unwrap(),
        };

        application.execute_command();

        assert_eq!(application.editor.current_text().unwrap(), "hello");
        assert!(application.message.is_empty());
    }
}
