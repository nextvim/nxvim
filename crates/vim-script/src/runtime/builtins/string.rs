use std::sync::Arc;

use super::{error, type_error, vim_display, vim_string};
use crate::runtime::{RuntimeResult, Value};

const BASE64_CHARS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64_encode_bytes(bytes: &[u8]) -> String {
    let mut result = String::new();
    let mut i = 0;
    while i < bytes.len() {
        let b0 = bytes[i];
        let b1 = bytes.get(i + 1).cloned();
        let b2 = bytes.get(i + 2).cloned();

        let val = ((b0 as u32) << 16) | ((b1.unwrap_or(0) as u32) << 8) | (b2.unwrap_or(0) as u32);

        result.push(BASE64_CHARS[((val >> 18) & 63) as usize] as char);
        result.push(BASE64_CHARS[((val >> 12) & 63) as usize] as char);
        if b1.is_some() {
            result.push(BASE64_CHARS[((val >> 6) & 63) as usize] as char);
        } else {
            result.push('=');
        }
        if b2.is_some() {
            result.push(BASE64_CHARS[(val & 63) as usize] as char);
        } else {
            result.push('=');
        }
        i += 3;
    }
    result
}

fn base64_decode_string(s: &str) -> Option<Vec<u8>> {
    let s = s.trim();
    if s.len() % 4 != 0 {
        return None;
    }
    let mut bytes = Vec::new();
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c0 = chars[i];
        let c1 = chars[i + 1];
        let c2 = chars[i + 2];
        let c3 = chars[i + 3];

        let idx0 = BASE64_CHARS.iter().position(|&x| x as char == c0)? as u32;
        let idx1 = BASE64_CHARS.iter().position(|&x| x as char == c1)? as u32;
        let idx2 = if c2 == '=' {
            0
        } else {
            BASE64_CHARS.iter().position(|&x| x as char == c2)? as u32
        };
        let idx3 = if c3 == '=' {
            0
        } else {
            BASE64_CHARS.iter().position(|&x| x as char == c3)? as u32
        };

        let val = (idx0 << 18) | (idx1 << 12) | (idx2 << 6) | idx3;
        bytes.push((val >> 16) as u8);
        if c2 != '=' {
            bytes.push((val >> 8) as u8);
        }
        if c3 != '=' {
            bytes.push(val as u8);
        }
        i += 4;
    }
    Some(bytes)
}

pub fn string(args: &[Value]) -> RuntimeResult<Value> {
    Ok(Value::String(Arc::from(vim_string(&args[0]))))
}

pub fn tolower(args: &[Value]) -> RuntimeResult<Value> {
    let Value::String(value) = &args[0] else {
        return Err(type_error("tolower", "String", &args[0]));
    };
    Ok(Value::String(Arc::from(value.to_lowercase())))
}

pub fn toupper(args: &[Value]) -> RuntimeResult<Value> {
    let Value::String(value) = &args[0] else {
        return Err(type_error("toupper", "String", &args[0]));
    };
    Ok(Value::String(Arc::from(value.to_uppercase())))
}

pub fn join(args: &[Value]) -> RuntimeResult<Value> {
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

pub fn split(args: &[Value]) -> RuntimeResult<Value> {
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

pub fn printf(args: &[Value]) -> RuntimeResult<Value> {
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

pub fn escape(args: &[Value]) -> RuntimeResult<Value> {
    let Value::String(s) = &args[0] else {
        return Err(type_error("escape", "String", &args[0]));
    };
    let Value::String(chars) = &args[1] else {
        return Err(type_error("escape", "String chars", &args[1]));
    };
    let mut result = String::new();
    for c in s.chars() {
        if chars.contains(c) {
            result.push('\\');
        }
        result.push(c);
    }
    Ok(Value::String(Arc::from(result)))
}

pub fn fnameescape(args: &[Value]) -> RuntimeResult<Value> {
    let Value::String(s) = &args[0] else {
        return Err(type_error("fnameescape", "String", &args[0]));
    };
    let escape_chars = " \t\n*?[{`$\\|%#~<>";
    let mut result = String::new();
    for c in s.chars() {
        if escape_chars.contains(c) {
            result.push('\\');
        }
        result.push(c);
    }
    Ok(Value::String(Arc::from(result)))
}

pub fn char2nr(args: &[Value]) -> RuntimeResult<Value> {
    let s = match &args[0] {
        Value::String(val) => val.as_ref(),
        Value::Integer(val) => return Ok(Value::Integer(*val)),
        ref other => return Err(type_error("char2nr", "String or Integer", other)),
    };
    let utf8 = match args.get(1) {
        Some(Value::Bool(v)) => *v,
        Some(Value::Integer(v)) => *v != 0,
        _ => true,
    };
    let nr = if s.is_empty() {
        0
    } else {
        let ch = s.chars().next().unwrap();
        if utf8 {
            ch as i64
        } else {
            let mut buf = [0; 4];
            let encoded = ch.encode_utf8(&mut buf);
            encoded.as_bytes()[0] as i64
        }
    };
    Ok(Value::Integer(nr))
}

pub fn nr2char(args: &[Value]) -> RuntimeResult<Value> {
    let Value::Integer(nr) = args[0] else {
        return Err(type_error("nr2char", "Integer", &args[0]));
    };
    let ch = std::char::from_u32(nr as u32).unwrap_or('\0');
    Ok(Value::String(Arc::from(ch.to_string())))
}

pub fn str2nr(args: &[Value]) -> RuntimeResult<Value> {
    let Value::String(s) = &args[0] else {
        return Err(type_error("str2nr", "String", &args[0]));
    };
    let base = match args.get(1) {
        Some(Value::Integer(b)) => *b as u32,
        _ => 10,
    };
    let s_trimmed = s.trim();
    let (prefix, content) = if s_trimmed.starts_with('-') {
        ("-", &s_trimmed[1..])
    } else if s_trimmed.starts_with('+') {
        ("", &s_trimmed[1..])
    } else {
        ("", s_trimmed)
    };

    let is_valid_digit = |c: char| -> bool {
        match base {
            2 => c == '0' || c == '1',
            8 => c >= '0' && c <= '7',
            16 => c.is_digit(16),
            _ => c.is_digit(10),
        }
    };

    let end_idx = content
        .find(|c| !is_valid_digit(c))
        .unwrap_or(content.len());
    let valid_part = &content[..end_idx];
    if valid_part.is_empty() {
        return Ok(Value::Integer(0));
    }

    let parsed = i64::from_str_radix(valid_part, base)
        .map(|v| if prefix == "-" { -v } else { v })
        .unwrap_or(0);
    Ok(Value::Integer(parsed))
}

pub fn str2float(args: &[Value]) -> RuntimeResult<Value> {
    let Value::String(s) = &args[0] else {
        return Err(type_error("str2float", "String", &args[0]));
    };
    let parsed: f64 = s.trim().parse().unwrap_or(0.0);
    Ok(Value::Float(parsed))
}

pub fn str2list(args: &[Value]) -> RuntimeResult<Value> {
    let Value::String(s) = &args[0] else {
        return Err(type_error("str2list", "String", &args[0]));
    };
    let utf8 = match args.get(1) {
        Some(Value::Bool(v)) => *v,
        Some(Value::Integer(v)) => *v != 0,
        _ => true,
    };
    let list = if utf8 {
        s.chars().map(|c| Value::Integer(c as i64)).collect()
    } else {
        s.as_bytes()
            .iter()
            .map(|&b| Value::Integer(b as i64))
            .collect()
    };
    Ok(Value::List(list))
}

pub fn strlen(args: &[Value]) -> RuntimeResult<Value> {
    let Value::String(s) = &args[0] else {
        return Err(type_error("strlen", "String", &args[0]));
    };
    Ok(Value::Integer(s.len() as i64))
}

pub fn strcharlen(args: &[Value]) -> RuntimeResult<Value> {
    let Value::String(s) = &args[0] else {
        return Err(type_error("strcharlen", "String", &args[0]));
    };
    Ok(Value::Integer(s.chars().count() as i64))
}

pub fn strchars(args: &[Value]) -> RuntimeResult<Value> {
    strcharlen(args)
}

pub fn strwidth(args: &[Value]) -> RuntimeResult<Value> {
    strcharlen(args)
}

pub fn strgetchar(args: &[Value]) -> RuntimeResult<Value> {
    let Value::String(s) = &args[0] else {
        return Err(type_error("strgetchar", "String", &args[0]));
    };
    let Value::Integer(idx) = args[1] else {
        return Err(type_error("strgetchar", "Integer index", &args[1]));
    };
    if idx < 0 {
        return Ok(Value::Integer(-1));
    }
    let code = s.chars().nth(idx as usize).map(|c| c as i64).unwrap_or(-1);
    Ok(Value::Integer(code))
}

pub fn stridx(args: &[Value]) -> RuntimeResult<Value> {
    let Value::String(haystack) = &args[0] else {
        return Err(type_error("stridx", "String haystack", &args[0]));
    };
    let Value::String(needle) = &args[1] else {
        return Err(type_error("stridx", "String needle", &args[1]));
    };
    let start = match args.get(2) {
        Some(Value::Integer(idx)) => *idx as usize,
        _ => 0,
    };
    if start > haystack.len() {
        return Ok(Value::Integer(-1));
    }
    let sub = &haystack[start..];
    let pos = sub
        .find(needle.as_ref())
        .map(|idx| (idx + start) as i64)
        .unwrap_or(-1);
    Ok(Value::Integer(pos))
}

pub fn strridx(args: &[Value]) -> RuntimeResult<Value> {
    let Value::String(haystack) = &args[0] else {
        return Err(type_error("strridx", "String haystack", &args[0]));
    };
    let Value::String(needle) = &args[1] else {
        return Err(type_error("strridx", "String needle", &args[1]));
    };
    let start = match args.get(2) {
        Some(Value::Integer(idx)) => *idx as usize,
        _ => haystack.len(),
    };
    let limit = start.min(haystack.len());
    let sub = &haystack[..limit];
    let pos = sub
        .rfind(needle.as_ref())
        .map(|idx| idx as i64)
        .unwrap_or(-1);
    Ok(Value::Integer(pos))
}

pub fn strpart(args: &[Value]) -> RuntimeResult<Value> {
    let Value::String(s) = &args[0] else {
        return Err(type_error("strpart", "String", &args[0]));
    };
    let Value::Integer(start) = args[1] else {
        return Err(type_error("strpart", "Integer start", &args[1]));
    };
    let start = if start < 0 { 0 } else { start as usize };
    if start >= s.len() {
        return Ok(Value::String(Arc::from("")));
    }
    let len = match args.get(2) {
        Some(Value::Integer(l)) => {
            if *l < 0 {
                0
            } else {
                *l as usize
            }
        }
        _ => s.len() - start,
    };
    let end = (start + len).min(s.len());
    Ok(Value::String(Arc::from(&s[start..end])))
}

pub fn strcharpart(args: &[Value]) -> RuntimeResult<Value> {
    let Value::String(s) = &args[0] else {
        return Err(type_error("strcharpart", "String", &args[0]));
    };
    let Value::Integer(start) = args[1] else {
        return Err(type_error("strcharpart", "Integer start", &args[1]));
    };
    let start = if start < 0 { 0 } else { start as usize };
    let total_chars = s.chars().count();
    if start >= total_chars {
        return Ok(Value::String(Arc::from("")));
    }
    let len = match args.get(2) {
        Some(Value::Integer(l)) => {
            if *l < 0 {
                0
            } else {
                *l as usize
            }
        }
        _ => total_chars - start,
    };
    let end = (start + len).min(total_chars);
    let sub: String = s.chars().skip(start).take(end - start).collect();
    Ok(Value::String(Arc::from(sub)))
}

pub fn trim(args: &[Value]) -> RuntimeResult<Value> {
    let Value::String(s) = &args[0] else {
        return Err(type_error("trim", "String", &args[0]));
    };
    let mask = match args.get(1) {
        Some(Value::String(m)) => Some(m.as_ref()),
        _ => None,
    };
    let is_trim_char = |c: char| -> bool {
        if let Some(m) = mask {
            m.contains(c)
        } else {
            c.is_whitespace()
        }
    };
    let trimmed = s.trim_matches(is_trim_char);
    Ok(Value::String(Arc::from(trimmed)))
}

pub fn tr(args: &[Value]) -> RuntimeResult<Value> {
    let Value::String(src) = &args[0] else {
        return Err(type_error("tr", "String src", &args[0]));
    };
    let Value::String(fromstr) = &args[1] else {
        return Err(type_error("tr", "String fromstr", &args[1]));
    };
    let Value::String(tostr) = &args[2] else {
        return Err(type_error("tr", "String tostr", &args[2]));
    };
    let from_chars: Vec<char> = fromstr.chars().collect();
    let to_chars: Vec<char> = tostr.chars().collect();

    let mut result = String::new();
    for c in src.chars() {
        if let Some(pos) = from_chars.iter().position(|&x| x == c) {
            if pos < to_chars.len() {
                result.push(to_chars[pos]);
            }
        } else {
            result.push(c);
        }
    }
    Ok(Value::String(Arc::from(result)))
}

pub fn uri_encode(args: &[Value]) -> RuntimeResult<Value> {
    let Value::String(s) = &args[0] else {
        return Err(type_error("uri_encode", "String", &args[0]));
    };
    let mut result = String::new();
    for b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                result.push(*b as char);
            }
            _ => {
                result.push_str(&format!("%{:02X}", b));
            }
        }
    }
    Ok(Value::String(Arc::from(result)))
}

pub fn uri_decode(args: &[Value]) -> RuntimeResult<Value> {
    let Value::String(s) = &args[0] else {
        return Err(type_error("uri_decode", "String", &args[0]));
    };
    let mut bytes = Vec::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '%' {
            let h0 = chars.next().unwrap_or('\0');
            let h1 = chars.next().unwrap_or('\0');
            if let Some(hex) = h0
                .to_digit(16)
                .and_then(|d0| h1.to_digit(16).map(|d1| (d0 << 4) | d1))
            {
                bytes.push(hex as u8);
            } else {
                bytes.push(b'%');
                if h0 != '\0' {
                    bytes.push(h0 as u8);
                }
                if h1 != '\0' {
                    bytes.push(h1 as u8);
                }
            }
        } else {
            bytes.push(c as u8);
        }
    }
    let decoded = String::from_utf8(bytes)
        .unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned());
    Ok(Value::String(Arc::from(decoded)))
}

pub fn base64_encode(args: &[Value]) -> RuntimeResult<Value> {
    let Value::Blob(blob) = &args[0] else {
        return Err(type_error("base64_encode", "Blob", &args[0]));
    };
    let encoded = base64_encode_bytes(blob.as_ref());
    Ok(Value::String(Arc::from(encoded)))
}

pub fn base64_decode(args: &[Value]) -> RuntimeResult<Value> {
    let Value::String(s) = &args[0] else {
        return Err(type_error("base64_decode", "String", &args[0]));
    };
    let decoded = base64_decode_string(s.as_ref())
        .ok_or_else(|| error("E15", "invalid base64 format for base64_decode()"))?;
    Ok(Value::Blob(Arc::from(decoded.into_boxed_slice())))
}

pub fn register(registry: &mut super::BuiltinRegistry) {
    use super::BuiltinArity;
    registry.register("base64_decode", BuiltinArity::Exact(1), base64_decode);
    registry.register("base64_encode", BuiltinArity::Exact(1), base64_encode);
    registry.register("char2nr", BuiltinArity::Range { min: 1, max: 2 }, char2nr);
    registry.register("escape", BuiltinArity::Exact(2), escape);
    registry.register("fnameescape", BuiltinArity::Exact(1), fnameescape);
    registry.register("join", BuiltinArity::Range { min: 1, max: 2 }, join);
    registry.register("nr2char", BuiltinArity::Range { min: 1, max: 2 }, nr2char);
    registry.register("printf", BuiltinArity::Variadic { min: 1 }, printf);
    registry.register("split", BuiltinArity::Range { min: 1, max: 2 }, split);
    registry.register("str2float", BuiltinArity::Exact(1), str2float);
    registry.register("str2list", BuiltinArity::Range { min: 1, max: 2 }, str2list);
    registry.register("str2nr", BuiltinArity::Range { min: 1, max: 2 }, str2nr);
    registry.register("strcharlen", BuiltinArity::Exact(1), strcharlen);
    registry.register(
        "strcharpart",
        BuiltinArity::Range { min: 2, max: 3 },
        strcharpart,
    );
    registry.register("strchars", BuiltinArity::Range { min: 1, max: 2 }, strchars);
    registry.register("strgetchar", BuiltinArity::Exact(2), strgetchar);
    registry.register("stridx", BuiltinArity::Range { min: 2, max: 3 }, stridx);
    registry.register("strlen", BuiltinArity::Exact(1), strlen);
    registry.register("strpart", BuiltinArity::Range { min: 2, max: 4 }, strpart);
    registry.register("strridx", BuiltinArity::Range { min: 2, max: 3 }, strridx);
    registry.register("strwidth", BuiltinArity::Exact(1), strwidth);
    registry.register("tolower", BuiltinArity::Exact(1), tolower);
    registry.register("toupper", BuiltinArity::Exact(1), toupper);
    registry.register("tr", BuiltinArity::Exact(3), tr);
    registry.register("trim", BuiltinArity::Range { min: 1, max: 3 }, trim);
    registry.register("uri_decode", BuiltinArity::Exact(1), uri_decode);
    registry.register("uri_encode", BuiltinArity::Exact(1), uri_encode);
    registry.register("string", BuiltinArity::Exact(1), string);
}
