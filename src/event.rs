use vim_input::Key;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScreenSize {
    pub columns: u16,
    pub rows: u16,
}

impl ScreenSize {
    pub const fn new(columns: u16, rows: u16) -> Self {
        Self { columns, rows }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppEvent {
    Key(Key),
    Resize(ScreenSize),
    Tick,
    EndOfInput,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorCommand {
    Quit,
    NewBuffer,
}
