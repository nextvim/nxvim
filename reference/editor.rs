use std::{error::Error, fmt, path::Path};

use vim_buffer::{BufferError, BufferId, BufferManager, BufferSnapshot, Mutator, SelectionSet};
use vim_input::{Action, KeyCode, Keymap, Mode, Modifiers, ResolveOutcome, Resolver};

use crate::{
    AppEvent, Document, DocumentCursor, EditorCommand, Globals, ScreenSize, ScriptRuntime,
};

pub struct Editor {
    buffers: BufferManager,
    #[allow(dead_code)]
    mutator: Mutator,
    document: Document,
    input: InputState,
    globals: Globals,
    scripts: ScriptRuntime,
    message: Option<Message>,
    screen: ScreenSize,
    lifecycle: Lifecycle,
}

struct InputState {
    mode: Mode,
    keymap: Keymap,
    resolver: Resolver,
    command_line: String,
}

pub type Cursor = DocumentCursor;

#[derive(Clone)]
pub struct EditorFrame {
    pub buffer_id: BufferId,
    pub snapshot: BufferSnapshot,
    pub selections: SelectionSet,
    pub cursor: Cursor,
    pub scroll_row: usize,
    pub scroll_col: usize,
    pub mode: Mode,
    pub name: String,
    pub screen: ScreenSize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub kind: MessageKind,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageKind {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lifecycle {
    Running,
    ExitRequested,
}

impl Editor {
    pub fn new(screen: ScreenSize) -> Result<Self, EditorError> {
        let mut buffers = BufferManager::new();
        let buffer_id = buffers.create("").id();
        buffers.set_current(buffer_id)?;
        Self::from_parts(buffers, buffer_id, screen)
    }

    pub fn open(path: impl AsRef<Path>, screen: ScreenSize) -> Result<Self, EditorError> {
        let mut buffers = BufferManager::new();
        let (buffer_id, _) = buffers.load(path)?;
        buffers.set_current(buffer_id)?;
        Self::from_parts(buffers, buffer_id, screen)
    }

    fn from_parts(
        buffers: BufferManager,
        buffer_id: BufferId,
        screen: ScreenSize,
    ) -> Result<Self, EditorError> {
        let document = Document::new(buffer_id, &buffers)?;
        Ok(Self {
            buffers,
            mutator: Mutator::default(),
            document,
            input: InputState {
                mode: Mode::Normal,
                keymap: Keymap::new(),
                resolver: Resolver::new(Mode::Normal),
                command_line: String::new(),
            },
            globals: Globals::nxvim_defaults(),
            scripts: ScriptRuntime::new(),
            message: None,
            screen,
            lifecycle: Lifecycle::Running,
        })
    }

    pub fn document(&self) -> &Document {
        &self.document
    }

    pub fn buffer_count(&self) -> usize {
        self.buffers.list().len()
    }

    pub const fn mode(&self) -> Mode {
        self.input.mode
    }

    pub const fn resolver_mode(&self) -> Mode {
        self.input.resolver.mode()
    }

    pub fn globals(&self) -> &Globals {
        &self.globals
    }

    pub fn selections(&self) -> &SelectionSet {
        self.document.selections()
    }

    pub fn cursor(&self) -> Result<Cursor, EditorError> {
        Ok(self.document.cursor(&self.buffers)?)
    }

    pub const fn screen(&self) -> ScreenSize {
        self.screen
    }

    pub const fn lifecycle(&self) -> Lifecycle {
        self.lifecycle
    }

    pub const fn is_running(&self) -> bool {
        matches!(self.lifecycle, Lifecycle::Running)
    }

    pub fn message(&self) -> Option<&Message> {
        self.message.as_ref()
    }

    pub fn scripts(&self) -> &ScriptRuntime {
        &self.scripts
    }

    pub fn frame(&self) -> Result<EditorFrame, EditorError> {
        let document = self.document.frame(&self.buffers)?;
        let buffer = self.buffers.get(document.buffer_id)?;
        let name = buffer
            .path()
            .and_then(Path::file_name)
            .and_then(|name| name.to_str())
            .unwrap_or("[No Name]")
            .to_owned();
        Ok(EditorFrame {
            buffer_id: document.buffer_id,
            snapshot: document.snapshot,
            selections: document.selections,
            cursor: document.cursor,
            scroll_row: document.scroll_row,
            scroll_col: document.scroll_col,
            mode: self.input.mode,
            name,
            screen: self.screen,
        })
    }

    pub fn handle_event(&mut self, event: AppEvent) -> Result<(), EditorError> {
        match event {
            AppEvent::Resize(size) => {
                self.screen = size;
                self.ensure_cursor_visible()?;
            }
            AppEvent::EndOfInput => self.lifecycle = Lifecycle::ExitRequested,
            AppEvent::Tick => {}
            AppEvent::Key(key) if key.code == KeyCode::Escape => {
                self.input.resolver.reset();
                self.set_mode(Mode::Normal);
            }
            AppEvent::Key(key)
                if key.code == KeyCode::Char('c') && key.modifiers.contains(Modifiers::CONTROL) =>
            {
                self.lifecycle = Lifecycle::ExitRequested;
            }
            AppEvent::Key(key) => match self.input.resolver.feed(key, &self.input.keymap) {
                ResolveOutcome::Resolved(resolved) => self.apply_action(resolved.action)?,
                ResolveOutcome::Pending | ResolveOutcome::Ignored => {}
                ResolveOutcome::Invalid(_) => self.unsupported_action(),
            },
        }
        Ok(())
    }

    pub fn apply_action(&mut self, action: Action) -> Result<(), EditorError> {
        let viewport_rows = self.screen.rows.saturating_sub(1) as usize;
        if self
            .document
            .apply_action(&action, &self.buffers, viewport_rows)?
        {
            return Ok(());
        }

        match action {
            Action::Quit => self.lifecycle = Lifecycle::ExitRequested,
            Action::SetToNormal => self.set_mode(Mode::Normal),
            Action::SetToInsert | Action::SetToAppend | Action::SetToAppendEndOfLine => {
                self.set_mode(Mode::Insert);
            }
            Action::SetToVisual => self.set_mode(Mode::Visual),
            Action::SetToVisualLine => self.set_mode(Mode::VisualLine),
            Action::SetToVisualBlock => self.set_mode(Mode::VisualBlock),
            Action::SetToCommand
            | Action::SetToCommandSearchForward
            | Action::SetToCommandSearchBackward => self.set_mode(Mode::Command),
            Action::NoOp | Action::Clear => {}
            _ => self.unsupported_action(),
        }
        Ok(())
    }

    pub fn apply_host_command(&mut self, command: EditorCommand) -> Result<(), EditorError> {
        match command {
            EditorCommand::Quit => self.lifecycle = Lifecycle::ExitRequested,
            EditorCommand::NewBuffer => self.create_buffer()?,
        }
        Ok(())
    }

    pub fn take_script_commands(&self) -> Vec<EditorCommand> {
        std::iter::from_fn(|| self.scripts.try_next_command()).collect()
    }

    fn create_buffer(&mut self) -> Result<(), EditorError> {
        let buffer_id = self.buffers.create("").id();
        self.buffers.set_current(buffer_id)?;
        self.document = Document::new(buffer_id, &self.buffers)?;
        Ok(())
    }

    fn ensure_cursor_visible(&mut self) -> Result<(), EditorError> {
        let viewport_rows = self.screen.rows.saturating_sub(1) as usize;
        self.document
            .ensure_cursor_visible(&self.buffers, viewport_rows)?;
        Ok(())
    }

    fn set_mode(&mut self, mode: Mode) {
        self.input.mode = mode;
        self.input.resolver.set_mode(mode);
        if mode != Mode::Command {
            self.input.command_line.clear();
        }
    }

    fn unsupported_action(&mut self) {
        self.message = Some(Message {
            kind: MessageKind::Info,
            text: "action is not available in phase 2".to_owned(),
        });
    }
}

#[derive(Debug)]
pub enum EditorError {
    Buffer(BufferError),
}

impl fmt::Display for EditorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Buffer(error) => write!(formatter, "buffer error: {error}"),
        }
    }
}

impl Error for EditorError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Buffer(error) => Some(error),
        }
    }
}

impl From<BufferError> for EditorError {
    fn from(error: BufferError) -> Self {
        Self::Buffer(error)
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };
    use vim_input::{Key, KeyCode, Modifiers};

    use super::*;

    fn editor() -> Editor {
        Editor::new(ScreenSize::new(80, 24)).unwrap()
    }

    fn editor_with_text(text: &str) -> Editor {
        let path = std::env::temp_dir().join(format!(
            "nxvim-document-{}-{}.txt",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::write(&path, text).unwrap();
        let editor = Editor::open(&path, ScreenSize::new(80, 4)).unwrap();
        fs::remove_file(path).unwrap();
        editor
    }

    #[test]
    fn construction_creates_one_document_with_a_primary_selection() {
        let editor = editor();
        assert_eq!(editor.buffer_count(), 1);
        assert_eq!(
            editor.buffers.current(),
            Some(editor.document().buffer_id())
        );
        assert_eq!(editor.selections().len(), 1);
        assert!(!editor.selections().is_empty());
    }

    #[test]
    fn mode_and_resolver_stay_synchronized() {
        let mut editor = editor();
        editor.apply_action(Action::SetToInsert).unwrap();
        assert_eq!(editor.mode(), Mode::Insert);
        assert_eq!(editor.resolver_mode(), Mode::Insert);
    }

    #[test]
    fn new_buffer_command_replaces_the_focused_document() {
        let mut editor = editor();
        let original = editor.document().buffer_id();
        editor.apply_host_command(EditorCommand::NewBuffer).unwrap();
        assert_eq!(editor.buffer_count(), 2);
        assert_ne!(editor.document().buffer_id(), original);
    }

    #[test]
    fn actions_move_the_document_cursor_on_utf8_boundaries() {
        let mut editor = editor_with_text("aéz\nxy\nlast");
        editor
            .apply_action(Action::MoveRight {
                count: 2,
                select: false,
            })
            .unwrap();
        assert_eq!(editor.cursor().unwrap(), Cursor { row: 0, column: 3 });
        editor
            .apply_action(Action::MoveDown {
                count: 1,
                select: false,
            })
            .unwrap();
        assert_eq!(editor.cursor().unwrap(), Cursor { row: 1, column: 1 });
    }

    #[test]
    fn control_c_requests_exit() {
        let mut editor = editor();
        let key = Key::new(KeyCode::Char('c'), Modifiers::CONTROL);
        editor.handle_event(AppEvent::Key(key)).unwrap();
        assert_eq!(editor.lifecycle(), Lifecycle::ExitRequested);
    }
}
