use crate::controller::ex::{Ex, ExCommand, Range};
use std::collections::HashMap;

pub struct ExMap {
    commands: HashMap<String, (Ex, usize)>,
}

impl ExMap {
    /// Creates a new `ExMap` initialized with all standard/default Vim ex commands.
    pub fn new() -> Self {
        let mut map = Self {
            commands: HashMap::new(),
        };
        map.register_defaults();
        map
    }

    /// Parses an input string (e.g. "1,10w file.txt") and tries to resolve it to an `ExCommand`.
    /// Handles ranges and whitespace-separated arguments.
    pub fn try_resolve(&self, input: &str) -> Option<ExCommand> {
        let input = input.trim();
        if input.is_empty() {
            return None;
        }

        // Try to parse range first
        let (range, remainder) = match self.try_resolve_range(input) {
            Some((r, rem)) => (Some(r), rem),
            None => (None, input),
        };

        let remainder = remainder.trim_start();
        if remainder.is_empty() {
            return None;
        }

        // Split the remainder into the command word and the remainder (arguments)
        let parts: Vec<&str> = remainder.splitn(2, |c: char| c.is_whitespace()).collect();
        let cmd_word = parts[0];

        let op = self.lookup(cmd_word)?;

        let arguments = if parts.len() > 1 {
            let args: Vec<String> = parts[1].split_whitespace().map(|s| s.to_string()).collect();
            if args.is_empty() { None } else { Some(args) }
        } else {
            None
        };

        Some(ExCommand {
            range,
            op,
            arguments,
        })
    }

    /// Parses the range portion at the start of a command string.
    /// Returns the parsed `Range` and the remaining command string on success.
    pub fn try_resolve_range<'a>(&self, input: &'a str) -> Option<(Range, &'a str)> {
        let input = input.trim_start();
        if input.is_empty() {
            return None;
        }

        enum Address {
            Line(u32),
            Cursor,
            Pattern(String),
        }

        fn parse_address(s: &str) -> Option<(Address, &str)> {
            let s = s.trim_start();
            if s.starts_with('/') {
                let next_slash = s[1..].find('/')?;
                let pattern = s[1..next_slash + 1].to_string();
                Some((Address::Pattern(pattern), &s[next_slash + 2..]))
            } else if s.starts_with("'<") {
                Some((Address::Cursor, &s[2..]))
            } else if s.starts_with("'>") {
                Some((Address::Cursor, &s[2..]))
            } else if s.starts_with('.') {
                Some((Address::Cursor, &s[1..]))
            } else {
                let digits: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
                if !digits.is_empty() {
                    let line_num = digits.parse::<u32>().ok()?;
                    Some((Address::Line(line_num), &s[digits.len()..]))
                } else {
                    None
                }
            }
        }

        // Try parsing the first address
        let (first_addr, rest) = match parse_address(input) {
            Some((addr, r)) => (Some(addr), r),
            None => {
                if input.starts_with('%') {
                    let range = Range {
                        start_line: Some(1),
                        end_line: None,
                        start_at_cursor: None,
                        end_at_cursor: None,
                        start_pattern: None,
                        end_pattern: None,
                    };
                    return Some((range, &input[1..]));
                }
                return None;
            }
        };

        let rest = rest.trim_start();
        // Check for address separators
        if rest.starts_with(',') || rest.starts_with(';') {
            let after_sep = &rest[1..];
            if let Some((second_addr, final_rest)) = parse_address(after_sep) {
                let mut range = Range {
                    start_line: None,
                    end_line: None,
                    start_at_cursor: None,
                    end_at_cursor: None,
                    start_pattern: None,
                    end_pattern: None,
                };

                match first_addr {
                    Some(Address::Line(l)) => range.start_line = Some(l),
                    Some(Address::Cursor) => range.start_at_cursor = Some(true),
                    Some(Address::Pattern(p)) => range.start_pattern = Some(p),
                    None => {}
                }

                match second_addr {
                    Address::Line(l) => range.end_line = Some(l),
                    Address::Cursor => range.end_at_cursor = Some(true),
                    Address::Pattern(p) => range.end_pattern = Some(p),
                }

                return Some((range, final_rest));
            }
        }

        // Only single address parsed
        if let Some(addr) = first_addr {
            let mut range = Range {
                start_line: None,
                end_line: None,
                start_at_cursor: None,
                end_at_cursor: None,
                start_pattern: None,
                end_pattern: None,
            };
            match addr {
                Address::Line(l) => {
                    range.start_line = Some(l);
                    range.end_line = Some(l);
                }
                Address::Cursor => {
                    range.start_at_cursor = Some(true);
                    range.end_at_cursor = Some(true);
                }
                Address::Pattern(p) => {
                    range.start_pattern = Some(p.clone());
                    range.end_pattern = Some(p);
                }
            }
            return Some((range, rest));
        }

        None
    }

    /// Dynamically registers a new ex command pattern.
    /// Supports bracket notation like "n[ext]" where the text outside the brackets
    /// is the minimum required prefix, and the full name is the concatenation of both parts.
    pub fn register(&mut self, pattern: &str, variant: Ex) {
        let (full_name, min_prefix_len) = if let Some(open_idx) = pattern.find('[') {
            if let Some(close_idx) = pattern.find(']') {
                let min_prefix = &pattern[..open_idx];
                let rest = &pattern[open_idx + 1..close_idx];
                let full = format!("{}{}", min_prefix, rest);
                (full, min_prefix.len())
            } else {
                (pattern.to_string(), pattern.len())
            }
        } else {
            (pattern.to_string(), pattern.len())
        };

        self.commands.insert(full_name, (variant, min_prefix_len));
    }

    /// Looks up a string command against the registered mappings.
    pub fn lookup(&self, cmd: &str) -> Option<Ex> {
        let cmd = cmd.trim();
        if cmd.is_empty() {
            return None;
        }

        // Special symbol commands
        if cmd == "#" {
            return Some(Ex::Number);
        }

        for (full_name, &(variant, min_prefix_len)) in &self.commands {
            if cmd.len() >= min_prefix_len
                && cmd.starts_with(&full_name[..min_prefix_len])
                && full_name.starts_with(cmd)
            {
                return Some(variant);
            }
        }

        None
    }

    fn register_defaults(&mut self) {
        // --- File Management & Lifecycle ---
        self.register("w[rite]", Ex::Write);
        self.register("q[uit]", Ex::Quit);
        self.register("wq", Ex::Wq);
        self.register("x[it]", Ex::Xit);
        self.register("up[date]", Ex::Update);

        // --- Buffer & File Loading ---
        self.register("e[dit]", Ex::Edit);
        self.register("r[ead]", Ex::Read);

        // --- Argument List Navigation ---
        self.register("n[ext]", Ex::Next);
        self.register("prev[ious]", Ex::Prev);
        self.register("fir[st]", Ex::First);
        self.register("la[st]", Ex::Last);

        // --- Buffer Management ---
        self.register("b[uffer]", Ex::Buffer);
        self.register("bn[ext]", Ex::Bnext);
        self.register("bp[revious]", Ex::Bprev);
        self.register("bprev", Ex::Bprev);
        self.register("bd[elete]", Ex::Bdelete);
        self.register("ls", Ex::Buffers);
        self.register("files", Ex::Buffers);
        self.register("buffers", Ex::Buffers);

        // --- Tab Management ---
        self.register("tabe[dit]", Ex::Tabnew);
        self.register("tabnew", Ex::Tabnew);
        self.register("tabn[ext]", Ex::Tabnext);
        self.register("tabp[revious]", Ex::Tabprev);
        self.register("tabN", Ex::Tabprev);
        self.register("tabc[lose]", Ex::Tabclose);
        self.register("tabo[nly]", Ex::Tabonly);

        // --- Window Splitting ---
        self.register("sp[lit]", Ex::Split);
        self.register("vs[plit]", Ex::Vsplit);
        self.register("clo[se]", Ex::Close);
        self.register("on[ly]", Ex::Only);

        // --- Editing & Manipulation ---
        self.register("d[elete]", Ex::Delete);
        self.register("y[ank]", Ex::Yank);
        self.register("pu[t]", Ex::Put);
        self.register("j[oin]", Ex::Join);
        self.register("s[ubstitute]", Ex::Substitute);

        // --- Search & Execution ---
        self.register("g[lobal]", Ex::Global);
        self.register("v[global]", Ex::Vglobal);

        // --- Display & Inspection ---
        self.register("p[rint]", Ex::Print);
        self.register("nu[mber]", Ex::Number);
        self.register("l[ist]", Ex::List);
        self.register("marks", Ex::Marks);
        self.register("reg[isters]", Ex::Registers);

        // --- Undo & Redo ---
        self.register("u[ndo]", Ex::Undo);
        self.register("red[o]", Ex::Redo);

        // --- Configuration & Help ---
        self.register("se[t]", Ex::Set);
        self.register("colo[rschemes]", Ex::Colorschemes);
        self.register("syn[tax]", Ex::Syntax);
        self.register("h[elp]", Ex::Help);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ex_lookup_defaults() {
        let map = ExMap::new();
        // Exact prefix
        assert_eq!(map.lookup("w"), Some(Ex::Write));
        assert_eq!(map.lookup("write"), Some(Ex::Write));
        assert_eq!(map.lookup("wr"), Some(Ex::Write));

        // Unknown Command
        assert_eq!(map.lookup("writer"), None);
        assert_eq!(map.lookup("unknown_cmd"), None);

        // Special Character
        assert_eq!(map.lookup("#"), Some(Ex::Number));
    }

    #[test]
    fn test_ex_custom_registration() {
        let mut map = ExMap::new();
        map.register("custom[cmd]", Ex::Help);

        // Minimum prefix "custom"
        assert_eq!(map.lookup("custom"), Some(Ex::Help));
        assert_eq!(map.lookup("customc"), Some(Ex::Help));
        assert_eq!(map.lookup("customcmd"), Some(Ex::Help));

        // Shorter than minimum prefix should fail
        assert_eq!(map.lookup("custo"), None);
        // Exceeding full name should fail
        assert_eq!(map.lookup("customcmdx"), None);
    }

    #[test]
    fn test_ex_try_resolve() {
        let map = ExMap::new();

        // No arguments
        let cmd1 = map.try_resolve("w").unwrap();
        assert_eq!(cmd1.op, Ex::Write);
        assert_eq!(cmd1.arguments, None);

        // With single argument
        let cmd2 = map.try_resolve("write file.txt").unwrap();
        assert_eq!(cmd2.op, Ex::Write);
        assert_eq!(cmd2.arguments, Some(vec!["file.txt".to_string()]));

        // With multiple arguments and extra spaces
        let cmd3 = map.try_resolve("   edit   foo   bar   ").unwrap();
        assert_eq!(cmd3.op, Ex::Edit);
        assert_eq!(
            cmd3.arguments,
            Some(vec!["foo".to_string(), "bar".to_string()])
        );

        // Invalid command
        assert!(map.try_resolve("invalid_command arg1").is_none());
    }

    #[test]
    fn test_ex_try_resolve_range() {
        let map = ExMap::new();

        // Numeric range: 1,10
        let (r1, rem1) = map.try_resolve_range("1,10d").unwrap();
        assert_eq!(r1.start_line, Some(1));
        assert_eq!(r1.end_line, Some(10));
        assert_eq!(rem1, "d");

        // Numeric range with semicolon: 1;10
        let (r2, rem2) = map.try_resolve_range("1;10y").unwrap();
        assert_eq!(r2.start_line, Some(1));
        assert_eq!(r2.end_line, Some(10));
        assert_eq!(rem2, "y");

        // Visual selection range: '<,'>
        let (r3, rem3) = map.try_resolve_range("'<,'>w").unwrap();
        assert_eq!(r3.start_at_cursor, Some(true));
        assert_eq!(r3.end_at_cursor, Some(true));
        assert_eq!(rem3, "w");

        // Pattern range: /pattern1/,/pattern2/
        let (r4, rem4) = map.try_resolve_range("/pattern1/,/pattern2/s").unwrap();
        assert_eq!(r4.start_pattern, Some("pattern1".to_string()));
        assert_eq!(r4.end_pattern, Some("pattern2".to_string()));
        assert_eq!(rem4, "s");

        // Single line number
        let (r5, rem5) = map.try_resolve_range("42p").unwrap();
        assert_eq!(r5.start_line, Some(42));
        assert_eq!(r5.end_line, Some(42));
        assert_eq!(rem5, "p");

        // Whole file range: %
        let (r6, rem6) = map.try_resolve_range("%d").unwrap();
        assert_eq!(r6.start_line, Some(1));
        assert_eq!(r6.end_line, None);
        assert_eq!(rem6, "d");
    }

    #[test]
    fn test_try_resolve_with_range() {
        let map = ExMap::new();

        // Try resolve parsing numeric range + command + arguments
        let cmd = map.try_resolve("10,20write file.txt").unwrap();
        assert_eq!(cmd.op, Ex::Write);
        assert_eq!(cmd.arguments, Some(vec!["file.txt".to_string()]));
        let range = cmd.range.unwrap();
        assert_eq!(range.start_line, Some(10));
        assert_eq!(range.end_line, Some(20));
    }
}
