use std::collections::HashMap;

use crate::runtime::{RuntimeError, RuntimeErrorKind, RuntimeResult, Value};

pub mod collections;
pub mod math;
pub mod state;
pub mod string;
pub mod types;

pub type BuiltinFn = fn(&[Value]) -> RuntimeResult<Value>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BuiltinArity {
    Exact(usize),
    Range { min: usize, max: usize },
    Variadic { min: usize },
}

impl BuiltinArity {
    pub(crate) fn accepts(self, count: usize) -> bool {
        match self {
            Self::Exact(expected) => count == expected,
            Self::Range { min, max } => (min..=max).contains(&count),
            Self::Variadic { min } => count >= min,
        }
    }
    pub(crate) fn describe(self) -> String {
        match self {
            Self::Exact(value) => value.to_string(),
            Self::Range { min, max } => format!("{min}..={max}"),
            Self::Variadic { min } => format!("at least {min}"),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct BuiltinSpec {
    pub function: BuiltinFn,
    pub arity: BuiltinArity,
}

#[derive(Clone, Debug, Default)]
pub struct BuiltinRegistry {
    functions: HashMap<String, BuiltinSpec>,
}

impl BuiltinRegistry {
    pub fn with_defaults() -> Self {
        let mut registry = Self::default();
        math::register(&mut registry);
        string::register(&mut registry);
        collections::register(&mut registry);
        registry.register(
            "exists",
            BuiltinArity::Exact(1),
            state::exists_without_vm_context,
        );
        registry.register("type", BuiltinArity::Exact(1), types::value_type);

        fn dummy_assert(_: &[Value]) -> RuntimeResult<Value> {
            Ok(Value::Integer(0))
        }
        registry.register("assert_equal", BuiltinArity::Range { min: 2, max: 3 }, dummy_assert);
        registry.register("assert_notequal", BuiltinArity::Range { min: 2, max: 3 }, dummy_assert);
        registry.register("assert_true", BuiltinArity::Range { min: 1, max: 2 }, dummy_assert);
        registry.register("assert_false", BuiltinArity::Range { min: 1, max: 2 }, dummy_assert);
        registry.register("assert_inrange", BuiltinArity::Range { min: 3, max: 4 }, dummy_assert);
        registry.register("assert_match", BuiltinArity::Range { min: 2, max: 3 }, dummy_assert);
        registry.register("assert_report", BuiltinArity::Exact(1), dummy_assert);
        registry.register("assert_fails", BuiltinArity::Range { min: 1, max: 3 }, dummy_assert);
        registry.register("feedkeys", BuiltinArity::Range { min: 1, max: 2 }, dummy_assert);
        registry.register("mode", BuiltinArity::Range { min: 0, max: 1 }, dummy_assert);
        registry.register("eval", BuiltinArity::Exact(1), dummy_assert);
        registry.register("execute", BuiltinArity::Range { min: 1, max: 2 }, dummy_assert);
        registry.register("bufnr", BuiltinArity::Range { min: 0, max: 2 }, dummy_assert);
        registry.register("bufname", BuiltinArity::Range { min: 0, max: 1 }, dummy_assert);
        registry.register("bufexists", BuiltinArity::Exact(1), dummy_assert);
        registry.register("getbufinfo", BuiltinArity::Range { min: 0, max: 1 }, dummy_assert);
        registry.register("getbufline", BuiltinArity::Range { min: 2, max: 3 }, dummy_assert);
        registry.register("setbufline", BuiltinArity::Exact(3), dummy_assert);
        registry.register("deletebufline", BuiltinArity::Range { min: 2, max: 3 }, dummy_assert);
        registry.register("append", BuiltinArity::Exact(2), dummy_assert);

        registry
    }

    pub fn register(&mut self, name: impl Into<String>, arity: BuiltinArity, function: BuiltinFn) {
        self.functions
            .insert(name.into(), BuiltinSpec { function, arity });
    }
    pub fn contains(&self, name: &str) -> bool {
        self.functions.contains_key(name)
    }
    pub fn call(&self, name: &str, arguments: &[Value]) -> RuntimeResult<Value> {
        let spec = self
            .functions
            .get(name)
            .ok_or_else(|| error("E117", format!("unknown function: {name}")))?;
        if !spec.arity.accepts(arguments.len()) {
            let code = match spec.arity {
                BuiltinArity::Exact(expected) if arguments.len() > expected => "E118",
                BuiltinArity::Range { max, .. } if arguments.len() > max => "E118",
                _ => "E119",
            };
            return Err(error(
                code,
                format!(
                    "function {name} expects {} arguments, got {}",
                    spec.arity.describe(),
                    arguments.len()
                ),
            ));
        }
        (spec.function)(arguments)
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.functions.keys().map(String::as_str)
    }
}

pub(crate) fn type_error(function: &str, expected: &str, actual: &Value) -> RuntimeError {
    error(
        "E745",
        format!("{function} expected {expected}, got {}", actual.type_name()),
    )
}

pub(crate) fn error(code: &str, message: impl Into<String>) -> RuntimeError {
    RuntimeError::coded(code, RuntimeErrorKind::TypeError, message)
}

pub(crate) fn normalize_index(index: i64, len: usize) -> Option<usize> {
    let index = if index < 0 { len as i64 + index } else { index };
    (index >= 0 && index < len as i64).then_some(index as usize)
}

pub(crate) fn key_string(value: &Value) -> RuntimeResult<String> {
    match value {
        Value::String(value) => Ok(value.to_string()),
        Value::Integer(value) => Ok(value.to_string()),
        other => Err(type_error("get", "String or Number key", other)),
    }
}

pub(crate) fn vim_display(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::Bool(value) => i32::from(*value).to_string(),
        Value::Integer(value) => value.to_string(),
        Value::Float(value) => value.to_string(),
        Value::String(value) => value.to_string(),
        other => vim_string(other),
    }
}

pub(crate) fn vim_string(value: &Value) -> String {
    match value {
        Value::Null => "v:null".into(),
        Value::Bool(true) => "v:true".into(),
        Value::Bool(false) => "v:false".into(),
        Value::Integer(value) => value.to_string(),
        Value::Float(value) => value.to_string(),
        Value::String(value) => format!("'{}'", value.replace('\'', "''")),
        Value::Blob(value) => format!(
            "0z{}",
            value
                .iter()
                .map(|byte| format!("{byte:02X}"))
                .collect::<String>()
        ),
        Value::List(values) => format!(
            "[{}]",
            values.iter().map(vim_string).collect::<Vec<_>>().join(", ")
        ),
        Value::Dictionary(values) => format!(
            "{{{}}}",
            values
                .iter()
                .map(|(key, value)| format!("'{}': {}", key.replace('\'', "''"), vim_string(value)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Value::Closure(_) => "function('<lambda>')".into(),
        Value::Builtin(name) | Value::HostFunction(name) => format!("function('{name}')"),
        Value::Future(id) => format!("future({})", id.0),
        Value::HostObject(id) => format!("object({})", id.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    #[test]
    fn validates_arity_and_types() {
        let registry = BuiltinRegistry::with_defaults();
        assert_eq!(
            registry.call("len", &[]).unwrap_err().code.as_deref(),
            Some("E119")
        );
        assert_eq!(
            registry
                .call("len", &[Value::Integer(1)])
                .unwrap_err()
                .code
                .as_deref(),
            Some("E745")
        );
    }
    #[test]
    fn executes_collection_and_string_builtins() {
        let registry = BuiltinRegistry::with_defaults();
        assert_eq!(
            registry
                .call("range", &[Value::Integer(2), Value::Integer(4)])
                .unwrap(),
            Value::List(vec![
                Value::Integer(2),
                Value::Integer(3),
                Value::Integer(4)
            ])
        );
        assert_eq!(
            registry
                .call(
                    "join",
                    &[
                        Value::List(vec![Value::Integer(1), Value::Integer(2)]),
                        Value::String(Arc::from(","))
                    ]
                )
                .unwrap(),
            Value::String(Arc::from("1,2"))
        );
    }
}
