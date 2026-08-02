use crate::{
    compiler::{CompileError, Diagnostic, DiagnosticKind, Phase},
    context::{MagicMode, TextRange},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TokenKind {
    Literal(char),
    Any,
    StartOfLine,
    EndOfLine,
    Collection(String),
    GroupOpen,
    GroupClose,
    Alternation,
    ZeroOrMore,
    OneOrMore,
    Optional,
    CountedRepeatOpen,
    Tilde,
    /// An escape whose meaning is independent of the current magic mode,
    /// such as `\d`, `\zs`, or `\%23l`. The parser combines adjacent escape
    /// tokens when Vim defines a multi-character atom.
    Escaped(char),
    MagicSwitch(MagicMode),
}

pub fn lex(source: &str, initial_magic: MagicMode) -> Result<Vec<Token>, CompileError> {
    Lexer::new(source, initial_magic).lex()
}

struct Lexer<'a> {
    source: &'a str,
    offset: usize,
    magic: MagicMode,
    tokens: Vec<Token>,
}

impl<'a> Lexer<'a> {
    fn new(source: &'a str, initial_magic: MagicMode) -> Self {
        Self {
            source,
            offset: 0,
            magic: initial_magic,
            tokens: Vec::new(),
        }
    }

    fn lex(mut self) -> Result<Vec<Token>, CompileError> {
        while let Some(character) = self.peek() {
            let start = self.offset;
            self.bump();

            if character == '\\' {
                self.lex_escape(start)?;
            } else if character == '[' && self.unescaped_collection_is_special() {
                self.lex_collection(start)?;
            } else {
                let kind = self.unescaped_kind(character);
                self.push(kind, start, self.offset);
            }
        }

        Ok(self.tokens)
    }

    fn lex_escape(&mut self, start: usize) -> Result<(), CompileError> {
        let Some(escaped) = self.peek() else {
            return Err(self.error(
                start..self.offset,
                "pattern ends with an unmatched backslash",
            ));
        };
        self.bump();

        if let Some(mode) = magic_switch(escaped) {
            self.magic = mode;
            self.push(TokenKind::MagicSwitch(mode), start, self.offset);
            return Ok(());
        }

        if escaped == '[' && self.escaped_collection_is_special() {
            return self.lex_collection(start);
        }

        let kind = match self.magic {
            MagicMode::VeryMagic => self.very_magic_escaped_kind(escaped),
            MagicMode::Magic => self.magic_escaped_kind(escaped),
            MagicMode::NoMagic => self.no_magic_escaped_kind(escaped),
            MagicMode::VeryNoMagic => self.very_no_magic_escaped_kind(escaped),
        };
        self.push(kind, start, self.offset);
        Ok(())
    }

    fn unescaped_kind(&self, character: char) -> TokenKind {
        match self.magic {
            MagicMode::VeryMagic => match character {
                '.' => TokenKind::Any,
                '^' => TokenKind::StartOfLine,
                '$' => TokenKind::EndOfLine,
                '(' => TokenKind::GroupOpen,
                ')' => TokenKind::GroupClose,
                '|' => TokenKind::Alternation,
                '*' => TokenKind::ZeroOrMore,
                '+' => TokenKind::OneOrMore,
                '?' | '=' => TokenKind::Optional,
                '{' => TokenKind::CountedRepeatOpen,
                '~' => TokenKind::Tilde,
                _ => TokenKind::Literal(character),
            },
            MagicMode::Magic => match character {
                '.' => TokenKind::Any,
                '^' => TokenKind::StartOfLine,
                '$' => TokenKind::EndOfLine,
                '*' => TokenKind::ZeroOrMore,
                '~' => TokenKind::Tilde,
                _ => TokenKind::Literal(character),
            },
            MagicMode::NoMagic => match character {
                '^' => TokenKind::StartOfLine,
                '$' => TokenKind::EndOfLine,
                _ => TokenKind::Literal(character),
            },
            MagicMode::VeryNoMagic => TokenKind::Literal(character),
        }
    }

    fn very_magic_escaped_kind(&self, escaped: char) -> TokenKind {
        if escaped.is_ascii_alphanumeric() || matches!(escaped, '_' | '%' | '@' | '&' | '<' | '>') {
            TokenKind::Escaped(escaped)
        } else {
            TokenKind::Literal(escaped)
        }
    }

    fn magic_escaped_kind(&self, escaped: char) -> TokenKind {
        match escaped {
            '(' => TokenKind::GroupOpen,
            ')' => TokenKind::GroupClose,
            '|' => TokenKind::Alternation,
            '+' => TokenKind::OneOrMore,
            '?' | '=' => TokenKind::Optional,
            '{' => TokenKind::CountedRepeatOpen,
            '.' | '*' | '~' | '^' | '$' | '\\' => TokenKind::Literal(escaped),
            _ => TokenKind::Escaped(escaped),
        }
    }

    fn no_magic_escaped_kind(&self, escaped: char) -> TokenKind {
        match escaped {
            '(' => TokenKind::GroupOpen,
            ')' => TokenKind::GroupClose,
            '|' => TokenKind::Alternation,
            '.' => TokenKind::Any,
            '*' => TokenKind::ZeroOrMore,
            '+' => TokenKind::OneOrMore,
            '?' | '=' => TokenKind::Optional,
            '{' => TokenKind::CountedRepeatOpen,
            '~' => TokenKind::Tilde,
            '^' | '$' | '\\' => TokenKind::Literal(escaped),
            _ => TokenKind::Escaped(escaped),
        }
    }

    fn very_no_magic_escaped_kind(&self, escaped: char) -> TokenKind {
        match escaped {
            '.' => TokenKind::Any,
            '^' => TokenKind::StartOfLine,
            '$' => TokenKind::EndOfLine,
            '(' => TokenKind::GroupOpen,
            ')' => TokenKind::GroupClose,
            '|' => TokenKind::Alternation,
            '*' => TokenKind::ZeroOrMore,
            '+' => TokenKind::OneOrMore,
            '?' | '=' => TokenKind::Optional,
            '{' => TokenKind::CountedRepeatOpen,
            '~' => TokenKind::Tilde,
            '\\' => TokenKind::Literal('\\'),
            _ => TokenKind::Escaped(escaped),
        }
    }

    fn lex_collection(&mut self, start: usize) -> Result<(), CompileError> {
        let content_start = self.offset;
        let mut escaped = false;
        let mut first = true;

        while let Some(character) = self.peek() {
            self.bump();

            if escaped {
                escaped = false;
                first = false;
                continue;
            }
            if character == '\\' {
                escaped = true;
                first = false;
                continue;
            }
            // POSIX classes, equivalence classes, and collating elements have
            // their own brackets. Consume their inner `]` so it cannot end
            // the surrounding Vim collection.
            if character == '[' && self.lex_bracketed_collection_item() {
                first = false;
                continue;
            }
            // A closing bracket is literal in the first collection position.
            if character == ']' && !first {
                let content_end = self.offset - character.len_utf8();
                let content = self.source[content_start..content_end].to_owned();
                self.push(TokenKind::Collection(content), start, self.offset);
                return Ok(());
            }
            if first && character == '^' {
                // Negation does not consume the first literal-character slot.
                continue;
            }
            first = false;
        }

        Err(self.error(start..self.offset, "unterminated character collection"))
    }

    fn lex_bracketed_collection_item(&mut self) -> bool {
        let Some(delimiter @ (':' | '=' | '.')) = self.peek() else {
            return false;
        };
        let remainder = &self.source[self.offset + delimiter.len_utf8()..];
        let terminator = [delimiter, ']'].iter().collect::<String>();
        let Some(relative_end) = remainder.find(&terminator) else {
            return false;
        };

        self.offset += delimiter.len_utf8() + relative_end + terminator.len();
        true
    }

    fn unescaped_collection_is_special(&self) -> bool {
        matches!(self.magic, MagicMode::VeryMagic | MagicMode::Magic)
    }

    fn escaped_collection_is_special(&self) -> bool {
        matches!(self.magic, MagicMode::NoMagic | MagicMode::VeryNoMagic)
    }

    fn peek(&self) -> Option<char> {
        self.source[self.offset..].chars().next()
    }

    fn bump(&mut self) {
        if let Some(character) = self.peek() {
            self.offset += character.len_utf8();
        }
    }

    fn push(&mut self, kind: TokenKind, start: usize, end: usize) {
        self.tokens.push(Token {
            kind,
            span: start..end,
        });
    }

    fn error(&self, span: TextRange, message: impl Into<String>) -> CompileError {
        CompileError {
            diagnostics: vec![Diagnostic {
                kind: DiagnosticKind::InvalidSyntax,
                phase: Phase::Lex,
                span,
                message: message.into(),
                help: None,
            }],
        }
    }
}

fn magic_switch(character: char) -> Option<MagicMode> {
    match character {
        'v' => Some(MagicMode::VeryMagic),
        'm' => Some(MagicMode::Magic),
        'M' => Some(MagicMode::NoMagic),
        'V' => Some(MagicMode::VeryNoMagic),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn kinds(source: &str, mode: MagicMode) -> Vec<TokenKind> {
        lex(source, mode)
            .expect("pattern should lex")
            .into_iter()
            .map(|token| token.kind)
            .collect()
    }

    #[test]
    fn follows_vims_documented_magic_table() {
        assert_eq!(
            kinds(r"a.*\(x\)\|y\+", MagicMode::Magic),
            vec![
                TokenKind::Literal('a'),
                TokenKind::Any,
                TokenKind::ZeroOrMore,
                TokenKind::GroupOpen,
                TokenKind::Literal('x'),
                TokenKind::GroupClose,
                TokenKind::Alternation,
                TokenKind::Literal('y'),
                TokenKind::OneOrMore,
            ]
        );
        assert_eq!(
            kinds(r"a\.\*\(x\)", MagicMode::NoMagic),
            vec![
                TokenKind::Literal('a'),
                TokenKind::Any,
                TokenKind::ZeroOrMore,
                TokenKind::GroupOpen,
                TokenKind::Literal('x'),
                TokenKind::GroupClose,
            ]
        );
    }

    #[test]
    fn switches_magic_mode_mid_pattern() {
        assert_eq!(
            kinds(r"a\v(x|y)+\Vz.*", MagicMode::Magic),
            vec![
                TokenKind::Literal('a'),
                TokenKind::MagicSwitch(MagicMode::VeryMagic),
                TokenKind::GroupOpen,
                TokenKind::Literal('x'),
                TokenKind::Alternation,
                TokenKind::Literal('y'),
                TokenKind::GroupClose,
                TokenKind::OneOrMore,
                TokenKind::MagicSwitch(MagicMode::VeryNoMagic),
                TokenKind::Literal('z'),
                TokenKind::Literal('.'),
                TokenKind::Literal('*'),
            ]
        );
    }

    #[test]
    fn records_utf8_byte_spans() {
        let tokens = lex("λ.", MagicMode::Magic).expect("pattern should lex");
        assert_eq!(tokens[0].span, 0..2);
        assert_eq!(tokens[1].span, 2..3);
    }

    #[test]
    fn keeps_collection_content_for_the_collection_parser() {
        let tokens = lex(r"[^]a-z\]]", MagicMode::Magic).expect("pattern should lex");
        assert_eq!(
            tokens,
            vec![Token {
                kind: TokenKind::Collection(r"^]a-z\]".into()),
                span: 0..9,
            }]
        );
    }

    #[test]
    fn keeps_bracketed_items_inside_their_collection() {
        assert_eq!(
            kinds("[[:alpha:][=a=][.x.]-]", MagicMode::Magic,),
            vec![TokenKind::Collection("[:alpha:][=a=][.x.]-".into())]
        );
        assert_eq!(
            lex("[[:print:]]", MagicMode::Magic).expect("POSIX class should lex")[0].span,
            0..11
        );
    }

    #[test]
    fn collection_magic_follows_vims_magic_table() {
        assert_eq!(
            kinds(r"[ab]", MagicMode::VeryMagic),
            vec![TokenKind::Collection("ab".into())]
        );
        assert_eq!(
            kinds(r"[ab]", MagicMode::Magic),
            vec![TokenKind::Collection("ab".into())]
        );
        assert_eq!(
            kinds(r"\[ab]", MagicMode::NoMagic),
            vec![TokenKind::Collection("ab".into())]
        );
        assert_eq!(
            kinds(r"\[ab]", MagicMode::VeryNoMagic),
            vec![TokenKind::Collection("ab".into())]
        );
    }

    #[test]
    fn distinguishes_reserved_escapes_from_escaped_punctuation() {
        assert_eq!(
            kinds(r"\d\zs\%23l\.", MagicMode::VeryMagic),
            vec![
                TokenKind::Escaped('d'),
                TokenKind::Escaped('z'),
                TokenKind::Literal('s'),
                TokenKind::Escaped('%'),
                TokenKind::Literal('2'),
                TokenKind::Literal('3'),
                TokenKind::Literal('l'),
                TokenKind::Literal('.'),
            ]
        );
        assert_eq!(
            kinds(r"\d\zs", MagicMode::VeryNoMagic),
            vec![
                TokenKind::Escaped('d'),
                TokenKind::Escaped('z'),
                TokenKind::Literal('s'),
            ]
        );
    }

    #[test]
    fn reports_unterminated_collection() {
        let error = lex("[abc", MagicMode::Magic).expect_err("collection should be invalid");
        assert_eq!(error.diagnostics[0].phase, Phase::Lex);
        assert_eq!(error.diagnostics[0].span, 0..4);
    }

    #[test]
    fn reports_trailing_backslash() {
        let error = lex("abc\\", MagicMode::Magic).expect_err("escape should be invalid");
        assert_eq!(error.diagnostics[0].span, 3..4);
    }

    proptest! {
        #[test]
        fn arbitrary_utf8_never_panics_and_always_returns_valid_spans(source in ".{0,256}") {
            match lex(&source, MagicMode::Magic) {
                Ok(tokens) => {
                    for token in tokens {
                        prop_assert!(token.span.start <= token.span.end);
                        prop_assert!(token.span.end <= source.len());
                        prop_assert!(source.is_char_boundary(token.span.start));
                        prop_assert!(source.is_char_boundary(token.span.end));
                    }
                }
                Err(error) => {
                    for diagnostic in error.diagnostics {
                        prop_assert!(diagnostic.span.start <= diagnostic.span.end);
                        prop_assert!(diagnostic.span.end <= source.len());
                    }
                }
            }
        }
    }
}
