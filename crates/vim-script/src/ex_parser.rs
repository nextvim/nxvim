use crate::ast::{Address, CommandModifier, CommandRange, ExCommand, RangeSeparator};
use crate::source::{Diagnostic, SourceId, Span};

type ExParseResult<T> = Result<T, Box<Diagnostic>>;

#[derive(Clone, Debug, PartialEq)]
pub struct ParsedExLine {
    pub command: ExCommand,
    pub span: Span,
    pub command_span: Span,
    pub arguments_span: Span,
    pub raw_arguments: String,
    pub next_command: Option<Span>,
}

#[derive(Clone, Debug)]
pub struct ExLineParser<'a> {
    source: &'a str,
    source_id: SourceId,
    base: usize,
    cursor: usize,
}

impl<'a> ExLineParser<'a> {
    pub fn new(source_id: SourceId, source: &'a str, base: usize) -> Self {
        Self {
            source,
            source_id,
            base,
            cursor: 0,
        }
    }

    pub fn parse(mut self) -> ExParseResult<ParsedExLine> {
        self.horizontal_space();
        if self.peek() == Some(':') {
            self.bump();
            self.horizontal_space();
        }
        let start = self.cursor;
        let is_search_cmd = self.peek() == Some('/') || self.peek() == Some('?');
        let command_start = self.cursor;

        let (modifiers, range, name, bang, command_end) = if is_search_cmd {
            let delimiter = self.bump().unwrap();
            (Vec::new(), None, delimiter.to_string(), false, self.cursor)
        } else {
            let modifiers = self.modifiers();
            let range = self.range()?;
            self.horizontal_space();
            let name = self.word();
            if name.is_empty() {
                return Err(self
                    .error(
                        "X001",
                        "expected Ex command name",
                        self.cursor,
                        self.cursor.saturating_add(1),
                    )
                    .into());
            }
            let command_end = self.cursor;
            let bang = if self.peek() == Some('!') {
                self.bump();
                true
            } else {
                false
            };
            (modifiers, range, name, bang, command_end)
        };

        self.horizontal_space();
        let arguments_start = self.cursor;
        let separator = find_command_separator(self.source, arguments_start);
        let arguments_end = separator.unwrap_or(self.source.len());
        let raw_arguments = self.source[arguments_start..arguments_end]
            .trim_end()
            .to_owned();
        let trimmed_end =
            arguments_start + self.source[arguments_start..arguments_end].trim_end().len();
        let next_command = separator.and_then(|separator| {
            let mut start = separator + 1;
            while self.source[start..]
                .chars()
                .next()
                .is_some_and(|ch| ch == ' ' || ch == '\t')
            {
                start += self.source[start..]
                    .chars()
                    .next()
                    .expect("character exists")
                    .len_utf8();
            }
            (start < self.source.len()).then(|| self.span(start, self.source.len()))
        });
        Ok(ParsedExLine {
            command: ExCommand {
                modifiers,
                range,
                name,
                bang,
                count: None,
                register: None,
                arguments: raw_arguments.clone(),
            },
            span: self.span(start, self.source.len()),
            command_span: self.span(command_start, command_end),
            arguments_span: self.span(arguments_start, trimmed_end),
            raw_arguments,
            next_command,
        })
    }

    fn modifiers(&mut self) -> Vec<CommandModifier> {
        let mut modifiers = Vec::new();
        loop {
            let checkpoint = self.cursor;
            let word = self.word();
            let modifier = match word.as_str() {
                "silent" => {
                    let errors = if self.peek() == Some('!') {
                        self.bump();
                        true
                    } else {
                        false
                    };
                    Some(CommandModifier::Silent { errors })
                }
                "keepjumps" => Some(CommandModifier::KeepJumps),
                "keepalt" => Some(CommandModifier::KeepAlt),
                "keepmarks" => Some(CommandModifier::KeepMarks),
                "noautocmd" => Some(CommandModifier::NoAutocmd),
                "sandbox" => Some(CommandModifier::Sandbox),
                "vertical" => Some(CommandModifier::Vertical),
                "unsilent" => Some(CommandModifier::Unsilent),
                "keeppatterns" => Some(CommandModifier::KeepPatterns),
                "lockmarks" => Some(CommandModifier::LockMarks),
                "horizontal" => Some(CommandModifier::Horizontal),
                "aboveleft" => Some(CommandModifier::AboveLeft),
                "belowright" => Some(CommandModifier::BelowRight),
                "topleft" => Some(CommandModifier::TopLeft),
                "botright" => Some(CommandModifier::BotRight),
                "leftabove" => Some(CommandModifier::LeftAbove),
                "rightbelow" => Some(CommandModifier::RightBelow),
                "confirm" => Some(CommandModifier::Confirm),
                "hide" => Some(CommandModifier::Hide),
                "browse" => Some(CommandModifier::Browse),
                "verbose" => {
                    self.horizontal_space();
                    let level = if self.peek().is_some_and(|ch| ch.is_ascii_digit()) {
                        let mut num = 0;
                        while let Some(ch) = self.peek() {
                            if ch.is_ascii_digit() {
                                num = num * 10 + (ch as u32 - '0' as u32);
                                self.bump();
                            } else {
                                break;
                            }
                        }
                        num
                    } else {
                        1
                    };
                    Some(CommandModifier::Verbose(level))
                }
                "tab" => {
                    self.horizontal_space();
                    let count = if self.peek().is_some_and(|ch| ch.is_ascii_digit()) {
                        let mut num = 0;
                        while let Some(ch) = self.peek() {
                            if ch.is_ascii_digit() {
                                num = num * 10 + (ch as u32 - '0' as u32);
                                self.bump();
                            } else {
                                break;
                            }
                        }
                        Some(num)
                    } else {
                        None
                    };
                    Some(CommandModifier::Tab(count))
                }
                "filter" => {
                    let bang = if self.peek() == Some('!') {
                        self.bump();
                        true
                    } else {
                        false
                    };
                    self.horizontal_space();
                    if let Some(delimiter) = self.peek() {
                        if !delimiter.is_ascii_alphanumeric() && delimiter != ' ' && delimiter != '\t' {
                            self.bump();
                            let start = self.cursor;
                            while let Some(ch) = self.peek() {
                                if ch == delimiter {
                                    break;
                                }
                                if ch == '\\' {
                                    self.bump();
                                }
                                self.bump();
                            }
                            let end = self.cursor;
                            if self.peek() == Some(delimiter) {
                                self.bump();
                            }
                            let pattern = self.source[start..end].to_owned();
                            Some(CommandModifier::Filter { pattern, bang })
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
                _ => None,
            };
            let Some(modifier) = modifier else {
                self.cursor = checkpoint;
                break;
            };
            modifiers.push(modifier);
            self.horizontal_space();
        }
        modifiers
    }

    fn range(&mut self) -> ExParseResult<Option<CommandRange>> {
        if self.peek() == Some('%') {
            self.bump();
            return Ok(Some(CommandRange {
                start: Address::WholeFile,
                end: None,
                separator: None,
            }));
        }
        let Some(start) = self.address()? else {
            return Ok(None);
        };
        let separator = match self.peek() {
            Some(',') => Some(RangeSeparator::Comma),
            Some(';') => Some(RangeSeparator::Semicolon),
            _ => None,
        };
        let end = if separator.is_some() {
            self.bump();
            self.horizontal_space();
            self.address()?
        } else {
            None
        };
        Ok(Some(CommandRange {
            start,
            end,
            separator,
        }))
    }

    fn address(&mut self) -> ExParseResult<Option<Address>> {
        let mut address = match self.peek() {
            Some('.') => {
                self.bump();
                Address::Current
            }
            Some('$') => {
                self.bump();
                Address::Last
            }
            Some('\'') => {
                self.bump();
                let Some(mark) = self.bump() else {
                    return Err(self
                        .error("X002", "expected mark name", self.cursor, self.cursor)
                        .into());
                };
                Address::Mark(mark)
            }
            Some('/') | Some('?') => {
                let delimiter = self.bump().expect("peeked");
                let pattern_start = self.cursor;
                while let Some(ch) = self.peek() {
                    if ch == delimiter {
                        break;
                    }
                    if ch == '\\' {
                        self.bump();
                    }
                    self.bump();
                }
                if self.peek() != Some(delimiter) {
                    return Err(self
                        .error(
                            "X003",
                            "unterminated search address",
                            pattern_start,
                            self.cursor,
                        )
                        .into());
                }
                let pattern = self.source[pattern_start..self.cursor].to_owned();
                self.bump();
                Address::Search {
                    pattern,
                    forward: delimiter == '/',
                }
            }
            Some(ch) if ch.is_ascii_digit() => {
                let start = self.cursor;
                while self.peek().is_some_and(|ch| ch.is_ascii_digit()) {
                    self.bump();
                }
                let line = self.source[start..self.cursor].parse().map_err(|_| {
                    Box::new(self.error("X004", "line number is too large", start, self.cursor))
                })?;
                Address::Line(line)
            }
            _ => return Ok(None),
        };
        while matches!(self.peek(), Some('+' | '-')) {
            let sign = self.bump().expect("peeked");
            let start = self.cursor;
            while self.peek().is_some_and(|ch| ch.is_ascii_digit()) {
                self.bump();
            }
            let amount = if start == self.cursor {
                1
            } else {
                self.source[start..self.cursor]
                    .parse::<i64>()
                    .map_err(|_| {
                        Box::new(self.error(
                            "X005",
                            "range offset is too large",
                            start,
                            self.cursor,
                        ))
                    })?
            };
            address = Address::Offset {
                base: Box::new(address),
                amount: if sign == '-' { -amount } else { amount },
            };
        }
        Ok(Some(address))
    }

    fn word(&mut self) -> String {
        let start = self.cursor;
        while self.peek().is_some_and(|ch| ch.is_ascii_alphabetic()) {
            self.bump();
        }
        self.source[start..self.cursor].to_owned()
    }
    fn horizontal_space(&mut self) {
        while matches!(self.peek(), Some(' ' | '\t')) {
            self.bump();
        }
    }
    fn peek(&self) -> Option<char> {
        self.source[self.cursor..].chars().next()
    }
    fn bump(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.cursor += ch.len_utf8();
        Some(ch)
    }
    fn span(&self, start: usize, end: usize) -> Span {
        Span::new(
            self.source_id,
            (self.base + start) as u32,
            (self.base + end) as u32,
        )
    }
    fn error(&self, code: &str, message: &str, start: usize, end: usize) -> Diagnostic {
        Diagnostic::error(
            code,
            message,
            self.span(start.min(self.source.len()), end.min(self.source.len())),
        )
    }
}

fn find_command_separator(source: &str, start: usize) -> Option<usize> {
    let mut escaped = false;
    for (relative, ch) in source[start..].char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == '|' {
            return Some(start + relative);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_arguments_while_parsing_prefixes() {
        let source = ":silent! keepjumps 10,20write! ++enc=utf-8 file name.txt | echo \"done\"";
        let parsed = ExLineParser::new(SourceId(3), source, 100).parse().unwrap();
        assert_eq!(
            parsed.command.modifiers,
            vec![
                CommandModifier::Silent { errors: true },
                CommandModifier::KeepJumps
            ]
        );
        assert_eq!(
            parsed.command.range,
            Some(CommandRange {
                start: Address::Line(10),
                end: Some(Address::Line(20)),
                separator: Some(RangeSeparator::Comma)
            })
        );
        assert_eq!(parsed.command.name, "write");
        assert!(parsed.command.bang);
        assert_eq!(parsed.raw_arguments, "++enc=utf-8 file name.txt");
        assert_eq!(
            &source[(parsed.next_command.unwrap().start - 100) as usize..],
            "echo \"done\""
        );
    }
    #[test]
    fn keeps_mapping_and_autocmd_syntax_verbatim() {
        for (source, expected) in [
            (
                "nnoremap <silent> <leader>w :write<CR>",
                "<silent> <leader>w :write<CR>",
            ),
            (
                "autocmd BufWritePost *.rs call demo#format()",
                "BufWritePost *.rs call demo#format()",
            ),
        ] {
            let parsed = ExLineParser::new(SourceId(0), source, 0).parse().unwrap();
            assert_eq!(parsed.raw_arguments, expected);
        }
    }
    #[test]
    fn escaped_bars_do_not_split_commands() {
        let parsed = ExLineParser::new(SourceId(0), "echo foo\\|bar", 0)
            .parse()
            .unwrap();
        assert_eq!(parsed.raw_arguments, "foo\\|bar");
        assert!(parsed.next_command.is_none());
    }

    #[test]
    fn parses_search_commands() {
        let parsed_forward = ExLineParser::new(SourceId(0), "/hello world", 0)
            .parse()
            .unwrap();
        assert_eq!(parsed_forward.command.name, "/");
        assert_eq!(parsed_forward.command.arguments, "hello world");

        let parsed_backward = ExLineParser::new(SourceId(0), "?foo bar", 0)
            .parse()
            .unwrap();
        assert_eq!(parsed_backward.command.name, "?");
        assert_eq!(parsed_backward.command.arguments, "foo bar");
    }
}
