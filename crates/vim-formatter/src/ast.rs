use crate::{
    dialect::{FormatDialect, TablineTarget},
    span::Spanned,
};

/// Parsed syntax together with the dialect under which it was parsed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormatAst {
    pub dialect: FormatDialect,
    pub items: Vec<Spanned<AstItem>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum AstItem {
    Literal(String),
    Escape(Escape),
    Group {
        field: FieldSpec,
        items: Vec<Spanned<AstItem>>,
    },
    Highlight(String),
    ResetHighlight,
    Expression(String),
    Align,
    Truncate,
    TablineTarget(TablineTarget),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Escape {
    pub field: FieldSpec,
    pub kind: EscapeKind,
}

/// Width and alignment modifiers preceding an escape code.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct FieldSpec {
    pub min_width: Option<u16>,
    pub max_width: Option<u16>,
    pub alignment: Alignment,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Alignment {
    #[default]
    Right,
    Left,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum EscapeKind {
    FileName,
    FullPath,
    Tail,
    Line,
    Column,
    VirtualColumn,
    TotalLines,
    Percentage,
    Modified,
    ReadOnly,
    Help,
    Preview,
    BufferNumber,
    FileType,
    Encoding,
    FileFormat,
    CharacterDecimal,
    CharacterHex,
    LiteralPercent,
    Unknown(char),
}
