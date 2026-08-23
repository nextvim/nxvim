use crate::runtime::{RuntimeResult, Value};

pub fn value_type(args: &[Value]) -> RuntimeResult<Value> {
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
