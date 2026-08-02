use std::collections::HashMap;
use std::sync::Arc;

use crate::runtime::{RuntimeError, RuntimeErrorKind, RuntimeResult, Value};

pub type BuiltinFn = fn(&[Value]) -> RuntimeResult<Value>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BuiltinArity {
    Exact(usize),
    Range { min: usize, max: usize },
    Variadic { min: usize },
}

impl BuiltinArity {
    fn accepts(self, count: usize) -> bool {
        match self {
            Self::Exact(expected) => count == expected,
            Self::Range { min, max } => (min..=max).contains(&count),
            Self::Variadic { min } => count >= min,
        }
    }
    fn describe(self) -> String {
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
        registry.register("abs", BuiltinArity::Exact(1), abs);
        registry.register("add", BuiltinArity::Exact(2), add);
        registry.register("empty", BuiltinArity::Exact(1), empty);
        registry.register("exists", BuiltinArity::Exact(1), exists_without_vm_context);
        registry.register("get", BuiltinArity::Range { min: 2, max: 3 }, get);
        registry.register("join", BuiltinArity::Range { min: 1, max: 2 }, join);
        registry.register("len", BuiltinArity::Exact(1), len);
        registry.register("max", BuiltinArity::Exact(1), max);
        registry.register("min", BuiltinArity::Exact(1), min);
        registry.register("printf", BuiltinArity::Variadic { min: 1 }, printf);
        registry.register("range", BuiltinArity::Range { min: 1, max: 3 }, range);
        registry.register("reverse", BuiltinArity::Exact(1), reverse);
        registry.register("sort", BuiltinArity::Exact(1), sort);
        registry.register("split", BuiltinArity::Range { min: 1, max: 2 }, split);
        registry.register("string", BuiltinArity::Exact(1), string);
        registry.register("tolower", BuiltinArity::Exact(1), tolower);
        registry.register("toupper", BuiltinArity::Exact(1), toupper);
        registry.register("type", BuiltinArity::Exact(1), value_type);
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

fn len(args: &[Value]) -> RuntimeResult<Value> {
    let length = match &args[0] {
        Value::String(value) => value.chars().count(),
        Value::Blob(value) => value.len(),
        Value::List(value) => value.len(),
        Value::Dictionary(value) => value.len(),
        other => {
            return Err(type_error(
                "len",
                "String, Blob, List, or Dictionary",
                other,
            ));
        }
    };
    Ok(Value::Integer(length as i64))
}
fn empty(args: &[Value]) -> RuntimeResult<Value> {
    Ok(Value::Bool(!args[0].is_truthy()))
}

fn exists_without_vm_context(args: &[Value]) -> RuntimeResult<Value> {
    if !matches!(args[0], Value::String(_)) {
        return Err(type_error("exists", "String", &args[0]));
    }
    // The VM intercepts exists() to inspect live runtime namespaces.
    Ok(Value::Integer(0))
}
fn value_type(args: &[Value]) -> RuntimeResult<Value> {
    let code = match args[0] {
        Value::Integer(_) => 0,
        Value::String(_) => 1,
        Value::Closure(_) | Value::Builtin(_) | Value::HostFunction(_) => 2,
        Value::List(_) => 3,
        Value::Dictionary(_) => 4,
        Value::Float(_) => 5,
        Value::Bool(_) => 6,
        Value::Null => 7,
        Value::Blob(_) => 10,
        Value::Future(_) => 11,
        Value::HostObject(_) => 12,
    };
    Ok(Value::Integer(code))
}
fn string(args: &[Value]) -> RuntimeResult<Value> {
    Ok(Value::String(Arc::from(vim_string(&args[0]))))
}
fn abs(args: &[Value]) -> RuntimeResult<Value> {
    match args[0] {
        Value::Integer(value) => value
            .checked_abs()
            .map(Value::Integer)
            .ok_or_else(|| error("E805", "integer overflow")),
        Value::Float(value) => Ok(Value::Float(value.abs())),
        ref other => Err(type_error("abs", "Number or Float", other)),
    }
}
fn add(args: &[Value]) -> RuntimeResult<Value> {
    let Value::List(values) = &args[0] else {
        return Err(type_error("add", "List", &args[0]));
    };
    let mut result = values.clone();
    result.push(args[1].clone());
    Ok(Value::List(result))
}
fn get(args: &[Value]) -> RuntimeResult<Value> {
    let default = args.get(2).cloned().unwrap_or(Value::Null);
    match (&args[0], &args[1]) {
        (Value::List(values), Value::Integer(index)) => Ok(normalize_index(*index, values.len())
            .and_then(|index| values.get(index).cloned())
            .unwrap_or(default)),
        (Value::Dictionary(values), key) => {
            Ok(values.get(&key_string(key)?).cloned().unwrap_or(default))
        }
        (Value::String(value), Value::Integer(index)) => {
            Ok(normalize_index(*index, value.chars().count())
                .and_then(|index| value.chars().nth(index))
                .map(|ch| Value::String(Arc::from(ch.to_string())))
                .unwrap_or(default))
        }
        (other, _) => Err(type_error("get", "List, Dictionary, or String", other)),
    }
}
fn range(args: &[Value]) -> RuntimeResult<Value> {
    let numbers: Result<Vec<_>, _> = args
        .iter()
        .map(|value| match value {
            Value::Integer(value) => Ok(*value),
            other => Err(type_error("range", "Number", other)),
        })
        .collect();
    let numbers = numbers?;
    let (start, end, stride) = match numbers.as_slice() {
        [end] => (0, *end - 1, 1),
        [start, end] => (*start, *end, 1),
        [start, end, stride] => (*start, *end, *stride),
        _ => unreachable!(),
    };
    if stride == 0 {
        return Err(error("E726", "stride is zero"));
    }
    let mut values = Vec::new();
    let mut current = start;
    while if stride > 0 {
        current <= end
    } else {
        current >= end
    } {
        values.push(Value::Integer(current));
        current = current
            .checked_add(stride)
            .ok_or_else(|| error("E805", "integer overflow"))?;
        if values.len() > 1_000_000 {
            return Err(error("E1240", "result is too large"));
        }
    }
    Ok(Value::List(values))
}
fn join(args: &[Value]) -> RuntimeResult<Value> {
    let Value::List(values) = &args[0] else {
        return Err(type_error("join", "List", &args[0]));
    };
    let separator = match args.get(1) {
        Some(Value::String(value)) => value.as_ref(),
        Some(other) => return Err(type_error("join", "String separator", other)),
        None => " ",
    };
    Ok(Value::String(Arc::from(
        values
            .iter()
            .map(vim_display)
            .collect::<Vec<_>>()
            .join(separator),
    )))
}
fn split(args: &[Value]) -> RuntimeResult<Value> {
    let Value::String(value) = &args[0] else {
        return Err(type_error("split", "String", &args[0]));
    };
    let parts: Vec<_> = match args.get(1) {
        Some(Value::String(separator)) if !separator.is_empty() => value
            .split(separator.as_ref())
            .map(|part| Value::String(Arc::from(part)))
            .collect(),
        Some(Value::String(_)) | None => value
            .split_whitespace()
            .map(|part| Value::String(Arc::from(part)))
            .collect(),
        Some(other) => return Err(type_error("split", "String separator", other)),
    };
    Ok(Value::List(parts))
}
fn min(args: &[Value]) -> RuntimeResult<Value> {
    extremum(&args[0], true)
}
fn max(args: &[Value]) -> RuntimeResult<Value> {
    extremum(&args[0], false)
}
fn extremum(value: &Value, minimum: bool) -> RuntimeResult<Value> {
    let Value::List(values) = value else {
        return Err(type_error(
            if minimum { "min" } else { "max" },
            "List",
            value,
        ));
    };
    let mut numbers = values.iter().map(|value| match value {
        Value::Integer(value) => Ok(*value),
        other => Err(type_error(
            if minimum { "min" } else { "max" },
            "List of Numbers",
            other,
        )),
    });
    let Some(first) = numbers.next() else {
        return Ok(Value::Integer(0));
    };
    let mut result = first?;
    for value in numbers {
        let value = value?;
        result = if minimum {
            result.min(value)
        } else {
            result.max(value)
        };
    }
    Ok(Value::Integer(result))
}
fn reverse(args: &[Value]) -> RuntimeResult<Value> {
    match &args[0] {
        Value::List(values) => {
            let mut result = values.clone();
            result.reverse();
            Ok(Value::List(result))
        }
        Value::String(value) => Ok(Value::String(Arc::from(
            value.chars().rev().collect::<String>(),
        ))),
        other => Err(type_error("reverse", "List or String", other)),
    }
}
fn sort(args: &[Value]) -> RuntimeResult<Value> {
    let Value::List(values) = &args[0] else {
        return Err(type_error("sort", "List", &args[0]));
    };
    let mut result = values.clone();
    result.sort_by_key(vim_display);
    Ok(Value::List(result))
}
fn tolower(args: &[Value]) -> RuntimeResult<Value> {
    let Value::String(value) = &args[0] else {
        return Err(type_error("tolower", "String", &args[0]));
    };
    Ok(Value::String(Arc::from(value.to_lowercase())))
}
fn toupper(args: &[Value]) -> RuntimeResult<Value> {
    let Value::String(value) = &args[0] else {
        return Err(type_error("toupper", "String", &args[0]));
    };
    Ok(Value::String(Arc::from(value.to_uppercase())))
}
fn printf(args: &[Value]) -> RuntimeResult<Value> {
    let Value::String(format) = &args[0] else {
        return Err(type_error("printf", "String format", &args[0]));
    };
    let mut output = String::new();
    let mut values = args[1..].iter();
    let mut chars = format.chars();
    while let Some(ch) = chars.next() {
        if ch != '%' {
            output.push(ch);
            continue;
        }
        match chars.next() {
            Some('%') => output.push('%'),
            Some('s' | 'd' | 'f') => {
                let value = values
                    .next()
                    .ok_or_else(|| error("E766", "insufficient arguments for printf"))?;
                output.push_str(&vim_display(value));
            }
            Some(specifier) => {
                return Err(error(
                    "E767",
                    format!("invalid printf conversion %{specifier}"),
                ));
            }
            None => return Err(error("E767", "trailing % in printf")),
        }
    }
    Ok(Value::String(Arc::from(output)))
}

fn type_error(function: &str, expected: &str, actual: &Value) -> RuntimeError {
    error(
        "E745",
        format!("{function} expected {expected}, got {}", actual.type_name()),
    )
}
fn error(code: &str, message: impl Into<String>) -> RuntimeError {
    RuntimeError::coded(code, RuntimeErrorKind::TypeError, message)
}
fn normalize_index(index: i64, len: usize) -> Option<usize> {
    let index = if index < 0 { len as i64 + index } else { index };
    (index >= 0 && index < len as i64).then_some(index as usize)
}
fn key_string(value: &Value) -> RuntimeResult<String> {
    match value {
        Value::String(value) => Ok(value.to_string()),
        Value::Integer(value) => Ok(value.to_string()),
        other => Err(type_error("get", "String or Number key", other)),
    }
}
fn vim_display(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::Bool(value) => i32::from(*value).to_string(),
        Value::Integer(value) => value.to_string(),
        Value::Float(value) => value.to_string(),
        Value::String(value) => value.to_string(),
        other => vim_string(other),
    }
}
fn vim_string(value: &Value) -> String {
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
