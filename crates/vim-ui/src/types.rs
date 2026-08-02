use crate::WindowId;
use crossterm::style::Color as CrossColor;

impl From<Color> for CrossColor {
    fn from(color: Color) -> Self {
        match color {
            Color::Reset => CrossColor::Reset,
            Color::Black => CrossColor::Black,
            Color::Red => CrossColor::Red,
            Color::Green => CrossColor::Green,
            Color::Yellow => CrossColor::Yellow,
            Color::Blue => CrossColor::Blue,
            Color::Magenta => CrossColor::Magenta,
            Color::Cyan => CrossColor::Cyan,
            Color::White => CrossColor::White,
            Color::Grey => CrossColor::Grey,
            Color::DarkGrey => CrossColor::DarkGrey,
            Color::Rgb(r, g, b) => CrossColor::Rgb { r, g, b },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitAxis {
    Rows,    // Vertical stacking (splits top/bottom)
    Columns, // Horizontal stacking (splits left/right)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavigationDirection {
    Left,
    Right,
    Up,
    Down,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SizeConstraint {
    Fixed(u16),
    Percentage(f32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    Reset,
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    Grey,
    DarkGrey,
    Rgb(u8, u8, u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelativeTo {
    Editor,
    Window(WindowId),
    Cursor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FloatingConfig {
    pub relative_to: RelativeTo,
    pub anchor: Anchor,
    pub row: i16,
    pub col: i16,
    pub width: u16,
    pub height: u16,
    pub zindex: u32,
    pub border: bool,
}
