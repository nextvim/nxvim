//! A compiler pipeline for Vim-style statusline and tabline format strings.
//!
//! The crate is intentionally split into syntax and execution layers:
//! lexer tokens -> AST -> compiled program -> render items.

pub mod ast;
pub mod compiler;
pub mod dialect;
pub mod error;
pub mod interpreter;
pub mod layout;
pub mod lexer;
pub mod parser;
pub mod render;
pub mod resolver;
pub mod span;

pub use ast::{Alignment, AstItem, Escape, EscapeKind, FieldSpec, FormatAst};
pub use compiler::{
    CompiledFormat, Compiler, ExprId, Instruction, Program, SpannedInstruction, StyleId,
};
pub use dialect::{FormatDialect, TablineTarget};
pub use error::{
    CompileError, CompileErrorKind, LexError, LexErrorKind, ParseError, ParseErrorKind,
    ResolveError, ResolveErrorKind,
};
pub use interpreter::Interpreter;
pub use layout::{LayoutEngine, layout};
pub use lexer::{Lexer, SpannedToken, Token, lex};
pub use parser::{Parser, parse};
pub use render::RenderItem;
pub use resolver::FormatResolver;
pub use span::{Span, Spanned};
