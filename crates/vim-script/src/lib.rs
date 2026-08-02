//! Embeddable, asynchronous Vim script runtime.
//!
//! The crate is organized as a pipeline: source -> tokens -> AST -> resolved AST
//! -> bytecode -> VM. Host integrations remain outside the language core.

pub mod ast;
pub mod bytecode;
pub mod compiler;
pub mod ex_parser;
pub mod host;
pub mod integration;
pub mod lexer;
pub mod mock_editor;
pub mod parser;
pub mod plugin;
pub mod resolver;
pub mod runtime;
pub mod source;

pub use source::{Diagnostic, SourceId, SourceMap, Span};
