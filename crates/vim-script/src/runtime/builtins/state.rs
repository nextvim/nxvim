use crate::runtime::{RuntimeResult, Value};
use super::type_error;

pub fn exists_without_vm_context(args: &[Value]) -> RuntimeResult<Value> {
    if !matches!(args[0], Value::String(_)) {
        return Err(type_error("exists", "String", &args[0]));
    }
    // The VM intercepts exists() to inspect live runtime namespaces.
    Ok(Value::Integer(0))
}
