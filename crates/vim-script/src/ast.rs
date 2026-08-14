use crate::source::{SourceId, Span};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct NodeId(pub u32);

#[derive(Clone, Debug, Default)]
pub struct Program {
    pub source: SourceId,
    pub statements: Vec<Stmt>,
}

#[derive(Clone, Debug)]
pub struct Stmt {
    pub id: NodeId,
    pub span: Span,
    pub kind: StmtKind,
}

#[derive(Clone, Debug)]
pub enum StmtKind {
    Assignment(Assignment),
    Unlet(Vec<Expr>),
    Expression(Expr),
    Echo(Vec<Expr>),
    Execute(Vec<Expr>),
    If(IfStmt),
    While(WhileStmt),
    For(ForStmt),
    Try(TryStmt),
    Throw(Expr),
    Function(FunctionDecl),
    Return(Option<Expr>),
    Break,
    Continue,
    Finish,
    ExCommand(ExCommand),
    UserCommand(UserCommandDecl),
    Mapping(MappingDecl),
    OptionSet(OptionSet),
    Autocmd(AutocmdDecl),
    Augroup(AugroupDecl),
}

#[derive(Clone, Debug)]
pub struct Assignment {
    pub target: AssignmentTarget,
    pub operator: AssignmentOperator,
    pub value: Expr,
    pub is_const: bool,
}

#[derive(Clone, Debug)]
pub enum AssignmentTarget {
    Name(ScopedName),
    Option(OptionName),
    Index {
        target: Box<Expr>,
        index: Box<Expr>,
    },
    Slice {
        target: Box<Expr>,
        start: Option<Box<Expr>>,
        end: Option<Box<Expr>>,
    },
    Destructure(Vec<AssignmentTarget>),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AssignmentOperator {
    Assign,
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
    Concatenate,
}

#[derive(Clone, Debug)]
pub struct IfStmt {
    pub branches: Vec<ConditionalBranch>,
    pub else_body: Vec<Stmt>,
}

#[derive(Clone, Debug)]
pub struct ConditionalBranch {
    pub condition: Expr,
    pub body: Vec<Stmt>,
}

#[derive(Clone, Debug)]
pub struct WhileStmt {
    pub condition: Expr,
    pub body: Vec<Stmt>,
}

#[derive(Clone, Debug)]
pub struct ForStmt {
    pub binding: AssignmentTarget,
    pub iterable: Expr,
    pub body: Vec<Stmt>,
}

#[derive(Clone, Debug)]
pub struct TryStmt {
    pub body: Vec<Stmt>,
    pub catches: Vec<CatchClause>,
    pub finally_body: Vec<Stmt>,
}

#[derive(Clone, Debug)]
pub struct CatchClause {
    pub pattern: Option<String>,
    pub binding: Option<ScopedName>,
    pub body: Vec<Stmt>,
}

#[derive(Clone, Debug)]
pub struct FunctionDecl {
    pub name: ScopedName,
    pub parameters: Vec<Parameter>,
    pub varargs: Option<String>,
    pub body: Vec<Stmt>,
    pub attributes: FunctionAttributes,
}

#[derive(Clone, Debug)]
pub struct Parameter {
    pub name: String,
    pub default: Option<Expr>,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FunctionAttributes {
    pub abort: bool,
    pub closure: bool,
    pub dict: bool,
    pub range: bool,
    pub replace: bool,
    pub asynchronous: bool,
}

#[derive(Clone, Debug)]
pub struct Expr {
    pub id: NodeId,
    pub span: Span,
    pub kind: ExprKind,
}

#[derive(Clone, Debug)]
pub enum ExprKind {
    Literal(Literal),
    Variable(ScopedName),
    Register(char),
    Option(OptionName),
    Environment(String),
    Unary {
        operator: UnaryOperator,
        operand: Box<Expr>,
    },
    Binary {
        left: Box<Expr>,
        operator: BinaryOperator,
        right: Box<Expr>,
    },
    Ternary {
        condition: Box<Expr>,
        then_expr: Box<Expr>,
        else_expr: Box<Expr>,
    },
    Call {
        callee: Box<Expr>,
        arguments: Vec<Expr>,
    },
    MethodCall {
        receiver: Box<Expr>,
        method: String,
        arguments: Vec<Expr>,
    },
    Index {
        target: Box<Expr>,
        index: Box<Expr>,
    },
    Slice {
        target: Box<Expr>,
        start: Option<Box<Expr>>,
        end: Option<Box<Expr>>,
    },
    Member {
        target: Box<Expr>,
        name: String,
    },
    List(Vec<Expr>),
    Dictionary(Vec<DictEntry>),
    Lambda(LambdaExpr),
    Await(Box<Expr>),
    InterpolatedString(Vec<StringPart>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Literal {
    Null,
    Bool(bool),
    Integer(i64),
    Float(f64),
    String(String),
    Blob(Vec<u8>),
}

#[derive(Clone, Debug)]
pub struct DictEntry {
    pub key: Expr,
    pub value: Expr,
}

#[derive(Clone, Debug)]
pub struct LambdaExpr {
    pub parameters: Vec<Parameter>,
    pub body: LambdaBody,
    pub asynchronous: bool,
}

#[derive(Clone, Debug)]
pub enum LambdaBody {
    Expression(Box<Expr>),
    Block(Vec<Stmt>),
}

#[derive(Clone, Debug)]
pub enum StringPart {
    Text(String),
    Expression(Expr),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UnaryOperator {
    Negate,
    Positive,
    Not,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BinaryOperator {
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
    Concatenate,
    Equal,
    NotEqual,
    Match,
    NoMatch,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    LogicalAnd,
    LogicalOr,
    Coalesce,
    Is,
    IsNot,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Scope {
    Global,
    Local,
    Script,
    Argument,
    Buffer,
    Window,
    Tab,
    Vim,
    Unqualified,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ScopedName {
    pub scope: Scope,
    pub name: String,
}

#[derive(Clone, Debug)]
pub struct OptionName {
    pub scope: OptionScope,
    pub name: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OptionScope {
    Global,
    Local,
    GlobalLocal,
    Unqualified,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ExCommand {
    pub modifiers: Vec<CommandModifier>,
    pub range: Option<CommandRange>,
    pub name: String,
    pub bang: bool,
    pub count: Option<u64>,
    pub register: Option<char>,
    pub arguments: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum CommandModifier {
    Silent { errors: bool },
    KeepJumps,
    KeepAlt,
    KeepMarks,
    NoAutocmd,
    Sandbox,
    Verbose(u32),
    Vertical,
    Tab(Option<u32>),
    Unsilent,
    KeepPatterns,
    LockMarks,
    Horizontal,
    AboveLeft,
    BelowRight,
    TopLeft,
    BotRight,
    LeftAbove,
    RightBelow,
    Confirm,
    Hide,
    Browse,
    Filter { pattern: String, bang: bool },
}

#[derive(Clone, Debug, PartialEq)]
pub struct CommandRange {
    pub start: Address,
    pub end: Option<Address>,
    pub separator: Option<RangeSeparator>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Address {
    Current,
    Last,
    Line(u64),
    Mark(char),
    Search { pattern: String, forward: bool },
    Offset { base: Box<Address>, amount: i64 },
    WholeFile,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RangeSeparator {
    Comma,
    Semicolon,
}

#[derive(Clone, Debug)]
pub struct UserCommandDecl {
    pub name: String,
    pub replacement: String,
    pub attributes: UserCommandAttributes,
}

#[derive(Clone, Debug, Default)]
pub struct UserCommandAttributes {
    pub nargs: Option<String>,
    pub complete: Option<String>,
    pub range: bool,
    pub count: bool,
    pub bang: bool,
    pub bar: bool,
    pub register: bool,
}

#[derive(Clone, Debug)]
pub struct MappingDecl {
    pub modes: Vec<MapMode>,
    pub lhs: String,
    pub rhs: MappingRhs,
    pub options: MappingOptions,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MapMode {
    Normal,
    Visual,
    Select,
    OperatorPending,
    Insert,
    CommandLine,
    LangArg,
    Terminal,
}

#[derive(Clone, Debug)]
pub enum MappingRhs {
    Keys(String),
    Expression(Expr),
    NoOp,
}

#[derive(Clone, Debug, Default)]
pub struct MappingOptions {
    pub non_recursive: bool,
    pub silent: bool,
    pub nowait: bool,
    pub expr: bool,
    pub buffer_local: bool,
    pub unique: bool,
    pub script: bool,
}

#[derive(Clone, Debug)]
pub struct OptionSet {
    pub operations: Vec<OptionOperation>,
}

#[derive(Clone, Debug)]
pub enum OptionOperation {
    Set {
        name: OptionName,
        value: Option<Expr>,
    },
    Reset(OptionName),
    Invert(OptionName),
    Append {
        name: OptionName,
        value: Expr,
    },
    Prepend {
        name: OptionName,
        value: Expr,
    },
    Remove {
        name: OptionName,
        value: Expr,
    },
    Query(OptionName),
    ResetAll,
}

#[derive(Clone, Debug)]
pub struct AutocmdDecl {
    pub group: Option<String>,
    pub events: Vec<String>,
    pub patterns: Vec<String>,
    pub nested: bool,
    pub once: bool,
    pub body: Vec<Stmt>,
}

#[derive(Clone, Debug)]
pub struct AugroupDecl {
    pub name: String,
    pub clear: bool,
    pub body: Vec<Stmt>,
}
