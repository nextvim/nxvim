use crate::{ast, context::CaseBehavior};

/// Backend-neutral expression after Vim syntax and options are resolved.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Expr {
    Empty,
    Literal(String),
    /// Literal text resolved from a Vim external capture reference (`\z1` ... `\z9`).
    ExternalReferenceLiteral(String),
    Any {
        include_newline: bool,
    },
    CharacterSet(CharacterSet),
    /// Zero or more Unicode composing marks (`\%C`).
    ComposingMarks,
    Anchor(ast::Anchor),
    Backreference(u8),
    Concat(Vec<Expr>),
    Alternation(Vec<Expr>),
    Capture {
        index: u8,
        expression: Box<Expr>,
    },
    /// A Vim external syntax capture (`\z(...)`).
    ExternalCapture {
        index: u8,
        expression: Box<Expr>,
    },
    NonCapturing(Box<Expr>),
    Repeat {
        expression: Box<Expr>,
        min: usize,
        max: Option<usize>,
        greedy: bool,
    },
    Lookaround {
        expression: Box<Expr>,
        kind: ast::LookaroundKind,
        limit: Option<usize>,
    },
    RuntimeAssertion(RuntimeAssertion),
    BoundaryMarker(BoundaryMarker),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CharacterSet {
    /// Ranges are inclusive Unicode scalar-value ranges.
    pub ranges: Vec<(char, char)>,
    pub negated: bool,
    pub include_newline: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeAssertion {
    Position(ast::PositionAtom),
    KeywordBoundary(KeywordBoundary),
    ExternalCapture(u8),
    Composing(ast::ComposingAtom),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KeywordBoundary {
    Start,
    End,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BoundaryMarker {
    MatchStart,
    MatchEnd,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Program {
    pub expression: Expr,
    pub case_behavior: CaseBehavior,
    pub vim_capture_count: u8,
    pub needs_match_context: bool,
}
