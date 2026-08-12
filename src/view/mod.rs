//! Read-only rendering projection.
//!
//! Views depend on immutable model/controller inputs while building an
//! `EditorViewModel`, then consume only `vim_ui` rendering abstractions.

mod commandline;
mod statusline;
mod tabline;
mod textview;
mod view_model;

pub use commandline::CommandLineView;
pub use statusline::StatusLineView;
pub use tabline::TabLineView;
pub use textview::TextView;
pub use view_model::{EditorViewModel, LayoutSnapshot};
