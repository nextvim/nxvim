use std::{error::Error, fmt};

use crate::span::Span;

macro_rules! define_error {
    ($name:ident, $kind:ident) => {
        #[derive(Clone, Debug, PartialEq, Eq)]
        pub struct $name {
            pub kind: $kind,
            pub span: Span,
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(
                    f,
                    "{:?} at bytes {}..{}",
                    self.kind, self.span.start, self.span.end
                )
            }
        }

        impl Error for $name {}
    };
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum LexErrorKind {
    IntegerOverflow,
    InvalidCharacter(char),
}

define_error!(LexError, LexErrorKind);

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ParseErrorKind {
    UnexpectedEnd,
    UnexpectedToken,
    UnclosedExpression,
    UnclosedGroup,
    UnclosedHighlight,
    WidthOutOfRange,
    UnsupportedInDialect,
}

define_error!(ParseError, ParseErrorKind);

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum CompileErrorKind {
    TooManyExpressions,
    InvalidAst,
}

define_error!(CompileError, CompileErrorKind);

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ResolveErrorKind {
    InvalidExpressionId,
    UnexpectedEndField,
    UnexpectedEndGroup,
    UnclosedField,
    UnclosedGroup,
}

define_error!(ResolveError, ResolveErrorKind);
