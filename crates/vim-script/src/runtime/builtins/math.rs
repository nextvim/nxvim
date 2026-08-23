use crate::runtime::{RuntimeResult, Value};
use super::{error, type_error};

fn unary_float<F>(args: &[Value], name: &str, f: F) -> RuntimeResult<Value>
where
    F: FnOnce(f64) -> f64,
{
    let val = match args[0] {
        Value::Float(v) => v,
        Value::Integer(v) => v as f64,
        ref other => return Err(type_error(name, "Float or Number", other)),
    };
    Ok(Value::Float(f(val)))
}

fn binary_float<F>(args: &[Value], name: &str, f: F) -> RuntimeResult<Value>
where
    F: FnOnce(f64, f64) -> f64,
{
    let val1 = match args[0] {
        Value::Float(v) => v,
        Value::Integer(v) => v as f64,
        ref other => return Err(type_error(name, "Float or Number", other)),
    };
    let val2 = match args[1] {
        Value::Float(v) => v,
        Value::Integer(v) => v as f64,
        ref other => return Err(type_error(name, "Float or Number", other)),
    };
    Ok(Value::Float(f(val1, val2)))
}

pub fn abs(args: &[Value]) -> RuntimeResult<Value> {
    match args[0] {
        Value::Integer(value) => value
            .checked_abs()
            .map(Value::Integer)
            .ok_or_else(|| error("E805", "integer overflow")),
        Value::Float(value) => Ok(Value::Float(value.abs())),
        ref other => Err(type_error("abs", "Number or Float", other)),
    }
}

pub fn acos(args: &[Value]) -> RuntimeResult<Value> {
    unary_float(args, "acos", |x| x.acos())
}

pub fn asin(args: &[Value]) -> RuntimeResult<Value> {
    unary_float(args, "asin", |x| x.asin())
}

pub fn atan(args: &[Value]) -> RuntimeResult<Value> {
    unary_float(args, "atan", |x| x.atan())
}

pub fn atan2(args: &[Value]) -> RuntimeResult<Value> {
    binary_float(args, "atan2", |y, x| y.atan2(x))
}

pub fn ceil(args: &[Value]) -> RuntimeResult<Value> {
    unary_float(args, "ceil", |x| x.ceil())
}

pub fn cos(args: &[Value]) -> RuntimeResult<Value> {
    unary_float(args, "cos", |x| x.cos())
}

pub fn cosh(args: &[Value]) -> RuntimeResult<Value> {
    unary_float(args, "cosh", |x| x.cosh())
}

pub fn exp(args: &[Value]) -> RuntimeResult<Value> {
    unary_float(args, "exp", |x| x.exp())
}

pub fn floor(args: &[Value]) -> RuntimeResult<Value> {
    unary_float(args, "floor", |x| x.floor())
}

pub fn fmod(args: &[Value]) -> RuntimeResult<Value> {
    binary_float(args, "fmod", |x, y| x % y)
}

pub fn log(args: &[Value]) -> RuntimeResult<Value> {
    unary_float(args, "log", |x| x.ln())
}

pub fn log10(args: &[Value]) -> RuntimeResult<Value> {
    unary_float(args, "log10", |x| x.log10())
}

pub fn pow(args: &[Value]) -> RuntimeResult<Value> {
    binary_float(args, "pow", |x, y| x.powf(y))
}

pub fn round(args: &[Value]) -> RuntimeResult<Value> {
    unary_float(args, "round", |x| x.round())
}

pub fn sin(args: &[Value]) -> RuntimeResult<Value> {
    unary_float(args, "sin", |x| x.sin())
}

pub fn sinh(args: &[Value]) -> RuntimeResult<Value> {
    unary_float(args, "sinh", |x| x.sinh())
}

pub fn sqrt(args: &[Value]) -> RuntimeResult<Value> {
    unary_float(args, "sqrt", |x| x.sqrt())
}

pub fn tan(args: &[Value]) -> RuntimeResult<Value> {
    unary_float(args, "tan", |x| x.tan())
}

pub fn tanh(args: &[Value]) -> RuntimeResult<Value> {
    unary_float(args, "tanh", |x| x.tanh())
}

pub fn trunc(args: &[Value]) -> RuntimeResult<Value> {
    unary_float(args, "trunc", |x| x.trunc())
}

pub fn float2nr(args: &[Value]) -> RuntimeResult<Value> {
    match args[0] {
        Value::Integer(val) => Ok(Value::Integer(val)),
        Value::Float(val) => {
            if val.is_nan() {
                Ok(Value::Integer(i64::MIN))
            } else if val >= i64::MAX as f64 {
                Ok(Value::Integer(i64::MAX))
            } else if val <= i64::MIN as f64 {
                Ok(Value::Integer(i64::MIN + 1))
            } else {
                Ok(Value::Integer(val as i64))
            }
        }
        ref other => Err(type_error("float2nr", "Float or Number", other)),
    }
}

pub fn isinf(args: &[Value]) -> RuntimeResult<Value> {
    match args[0] {
        Value::Float(v) => {
            if v.is_infinite() {
                if v.is_sign_positive() {
                    Ok(Value::Integer(1))
                } else {
                    Ok(Value::Integer(-1))
                }
            } else {
                Ok(Value::Integer(0))
            }
        }
        _ => Ok(Value::Integer(0)),
    }
}

pub fn isnan(args: &[Value]) -> RuntimeResult<Value> {
    match args[0] {
        Value::Float(v) => Ok(Value::Integer(if v.is_nan() { 1 } else { 0 })),
        _ => Ok(Value::Integer(0)),
    }
}

pub fn and(args: &[Value]) -> RuntimeResult<Value> {
    let Value::Integer(val1) = args[0] else {
        return Err(type_error("and", "Integer", &args[0]));
    };
    let Value::Integer(val2) = args[1] else {
        return Err(type_error("and", "Integer", &args[1]));
    };
    Ok(Value::Integer(val1 & val2))
}

pub fn or(args: &[Value]) -> RuntimeResult<Value> {
    let Value::Integer(val1) = args[0] else {
        return Err(type_error("or", "Integer", &args[0]));
    };
    let Value::Integer(val2) = args[1] else {
        return Err(type_error("or", "Integer", &args[1]));
    };
    Ok(Value::Integer(val1 | val2))
}

pub fn xor(args: &[Value]) -> RuntimeResult<Value> {
    let Value::Integer(val1) = args[0] else {
        return Err(type_error("xor", "Integer", &args[0]));
    };
    let Value::Integer(val2) = args[1] else {
        return Err(type_error("xor", "Integer", &args[1]));
    };
    Ok(Value::Integer(val1 ^ val2))
}

pub fn invert(args: &[Value]) -> RuntimeResult<Value> {
    let Value::Integer(val) = args[0] else {
        return Err(type_error("invert", "Integer", &args[0]));
    };
    Ok(Value::Integer(!val))
}

fn xoshiro128_star_star(s: &mut [u32; 4]) -> u32 {
    let result = s[0].wrapping_mul(5).rotate_left(7).wrapping_mul(9);
    let t = s[1] << 9;
    s[2] ^= s[0];
    s[3] ^= s[1];
    s[1] ^= s[2];
    s[0] ^= s[3];
    s[2] ^= t;
    s[3] = s[3].rotate_left(11);
    result
}

pub fn srand(args: &[Value]) -> RuntimeResult<Value> {
    let mut s = [0u32; 4];
    if args.is_empty() {
        use std::time::SystemTime;
        let nanos = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        s[0] = (nanos & 0xFFFFFFFF) as u32;
        s[1] = ((nanos >> 32) & 0xFFFFFFFF) as u32;
        s[2] = ((nanos >> 64) & 0xFFFFFFFF) as u32;
        s[3] = ((nanos >> 96) & 0xFFFFFFFF) as u32;
    } else {
        let Value::Integer(val) = args[0] else {
            return Err(type_error("srand", "Integer", &args[0]));
        };
        let mut x = val as u64;
        for i in 0..4 {
            x = x.wrapping_add(0x9e3779b97f4a7c15);
            let mut z = x;
            z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
            z = z ^ (z >> 31);
            s[i] = (z & 0xFFFFFFFF) as u32;
        }
    }
    if s.iter().all(|&x| x == 0) {
        s[0] = 1;
    }
    Ok(Value::List(s.iter().map(|&x| Value::Integer(x as i64)).collect()))
}

pub fn rand(args: &[Value]) -> RuntimeResult<Value> {
    if args.is_empty() {
        use std::cell::RefCell;
        thread_local! {
            static INTERNAL_SEED: RefCell<[u32; 4]> = const { RefCell::new([1, 2, 3, 4]) };
        }
        let val = INTERNAL_SEED.with(|seed| {
            let mut s = seed.borrow_mut();
            let res = xoshiro128_star_star(&mut s);
            res
        });
        Ok(Value::Integer(val as i64))
    } else {
        let Value::List(ref list) = args[0] else {
            return Err(type_error("rand", "List", &args[0]));
        };
        if list.len() != 4 {
            return Err(error("E15", "invalid argument for rand(): seed list must have 4 elements"));
        }
        let mut s = [0u32; 4];
        for i in 0..4 {
            let Value::Integer(x) = list[i] else {
                return Err(type_error("rand", "Integer seed elements", &list[i]));
            };
            s[i] = x as u32;
        }
        let val = xoshiro128_star_star(&mut s);
        Ok(Value::Integer(val as i64))
    }
}

pub fn register(registry: &mut super::BuiltinRegistry) {
    use super::BuiltinArity;
    registry.register("abs", BuiltinArity::Exact(1), abs);
    registry.register("acos", BuiltinArity::Exact(1), acos);
    registry.register("and", BuiltinArity::Exact(2), and);
    registry.register("asin", BuiltinArity::Exact(1), asin);
    registry.register("atan", BuiltinArity::Exact(1), atan);
    registry.register("atan2", BuiltinArity::Exact(2), atan2);
    registry.register("ceil", BuiltinArity::Exact(1), ceil);
    registry.register("cos", BuiltinArity::Exact(1), cos);
    registry.register("cosh", BuiltinArity::Exact(1), cosh);
    registry.register("exp", BuiltinArity::Exact(1), exp);
    registry.register("float2nr", BuiltinArity::Exact(1), float2nr);
    registry.register("floor", BuiltinArity::Exact(1), floor);
    registry.register("fmod", BuiltinArity::Exact(2), fmod);
    registry.register("invert", BuiltinArity::Exact(1), invert);
    registry.register("isinf", BuiltinArity::Exact(1), isinf);
    registry.register("isnan", BuiltinArity::Exact(1), isnan);
    registry.register("log", BuiltinArity::Exact(1), log);
    registry.register("log10", BuiltinArity::Exact(1), log10);
    registry.register("or", BuiltinArity::Exact(2), or);
    registry.register("pow", BuiltinArity::Exact(2), pow);
    registry.register("rand", BuiltinArity::Range { min: 0, max: 1 }, rand);
    registry.register("round", BuiltinArity::Exact(1), round);
    registry.register("sin", BuiltinArity::Exact(1), sin);
    registry.register("sinh", BuiltinArity::Exact(1), sinh);
    registry.register("sqrt", BuiltinArity::Exact(1), sqrt);
    registry.register("srand", BuiltinArity::Range { min: 0, max: 1 }, srand);
    registry.register("tan", BuiltinArity::Exact(1), tan);
    registry.register("tanh", BuiltinArity::Exact(1), tanh);
    registry.register("trunc", BuiltinArity::Exact(1), trunc);
    registry.register("xor", BuiltinArity::Exact(2), xor);
}
