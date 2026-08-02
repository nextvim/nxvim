mod cli;
mod editor;
mod error;

pub use cli::{CliError, run_cli};
pub use editor::HeadlessEditor;
pub use error::EditorError;
