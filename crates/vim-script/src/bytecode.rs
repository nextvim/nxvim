use crate::ast::{BinaryOperator, ExCommand, UnaryOperator};
use crate::resolver::FunctionId;
use crate::source::{SourceId, Span};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ConstantId(pub u32);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OptionScopeOperand {
    Unqualified,
    Local,
    Global,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Constant {
    Null,
    Bool(bool),
    Integer(i64),
    Float(f64),
    String(String),
    Blob(Vec<u8>),
    Function(FunctionId),
    Command(Box<ExCommand>),
}

/// Jump operands are absolute instruction indices within the current function.
#[derive(Clone, Debug)]
pub enum Instruction {
    LoadConstant(ConstantId),
    LoadNull,
    LoadLocal(u32),
    StoreLocal(u32),
    LoadCapture(u32),
    StoreCapture(u32),
    LoadGlobal(ConstantId),
    StoreGlobal(ConstantId),
    LoadScoped {
        scope: u8,
        name: ConstantId,
    },
    StoreScoped {
        scope: u8,
        name: ConstantId,
    },
    LoadOption {
        scope: OptionScopeOperand,
        name: ConstantId,
    },
    StoreOption {
        scope: OptionScopeOperand,
        name: ConstantId,
    },
    Pop,
    Duplicate,
    Unary(UnaryOperator),
    Binary(BinaryOperator),
    BuildList(u32),
    BuildDictionary(u32),
    GetIndex,
    SetIndex,
    GetMember(ConstantId),
    Call(u16),
    CallNamed {
        name: ConstantId,
        argc: u16,
    },
    Return,
    MakeClosure {
        function: FunctionId,
        captures: u16,
    },
    Jump(u32),
    JumpIfFalse(u32),
    JumpIfTrue(u32),
    Loop(u32),
    IterStart,
    IterNext {
        end: u32,
    },
    IterEnd,
    TryBegin {
        handler: u32,
        stack_depth: u32,
    },
    TryEnd,
    Throw,
    Await,
    ExecuteCommand(ConstantId),
    EmitEvent(ConstantId),
}

#[derive(Clone, Debug)]
pub struct ExceptionHandler {
    pub start: u32,
    pub end: u32,
    pub handler: u32,
    pub finally: Option<u32>,
}

#[derive(Clone, Debug)]
pub struct FunctionPrototype {
    pub id: FunctionId,
    pub name: Option<String>,
    pub arity: u16,
    pub optional_parameters: u16,
    pub variadic: bool,
    pub local_count: u32,
    pub capture_count: u16,
    pub asynchronous: bool,
    pub code: Vec<Instruction>,
    pub constants: Vec<Constant>,
    pub spans: Vec<(u32, Span)>,
    pub handlers: Vec<ExceptionHandler>,
}

#[derive(Clone, Debug)]
pub struct BytecodeModule {
    pub source: SourceId,
    pub entrypoint: FunctionId,
    pub functions: Vec<FunctionPrototype>,
}

impl BytecodeModule {
    pub fn function(&self, id: FunctionId) -> Option<&FunctionPrototype> {
        self.functions.iter().find(|function| function.id == id)
    }
}
