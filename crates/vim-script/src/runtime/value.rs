use std::collections::BTreeMap;
use std::sync::Arc;

use crate::bytecode::BytecodeModule;
use crate::resolver::FunctionId;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TaskId(pub u64);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct OperationId(pub u64);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct HostObjectId(pub u64);

#[derive(Clone, Debug)]
pub struct FunctionRef {
    pub module: Arc<BytecodeModule>,
    pub function: FunctionId,
}

impl PartialEq for FunctionRef {
    fn eq(&self, other: &Self) -> bool {
        self.function == other.function && Arc::ptr_eq(&self.module, &other.module)
    }
}

#[derive(Clone, Debug)]
pub struct Closure {
    pub function: FunctionRef,
    pub captures: Vec<Value>,
}

#[derive(Clone, Debug)]
pub enum Value {
    Null,
    Bool(bool),
    Integer(i64),
    Float(f64),
    String(Arc<str>),
    Blob(Arc<[u8]>),
    List(Vec<Value>),
    Dictionary(BTreeMap<String, Value>),
    Closure(Arc<Closure>),
    Builtin(Arc<str>),
    HostFunction(Arc<str>),
    Future(OperationId),
    HostObject(HostObjectId),
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Null, Self::Null) => true,
            (Self::Bool(left), Self::Bool(right)) => left == right,
            (Self::Integer(left), Self::Integer(right)) => left == right,
            (Self::Float(left), Self::Float(right)) => left == right,
            (Self::Integer(left), Self::Float(right)) => *left as f64 == *right,
            (Self::Float(left), Self::Integer(right)) => *left == *right as f64,
            (Self::String(left), Self::String(right)) => left == right,
            (Self::Blob(left), Self::Blob(right)) => left == right,
            (Self::List(left), Self::List(right)) => left == right,
            (Self::Dictionary(left), Self::Dictionary(right)) => left == right,
            (Self::Closure(left), Self::Closure(right)) => Arc::ptr_eq(left, right),
            (Self::Builtin(left), Self::Builtin(right)) => left == right,
            (Self::HostFunction(left), Self::HostFunction(right)) => left == right,
            (Self::Future(left), Self::Future(right)) => left == right,
            (Self::HostObject(left), Self::HostObject(right)) => left == right,
            _ => false,
        }
    }
}

impl Value {
    pub fn is_truthy(&self) -> bool {
        match self {
            Self::Null => false,
            Self::Bool(value) => *value,
            Self::Integer(value) => *value != 0,
            Self::Float(value) => *value != 0.0,
            Self::String(value) => value.parse::<f64>().is_ok_and(|number| number != 0.0),
            Self::Blob(value) => !value.is_empty(),
            Self::List(value) => !value.is_empty(),
            Self::Dictionary(value) => !value.is_empty(),
            Self::Closure(_)
            | Self::Builtin(_)
            | Self::HostFunction(_)
            | Self::Future(_)
            | Self::HostObject(_) => true,
        }
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Null => "null",
            Self::Bool(_) => "bool",
            Self::Integer(_) => "number",
            Self::Float(_) => "float",
            Self::String(_) => "string",
            Self::Blob(_) => "blob",
            Self::List(_) => "list",
            Self::Dictionary(_) => "dictionary",
            Self::Closure(_) | Self::Builtin(_) | Self::HostFunction(_) => "function",
            Self::Future(_) => "future",
            Self::HostObject(_) => "object",
        }
    }
}
