//! Vim-compatible syntax highlighting engine.
//!
//! The implementation plan and compatibility contract live in
//! [`DESIGN.md`](../DESIGN.md).

pub mod command;
pub mod engine;
pub mod highlight;
pub mod parser;
pub mod program;
pub mod runtime;

pub use command::*;
pub use engine::{HighlightSpan, SyntaxState};
pub use highlight::{GroupId, HighlightGroups, HighlightLinks, LinkMode, resolve_style};
pub use parser::{ParseError, parse_syntax_command};
pub use program::{BuildError, SyntaxBuilder, SyntaxProgram};
pub use runtime::{LoadError, RuntimePath, SyntaxSource};
