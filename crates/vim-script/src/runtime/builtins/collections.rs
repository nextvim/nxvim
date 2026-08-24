use std::sync::Arc;

use super::{error, key_string, normalize_index, type_error, vim_display};
use crate::runtime::{RuntimeResult, Value};

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

pub fn copy(args: &[Value]) -> RuntimeResult<Value> {
    Ok(args[0].clone())
}

pub fn deepcopy(args: &[Value]) -> RuntimeResult<Value> {
    Ok(args[0].clone())
}

pub fn has_key(args: &[Value]) -> RuntimeResult<Value> {
    let Value::Dictionary(dict) = &args[0] else {
        return Err(type_error("has_key", "Dictionary", &args[0]));
    };
    let Value::String(key) = &args[1] else {
        return Err(type_error("has_key", "String key", &args[1]));
    };
    Ok(Value::Bool(dict.contains_key(key.as_ref())))
}

pub fn keys(args: &[Value]) -> RuntimeResult<Value> {
    let Value::Dictionary(dict) = &args[0] else {
        return Err(type_error("keys", "Dictionary", &args[0]));
    };
    let list = dict
        .keys()
        .map(|k| Value::String(Arc::from(k.as_str())))
        .collect();
    Ok(Value::List(list))
}

pub fn values(args: &[Value]) -> RuntimeResult<Value> {
    let Value::Dictionary(dict) = &args[0] else {
        return Err(type_error("values", "Dictionary", &args[0]));
    };
    let list = dict.values().cloned().collect();
    Ok(Value::List(list))
}

pub fn items(args: &[Value]) -> RuntimeResult<Value> {
    let Value::Dictionary(dict) = &args[0] else {
        return Err(type_error("items", "Dictionary", &args[0]));
    };
    let list = dict
        .iter()
        .map(|(k, v)| Value::List(vec![Value::String(Arc::from(k.as_str())), v.clone()]))
        .collect();
    Ok(Value::List(list))
}

pub fn insert(args: &[Value]) -> RuntimeResult<Value> {
    let Value::List(values) = &args[0] else {
        return Err(type_error("insert", "List", &args[0]));
    };
    let item = args[1].clone();
    let idx = match args.get(2) {
        Some(Value::Integer(i)) => {
            if *i < 0 {
                let pos = values.len() as i64 + *i;
                if pos < 0 { 0 } else { pos as usize }
            } else {
                (*i as usize).min(values.len())
            }
        }
        _ => 0,
    };
    let mut result = values.clone();
    result.insert(idx, item);
    Ok(Value::List(result))
}

pub fn remove(args: &[Value]) -> RuntimeResult<Value> {
    match &args[0] {
        Value::List(values) => {
            let Value::Integer(idx) = &args[1] else {
                return Err(type_error("remove", "Integer index", &args[1]));
            };
            let idx_u = if *idx < 0 {
                (values.len() as i64 + *idx).max(0) as usize
            } else {
                *idx as usize
            };
            if idx_u >= values.len() {
                return Err(error("E684", "list index out of range"));
            }
            match args.get(2) {
                Some(Value::Integer(end)) => {
                    let end_u = if *end < 0 {
                        (values.len() as i64 + *end).max(0) as usize
                    } else {
                        *end as usize
                    };
                    if end_u >= values.len() || end_u < idx_u {
                        return Err(error("E684", "list index out of range"));
                    }
                    let removed = values[idx_u..=end_u].to_vec();
                    Ok(Value::List(removed))
                }
                _ => Ok(values[idx_u].clone()),
            }
        }
        Value::Dictionary(dict) => {
            let Value::String(key) = &args[1] else {
                return Err(type_error("remove", "String key", &args[1]));
            };
            Ok(dict.get(key.as_ref()).cloned().unwrap_or(Value::Null))
        }
        other => Err(type_error("remove", "List or Dictionary", other)),
    }
}

pub fn repeat(args: &[Value]) -> RuntimeResult<Value> {
    let Value::Integer(count) = &args[1] else {
        return Err(type_error("repeat", "Integer count", &args[1]));
    };
    if *count <= 0 {
        return match &args[0] {
            Value::List(_) => Ok(Value::List(Vec::new())),
            Value::String(_) => Ok(Value::String(Arc::from(""))),
            other => Err(type_error("repeat", "List or String", other)),
        };
    }
    match &args[0] {
        Value::List(values) => {
            let mut result = Vec::new();
            for _ in 0..*count {
                result.extend(values.clone());
            }
            Ok(Value::List(result))
        }
        Value::String(s) => {
            let repeated = s.repeat(*count as usize);
            Ok(Value::String(Arc::from(repeated)))
        }
        other => Err(type_error("repeat", "List or String", other)),
    }
}

pub fn slice(args: &[Value]) -> RuntimeResult<Value> {
    let Value::Integer(start) = &args[1] else {
        return Err(type_error("slice", "Integer start", &args[1]));
    };
    match &args[0] {
        Value::List(values) => {
            let start_idx = if *start < 0 {
                (values.len() as i64 + *start).max(0) as usize
            } else {
                (*start as usize).min(values.len())
            };
            let end_idx = match args.get(2) {
                Some(Value::Integer(end)) => {
                    if *end < 0 {
                        (values.len() as i64 + *end).max(0) as usize
                    } else {
                        (*end as usize).min(values.len())
                    }
                }
                _ => values.len(),
            };
            if start_idx >= end_idx {
                Ok(Value::List(Vec::new()))
            } else {
                Ok(Value::List(values[start_idx..end_idx].to_vec()))
            }
        }
        Value::String(s) => {
            let chars: Vec<char> = s.chars().collect();
            let start_idx = if *start < 0 {
                (chars.len() as i64 + *start).max(0) as usize
            } else {
                (*start as usize).min(chars.len())
            };
            let end_idx = match args.get(2) {
                Some(Value::Integer(end)) => {
                    if *end < 0 {
                        (chars.len() as i64 + *end).max(0) as usize
                    } else {
                        (*end as usize).min(chars.len())
                    }
                }
                _ => chars.len(),
            };
            if start_idx >= end_idx {
                Ok(Value::String(Arc::from("")))
            } else {
                let sub: String = chars[start_idx..end_idx].iter().collect();
                Ok(Value::String(Arc::from(sub)))
            }
        }
        other => Err(type_error("slice", "List or String", other)),
    }
}

pub fn list2blob(args: &[Value]) -> RuntimeResult<Value> {
    let Value::List(values) = &args[0] else {
        return Err(type_error("list2blob", "List", &args[0]));
    };
    let mut blob = Vec::new();
    for val in values {
        let Value::Integer(b) = val else {
            return Err(type_error("list2blob", "List of Integers", val));
        };
        blob.push(*b as u8);
    }
    Ok(Value::Blob(Arc::from(blob.into_boxed_slice())))
}

pub fn blob2list(args: &[Value]) -> RuntimeResult<Value> {
    let Value::Blob(blob) = &args[0] else {
        return Err(type_error("blob2list", "Blob", &args[0]));
    };
    let list = blob.iter().map(|&b| Value::Integer(b as i64)).collect();
    Ok(Value::List(list))
}

pub fn list2str(args: &[Value]) -> RuntimeResult<Value> {
    let Value::List(values) = &args[0] else {
        return Err(type_error("list2str", "List", &args[0]));
    };
    let utf8 = match args.get(1) {
        Some(Value::Bool(v)) => *v,
        Some(Value::Integer(v)) => *v != 0,
        _ => true,
    };
    let mut result = String::new();
    if utf8 {
        for val in values {
            let Value::Integer(nr) = val else {
                return Err(type_error("list2str", "List of Integers", val));
            };
            if let Some(ch) = std::char::from_u32(*nr as u32) {
                result.push(ch);
            }
        }
    } else {
        let mut bytes = Vec::new();
        for val in values {
            let Value::Integer(nr) = val else {
                return Err(type_error("list2str", "List of Integers", val));
            };
            bytes.push(*nr as u8);
        }
        result = String::from_utf8(bytes)
            .unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned());
    }
    Ok(Value::String(Arc::from(result)))
}

pub fn count(args: &[Value]) -> RuntimeResult<Value> {
    let ic = match args.get(2) {
        Some(Value::Bool(b)) => *b,
        Some(Value::Integer(v)) => *v != 0,
        _ => false,
    };
    let start = match args.get(3) {
        Some(Value::Integer(idx)) => *idx as usize,
        _ => 0,
    };
    match &args[0] {
        Value::List(values) => {
            if start >= values.len() {
                return Ok(Value::Integer(0));
            }
            let target = &args[1];
            let count = values[start..].iter().filter(|&x| x == target).count();
            Ok(Value::Integer(count as i64))
        }
        Value::String(s) => {
            let Value::String(target) = &args[1] else {
                return Err(type_error("count", "String target", &args[1]));
            };
            if start >= s.len() {
                return Ok(Value::Integer(0));
            }
            let source = &s[start..];
            let count = if ic {
                let s_lower = source.to_lowercase();
                let t_lower = target.to_lowercase();
                s_lower.matches(&t_lower).count()
            } else {
                source.matches(target.as_ref()).count()
            };
            Ok(Value::Integer(count as i64))
        }
        Value::Dictionary(dict) => {
            let target = &args[1];
            let count = dict.values().filter(|&x| x == target).count();
            Ok(Value::Integer(count as i64))
        }
        other => Err(type_error("count", "List, String, or Dictionary", other)),
    }
}

pub fn extend(args: &[Value]) -> RuntimeResult<Value> {
    match (&args[0], &args[1]) {
        (Value::List(list1), Value::List(list2)) => {
            let mut result = list1.clone();
            result.extend(list2.clone());
            Ok(Value::List(result))
        }
        (Value::Dictionary(dict1), Value::Dictionary(dict2)) => {
            let mut result = dict1.clone();
            result.extend(dict2.clone());
            Ok(Value::Dictionary(result))
        }
        (other, _) => Err(type_error("extend", "List or Dictionary", other)),
    }
}

pub fn extendnew(args: &[Value]) -> RuntimeResult<Value> {
    extend(args)
}

pub fn flatten(args: &[Value]) -> RuntimeResult<Value> {
    let Value::List(values) = &args[0] else {
        return Err(type_error("flatten", "List", &args[0]));
    };
    let max_depth = match args.get(1) {
        Some(Value::Integer(d)) => *d,
        _ => 1,
    };
    let mut result = Vec::new();
    fn flatten_helper(list: &[Value], depth: i64, max_depth: i64, out: &mut Vec<Value>) {
        for item in list {
            if let Value::List(sub) = item {
                if depth < max_depth {
                    flatten_helper(sub, depth + 1, max_depth, out);
                } else {
                    out.push(item.clone());
                }
            } else {
                out.push(item.clone());
            }
        }
    }
    flatten_helper(values, 0, max_depth, &mut result);
    Ok(Value::List(result))
}

pub fn flattennew(args: &[Value]) -> RuntimeResult<Value> {
    flatten(args)
}

pub fn uniq(args: &[Value]) -> RuntimeResult<Value> {
    let Value::List(values) = &args[0] else {
        return Err(type_error("uniq", "List", &args[0]));
    };
    let mut result = Vec::new();
    for item in values {
        if result.last() != Some(item) {
            result.push(item.clone());
        }
    }
    Ok(Value::List(result))
}

pub fn index(args: &[Value]) -> RuntimeResult<Value> {
    let target = &args[1];
    let start = match args.get(2) {
        Some(Value::Integer(idx)) => *idx as usize,
        _ => 0,
    };
    match &args[0] {
        Value::List(values) => {
            if start >= values.len() {
                return Ok(Value::Integer(-1));
            }
            let pos = values[start..].iter().position(|x| x == target);
            Ok(Value::Integer(
                pos.map(|idx| (idx + start) as i64).unwrap_or(-1),
            ))
        }
        other => Err(type_error("index", "List", other)),
    }
}

pub fn register(registry: &mut super::BuiltinRegistry) {
    use super::BuiltinArity;
    registry.register("add", BuiltinArity::Exact(2), add);
    registry.register("blob2list", BuiltinArity::Exact(1), blob2list);
    registry.register("copy", BuiltinArity::Exact(1), copy);
    registry.register("count", BuiltinArity::Range { min: 2, max: 4 }, count);
    registry.register("deepcopy", BuiltinArity::Range { min: 1, max: 2 }, deepcopy);
    registry.register("empty", BuiltinArity::Exact(1), empty);
    registry.register("extend", BuiltinArity::Range { min: 2, max: 3 }, extend);
    registry.register(
        "extendnew",
        BuiltinArity::Range { min: 2, max: 3 },
        extendnew,
    );
    registry.register("flatten", BuiltinArity::Range { min: 1, max: 2 }, flatten);
    registry.register(
        "flattennew",
        BuiltinArity::Range { min: 1, max: 2 },
        flattennew,
    );
    registry.register("get", BuiltinArity::Range { min: 2, max: 3 }, get);
    registry.register("has_key", BuiltinArity::Exact(2), has_key);
    registry.register("index", BuiltinArity::Range { min: 2, max: 4 }, index);
    registry.register("insert", BuiltinArity::Range { min: 2, max: 3 }, insert);
    registry.register("items", BuiltinArity::Exact(1), items);
    registry.register("keys", BuiltinArity::Exact(1), keys);
    registry.register("len", BuiltinArity::Exact(1), len);
    registry.register("list2blob", BuiltinArity::Exact(1), list2blob);
    registry.register("list2str", BuiltinArity::Range { min: 1, max: 2 }, list2str);
    registry.register("max", BuiltinArity::Exact(1), max);
    registry.register("min", BuiltinArity::Exact(1), min);
    registry.register("range", BuiltinArity::Range { min: 1, max: 3 }, range);
    registry.register("remove", BuiltinArity::Range { min: 2, max: 3 }, remove);
    registry.register("repeat", BuiltinArity::Exact(2), repeat);
    registry.register("reverse", BuiltinArity::Exact(1), reverse);
    registry.register("slice", BuiltinArity::Range { min: 2, max: 3 }, slice);
    registry.register("sort", BuiltinArity::Exact(1), sort);
    registry.register("uniq", BuiltinArity::Exact(1), uniq);
    registry.register("values", BuiltinArity::Exact(1), values);
}
