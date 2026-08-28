//! Substitute command logic.
//!
//! Implementation of `:s` / `:substitute` command.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubstituteArgs {
    pub pattern: String,
    pub replacement: String,
    pub flags: String,
}

pub fn parse_substitute(args: &str) -> Result<SubstituteArgs, String> {
    let mut chars = args.chars().peekable();
    // Trim leading whitespace
    while chars.peek().map(|&c| c.is_whitespace()).unwrap_or(false) {
        chars.next();
    }

    let delimiter = match chars.next() {
        None => {
            // Empty arguments: reuse last substitute
            return Ok(SubstituteArgs {
                pattern: String::new(),
                replacement: String::new(),
                flags: String::new(),
            });
        }
        Some(c) if is_valid_delimiter(c) => c,
        Some(c) => return Err(format!("Invalid delimiter: {}", c)),
    };

    // Parse pattern
    let mut pattern = String::new();
    let mut found_second_delim = false;
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(&next_c) = chars.peek() {
                if next_c == delimiter {
                    pattern.push(delimiter);
                    chars.next();
                    continue;
                }
            }
            pattern.push(c);
        } else if c == delimiter {
            found_second_delim = true;
            break;
        } else {
            pattern.push(c);
        }
    }

    if !found_second_delim {
        return Ok(SubstituteArgs {
            pattern,
            replacement: String::new(),
            flags: String::new(),
        });
    }

    // Parse replacement
    let mut replacement = String::new();
    let mut found_third_delim = false;
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(&next_c) = chars.peek() {
                if next_c == delimiter {
                    replacement.push(delimiter);
                    chars.next();
                    continue;
                }
            }
            replacement.push(c);
        } else if c == delimiter {
            found_third_delim = true;
            break;
        } else {
            replacement.push(c);
        }
    }

    // Parse flags
    let mut flags = String::new();
    while let Some(c) = chars.next() {
        if !c.is_whitespace() {
            flags.push(c);
        }
    }

    Ok(SubstituteArgs {
        pattern,
        replacement,
        flags,
    })
}

fn is_valid_delimiter(c: char) -> bool {
    !c.is_alphanumeric() && !c.is_whitespace() && c != '\\' && c != '"'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_substitute() {
        assert_eq!(
            parse_substitute("/foo/bar/g").unwrap(),
            SubstituteArgs {
                pattern: "foo".to_string(),
                replacement: "bar".to_string(),
                flags: "g".to_string(),
            }
        );
        assert_eq!(
            parse_substitute("#foo#bar#gc").unwrap(),
            SubstituteArgs {
                pattern: "foo".to_string(),
                replacement: "bar".to_string(),
                flags: "gc".to_string(),
            }
        );
        assert_eq!(
            parse_substitute("/foo\\/baz/bar/").unwrap(),
            SubstituteArgs {
                pattern: "foo/baz".to_string(),
                replacement: "bar".to_string(),
                flags: "".to_string(),
            }
        );
        assert_eq!(
            parse_substitute("").unwrap(),
            SubstituteArgs {
                pattern: "".to_string(),
                replacement: "".to_string(),
                flags: "".to_string(),
            }
        );
    }
}
