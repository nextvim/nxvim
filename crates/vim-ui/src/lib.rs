pub mod colorscheme;
pub mod error;
pub mod event;
pub mod focus;
pub mod id;
pub mod layout;
pub mod manager;
pub mod model;
pub mod overlay;
pub mod rect;
pub mod renderer;
pub mod types;
pub mod views;
pub mod window;
pub mod window_store;

pub use colorscheme::{ColorScheme, Metadata, Style};
pub use error::{UiError, UiResult};
pub use event::{
    EventResult, KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind, UiCommand, UiEvent,
};
pub use focus::FocusManager;
pub use id::{BufferId, TabPageId, WindowId};
pub use layout::{ComputedLayout, LayoutEngine, LayoutNode, SlotLayout, WindowSlot};
pub use manager::Ui;
pub use model::{
    BufferPosition, BufferViewModel, CursorShape, DisplayPosition, DisplayRow, DisplayRowKind,
    DisplaySelection, EditorMode, GutterCell, LineSource, ScrollbarModel, Selection, TextCursor,
    TextModelError, TextSpan, TextViewModel,
};
pub use overlay::OverlayManager;
pub use rect::Rect;
pub use renderer::{BufferedRenderer, CrosstermRenderer, Renderer};
pub use types::{
    Anchor, Color, FloatingConfig, NavigationDirection, RelativeTo, SizeConstraint, SplitAxis,
};
pub use views::buffer::BufferView;
pub use views::statusline::StatusLineView;
pub use views::tabline::TabLineView;
pub use views::text::TextView;
pub use window::{Controller, UIContext, View, Window};
pub use window_store::WindowStore;
