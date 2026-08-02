use std::collections::VecDeque;

use crate::{
    error::{LexError, LexErrorKind},
    span::{Span, Spanned},
};

/// A lossless lexical unit. Text borrows from the source string.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Token<'src> {
    Text(&'src str),
    Percent,
    Number(u32),
    Dot,
    Minus,
    LBrace,
    RBrace,
    LParen,
    RParen,
    Hash,
    Star,
    Equal,
    LessThan,
    Character(char),
}

pub type SpannedToken<'src> = Spanned<Token<'src>>;

/// Tokenizes a Vim format string while borrowing all text from the input.
///
/// Punctuation is only structural immediately after `%`. Consequently, text
/// such as `file (readonly)` is emitted as one `Text` token, while `%10.20f`
/// is emitted as `%`, `10`, `.`, `20`, and `f`.
#[derive(Clone, Debug)]
pub struct Lexer<'src> {
    source: &'src str,
    cursor: usize,
    pending: VecDeque<SpannedToken<'src>>,
    failed: bool,
    error_span: Option<Span>,
}

impl<'src> Lexer<'src> {
    pub fn new(source: &'src str) -> Self {
        Self {
            source,
            cursor: 0,
            pending: VecDeque::new(),
            failed: false,
            error_span: None,
        }
    }

    fn token(&self, value: Token<'src>, start: usize, end: usize) -> SpannedToken<'src> {
        Spanned::new(value, Span::new(start, end))
    }

    fn scan_text(&mut self) -> Option<SpannedToken<'src>> {
        if self.cursor == self.source.len() {
            return None;
        }

        let start = self.cursor;
        let relative_end = self.source[start..]
            .find('%')
            .unwrap_or(self.source.len() - start);
        let end = start + relative_end;

        if start == end {
            self.cursor += 1;
            self.scan_directive(start)
        } else {
            self.cursor = end;
            Some(self.token(Token::Text(&self.source[start..end]), start, end))
        }
    }

    fn scan_directive(&mut self, percent_start: usize) -> Option<SpannedToken<'src>> {
        self.pending
            .push_back(self.token(Token::Percent, percent_start, percent_start + 1));

        if self.cursor == self.source.len() {
            return self.pending.pop_front();
        }

        let (offset, first) = self.next_char(self.cursor).expect("cursor is in bounds");
        debug_assert_eq!(offset, self.cursor);

        match first {
            '{' => self.scan_delimited('{', '}', Token::LBrace, Token::RBrace),
            '#' => self.scan_delimited('#', '#', Token::Hash, Token::Hash),
            _ => self.scan_escape_head(),
        }

        self.pending.pop_front()
    }

    fn scan_delimited(
        &mut self,
        open: char,
        close: char,
        open_token: Token<'src>,
        close_token: Token<'src>,
    ) {
        let open_start = self.cursor;
        self.cursor += open.len_utf8();
        self.pending
            .push_back(self.token(open_token, open_start, open_start + open.len_utf8()));

        let body_start = self.cursor;
        let close_start = if open == '{' {
            self.find_expression_end(body_start)
        } else {
            self.source[body_start..]
                .find(close)
                .map(|i| body_start + i)
        };
        let body_end = close_start.unwrap_or(self.source.len());

        if body_start < body_end {
            self.pending.push_back(self.token(
                Token::Text(&self.source[body_start..body_end]),
                body_start,
                body_end,
            ));
        }

        self.cursor = body_end;
        if let Some(close_start) = close_start {
            self.cursor += close.len_utf8();
            self.pending.push_back(self.token(
                close_token,
                close_start,
                close_start + close.len_utf8(),
            ));
        }
    }

    /// Finds the matching expression brace, ignoring braces inside Vim-style
    /// single- and double-quoted strings and accounting for nested braces.
    fn find_expression_end(&self, start: usize) -> Option<usize> {
        let mut depth = 0usize;
        let mut quote = None;
        let mut escaped = false;

        for (relative, ch) in self.source[start..].char_indices() {
            let position = start + relative;
            if let Some(active_quote) = quote {
                if active_quote == '"' && ch == '\\' && !escaped {
                    escaped = true;
                    continue;
                }
                if ch == active_quote && !escaped {
                    quote = None;
                }
                escaped = false;
                continue;
            }

            match ch {
                '\'' | '"' => quote = Some(ch),
                '{' => depth += 1,
                '}' if depth == 0 => return Some(position),
                '}' => depth -= 1,
                _ => {}
            }
        }
        None
    }

    fn scan_escape_head(&mut self) {
        if self.peek_char() == Some('-') {
            self.push_char_token(Token::Minus);
        }

        if self.peek_char().is_some_and(|ch| ch.is_ascii_digit()) {
            self.scan_number();
        }

        if self.peek_char() == Some('.') {
            self.push_char_token(Token::Dot);
            if self.peek_char().is_some_and(|ch| ch.is_ascii_digit()) {
                self.scan_number();
            }
        }

        let Some(ch) = self.peek_char() else {
            return;
        };
        let token = match ch {
            '%' => Token::Percent,
            '(' => Token::LParen,
            ')' => Token::RParen,
            '*' => Token::Star,
            '=' => Token::Equal,
            '<' => Token::LessThan,
            '{' => Token::LBrace,
            '}' => Token::RBrace,
            '#' => Token::Hash,
            '-' => Token::Minus,
            '.' => Token::Dot,
            ch => Token::Character(ch),
        };
        self.push_char_token(token);
    }

    fn scan_number(&mut self) {
        let start = self.cursor;
        while self.peek_char().is_some_and(|ch| ch.is_ascii_digit()) {
            self.cursor += 1;
        }
        let end = self.cursor;
        match self.source[start..end].parse::<u32>() {
            Ok(number) => self
                .pending
                .push_back(self.token(Token::Number(number), start, end)),
            Err(_) => {
                self.failed = true;
                self.error_span = Some(Span::new(start, end));
                self.pending.clear();
            }
        }
    }

    fn push_char_token(&mut self, token: Token<'src>) {
        let start = self.cursor;
        let ch = self.peek_char().expect("character token requires input");
        self.cursor += ch.len_utf8();
        self.pending
            .push_back(self.token(token, start, self.cursor));
    }

    fn peek_char(&self) -> Option<char> {
        self.source[self.cursor..].chars().next()
    }

    fn next_char(&self, position: usize) -> Option<(usize, char)> {
        self.source[position..]
            .char_indices()
            .next()
            .map(|(offset, ch)| (position + offset, ch))
    }
}

impl<'src> Iterator for Lexer<'src> {
    type Item = Result<SpannedToken<'src>, LexError>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(token) = self.pending.pop_front() {
            return Some(Ok(token));
        }
        if self.failed {
            return None;
        }

        let token = self.scan_text();
        if self.failed {
            return Some(Err(LexError {
                kind: LexErrorKind::IntegerOverflow,
                span: self
                    .error_span
                    .expect("failed lexer must record an error span"),
            }));
        }
        token.map(Ok)
    }
}

/// Collects all tokens from `source`.
pub fn lex(source: &str) -> Result<Vec<SpannedToken<'_>>, LexError> {
    Lexer::new(source).collect()
}

#[cfg(test)]
mod tests {
    use super::{Token, lex};
    use crate::{error::LexErrorKind, span::Span};

    fn values(source: &str) -> Vec<Token<'_>> {
        lex(source)
            .unwrap()
            .into_iter()
            .map(|token| token.value)
            .collect()
    }

    #[test]
    fn plain_text_is_not_split_on_punctuation_or_unicode() {
        assert_eq!(
            values("λ.rs (readonly) #1"),
            [Token::Text("λ.rs (readonly) #1")]
        );
    }

    #[test]
    fn lexes_width_modifiers_and_escape_code() {
        assert_eq!(
            values("%-10.20f"),
            [
                Token::Percent,
                Token::Minus,
                Token::Number(10),
                Token::Dot,
                Token::Number(20),
                Token::Character('f'),
            ]
        );
    }

    #[test]
    fn lexes_special_escapes_and_groups() {
        assert_eq!(
            values("left%=mid%<%(x%)%%"),
            [
                Token::Text("left"),
                Token::Percent,
                Token::Equal,
                Token::Text("mid"),
                Token::Percent,
                Token::LessThan,
                Token::Percent,
                Token::LParen,
                Token::Text("x"),
                Token::Percent,
                Token::RParen,
                Token::Percent,
                Token::Percent,
            ]
        );
    }

    #[test]
    fn expression_body_is_borrowed_as_one_token() {
        assert_eq!(
            values("%{get({'key': \"}\"}, 'key')}"),
            [
                Token::Percent,
                Token::LBrace,
                Token::Text("get({'key': \"}\"}, 'key')"),
                Token::RBrace,
            ]
        );
    }

    #[test]
    fn lexes_highlight_name_and_reset() {
        assert_eq!(
            values("%#WarningMsg#error%*"),
            [
                Token::Percent,
                Token::Hash,
                Token::Text("WarningMsg"),
                Token::Hash,
                Token::Text("error"),
                Token::Percent,
                Token::Star,
            ]
        );
    }

    #[test]
    fn unclosed_delimiter_is_left_for_parser_to_diagnose() {
        assert_eq!(
            values("%{getline('.')"),
            [Token::Percent, Token::LBrace, Token::Text("getline('.')"),]
        );
    }

    #[test]
    fn spans_are_utf8_byte_offsets() {
        let tokens = lex("λ%f").unwrap();
        assert_eq!(tokens[0].span, Span::new(0, 2));
        assert_eq!(tokens[1].span, Span::new(2, 3));
        assert_eq!(tokens[2].span, Span::new(3, 4));
    }

    #[test]
    fn reports_number_overflow() {
        let error = lex("%999999999999999999999f").unwrap_err();
        assert_eq!(error.kind, LexErrorKind::IntegerOverflow);
        assert_eq!(error.span, Span::new(1, 22));
    }
}
