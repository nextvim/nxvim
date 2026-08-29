use std::ops::Range;

/// A source-independent byte span relative to the syntax command arguments.
pub type CommandSpan = Range<usize>;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SyntaxCase {
    #[default]
    Match,
    Ignore,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Pattern {
    /// Pattern text without its delimiters. Backslashes are preserved verbatim.
    pub text: String,
    pub delimiter: char,
    pub offsets: Vec<PatternOffset>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OffsetKind {
    HighlightStart,
    HighlightEnd,
    MatchStart,
    MatchEnd,
    RegionStart,
    RegionEnd,
    LeadingContext,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OffsetBase {
    Start,
    End,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PatternOffset {
    pub kind: OffsetKind,
    pub base: OffsetBase,
    pub amount: i32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GroupName {
    Group(String),
    Cluster(String),
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CommonOptions {
    pub contains: Option<Vec<GroupName>>,
    pub contained: bool,
    pub contained_in: Option<Vec<GroupName>>,
    pub next_group: Option<Vec<GroupName>>,
    pub skip_white: bool,
    pub skip_nl: bool,
    pub skip_empty: bool,
    pub transparent: bool,
    pub display: bool,
    pub extend: bool,
    pub conceal: bool,
    pub conceal_ends: bool,
    pub conceal_char: Option<char>,
    pub fold: bool,
    pub spell: Option<bool>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KeywordCommand {
    pub group: String,
    pub keywords: Vec<String>,
    pub options: CommonOptions,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MatchCommand {
    pub group: String,
    pub pattern: Pattern,
    pub options: CommonOptions,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegionCommand {
    pub group: String,
    pub starts: Vec<Pattern>,
    pub skip: Option<Pattern>,
    pub ends: Vec<Pattern>,
    pub match_group: Option<String>,
    pub keep_end: bool,
    pub oneline: bool,
    pub exclude_nl: bool,
    pub options: CommonOptions,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClusterCommand {
    pub name: String,
    pub contains: Option<Vec<GroupName>>,
    pub add: Option<Vec<GroupName>>,
    pub remove: Option<Vec<GroupName>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ClearCommand {
    All,
    Groups(Vec<String>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyncMatchCommand {
    pub group: String,
    pub pattern: Pattern,
    pub location: Option<SyncLocation>,
    pub options: CommonOptions,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SyncLocation {
    GroupHere(Option<String>),
    GroupThere(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SyncCommand {
    FromStart,
    Clear(ClearCommand),
    MinLines(u32),
    MaxLines(u32),
    LineBreaks(u32),
    Match(SyncMatchCommand),
    CComment(Option<String>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SyntaxCommand {
    Case(SyntaxCase),
    Keyword(KeywordCommand),
    Match(MatchCommand),
    Region(RegionCommand),
    Cluster(ClusterCommand),
    Clear(ClearCommand),
    Sync(SyncCommand),
}
