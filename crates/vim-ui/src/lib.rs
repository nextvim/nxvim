pub use vim_colorscheme::{ColorScheme, Metadata, Style};
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

pub use error::{UiError, UiResult};
pub use event::{
    EventResult, KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind, UiCommand, UiEvent,
};
pub use focus::FocusManager;
pub use id::{BufferId, TabPageId, WindowId};
pub use layout::{ComputedLayout, LayoutEngine, LayoutNode};
pub use manager::Ui;
pub use model::{
    CursorShape, DisplayPosition, DisplayRow, DisplayRowKind, DisplayDecoration, DisplaySelection, GutterCell,
    ScrollbarModel, TextCursor, TextModelError, TextSpan, TextViewModel,
};
pub use overlay::OverlayManager;
pub use rect::Rect;
pub use renderer::{BufferedRenderer, CrosstermRenderer, Renderer};
pub use types::{
    Anchor, Axis, Color, FloatingConfig, NavigationDirection, RelativeTo,
};
pub use views::text::TextView;
pub use window::{View, Viewport, Window, WindowState};
pub use window_store::WindowStore;
