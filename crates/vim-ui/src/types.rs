use crate::WindowId;
pub use vim_colorscheme::Color;

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
