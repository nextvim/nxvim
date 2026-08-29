use crate::id::WindowId;
use crate::types::{FloatingConfig, NavigationDirection, Axis};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyCode {
    Char(char),
    F(u8),
    Backspace,
    Enter,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    Tab,
    BackTab,
    Delete,
    Insert,
    Esc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyModifiers {
    pub shift: bool,
    pub control: bool,
    pub alt: bool,
}

impl KeyModifiers {
    pub const NONE: Self = Self {
        shift: false,
        control: false,
        alt: false,
    };
    pub const CONTROL: Self = Self {
        shift: false,
        control: true,
        alt: false,
    };
    pub const SHIFT: Self = Self {
        shift: true,
        control: false,
        alt: false,
    };
    pub const ALT: Self = Self {
        shift: false,
        control: false,
        alt: true,
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyEvent {
    pub code: KeyCode,
    pub modifiers: KeyModifiers,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MouseEventKind {
    Down,
    Up,
    Drag,
    Moved,
    ScrollUp,
    ScrollDown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MouseEvent {
    pub kind: MouseEventKind,
    pub column: u16,
    pub row: u16,
    pub modifiers: KeyModifiers,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiEvent {
    Key(KeyEvent),
    Mouse(MouseEvent),
    Resize { width: u16, height: u16 },
    Paste(String),
    Tick,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiCommand {
    FocusWindow(WindowId),
    FocusDirection(NavigationDirection),
    SplitWindow(Axis),
    CloseWindow(WindowId),
    OpenOverlay(WindowId, FloatingConfig),
    CloseOverlay(WindowId),
    SwitchTabPage(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventResult {
    Ignored,
    Consumed,
    Redraw,
    Command(UiCommand),
}

impl From<crossterm::event::KeyCode> for KeyCode {
    fn from(code: crossterm::event::KeyCode) -> Self {
        match code {
            crossterm::event::KeyCode::Char(c) => KeyCode::Char(c),
            crossterm::event::KeyCode::F(n) => KeyCode::F(n),
            crossterm::event::KeyCode::Backspace => KeyCode::Backspace,
            crossterm::event::KeyCode::Enter => KeyCode::Enter,
            crossterm::event::KeyCode::Left => KeyCode::Left,
            crossterm::event::KeyCode::Right => KeyCode::Right,
            crossterm::event::KeyCode::Up => KeyCode::Up,
            crossterm::event::KeyCode::Down => KeyCode::Down,
            crossterm::event::KeyCode::Home => KeyCode::Home,
            crossterm::event::KeyCode::End => KeyCode::End,
            crossterm::event::KeyCode::PageUp => KeyCode::PageUp,
            crossterm::event::KeyCode::PageDown => KeyCode::PageDown,
            crossterm::event::KeyCode::Tab => KeyCode::Tab,
            crossterm::event::KeyCode::BackTab => KeyCode::BackTab,
            crossterm::event::KeyCode::Delete => KeyCode::Delete,
            crossterm::event::KeyCode::Insert => KeyCode::Insert,
            crossterm::event::KeyCode::Esc => KeyCode::Esc,
            _ => KeyCode::Char('?'),
        }
    }
}

impl From<crossterm::event::KeyModifiers> for KeyModifiers {
    fn from(mods: crossterm::event::KeyModifiers) -> Self {
        KeyModifiers {
            shift: mods.contains(crossterm::event::KeyModifiers::SHIFT),
            control: mods.contains(crossterm::event::KeyModifiers::CONTROL),
            alt: mods.contains(crossterm::event::KeyModifiers::ALT),
        }
    }
}

impl From<crossterm::event::KeyEvent> for KeyEvent {
    fn from(event: crossterm::event::KeyEvent) -> Self {
        KeyEvent {
            code: KeyCode::from(event.code),
            modifiers: KeyModifiers::from(event.modifiers),
        }
    }
}

impl From<crossterm::event::MouseEventKind> for MouseEventKind {
    fn from(kind: crossterm::event::MouseEventKind) -> Self {
        match kind {
            crossterm::event::MouseEventKind::Down(_) => MouseEventKind::Down,
            crossterm::event::MouseEventKind::Up(_) => MouseEventKind::Up,
            crossterm::event::MouseEventKind::Drag(_) => MouseEventKind::Drag,
            crossterm::event::MouseEventKind::Moved => MouseEventKind::Moved,
            crossterm::event::MouseEventKind::ScrollUp => MouseEventKind::ScrollUp,
            crossterm::event::MouseEventKind::ScrollDown => MouseEventKind::ScrollDown,
            _ => MouseEventKind::Moved,
        }
    }
}

impl From<crossterm::event::MouseEvent> for MouseEvent {
    fn from(event: crossterm::event::MouseEvent) -> Self {
        MouseEvent {
            kind: MouseEventKind::from(event.kind),
            column: event.column,
            row: event.row,
            modifiers: KeyModifiers::from(event.modifiers),
        }
    }
}

impl TryFrom<crossterm::event::Event> for UiEvent {
    type Error = &'static str;

    fn try_from(event: crossterm::event::Event) -> Result<Self, Self::Error> {
        match event {
            crossterm::event::Event::Key(key) => Ok(UiEvent::Key(KeyEvent::from(key))),
            crossterm::event::Event::Mouse(mouse) => Ok(UiEvent::Mouse(MouseEvent::from(mouse))),
            crossterm::event::Event::Resize(w, h) => Ok(UiEvent::Resize {
                width: w,
                height: h,
            }),
            crossterm::event::Event::Paste(s) => Ok(UiEvent::Paste(s)),
            crossterm::event::Event::FocusGained | crossterm::event::Event::FocusLost => {
                Err("unsupported event")
            }
        }
    }
}
