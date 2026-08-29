//! Vim-compatible syntax highlighting engine.
//!
//! The implementation plan and compatibility contract live in
//! [`DESIGN.md`](../DESIGN.md).

pub mod runtime;

pub use runtime::{LoadError, RuntimePath, SyntaxSource};
