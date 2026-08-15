//! Concrete window views.
//!
//! Each view owns whatever small, cheap model it needs and rebuilds it each
//! frame through an ordinary `refresh` method (not part of `vim_ui::View`)
//! from window state, buffer state, and `RenderGlobals`. Views consume only
//! `vim_ui` rendering abstractions; there is no shared context object.

pub mod commandline;
pub mod globals;
pub mod layout_snapshot;
pub mod statusline;
pub mod tabline;
pub mod textview;

pub use commandline::CommandLineView;
pub use globals::RenderGlobals;
pub use layout_snapshot::{LayoutSnapshot, WindowLayout};
pub use statusline::StatusLineView;
pub use tabline::TabLineView;
pub use textview::TextView;
