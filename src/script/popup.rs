//! VimScript popup function bindings and helper parsing.

use std::collections::BTreeMap;
use std::sync::Arc;
use vim_script::runtime::{RuntimeError, RuntimeErrorKind, RuntimeResult, Value};

pub fn dummy_popup_fn(_args: &[Value]) -> RuntimeResult<Value> {
    Ok(Value::Integer(1))
}

pub fn parse_popup_args(what: &Value, opts: &Value) -> (Vec<String>, BTreeMap<String, Value>) {
    let lines = match what {
        Value::String(s) => vec![s.to_string()],
        Value::List(l) => l.iter().map(|v| v.to_string()).collect(),
        Value::Integer(bufnr) => vec![format!("Buffer {bufnr}")],
        _ => Vec::new(),
    };
    let options = match opts {
        Value::Dictionary(d) => d.clone(),
        _ => BTreeMap::new(),
    };
    (lines, options)
}

pub fn make_pos_dict(line: u32, col: u32, width: u32, height: u32) -> Value {
    let mut dict = BTreeMap::new();
    dict.insert("line".to_string(), Value::Integer(line as i64));
    dict.insert("col".to_string(), Value::Integer(col as i64));
    dict.insert("width".to_string(), Value::Integer(width as i64));
    dict.insert("height".to_string(), Value::Integer(height as i64));
    dict.insert("core_line".to_string(), Value::Integer(line as i64));
    dict.insert("core_col".to_string(), Value::Integer(col as i64));
    dict.insert("core_width".to_string(), Value::Integer(width as i64));
    dict.insert("core_height".to_string(), Value::Integer(height as i64));
    dict.insert("firstline".to_string(), Value::Integer(1));
    dict.insert("visible".to_string(), Value::Integer(1));
    Value::Dictionary(dict)
}

pub fn make_options_dict(zindex: i32, title: Option<&str>, wrap: bool) -> Value {
    let mut dict = BTreeMap::new();
    dict.insert("zindex".to_string(), Value::Integer(zindex as i64));
    if let Some(t) = title {
        dict.insert("title".to_string(), Value::String(Arc::from(t)));
    }
    dict.insert("wrap".to_string(), Value::Integer(if wrap { 1 } else { 0 }));
    dict.insert("tabpage".to_string(), Value::Integer(0));
    Value::Dictionary(dict)
}
