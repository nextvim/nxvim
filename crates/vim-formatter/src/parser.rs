use crate::{
    ast::{Alignment, AstItem, Escape, EscapeKind, FieldSpec, FormatAst},
    dialect::{FormatDialect, TablineTarget},
    error::{LexErrorKind, ParseError, ParseErrorKind},
    lexer::{SpannedToken, Token, lex},
    span::{Span, Spanned},
};

/// Parses a source string using the requested formatting dialect.
pub fn parse(source: &str, dialect: FormatDialect) -> Result<FormatAst, ParseError> {
    let tokens = lex(source).map_err(|error| ParseError {
        kind: match error.kind {
            LexErrorKind::IntegerOverflow => ParseErrorKind::WidthOutOfRange,
            LexErrorKind::InvalidCharacter(_) => ParseErrorKind::UnexpectedToken,
        },
        span: error.span,
    })?;
    Parser::new(tokens, dialect).parse()
}

/// Recursive-descent parser for the token stream produced by [`crate::Lexer`].
#[derive(Clone, Debug)]
pub struct Parser<'src> {
    tokens: Vec<SpannedToken<'src>>,
    cursor: usize,
    dialect: FormatDialect,
}

impl<'src> Parser<'src> {
    pub fn new(tokens: Vec<SpannedToken<'src>>, dialect: FormatDialect) -> Self {
        Self {
            tokens,
            cursor: 0,
            dialect,
        }
    }

    pub fn parse(mut self) -> Result<FormatAst, ParseError> {
        let items = self.parse_items(None)?;
        Ok(FormatAst {
            dialect: self.dialect,
            items,
        })
    }

    fn parse_items(
        &mut self,
        group_start: Option<usize>,
    ) -> Result<Vec<Spanned<AstItem>>, ParseError> {
        let mut items = Vec::new();
        while self.cursor < self.tokens.len() {
            if self.at_group_end() {
                if group_start.is_some() {
                    break;
                }
                return Err(self.error_here(ParseErrorKind::UnexpectedToken));
            }

            let token = self.advance().expect("cursor was checked");
            match token.value {
                Token::Text(text) => {
                    items.push(Spanned::new(AstItem::Literal(text.to_owned()), token.span))
                }
                Token::Percent => items.push(self.parse_percent(token.span)?),
                _ => {
                    return Err(ParseError {
                        kind: ParseErrorKind::UnexpectedToken,
                        span: token.span,
                    });
                }
            }
        }

        if let Some(start) = group_start
            && self.cursor == self.tokens.len()
        {
            return Err(ParseError {
                kind: ParseErrorKind::UnclosedGroup,
                span: Span::new(start, self.source_end()),
            });
        }
        Ok(items)
    }

    fn parse_percent(&mut self, percent_span: Span) -> Result<Spanned<AstItem>, ParseError> {
        if let Some(target) = self.parse_tabline_target(percent_span.start)? {
            return Ok(target);
        }

        let field = self.parse_field_spec()?;
        let token = self.advance().ok_or(ParseError {
            kind: ParseErrorKind::UnexpectedEnd,
            span: Span::new(percent_span.start, percent_span.end),
        })?;

        let (item, end) = match token.value {
            Token::Character(code) => (
                AstItem::Escape(Escape {
                    field,
                    kind: escape_kind(code),
                }),
                token.span.end,
            ),
            Token::Percent if field == FieldSpec::default() => (
                AstItem::Escape(Escape {
                    field,
                    kind: EscapeKind::LiteralPercent,
                }),
                token.span.end,
            ),
            Token::Equal if field == FieldSpec::default() => (AstItem::Align, token.span.end),
            Token::LessThan if field == FieldSpec::default() => (AstItem::Truncate, token.span.end),
            Token::Star if field == FieldSpec::default() => {
                (AstItem::ResetHighlight, token.span.end)
            }
            Token::Hash if field == FieldSpec::default() => {
                return self.parse_highlight(percent_span.start, token.span.end);
            }
            Token::LBrace if field == FieldSpec::default() => {
                return self.parse_expression(percent_span.start, token.span.end);
            }
            Token::LParen => return self.parse_group(percent_span.start, field),
            _ => {
                return Err(ParseError {
                    kind: ParseErrorKind::UnexpectedToken,
                    span: token.span,
                });
            }
        };

        Ok(Spanned::new(item, Span::new(percent_span.start, end)))
    }

    fn parse_tabline_target(
        &mut self,
        start: usize,
    ) -> Result<Option<Spanned<AstItem>>, ParseError> {
        let (number, code_index) = match self.tokens.get(self.cursor).map(|token| token.value) {
            Some(Token::Number(number)) => (Some(number), self.cursor + 1),
            _ => (None, self.cursor),
        };
        let Some(code_token) = self.tokens.get(code_index) else {
            return Ok(None);
        };
        let Token::Character(code @ ('T' | 'X')) = code_token.value else {
            return Ok(None);
        };
        if self.dialect != FormatDialect::TabLine {
            return Err(ParseError {
                kind: ParseErrorKind::UnsupportedInDialect,
                span: Span::new(start, code_token.span.end),
            });
        }

        self.cursor = code_index + 1;
        let target = match (code, number) {
            ('T', Some(tab)) => TablineTarget::Tab(tab),
            ('T', None) => TablineTarget::Reset,
            ('X', tab) => TablineTarget::Close(tab.unwrap_or(0)),
            _ => unreachable!("only T and X are matched"),
        };
        Ok(Some(Spanned::new(
            AstItem::TablineTarget(target),
            Span::new(start, code_token.span.end),
        )))
    }

    fn parse_field_spec(&mut self) -> Result<FieldSpec, ParseError> {
        let alignment = if self.at(Token::Minus) {
            self.cursor += 1;
            Alignment::Left
        } else {
            Alignment::Right
        };

        let min_width = self.take_width()?;
        let max_width = if self.at(Token::Dot) {
            self.cursor += 1;
            Some(self.take_required_width()?)
        } else {
            None
        };

        Ok(FieldSpec {
            min_width,
            max_width,
            alignment,
        })
    }

    fn take_width(&mut self) -> Result<Option<u16>, ParseError> {
        let Some(token) = self.tokens.get(self.cursor) else {
            return Ok(None);
        };
        let Token::Number(number) = token.value else {
            return Ok(None);
        };
        self.cursor += 1;
        u16::try_from(number).map(Some).map_err(|_| ParseError {
            kind: ParseErrorKind::WidthOutOfRange,
            span: token.span,
        })
    }

    fn take_required_width(&mut self) -> Result<u16, ParseError> {
        let token = self.tokens.get(self.cursor).ok_or(ParseError {
            kind: ParseErrorKind::UnexpectedEnd,
            span: Span::new(self.source_end(), self.source_end()),
        })?;
        let Token::Number(number) = token.value else {
            return Err(ParseError {
                kind: ParseErrorKind::UnexpectedToken,
                span: token.span,
            });
        };
        self.cursor += 1;
        u16::try_from(number).map_err(|_| ParseError {
            kind: ParseErrorKind::WidthOutOfRange,
            span: token.span,
        })
    }

    fn parse_expression(
        &mut self,
        start: usize,
        open_end: usize,
    ) -> Result<Spanned<AstItem>, ParseError> {
        let (body, body_end) = self.take_optional_text();
        let Some(close) = self.advance_if(Token::RBrace) else {
            return Err(ParseError {
                kind: ParseErrorKind::UnclosedExpression,
                span: Span::new(start, body_end.unwrap_or(open_end)),
            });
        };
        Ok(Spanned::new(
            AstItem::Expression(body.unwrap_or_default().to_owned()),
            Span::new(start, close.span.end),
        ))
    }

    fn parse_highlight(
        &mut self,
        start: usize,
        open_end: usize,
    ) -> Result<Spanned<AstItem>, ParseError> {
        let (name, body_end) = self.take_optional_text();
        let Some(close) = self.advance_if(Token::Hash) else {
            return Err(ParseError {
                kind: ParseErrorKind::UnclosedHighlight,
                span: Span::new(start, body_end.unwrap_or(open_end)),
            });
        };
        Ok(Spanned::new(
            AstItem::Highlight(name.unwrap_or_default().to_owned()),
            Span::new(start, close.span.end),
        ))
    }

    fn parse_group(
        &mut self,
        start: usize,
        field: FieldSpec,
    ) -> Result<Spanned<AstItem>, ParseError> {
        let items = self.parse_items(Some(start))?;
        debug_assert!(self.at_group_end());
        self.cursor += 1; // `%`
        let close = self.advance().expect("group end has a closing parenthesis");
        Ok(Spanned::new(
            AstItem::Group { field, items },
            Span::new(start, close.span.end),
        ))
    }

    fn take_optional_text(&mut self) -> (Option<&'src str>, Option<usize>) {
        let Some(token) = self.tokens.get(self.cursor) else {
            return (None, None);
        };
        let Token::Text(text) = token.value else {
            return (None, None);
        };
        self.cursor += 1;
        (Some(text), Some(token.span.end))
    }

    fn at_group_end(&self) -> bool {
        matches!(
            (
                self.tokens.get(self.cursor),
                self.tokens.get(self.cursor + 1)
            ),
            (
                Some(Spanned {
                    value: Token::Percent,
                    ..
                }),
                Some(Spanned {
                    value: Token::RParen,
                    ..
                })
            )
        )
    }

    fn at(&self, expected: Token<'src>) -> bool {
        self.tokens
            .get(self.cursor)
            .is_some_and(|token| token.value == expected)
    }

    fn advance_if(&mut self, expected: Token<'src>) -> Option<SpannedToken<'src>> {
        if self.at(expected) {
            self.advance()
        } else {
            None
        }
    }

    fn advance(&mut self) -> Option<SpannedToken<'src>> {
        let token = self.tokens.get(self.cursor)?.clone();
        self.cursor += 1;
        Some(token)
    }

    fn source_end(&self) -> usize {
        self.tokens.last().map_or(0, |token| token.span.end)
    }

    fn error_here(&self, kind: ParseErrorKind) -> ParseError {
        ParseError {
            kind,
            span: self
                .tokens
                .get(self.cursor)
                .map_or(Span::new(self.source_end(), self.source_end()), |token| {
                    token.span
                }),
        }
    }
}

fn escape_kind(code: char) -> EscapeKind {
    match code {
        'f' => EscapeKind::FileName,
        'F' => EscapeKind::FullPath,
        't' => EscapeKind::Tail,
        'l' => EscapeKind::Line,
        'c' => EscapeKind::Column,
        'v' => EscapeKind::VirtualColumn,
        'L' => EscapeKind::TotalLines,
        'p' | 'P' => EscapeKind::Percentage,
        'm' | 'M' => EscapeKind::Modified,
        'r' | 'R' => EscapeKind::ReadOnly,
        'h' | 'H' => EscapeKind::Help,
        'w' | 'W' => EscapeKind::Preview,
        'n' => EscapeKind::BufferNumber,
        'y' | 'Y' => EscapeKind::FileType,
        'e' => EscapeKind::Encoding,
        'o' => EscapeKind::FileFormat,
        'b' => EscapeKind::CharacterDecimal,
        'B' => EscapeKind::CharacterHex,
        unknown => EscapeKind::Unknown(unknown),
    }
}

#[cfg(test)]
mod tests {
    use super::parse;
    use crate::{
        ast::{Alignment, AstItem, EscapeKind},
        dialect::FormatDialect,
        error::ParseErrorKind,
        span::Span,
    };

    fn statusline(source: &str) -> crate::FormatAst {
        parse(source, FormatDialect::StatusLine).unwrap()
    }

    #[test]
    fn parses_literals_escapes_and_special_items() {
        let ast = statusline("file: %-10.20f %= %l%% %<");
        assert_eq!(ast.items.len(), 9);
        let AstItem::Escape(escape) = &ast.items[1].value else {
            panic!("expected escape")
        };
        assert_eq!(escape.kind, EscapeKind::FileName);
        assert_eq!(escape.field.min_width, Some(10));
        assert_eq!(escape.field.max_width, Some(20));
        assert_eq!(escape.field.alignment, Alignment::Left);
        assert!(matches!(ast.items[3].value, AstItem::Align));
        assert!(matches!(ast.items[6].value, AstItem::Escape(_)));
        assert!(matches!(ast.items[8].value, AstItem::Truncate));
    }

    #[test]
    fn parses_expression_highlight_and_reset() {
        let ast = statusline("%#WarningMsg#%{getline('.')}%*");
        assert!(matches!(&ast.items[0].value, AstItem::Highlight(name) if name == "WarningMsg"));
        assert!(matches!(&ast.items[1].value, AstItem::Expression(expr) if expr == "getline('.')"));
        assert!(matches!(ast.items[2].value, AstItem::ResetHighlight));
    }

    #[test]
    fn parses_nested_groups_with_field_specs() {
        let ast = statusline("%10(outer %(inner%)%)");
        let AstItem::Group { field, items } = &ast.items[0].value else {
            panic!("expected group")
        };
        assert_eq!(field.min_width, Some(10));
        assert!(matches!(items[1].value, AstItem::Group { .. }));
        assert_eq!(ast.items[0].span, Span::new(0, 21));
    }

    #[test]
    fn preserves_unknown_escape_for_forward_compatibility() {
        let ast = statusline("%Z");
        assert!(matches!(
            &ast.items[0].value,
            AstItem::Escape(crate::Escape {
                kind: EscapeKind::Unknown('Z'),
                ..
            })
        ));
    }

    #[test]
    fn diagnoses_unclosed_structures() {
        assert_eq!(
            parse("%{abc", FormatDialect::StatusLine).unwrap_err().kind,
            ParseErrorKind::UnclosedExpression
        );
        assert_eq!(
            parse("%#Error", FormatDialect::StatusLine)
                .unwrap_err()
                .kind,
            ParseErrorKind::UnclosedHighlight
        );
        assert_eq!(
            parse("%(abc", FormatDialect::StatusLine).unwrap_err().kind,
            ParseErrorKind::UnclosedGroup
        );
    }

    #[test]
    fn parses_tabline_targets_and_rejects_them_elsewhere() {
        let ast = parse("%1Tone%T%X%3X", FormatDialect::TabLine).unwrap();
        assert!(matches!(
            ast.items[0].value,
            AstItem::TablineTarget(crate::TablineTarget::Tab(1))
        ));
        assert!(matches!(
            ast.items[2].value,
            AstItem::TablineTarget(crate::TablineTarget::Reset)
        ));
        assert!(matches!(
            ast.items[3].value,
            AstItem::TablineTarget(crate::TablineTarget::Close(0))
        ));
        assert!(matches!(
            ast.items[4].value,
            AstItem::TablineTarget(crate::TablineTarget::Close(3))
        ));
        assert_eq!(
            parse("%1T", FormatDialect::StatusLine).unwrap_err().kind,
            ParseErrorKind::UnsupportedInDialect
        );
    }

    #[test]
    fn validates_widths_and_incomplete_directives() {
        assert_eq!(
            parse("%65536f", FormatDialect::StatusLine)
                .unwrap_err()
                .kind,
            ParseErrorKind::WidthOutOfRange
        );
        assert_eq!(
            parse("%.", FormatDialect::StatusLine).unwrap_err().kind,
            ParseErrorKind::UnexpectedEnd
        );
        assert_eq!(
            parse("%", FormatDialect::StatusLine).unwrap_err().kind,
            ParseErrorKind::UnexpectedEnd
        );
    }
}
