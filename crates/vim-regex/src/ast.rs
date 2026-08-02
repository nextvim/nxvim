use crate::context::{MagicMode, TextRange};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Pattern {
    pub source: String,
    pub initial_magic: MagicMode,
    pub expression: Spanned<Expr>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Spanned<T> {
    pub value: T,
    pub span: TextRange,
}

impl<T> Spanned<T> {
    pub fn new(value: T, span: TextRange) -> Self {
        Self { value, span }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Expr {
    Empty,
    Literal(String),
    Dot {
        include_newline: bool,
    },
    Class(CharacterClass),
    Collection(Collection),
    Anchor(Anchor),
    Position(PositionAtom),
    Backreference(Backreference),
    Concat(Vec<Spanned<Expr>>),
    Alternation(Vec<Spanned<Expr>>),
    Group {
        kind: GroupKind,
        expression: Box<Spanned<Expr>>,
    },
    Repeat {
        expression: Box<Spanned<Expr>>,
        quantifier: Quantifier,
    },
    Lookaround {
        expression: Box<Spanned<Expr>>,
        kind: LookaroundKind,
        limit: Option<usize>,
    },
    MagicSwitch(MagicMode),
    CaseSwitch(CaseSwitch),
    MatchBoundary(MatchBoundary),
    Composing(ComposingAtom),
    EnginePreference(EnginePreference),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GroupKind {
    Capture { index: u8 },
    NonCapturing,
    ExternalCapture { index: u8 },
    OptionalTail,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Quantifier {
    pub min: usize,
    pub max: Option<usize>,
    pub preference: RepeatPreference,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RepeatPreference {
    Greedy,
    Minimal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LookaroundKind {
    Ahead,
    NegativeAhead,
    Behind,
    NegativeBehind,
    Atomic,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Anchor {
    StartOfLine,
    EndOfLine,
    StartOfFile,
    EndOfFile,
    StartOfWord,
    EndOfWord,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PositionAtom {
    Line(Comparison),
    ByteColumn(Comparison),
    VirtualColumn(Comparison),
    Cursor,
    VisualArea,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Comparison {
    pub ordering: Ordering,
    pub value: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Ordering {
    Equal,
    LessThan,
    GreaterThan,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Backreference {
    Capture(u8),
    External(u8),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CaseSwitch {
    Sensitive,
    Insensitive,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MatchBoundary {
    Start,
    End,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComposingAtom {
    IgnoreFollowing,
    AnyCombiningMark,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EnginePreference {
    Automatic,
    Backtracking,
    Nfa,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CharacterClass {
    pub kind: ClassKind,
    pub negated: bool,
    pub include_newline: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClassKind {
    Alphabetic,
    Digit,
    HexDigit,
    OctalDigit,
    HeadOfWord,
    Lowercase,
    Uppercase,
    Word,
    Keyword,
    FileName,
    Printable,
    Whitespace,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Collection {
    pub negated: bool,
    pub include_newline: bool,
    pub items: Vec<CollectionItem>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CollectionItem {
    Character(char),
    Range(char, char),
    Posix(PosixClass),
    Equivalence(char),
    CollatingElement(String),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PosixClass {
    Alnum,
    Alpha,
    Blank,
    Cntrl,
    Digit,
    Graph,
    Lower,
    Print,
    Punct,
    Space,
    Upper,
    Xdigit,
}
