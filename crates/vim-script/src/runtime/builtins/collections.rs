use std::sync::Arc;

use crate::runtime::{RuntimeResult, Value};
use super::{error, type_error, normalize_index, key_string, vim_display};

pub fn len(args: &[Value]) -> RuntimeResult<Value> {
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

pub fn empty(args: &[Value]) -> RuntimeResult<Value> {
    Ok(Value::Bool(!args[0].is_truthy()))
}

pub fn add(args: &[Value]) -> RuntimeResult<Value> {
    let Value::List(values) = &args[0] else {
        return Err(type_error("add", "List", &args[0]));
    };
    let mut result = values.clone();
    result.push(args[1].clone());
    Ok(Value::List(result))
}

pub fn get(args: &[Value]) -> RuntimeResult<Value> {
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

pub fn range(args: &[Value]) -> RuntimeResult<Value> {
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

pub fn min(args: &[Value]) -> RuntimeResult<Value> {
    extremum(&args[0], true)
}

pub fn max(args: &[Value]) -> RuntimeResult<Value> {
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

pub fn reverse(args: &[Value]) -> RuntimeResult<Value> {
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

pub fn sort(args: &[Value]) -> RuntimeResult<Value> {
    let Value::List(values) = &args[0] else {
        return Err(type_error("sort", "List", &args[0]));
    };
    let mut result = values.clone();
    result.sort_by_key(vim_display);
    Ok(Value::List(result))
}
