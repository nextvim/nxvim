use crate::source::{Diagnostic, SourceId, Span};

#[derive(Clone, Debug, PartialEq)]
pub enum TokenKind {
    Identifier(String),
    Integer(i64),
    Float(f64),
    SingleQuotedString(String),
    DoubleQuotedString(String),
    Heredoc {
        marker: String,
        trim: bool,
        content: String,
    },
    Keyword(Keyword),
    Operator(Operator),
    LeftParen,
    RightParen,
    LeftBracket,
    RightBracket,
    LeftBrace,
    RightBrace,
    Comma,
    Colon,
    Question,
    Ampersand,
    Semicolon,
    Dot,
    Newline,
    LineContinuation,
    EndOfFile,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Keyword {
    Let,
    Const,
    Unlet,
    Call,
    If,
    ElseIf,
    Else,
    EndIf,
    While,
    EndWhile,
    For,
    In,
    EndFor,
    Try,
    Catch,
    Finally,
    EndTry,
    Throw,
    Function,
    EndFunction,
    Return,
    Break,
    Continue,
    Finish,
    Echo,
    Execute,
    Lambda,
    EndLambda,
    Await,
    True,
    False,
    Null,
    Is,
    IsNot,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Operator {
    Assign,
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
    Concatenate,
    AddAssign,
    SubtractAssign,
    MultiplyAssign,
    DivideAssign,
    RemainderAssign,
    ConcatenateAssign,
    Equal,
    NotEqual,
    Match,
    NoMatch,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    LogicalAnd,
    LogicalOr,
    Not,
    Coalesce,
    Arrow,
    CaseSensitive,
    CaseInsensitive,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Lexer<'a> {
    pub source_id: SourceId,
    pub source: &'a str,
    pub cursor: usize,
    pub line_start: usize,
    pub diagnostics: Vec<Diagnostic>,
    line_has_token: bool,
    expects_operand: bool,
}

#[derive(Clone, Debug, Default)]
pub struct LexOutput {
    pub tokens: Vec<Token>,
    pub diagnostics: Vec<Diagnostic>,
}

impl<'a> Lexer<'a> {
    pub fn new(source_id: SourceId, source: &'a str) -> Self {
        Self {
            source_id,
            source,
            cursor: 0,
            line_start: 0,
            diagnostics: Vec::new(),
            line_has_token: false,
            expects_operand: true,
        }
    }

    pub fn lex(mut self) -> LexOutput {
        let mut tokens = Vec::new();
        while !self.is_eof() {
            self.skip_horizontal_whitespace();
            if self.is_eof() {
                break;
            }
            let start = self.cursor;
            let Some(ch) = self.peek() else { break };
            let kind = match ch {
                '\n' | '\r' => {
                    self.consume_newline();
                    self.line_has_token = false;
                    self.expects_operand = true;
                    TokenKind::Newline
                }
                '\\' if self
                    .peek_next()
                    .is_some_and(|next| next == '\n' || next == '\r') =>
                {
                    self.bump();
                    self.consume_newline();
                    self.line_has_token = false;
                    TokenKind::LineContinuation
                }
                '"' if !self.line_has_token || !self.expects_operand => {
                    self.skip_comment();
                    continue;
                }
                '"' => self.double_quoted(start),
                '\'' => self.single_quoted(start),
                '0'..='9' => self.number(start),
                c if is_identifier_start(c) => self.identifier(),
                '(' => {
                    self.bump();
                    self.expects_operand = true;
                    TokenKind::LeftParen
                }
                ')' => {
                    self.bump();
                    self.expects_operand = false;
                    TokenKind::RightParen
                }
                '[' => {
                    self.bump();
                    self.expects_operand = true;
                    TokenKind::LeftBracket
                }
                ']' => {
                    self.bump();
                    self.expects_operand = false;
                    TokenKind::RightBracket
                }
                '{' => {
                    self.bump();
                    self.expects_operand = true;
                    TokenKind::LeftBrace
                }
                '}' => {
                    self.bump();
                    self.expects_operand = false;
                    TokenKind::RightBrace
                }
                ',' => {
                    self.bump();
                    self.expects_operand = true;
                    TokenKind::Comma
                }
                ':' => {
                    self.bump();
                    self.expects_operand = true;
                    TokenKind::Colon
                }
                '|' if self.peek_next() == Some('|') => {
                    self.operator("||", Operator::LogicalOr, true)
                }
                ';' | '|' => {
                    self.bump();
                    self.expects_operand = true;
                    TokenKind::Semicolon
                }
                '.' if self.peek_next() == Some('.') => {
                    self.operator("..", Operator::Concatenate, true)
                }
                '.' if self.peek_next() == Some('=') => {
                    self.operator(".=", Operator::ConcatenateAssign, true)
                }
                '.' => {
                    self.bump();
                    self.expects_operand = false;
                    TokenKind::Dot
                }
                '<' if self.peek_next() == Some('<') => self.heredoc(start),
                '=' => self.equals_operator(),
                '!' => self.bang_operator(),
                '<' => self.comparison_operator('<'),
                '>' => self.comparison_operator('>'),
                '+' => self.simple_or_assign(Operator::Add, Operator::AddAssign),
                '-' if self.peek_next() == Some('>') => self.operator("->", Operator::Arrow, true),
                '-' => self.simple_or_assign(Operator::Subtract, Operator::SubtractAssign),
                '*' => self.simple_or_assign(Operator::Multiply, Operator::MultiplyAssign),
                '/' => self.simple_or_assign(Operator::Divide, Operator::DivideAssign),
                '%' => self.simple_or_assign(Operator::Remainder, Operator::RemainderAssign),
                '&' if self.peek_next() == Some('&') => {
                    self.operator("&&", Operator::LogicalAnd, true)
                }
                '&' => {
                    self.bump();
                    self.expects_operand = true;
                    TokenKind::Ampersand
                }
                '?' if self.peek_next() == Some('?') => {
                    self.operator("??", Operator::Coalesce, true)
                }
                '?' => {
                    self.bump();
                    self.expects_operand = true;
                    TokenKind::Question
                }
                _ => {
                    self.bump();
                    self.diagnostics.push(Diagnostic::error(
                        "L001",
                        format!("unexpected character {ch:?}"),
                        self.span(start, self.cursor),
                    ));
                    continue;
                }
            };
            if !matches!(kind, TokenKind::Newline | TokenKind::LineContinuation) {
                self.line_has_token = true;
            }
            tokens.push(Token {
                kind,
                span: self.span(start, self.cursor),
            });
        }
        let end = self.cursor;
        tokens.push(Token {
            kind: TokenKind::EndOfFile,
            span: self.span(end, end),
        });
        LexOutput {
            tokens,
            diagnostics: self.diagnostics,
        }
    }

    fn identifier(&mut self) -> TokenKind {
        let start = self.cursor;
        self.bump();
        while self.peek().is_some_and(is_identifier_continue) {
            self.bump();
        }
        // Vim namespaces use a one-character prefix, e.g. g:name and s:name.
        if self.cursor == start + 1
            && self.peek() == Some(':')
            && matches!(
                &self.source[start..self.cursor],
                "g" | "b" | "w" | "t" | "s" | "l" | "a" | "v"
            )
        {
            self.bump();
            while self.peek().is_some_and(is_identifier_continue) {
                self.bump();
            }
        } else if &self.source[start..self.cursor] == "function" && self.peek() == Some('!') {
            self.bump();
        }
        let text = &self.source[start..self.cursor];
        if let Some(keyword) = keyword(text) {
            self.expects_operand = matches!(
                keyword,
                Keyword::Let
                    | Keyword::Const
                    | Keyword::Unlet
                    | Keyword::Call
                    | Keyword::Return
                    | Keyword::Throw
                    | Keyword::Await
                    | Keyword::Echo
                    | Keyword::Execute
                    | Keyword::In
                    | Keyword::Is
                    | Keyword::IsNot
            );
            TokenKind::Keyword(keyword)
        } else {
            self.expects_operand = false;
            TokenKind::Identifier(text.to_owned())
        }
    }

    fn number(&mut self, start: usize) -> TokenKind {
        let radix = if self.remaining().starts_with("0x") || self.remaining().starts_with("0X") {
            self.cursor += 2;
            16
        } else if self.remaining().starts_with("0b") || self.remaining().starts_with("0B") {
            self.cursor += 2;
            2
        } else if self.remaining().starts_with("0o") || self.remaining().starts_with("0O") {
            self.cursor += 2;
            8
        } else {
            10
        };
        while self.peek().is_some_and(|c| c.is_digit(radix) || c == '_') {
            self.bump();
        }
        if radix == 10
            && self.peek() == Some('.')
            && self.peek_next().is_some_and(|c| c.is_ascii_digit())
        {
            self.bump();
            while self.peek().is_some_and(|c| c.is_ascii_digit() || c == '_') {
                self.bump();
            }
            if matches!(self.peek(), Some('e' | 'E')) {
                self.bump();
                if matches!(self.peek(), Some('+' | '-')) {
                    self.bump();
                }
                while self.peek().is_some_and(|c| c.is_ascii_digit() || c == '_') {
                    self.bump();
                }
            }
            let raw = self.source[start..self.cursor].replace('_', "");
            self.expects_operand = false;
            return raw.parse().map(TokenKind::Float).unwrap_or_else(|_| {
                self.diagnostics.push(Diagnostic::error(
                    "L002",
                    "invalid floating-point literal",
                    self.span(start, self.cursor),
                ));
                TokenKind::Float(0.0)
            });
        }
        let digits_start = start + usize::from(radix != 10) * 2;
        let raw = self.source[digits_start..self.cursor].replace('_', "");
        self.expects_operand = false;
        i64::from_str_radix(&raw, radix)
            .map(TokenKind::Integer)
            .unwrap_or_else(|_| {
                self.diagnostics.push(Diagnostic::error(
                    "L003",
                    "invalid or overflowing integer literal",
                    self.span(start, self.cursor),
                ));
                TokenKind::Integer(0)
            })
    }

    fn single_quoted(&mut self, start: usize) -> TokenKind {
        self.bump();
        let mut value = String::new();
        let mut terminated = false;
        while let Some(ch) = self.peek() {
            if ch == '\'' {
                self.bump();
                if self.peek() == Some('\'') {
                    self.bump();
                    value.push('\'');
                } else {
                    terminated = true;
                    break;
                }
            } else if ch == '\n' || ch == '\r' {
                break;
            } else {
                value.push(ch);
                self.bump();
            }
        }
        if !terminated {
            self.diagnostics.push(Diagnostic::error(
                "L004",
                "unterminated single-quoted string",
                self.span(start, self.cursor),
            ));
        }
        self.expects_operand = false;
        TokenKind::SingleQuotedString(value)
    }

    fn double_quoted(&mut self, start: usize) -> TokenKind {
        self.bump();
        let mut value = String::new();
        let mut terminated = false;
        while let Some(ch) = self.peek() {
            if ch == '"' {
                self.bump();
                terminated = true;
                break;
            }
            if ch == '\n' || ch == '\r' {
                break;
            }
            if ch == '\\' {
                self.bump();
                match self.peek() {
                    Some('n') => {
                        self.bump();
                        value.push('\n');
                    }
                    Some('r') => {
                        self.bump();
                        value.push('\r');
                    }
                    Some('t') => {
                        self.bump();
                        value.push('\t');
                    }
                    Some('e') => {
                        self.bump();
                        value.push('\u{1b}');
                    }
                    Some('"') => {
                        self.bump();
                        value.push('"');
                    }
                    Some('\\') => {
                        self.bump();
                        value.push('\\');
                    }
                    Some(other) => {
                        value.push('\\');
                        value.push(other);
                        self.bump();
                    }
                    None => break,
                }
            } else {
                value.push(ch);
                self.bump();
            }
        }
        if !terminated {
            self.diagnostics.push(Diagnostic::error(
                "L005",
                "unterminated double-quoted string",
                self.span(start, self.cursor),
            ));
        }
        self.expects_operand = false;
        TokenKind::DoubleQuotedString(value)
    }

    fn heredoc(&mut self, start: usize) -> TokenKind {
        self.cursor += 2;
        self.skip_horizontal_whitespace();
        let trim = if self.remaining().starts_with("trim")
            && self.remaining()[4..]
                .chars()
                .next()
                .is_none_or(char::is_whitespace)
        {
            self.cursor += 4;
            self.skip_horizontal_whitespace();
            true
        } else {
            false
        };
        let marker_start = self.cursor;
        while self
            .peek()
            .is_some_and(|c| c != '\n' && c != '\r' && !c.is_whitespace())
        {
            self.bump();
        }
        let marker = self.source[marker_start..self.cursor].to_owned();
        if marker.is_empty() {
            self.diagnostics.push(Diagnostic::error(
                "L006",
                "heredoc requires an end marker",
                self.span(start, self.cursor),
            ));
            return TokenKind::Heredoc {
                marker,
                trim,
                content: String::new(),
            };
        }
        while self.peek().is_some_and(|c| c != '\n' && c != '\r') {
            self.bump();
        }
        if !self.is_eof() {
            self.consume_newline();
        }
        let mut content = String::new();
        let mut found = false;
        while !self.is_eof() {
            let line_start = self.cursor;
            while self.peek().is_some_and(|c| c != '\n' && c != '\r') {
                self.bump();
            }
            let line = &self.source[line_start..self.cursor];
            let compared = if trim { line.trim_start() } else { line };
            if compared == marker {
                found = true;
                if !self.is_eof() {
                    self.consume_newline();
                }
                break;
            }
            content.push_str(if trim { line.trim_start() } else { line });
            if !self.is_eof() {
                self.consume_newline();
                content.push('\n');
            }
        }
        if !found {
            self.diagnostics.push(Diagnostic::error(
                "L007",
                format!("unterminated heredoc; expected {marker:?}"),
                self.span(start, self.cursor),
            ));
        }
        self.expects_operand = false;
        TokenKind::Heredoc {
            marker,
            trim,
            content,
        }
    }

    fn equals_operator(&mut self) -> TokenKind {
        for (text, op) in [
            ("==#", Operator::CaseSensitive),
            ("==?", Operator::CaseInsensitive),
            ("==", Operator::Equal),
            ("=~", Operator::Match),
        ] {
            if self.remaining().starts_with(text) {
                return self.operator(text, op, true);
            }
        }
        self.operator("=", Operator::Assign, true)
    }

    fn bang_operator(&mut self) -> TokenKind {
        for (text, op) in [("!=", Operator::NotEqual), ("!~", Operator::NoMatch)] {
            if self.remaining().starts_with(text) {
                return self.operator(text, op, true);
            }
        }
        self.operator("!", Operator::Not, true)
    }

    fn comparison_operator(&mut self, ch: char) -> TokenKind {
        let op = match (ch, self.peek_next() == Some('=')) {
            ('<', true) => Operator::LessEqual,
            ('>', true) => Operator::GreaterEqual,
            ('<', false) => Operator::Less,
            _ => Operator::Greater,
        };
        let len = if self.peek_next() == Some('=') { 2 } else { 1 };
        self.operator(&self.source[self.cursor..self.cursor + len], op, true)
    }
    fn simple_or_assign(&mut self, simple: Operator, assign: Operator) -> TokenKind {
        if self.peek_next() == Some('=') {
            let end = self.cursor + 2;
            self.operator(&self.source[self.cursor..end], assign, true)
        } else {
            self.bump();
            self.expects_operand = true;
            TokenKind::Operator(simple)
        }
    }
    fn operator(&mut self, text: &str, op: Operator, expects_operand: bool) -> TokenKind {
        self.cursor += text.len();
        self.expects_operand = expects_operand;
        TokenKind::Operator(op)
    }
    fn skip_comment(&mut self) {
        while self.peek().is_some_and(|c| c != '\n' && c != '\r') {
            self.bump();
        }
    }
    fn skip_horizontal_whitespace(&mut self) {
        while matches!(self.peek(), Some(' ' | '\t' | '\u{000c}')) {
            self.bump();
        }
    }
    fn consume_newline(&mut self) {
        if self.peek() == Some('\r') {
            self.bump();
            if self.peek() == Some('\n') {
                self.bump();
            }
        } else {
            self.bump();
        }
        self.line_start = self.cursor;
    }
    fn remaining(&self) -> &'a str {
        &self.source[self.cursor..]
    }
    fn peek(&self) -> Option<char> {
        self.remaining().chars().next()
    }
    fn peek_next(&self) -> Option<char> {
        self.remaining().chars().nth(1)
    }
    fn bump(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.cursor += ch.len_utf8();
        Some(ch)
    }
    fn is_eof(&self) -> bool {
        self.cursor >= self.source.len()
    }
    fn span(&self, start: usize, end: usize) -> Span {
        Span::new(self.source_id, start as u32, end as u32)
    }
}

fn is_identifier_start(ch: char) -> bool {
    ch == '_' || ch == '#' || ch.is_alphabetic()
}
fn is_identifier_continue(ch: char) -> bool {
    ch == '_' || ch == '#' || ch.is_alphanumeric()
}
fn keyword(text: &str) -> Option<Keyword> {
    Some(match text {
        "let" => Keyword::Let,
        "const" => Keyword::Const,
        "unlet" => Keyword::Unlet,
        "call" | "cal" | "call!" | "cal!" => Keyword::Call,
        "if" => Keyword::If,
        "elseif" => Keyword::ElseIf,
        "else" => Keyword::Else,
        "endif" => Keyword::EndIf,
        "while" => Keyword::While,
        "endwhile" => Keyword::EndWhile,
        "for" => Keyword::For,
        "in" => Keyword::In,
        "endfor" => Keyword::EndFor,
        "try" => Keyword::Try,
        "catch" => Keyword::Catch,
        "finally" => Keyword::Finally,
        "endtry" => Keyword::EndTry,
        "throw" => Keyword::Throw,
        "function" | "function!" => Keyword::Function,
        "endfunction" => Keyword::EndFunction,
        "return" => Keyword::Return,
        "break" => Keyword::Break,
        "continue" => Keyword::Continue,
        "finish" => Keyword::Finish,
        "echo" | "echomsg" | "echoerr" => Keyword::Echo,
        "execute" => Keyword::Execute,
        "lambda" => Keyword::Lambda,
        "endlambda" => Keyword::EndLambda,
        "await" => Keyword::Await,
        "true" | "v:true" => Keyword::True,
        "false" | "v:false" => Keyword::False,
        "null" | "v:null" => Keyword::Null,
        "is" => Keyword::Is,
        "isnot" => Keyword::IsNot,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn lex(source: &str) -> LexOutput {
        Lexer::new(SourceId(0), source).lex()
    }

    #[test]
    fn lexes_a_basic_script_and_comment() {
        let output = lex("let g:name = 0x2a + 1.5 \" comment\necho 'it''s ok'\n");
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        assert_eq!(output.tokens[0].kind, TokenKind::Keyword(Keyword::Let));
        assert_eq!(
            output.tokens[1].kind,
            TokenKind::Identifier("g:name".into())
        );
        assert_eq!(output.tokens[3].kind, TokenKind::Integer(42));
        assert_eq!(output.tokens[5].kind, TokenKind::Float(1.5));
        assert!(
            output
                .tokens
                .iter()
                .any(|token| token.kind == TokenKind::SingleQuotedString("it's ok".into()))
        );
    }

    #[test]
    fn distinguishes_double_strings_from_comments() {
        let output =
            lex("\" whole-line comment\nlet x = \"value\\n\" \" trailing\necho \"shown\"\n");
        assert!(
            output
                .tokens
                .iter()
                .any(|token| token.kind == TokenKind::DoubleQuotedString("value\n".into()))
        );
        assert!(
            output
                .tokens
                .iter()
                .any(|token| token.kind == TokenKind::DoubleQuotedString("shown".into()))
        );
        assert!(output.diagnostics.is_empty());
    }

    #[test]
    fn lexes_trimmed_heredoc() {
        let output = lex("let x =<< trim END\n  first\n  second\n  END\n");
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        assert!(output.tokens.iter().any(|token| token.kind
            == TokenKind::Heredoc {
                marker: "END".into(),
                trim: true,
                content: "first\nsecond\n".into()
            }));
    }

    #[test]
    fn reports_bad_input_and_always_reaches_eof() {
        let output = lex("let x = 'missing\n@");
        assert_eq!(output.diagnostics.len(), 2);
        assert!(matches!(
            output.tokens.last().unwrap().kind,
            TokenKind::EndOfFile
        ));
    }
}
