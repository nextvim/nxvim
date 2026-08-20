use crate::{
    ast::{
        Anchor, Backreference, CaseSwitch, CharacterClass, ClassKind, Collection, CollectionItem,
        Comparison, ComposingAtom, EnginePreference, Expr, GroupKind, LookaroundKind,
        MatchBoundary, Ordering, Pattern, PositionAtom, PosixClass, Quantifier, RepeatPreference,
        Spanned,
    },
    compiler::{CompileError, Diagnostic, DiagnosticKind, Phase},
    context::{MagicMode, TextRange},
    lexer::{Token, TokenKind, lex},
};

/// Parses a Vim pattern through the precedence levels implemented so far:
/// alternation, concatenation, groups, and primary atoms.
pub fn parse(source: &str, initial_magic: MagicMode) -> Result<Pattern, CompileError> {
    let tokens = lex(source, initial_magic)?;
    Parser::new(source, initial_magic, tokens).parse()
}

struct Parser<'a> {
    source: &'a str,
    initial_magic: MagicMode,
    tokens: Vec<Token>,
    cursor: usize,
    next_capture: u8,
    next_external_capture: u8,
}

impl<'a> Parser<'a> {
    fn new(source: &'a str, initial_magic: MagicMode, tokens: Vec<Token>) -> Self {
        Self {
            source,
            initial_magic,
            tokens,
            cursor: 0,
            next_capture: 1,
            next_external_capture: 1,
        }
    }

    fn parse(mut self) -> Result<Pattern, CompileError> {
        let expression = self.parse_alternation(false)?;
        if let Some(token) = self.peek() {
            return Err(self.error(
                token.span.clone(),
                "unmatched closing parenthesis",
                Some("remove the closing parenthesis or add a matching group opener"),
            ));
        }

        Ok(Pattern {
            source: self.source.to_owned(),
            initial_magic: self.initial_magic,
            expression,
        })
    }

    fn parse_alternation(&mut self, in_group: bool) -> Result<Spanned<Expr>, CompileError> {
        let start = self.current_offset();
        let mut branches = vec![self.parse_concatenation(in_group)?];

        while self.at(|kind| matches!(kind, TokenKind::Alternation)) {
            self.bump();
            branches.push(self.parse_concatenation(in_group)?);
        }

        let end = branches.last().map_or(start, |branch| branch.span.end);
        Ok(if branches.len() == 1 {
            branches.pop().expect("one branch was inserted")
        } else {
            Spanned::new(Expr::Alternation(branches), start..end)
        })
    }

    fn parse_concatenation(&mut self, in_group: bool) -> Result<Spanned<Expr>, CompileError> {
        let start = self.current_offset();
        let mut expressions = Vec::new();

        while let Some(token) = self.peek() {
            if matches!(token.kind, TokenKind::Alternation)
                || (in_group && matches!(token.kind, TokenKind::GroupClose))
            {
                break;
            }
            expressions.push(self.parse_postfix()?);
        }

        let end = expressions
            .last()
            .map_or(start, |expression| expression.span.end);
        Ok(match expressions.len() {
            0 => Spanned::new(Expr::Empty, start..start),
            1 => expressions.pop().expect("one expression was inserted"),
            _ => Spanned::new(Expr::Concat(expressions), start..end),
        })
    }

    fn parse_postfix(&mut self) -> Result<Spanned<Expr>, CompileError> {
        let mut expression = self.parse_primary()?;

        loop {
            if let Some(quantifier) = self.parse_quantifier()? {
                let end = self.previous_end();
                let start = expression.span.start;
                expression = Spanned::new(
                    Expr::Repeat {
                        expression: Box::new(expression),
                        quantifier,
                    },
                    start..end,
                );
                continue;
            }
            if let Some((kind, limit, end)) = self.parse_lookaround()? {
                let start = expression.span.start;
                expression = Spanned::new(
                    Expr::Lookaround {
                        expression: Box::new(expression),
                        kind,
                        limit,
                    },
                    start..end,
                );
                continue;
            }
            break;
        }

        Ok(expression)
    }

    fn parse_quantifier(&mut self) -> Result<Option<Quantifier>, CompileError> {
        let Some(token) = self.peek().cloned() else {
            return Ok(None);
        };
        let quantifier = match token.kind {
            TokenKind::ZeroOrMore => {
                self.bump();
                Quantifier {
                    min: 0,
                    max: None,
                    preference: RepeatPreference::Greedy,
                }
            }
            TokenKind::OneOrMore => {
                self.bump();
                Quantifier {
                    min: 1,
                    max: None,
                    preference: RepeatPreference::Greedy,
                }
            }
            TokenKind::Optional => {
                self.bump();
                Quantifier {
                    min: 0,
                    max: Some(1),
                    preference: RepeatPreference::Greedy,
                }
            }
            TokenKind::CountedRepeatOpen => return self.parse_counted_quantifier().map(Some),
            _ => return Ok(None),
        };
        Ok(Some(quantifier))
    }

    fn parse_counted_quantifier(&mut self) -> Result<Quantifier, CompileError> {
        let open = self.bump().expect("counted repeat opener exists");
        let content_start = open.span.end;
        let mut close_index = self.cursor;
        while let Some(token) = self.tokens.get(close_index) {
            if matches!(token.kind, TokenKind::Literal('}')) {
                break;
            }
            close_index += 1;
        }
        let Some(close) = self.tokens.get(close_index) else {
            return Err(self.error(
                open.span.start..self.source.len(),
                "unterminated counted quantifier",
                Some("add a closing `}`"),
            ));
        };
        let content = &self.source[content_start..close.span.start];
        let span = open.span.start..close.span.end;
        let quantifier = parse_counted_bounds(content).ok_or_else(|| {
            self.error(
                span.clone(),
                "E554: invalid counted quantifier",
                Some("use `{n}`, `{n,}`, `{,m}`, `{n,m}`, or `{-n,m}`"),
            )
        })?;
        self.cursor = close_index + 1;
        Ok(quantifier)
    }

    fn parse_lookaround(
        &mut self,
    ) -> Result<Option<(LookaroundKind, Option<usize>, usize)>, CompileError> {
        let Some(token) = self.peek() else {
            return Ok(None);
        };
        if !matches!(token.kind, TokenKind::Escaped('@')) {
            return Ok(None);
        }

        let start = token.span.start;
        let tail = &self.source[token.span.end..];
        let digits = tail.bytes().take_while(u8::is_ascii_digit).count();
        let limit = if digits == 0 {
            None
        } else {
            Some(tail[..digits].parse::<usize>().map_err(|_| {
                self.error(
                    start..token.span.end + digits,
                    "lookbehind limit is too large",
                    None,
                )
            })?)
        };
        let operator_tail = &tail[digits..];
        let (kind, operator_len) = if operator_tail.starts_with("<=") {
            (LookaroundKind::Behind, 2)
        } else if operator_tail.starts_with("<!") {
            (LookaroundKind::NegativeBehind, 2)
        } else if operator_tail.starts_with('=') {
            (LookaroundKind::Ahead, 1)
        } else if operator_tail.starts_with('!') {
            (LookaroundKind::NegativeAhead, 1)
        } else if operator_tail.starts_with('>') {
            (LookaroundKind::Atomic, 1)
        } else {
            return Err(self.error(
                token.span.clone(),
                "invalid lookaround operator",
                Some("expected `\\@=`, `\\@!`, `\\@<=`, `\\@<!`, or `\\@>`"),
            ));
        };
        if limit.is_some()
            && !matches!(
                kind,
                LookaroundKind::Behind | LookaroundKind::NegativeBehind
            )
        {
            return Err(self.error(
                start..token.span.end + digits + operator_len,
                "a numeric limit is only valid for lookbehind",
                None,
            ));
        }

        let end = token.span.end + digits + operator_len;
        while self.peek().is_some_and(|token| token.span.end <= end) {
            self.bump();
        }
        Ok(Some((kind, limit, end)))
    }

    fn parse_primary(&mut self) -> Result<Spanned<Expr>, CompileError> {
        if let Some(atom) = self.parse_vim_atom()? {
            return Ok(atom);
        }
        if let Some(anchor) = self.at_file_anchor() {
            let start = self.bump().expect("percent escape exists").span.start;
            let end = self.bump().expect("anchor token exists").span.end;
            return Ok(Spanned::new(Expr::Anchor(anchor), start..end));
        }
        if self.at_external_group_open() {
            let start = self.bump().expect("external escape exists").span.start;
            self.bump();
            let index = self.allocate_external_capture(start..self.previous_end())?;
            return self.parse_group(start, GroupKind::ExternalCapture { index });
        }
        if self.at_non_capturing_group_open() {
            let start = self.bump().expect("escape token exists").span.start;
            self.bump();
            return self.parse_group(start, GroupKind::NonCapturing);
        }

        let token = self.bump().expect("parse_primary is called with a token");
        let span = token.span.clone();
        let expression = match token.kind {
            TokenKind::Literal(character) => Expr::Literal(character.to_string()),
            TokenKind::Any => Expr::Dot {
                include_newline: false,
            },
            TokenKind::StartOfLine => {
                if self.is_start_of_line_anchor() {
                    Expr::Anchor(Anchor::StartOfLine)
                } else {
                    Expr::Literal("^".to_string())
                }
            }
            TokenKind::EndOfLine => {
                if self.is_end_of_line_anchor() {
                    Expr::Anchor(Anchor::EndOfLine)
                } else {
                    Expr::Literal("$".to_string())
                }
            }
            TokenKind::MagicSwitch(mode) => Expr::MagicSwitch(mode),
            TokenKind::GroupOpen => {
                let index = self.allocate_capture(span.clone())?;
                return self.parse_group(span.start, GroupKind::Capture { index });
            }
            TokenKind::GroupClose => {
                return Err(self.error(
                    span,
                    "unmatched closing parenthesis",
                    Some("remove the closing parenthesis or add a matching group opener"),
                ));
            }
            TokenKind::Collection(content) => Expr::Collection(
                parse_collection(&content)
                    .map_err(|message| self.error(span.clone(), message, None))?,
            ),
            TokenKind::Escaped(character @ '1'..='9') => {
                Expr::Backreference(Backreference::Capture(character as u8 - b'0'))
            }
            TokenKind::Escaped(character) if class_kind(character).is_some() => {
                let (kind, negated) = class_kind(character).expect("class was checked");
                Expr::Class(CharacterClass {
                    kind,
                    negated,
                    include_newline: false,
                })
            }
            TokenKind::Escaped('<') => Expr::Anchor(Anchor::StartOfWord),
            TokenKind::Escaped('>') => Expr::Anchor(Anchor::EndOfWord),
            TokenKind::Escaped(character) => {
                return Err(self.unsupported(
                    span,
                    format!("escaped atom `\\{character}` is not parsed yet"),
                ));
            }
            TokenKind::Alternation => unreachable!("alternation ends concatenation"),
            TokenKind::ZeroOrMore
            | TokenKind::OneOrMore
            | TokenKind::Optional
            | TokenKind::CountedRepeatOpen => {
                return Err(self.error(
                    span,
                    "quantifier has no preceding atom",
                    Some("place the quantifier after an atom"),
                ));
            }
            TokenKind::Tilde => {
                return Err(
                    self.unsupported(span, "the previous-substitute atom is not parsed yet")
                );
            }
        };
        Ok(Spanned::new(expression, span))
    }

    fn parse_vim_atom(&mut self) -> Result<Option<Spanned<Expr>>, CompileError> {
        let start = self.current_offset();
        let tail = &self.source[start..];

        if tail.starts_with(r"\_[")
            && let Some(Token {
                kind: TokenKind::Collection(content),
                span,
            }) = self.tokens.get(self.cursor + 1).cloned()
        {
            let mut collection = parse_collection(&content)
                .map_err(|message| self.error(start..span.end, message, None))?;
            collection.include_newline = true;
            self.cursor += 2;
            return Ok(Some(Spanned::new(
                Expr::Collection(collection),
                start..span.end,
            )));
        }

        let (expression, length) = if tail.starts_with(r"\z")
            && tail
                .as_bytes()
                .get(2)
                .is_some_and(|byte| matches!(byte, b'1'..=b'9'))
        {
            (
                Expr::Backreference(Backreference::External(tail.as_bytes()[2] - b'0')),
                3,
            )
        } else if tail.starts_with(r"\n") {
            (Expr::Literal("\n".into()), 2)
        } else if tail.starts_with(r"\_.") {
            (
                Expr::Dot {
                    include_newline: true,
                },
                3,
            )
        } else if let Some(character) = tail
            .strip_prefix(r"\_")
            .and_then(|tail| tail.chars().next())
            && let Some((kind, negated)) = class_kind(character)
        {
            (
                Expr::Class(CharacterClass {
                    kind,
                    negated,
                    include_newline: true,
                }),
                2 + character.len_utf8(),
            )
        } else if tail.starts_with(r"\zs") {
            (Expr::MatchBoundary(MatchBoundary::Start), 3)
        } else if tail.starts_with(r"\ze") {
            (Expr::MatchBoundary(MatchBoundary::End), 3)
        } else if tail.starts_with(r"\c") {
            (Expr::CaseSwitch(CaseSwitch::Insensitive), 2)
        } else if tail.starts_with(r"\C") {
            (Expr::CaseSwitch(CaseSwitch::Sensitive), 2)
        } else if tail.starts_with(r"\Z") {
            (Expr::Composing(ComposingAtom::IgnoreFollowing), 2)
        } else if tail.starts_with(r"\%C") {
            (Expr::Composing(ComposingAtom::AnyCombiningMark), 3)
        } else if let Some(engine_tail) = tail.strip_prefix(r"\%#=") {
            let span_end = start + 4 + engine_tail.chars().next().map_or(0, char::len_utf8);
            let preference = match engine_tail.as_bytes().first() {
                Some(b'0') => EnginePreference::Automatic,
                Some(b'1') => EnginePreference::Backtracking,
                Some(b'2') => EnginePreference::Nfa,
                _ => {
                    return Err(self.error(
                        start..span_end,
                        "invalid Vim regex engine selection",
                        Some("use `\\%#=0`, `\\%#=1`, or `\\%#=2`"),
                    ));
                }
            };
            (Expr::EnginePreference(preference), 5)
        } else if tail.starts_with(r"\%#") {
            (Expr::Position(PositionAtom::Cursor), 3)
        } else if tail.starts_with(r"\%V") {
            (Expr::Position(PositionAtom::VisualArea), 3)
        } else if tail.starts_with(r"\%") {
            let bytes = tail.as_bytes();
            let mut offset = 2;
            let ordering = match bytes.get(offset) {
                Some(b'<') => {
                    offset += 1;
                    Ordering::LessThan
                }
                Some(b'>') => {
                    offset += 1;
                    Ordering::GreaterThan
                }
                _ => Ordering::Equal,
            };
            let digits_start = offset;
            while bytes.get(offset).is_some_and(u8::is_ascii_digit) {
                offset += 1;
            }
            let Some(kind) = bytes.get(offset).copied() else {
                return Ok(None);
            };
            if digits_start == offset || !matches!(kind, b'l' | b'c' | b'v') {
                return Ok(None);
            }
            let value = tail[digits_start..offset].parse::<usize>().map_err(|_| {
                self.error(
                    start..start + offset,
                    "Vim position value is too large",
                    None,
                )
            })?;
            let comparison = Comparison { ordering, value };
            let position = match kind {
                b'l' => PositionAtom::Line(comparison),
                b'c' => PositionAtom::ByteColumn(comparison),
                b'v' => PositionAtom::VirtualColumn(comparison),
                _ => unreachable!("position kind was validated"),
            };
            (Expr::Position(position), offset + 1)
        } else {
            return Ok(None);
        };

        let end = start + length;
        while self.peek().is_some_and(|token| token.span.end <= end) {
            self.bump();
        }
        Ok(Some(Spanned::new(expression, start..end)))
    }

    fn parse_group(
        &mut self,
        start: usize,
        kind: GroupKind,
    ) -> Result<Spanned<Expr>, CompileError> {
        let expression = self.parse_alternation(true)?;
        let Some(close) = self.peek() else {
            return Err(self.error(
                start..self.source.len(),
                "unterminated group",
                Some("add a matching closing parenthesis"),
            ));
        };
        if !matches!(close.kind, TokenKind::GroupClose) {
            return Err(self.error(
                start..close.span.end,
                "unterminated group",
                Some("add a matching closing parenthesis"),
            ));
        }
        let end = close.span.end;
        self.bump();
        Ok(Spanned::new(
            Expr::Group {
                kind,
                expression: Box::new(expression),
            },
            start..end,
        ))
    }

    fn allocate_capture(&mut self, span: TextRange) -> Result<u8, CompileError> {
        if self.next_capture > 9 {
            return Err(self.error(
                span,
                "Vim patterns support at most nine capturing groups",
                Some("use `\\%(` for a non-capturing group"),
            ));
        }
        let index = self.next_capture;
        self.next_capture += 1;
        Ok(index)
    }

    fn at_file_anchor(&self) -> Option<Anchor> {
        match self.tokens.get(self.cursor..self.cursor + 2) {
            Some(
                [
                    Token {
                        kind: TokenKind::Escaped('%'),
                        ..
                    },
                    Token {
                        kind: TokenKind::StartOfLine,
                        ..
                    },
                ],
            ) => Some(Anchor::StartOfFile),
            Some(
                [
                    Token {
                        kind: TokenKind::Escaped('%'),
                        ..
                    },
                    Token {
                        kind: TokenKind::EndOfLine,
                        ..
                    },
                ],
            ) => Some(Anchor::EndOfFile),
            _ => None,
        }
    }

    fn at_external_group_open(&self) -> bool {
        matches!(
            self.tokens.get(self.cursor..self.cursor + 2),
            Some([
                Token {
                    kind: TokenKind::Escaped('z'),
                    ..
                },
                Token {
                    kind: TokenKind::Literal('('),
                    ..
                }
            ])
        )
    }

    fn at_non_capturing_group_open(&self) -> bool {
        matches!(
            self.tokens.get(self.cursor..self.cursor + 2),
            Some([
                Token {
                    kind: TokenKind::Escaped('%') | TokenKind::Literal('%'),
                    ..
                },
                Token {
                    kind: TokenKind::Literal('(') | TokenKind::GroupOpen,
                    ..
                }
            ])
        )
    }

    fn allocate_external_capture(&mut self, span: TextRange) -> Result<u8, CompileError> {
        if self.next_external_capture > 9 {
            return Err(self.error(
                span,
                "Vim patterns support at most nine external captures",
                None,
            ));
        }
        let index = self.next_external_capture;
        self.next_external_capture += 1;
        Ok(index)
    }

    fn is_start_of_line_anchor(&self) -> bool {
        if self.cursor <= 1 {
            return true;
        }
        let mut idx = self.cursor - 1;
        while idx > 0 {
            idx -= 1;
            match &self.tokens[idx].kind {
                TokenKind::MagicSwitch(_) => continue,
                TokenKind::Escaped('c' | 'C' | 'Z') => continue,
                TokenKind::Alternation | TokenKind::GroupOpen => return true,
                _ => return false,
            }
        }
        true
    }

    fn is_end_of_line_anchor(&self) -> bool {
        let mut idx = self.cursor;
        while idx < self.tokens.len() {
            match &self.tokens[idx].kind {
                TokenKind::MagicSwitch(_) => {}
                TokenKind::Escaped('c' | 'C' | 'Z') => {}
                TokenKind::Alternation | TokenKind::GroupClose => return true,
                _ => return false,
            }
            idx += 1;
        }
        true
    }

    fn at(&self, predicate: impl FnOnce(&TokenKind) -> bool) -> bool {
        self.peek().is_some_and(|token| predicate(&token.kind))
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.cursor)
    }

    fn bump(&mut self) -> Option<Token> {
        let token = self.tokens.get(self.cursor).cloned()?;
        self.cursor += 1;
        Some(token)
    }

    fn current_offset(&self) -> usize {
        self.peek()
            .map_or(self.source.len(), |token| token.span.start)
    }

    fn previous_end(&self) -> usize {
        self.tokens
            .get(self.cursor.saturating_sub(1))
            .map_or(0, |token| token.span.end)
    }

    fn error(
        &self,
        span: TextRange,
        message: impl Into<String>,
        help: Option<&str>,
    ) -> CompileError {
        CompileError {
            diagnostics: vec![Diagnostic {
                kind: DiagnosticKind::InvalidSyntax,
                phase: Phase::Parse,
                span,
                message: message.into(),
                help: help.map(str::to_owned),
            }],
        }
    }

    fn unsupported(&self, span: TextRange, message: impl Into<String>) -> CompileError {
        CompileError {
            diagnostics: vec![Diagnostic {
                kind: DiagnosticKind::Unsupported,
                phase: Phase::Parse,
                span,
                message: message.into(),
                help: None,
            }],
        }
    }
}

fn class_kind(character: char) -> Option<(ClassKind, bool)> {
    let negated = character.is_ascii_uppercase();
    let kind = match character.to_ascii_lowercase() {
        'a' => ClassKind::Alphabetic,
        'd' => ClassKind::Digit,
        'x' => ClassKind::HexDigit,
        'o' => ClassKind::OctalDigit,
        'h' => ClassKind::HeadOfWord,
        'l' => ClassKind::Lowercase,
        'u' => ClassKind::Uppercase,
        'w' => ClassKind::Word,
        'k' => ClassKind::Keyword,
        'f' => ClassKind::FileName,
        'p' => ClassKind::Printable,
        's' => ClassKind::Whitespace,
        _ => return None,
    };
    Some((kind, negated))
}

fn parse_collection(content: &str) -> Result<Collection, &'static str> {
    let (negated, content) = content
        .strip_prefix('^')
        .map_or((false, content), |rest| (true, rest));
    let mut atoms = Vec::new();
    let mut offset = 0;
    while offset < content.len() {
        let tail = &content[offset..];
        if let Some(inner) = tail
            .strip_prefix("[:")
            .and_then(|tail| tail.split_once(":]"))
        {
            atoms.push(CollectionItem::Posix(parse_posix_class(inner.0)?));
            offset += inner.0.len() + 4;
            continue;
        }
        if let Some(inner) = tail
            .strip_prefix("[=")
            .and_then(|tail| tail.split_once("=]"))
        {
            let mut characters = inner.0.chars();
            let character = characters.next().ok_or("empty equivalence class")?;
            if characters.next().is_some() {
                return Err("equivalence class must contain one character");
            }
            atoms.push(CollectionItem::Equivalence(character));
            offset += inner.0.len() + 4;
            continue;
        }
        if let Some(inner) = tail
            .strip_prefix("[.")
            .and_then(|tail| tail.split_once(".]"))
        {
            if inner.0.is_empty() {
                return Err("empty collating element");
            }
            atoms.push(CollectionItem::CollatingElement(inner.0.to_owned()));
            offset += inner.0.len() + 4;
            continue;
        }

        let mut characters = tail.chars();
        let mut character = characters.next().expect("tail is non-empty");
        offset += character.len_utf8();
        if character == '\\' {
            character = characters.next().ok_or("trailing escape in collection")?;
            offset += character.len_utf8();
        }
        atoms.push(CollectionItem::Character(character));
    }

    let mut items = Vec::new();
    let mut atoms = atoms.into_iter().peekable();
    while let Some(atom) = atoms.next() {
        if let CollectionItem::Character(start) = atom
            && matches!(atoms.peek(), Some(CollectionItem::Character('-')))
        {
            atoms.next();
            if let Some(CollectionItem::Character(end)) = atoms.next() {
                if start > end {
                    return Err("E944: reversed character range");
                }
                items.push(CollectionItem::Range(start, end));
                continue;
            }
            items.push(CollectionItem::Character(start));
            items.push(CollectionItem::Character('-'));
            break;
        }
        items.push(atom);
    }

    Ok(Collection {
        negated,
        include_newline: false,
        items,
    })
}

fn parse_posix_class(name: &str) -> Result<PosixClass, &'static str> {
    match name {
        "alnum" => Ok(PosixClass::Alnum),
        "alpha" => Ok(PosixClass::Alpha),
        "blank" => Ok(PosixClass::Blank),
        "cntrl" => Ok(PosixClass::Cntrl),
        "digit" => Ok(PosixClass::Digit),
        "graph" => Ok(PosixClass::Graph),
        "lower" => Ok(PosixClass::Lower),
        "print" => Ok(PosixClass::Print),
        "punct" => Ok(PosixClass::Punct),
        "space" => Ok(PosixClass::Space),
        "upper" => Ok(PosixClass::Upper),
        "xdigit" => Ok(PosixClass::Xdigit),
        _ => Err("unknown POSIX character class"),
    }
}

fn parse_counted_bounds(content: &str) -> Option<Quantifier> {
    let (preference, bounds) = match content.strip_prefix('-') {
        Some(bounds) => (RepeatPreference::Minimal, bounds),
        None => (RepeatPreference::Greedy, content),
    };

    let (min, max) = if let Some((minimum, maximum)) = bounds.split_once(',') {
        if maximum.contains(',') {
            return None;
        }
        let min = if minimum.is_empty() {
            0
        } else {
            minimum.parse().ok()?
        };
        let max = if maximum.is_empty() {
            None
        } else {
            Some(maximum.parse().ok()?)
        };
        (min, max)
    } else if bounds.is_empty() && preference == RepeatPreference::Minimal {
        (0, None)
    } else {
        let exact = bounds.parse().ok()?;
        (exact, Some(exact))
    };

    if max.is_some_and(|maximum| min > maximum) {
        return None;
    }
    Some(Quantifier {
        min,
        max,
        preference,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn expression(source: &str) -> Spanned<Expr> {
        parse(source, MagicMode::Magic)
            .expect("pattern should parse")
            .expression
    }

    #[test]
    fn alternation_has_lower_precedence_than_concatenation() {
        let parsed = expression(r"ab\|cd");
        assert_eq!(parsed.span, 0..6);
        let Expr::Alternation(branches) = parsed.value else {
            panic!("expected alternation");
        };
        assert_eq!(branches.len(), 2);
        assert!(matches!(branches[0].value, Expr::Concat(_)));
        assert!(matches!(branches[1].value, Expr::Concat(_)));
    }

    #[test]
    fn groups_override_precedence_and_number_captures_by_opening_order() {
        let parsed = expression(r"\(a\|\(bc\)\)d");
        let Expr::Concat(expressions) = parsed.value else {
            panic!("expected outer concatenation");
        };
        let Expr::Group { kind, expression } = &expressions[0].value else {
            panic!("expected capture group");
        };
        assert_eq!(*kind, GroupKind::Capture { index: 1 });
        let Expr::Alternation(branches) = &expression.value else {
            panic!("expected grouped alternation");
        };
        assert!(matches!(
            branches[1].value,
            Expr::Group {
                kind: GroupKind::Capture { index: 2 },
                ..
            }
        ));
    }

    #[test]
    fn parses_non_capturing_groups_and_empty_branches() {
        let parsed = expression(r"\%(a\|\|b\)");
        let Expr::Group { kind, expression } = parsed.value else {
            panic!("expected group");
        };
        assert_eq!(kind, GroupKind::NonCapturing);
        let Expr::Alternation(branches) = expression.value else {
            panic!("expected alternation");
        };
        assert_eq!(branches.len(), 3);
        assert!(matches!(branches[1].value, Expr::Empty));
    }

    #[test]
    fn reports_unmatched_group_delimiters_with_byte_spans() {
        let error = parse("λ\\(x", MagicMode::Magic).expect_err("group should be invalid");
        assert_eq!(error.diagnostics[0].phase, Phase::Parse);
        assert_eq!(error.diagnostics[0].span, 2..5);

        let error = parse(r"x\)", MagicMode::Magic).expect_err("close should be invalid");
        assert_eq!(error.diagnostics[0].span, 1..3);
    }

    #[test]
    fn quantifiers_bind_more_tightly_than_concatenation() {
        let parsed = expression(r"ab*c\+d\=e\{2,4}");
        let Expr::Concat(expressions) = parsed.value else {
            panic!("expected concatenation");
        };
        assert_eq!(expressions.len(), 5);
        assert!(matches!(
            expressions[1].value,
            Expr::Repeat {
                quantifier: Quantifier {
                    min: 0,
                    max: None,
                    ..
                },
                ..
            }
        ));
        assert!(matches!(
            expressions[2].value,
            Expr::Repeat {
                quantifier: Quantifier {
                    min: 1,
                    max: None,
                    ..
                },
                ..
            }
        ));
        assert!(matches!(
            expressions[3].value,
            Expr::Repeat {
                quantifier: Quantifier {
                    min: 0,
                    max: Some(1),
                    ..
                },
                ..
            }
        ));
        assert!(matches!(
            expressions[4].value,
            Expr::Repeat {
                quantifier: Quantifier {
                    min: 2,
                    max: Some(4),
                    ..
                },
                ..
            }
        ));
    }

    #[test]
    fn parses_minimal_and_open_counted_quantifiers() {
        for (source, min, max) in [
            (r"x\{-}", 0, None),
            (r"x\{-2,}", 2, None),
            (r"x\{,3}", 0, Some(3)),
        ] {
            let Expr::Repeat { quantifier, .. } = expression(source).value else {
                panic!("expected repeat");
            };
            assert_eq!(quantifier.min, min);
            assert_eq!(quantifier.max, max);
            assert_eq!(
                quantifier.preference,
                if source.contains("{-") {
                    RepeatPreference::Minimal
                } else {
                    RepeatPreference::Greedy
                }
            );
        }
    }

    #[test]
    fn parses_postfix_lookarounds_and_limits() {
        for (source, expected_kind, expected_limit) in [
            (r"foo\@=", LookaroundKind::Ahead, None),
            (r"foo\@!", LookaroundKind::NegativeAhead, None),
            (r"foo\@<=", LookaroundKind::Behind, None),
            (r"foo\@<!", LookaroundKind::NegativeBehind, None),
            (r"foo\@>", LookaroundKind::Atomic, None),
            (r"foo\@12<=", LookaroundKind::Behind, Some(12)),
        ] {
            let Expr::Concat(parts) = expression(source).value else {
                panic!("expected concatenation");
            };
            assert!(matches!(
                parts[2].value,
                Expr::Lookaround { kind, limit, .. } if kind == expected_kind && limit == expected_limit
            ));
        }
    }

    #[test]
    fn parses_anchors_and_backreferences() {
        let Expr::Concat(parts) = expression(r"\%^\<\(x\)\1\>\%$").value else {
            panic!("expected concatenation");
        };
        assert!(matches!(parts[0].value, Expr::Anchor(Anchor::StartOfFile)));
        assert!(matches!(parts[1].value, Expr::Anchor(Anchor::StartOfWord)));
        assert!(matches!(
            parts[3].value,
            Expr::Backreference(Backreference::Capture(1))
        ));
        assert!(matches!(parts[4].value, Expr::Anchor(Anchor::EndOfWord)));
        assert!(matches!(parts[5].value, Expr::Anchor(Anchor::EndOfFile)));
    }

    #[test]
    fn parses_vim_only_control_and_composing_atoms() {
        let Expr::Concat(parts) = expression(r"\c\C\zs\ze\Z\%C").value else {
            panic!("expected concatenation");
        };
        assert_eq!(parts.len(), 6);
        assert!(matches!(
            parts[0].value,
            Expr::CaseSwitch(CaseSwitch::Insensitive)
        ));
        assert!(matches!(
            parts[1].value,
            Expr::CaseSwitch(CaseSwitch::Sensitive)
        ));
        assert!(matches!(
            parts[2].value,
            Expr::MatchBoundary(MatchBoundary::Start)
        ));
        assert!(matches!(
            parts[3].value,
            Expr::MatchBoundary(MatchBoundary::End)
        ));
        assert!(matches!(
            parts[4].value,
            Expr::Composing(ComposingAtom::IgnoreFollowing)
        ));
        assert!(matches!(
            parts[5].value,
            Expr::Composing(ComposingAtom::AnyCombiningMark)
        ));
        assert_eq!(parts[5].span, 12..15);
    }

    #[test]
    fn parses_vim_position_and_engine_atoms_without_losing_semantics() {
        let Expr::Concat(parts) = expression(r"\%23l\%>4c\%<9v\%#\%V\%#=0\%#=1\%#=2").value else {
            panic!("expected concatenation");
        };
        assert!(matches!(
            parts[0].value,
            Expr::Position(PositionAtom::Line(Comparison {
                ordering: Ordering::Equal,
                value: 23
            }))
        ));
        assert!(matches!(
            parts[1].value,
            Expr::Position(PositionAtom::ByteColumn(Comparison {
                ordering: Ordering::GreaterThan,
                value: 4
            }))
        ));
        assert!(matches!(
            parts[2].value,
            Expr::Position(PositionAtom::VirtualColumn(Comparison {
                ordering: Ordering::LessThan,
                value: 9
            }))
        ));
        assert!(matches!(
            parts[3].value,
            Expr::Position(PositionAtom::Cursor)
        ));
        assert!(matches!(
            parts[4].value,
            Expr::Position(PositionAtom::VisualArea)
        ));
        assert!(matches!(
            parts[5].value,
            Expr::EnginePreference(EnginePreference::Automatic)
        ));
        assert!(matches!(
            parts[6].value,
            Expr::EnginePreference(EnginePreference::Backtracking)
        ));
        assert!(matches!(
            parts[7].value,
            Expr::EnginePreference(EnginePreference::Nfa)
        ));
    }

    #[test]
    fn rejects_invalid_vim_engine_selection() {
        for source in [r"\%#=", r"\%#=3", r"λ\%#=x"] {
            let error = parse(source, MagicMode::Magic).expect_err("engine atom should be invalid");
            assert_eq!(error.diagnostics[0].kind, DiagnosticKind::InvalidSyntax);
            assert_eq!(error.diagnostics[0].phase, Phase::Parse);
        }
        let error = parse(r"λ\%#=x", MagicMode::Magic).expect_err("engine atom should be invalid");
        assert_eq!(error.diagnostics[0].span, 2..7);
    }

    #[test]
    fn rejects_malformed_postfix_syntax() {
        for source in [r"*x", r"x\{4,2}", r"x\{2", r"x\@7="] {
            let error = parse(source, MagicMode::Magic).expect_err("pattern should be invalid");
            assert_eq!(error.diagnostics[0].kind, DiagnosticKind::InvalidSyntax);
            assert_eq!(error.diagnostics[0].phase, Phase::Parse);
        }
    }

    #[test]
    fn parses_builtin_classes_with_negation() {
        let pattern = parse(r"\d\K\f\P", MagicMode::Magic).unwrap();
        let Expr::Concat(parts) = pattern.expression.value else {
            panic!("expected class concatenation")
        };
        assert!(matches!(
            parts[0].value,
            Expr::Class(CharacterClass {
                kind: ClassKind::Digit,
                negated: false,
                ..
            })
        ));
        assert!(matches!(
            parts[1].value,
            Expr::Class(CharacterClass {
                kind: ClassKind::Keyword,
                negated: true,
                ..
            })
        ));
        assert!(matches!(
            parts[2].value,
            Expr::Class(CharacterClass {
                kind: ClassKind::FileName,
                negated: false,
                ..
            })
        ));
        assert!(matches!(
            parts[3].value,
            Expr::Class(CharacterClass {
                kind: ClassKind::Printable,
                negated: true,
                ..
            })
        ));
    }
}
