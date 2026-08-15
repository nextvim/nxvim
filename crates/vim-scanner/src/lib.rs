//! A lightweight, purely lexical "structural scanner" for text buffers.
//!
//! [`StructuralScanner`] walks a buffer's raw text and tracks brace/paren/
//! bracket/quote nesting with a simple stack, without understanding any
//! particular language's grammar. It exists as a cheap fallback for editor
//! features (folding, `i{`/`a{`-style text objects, etc.) that normally rely
//! on a tree-sitter [`SyntaxTree`] but need to keep working reasonably well
//! when no grammar is available for a buffer (or parsing hasn't completed
//! yet).
//!
//! The scanning rules are intentionally simple:
//!
//! - `{`, `(`, `[` push a [`Delimiter`] onto the stack.
//! - `"`, `'` also push a [`Delimiter`] and put the scanner into "string
//!   mode": while inside a string, every character other than a matching,
//!   unescaped closing quote (or a backslash starting an escape) is treated
//!   as plain text, so brace-like characters inside string literals are
//!   never mistaken for real structure.
//! - `}`, `)`, `]` pop the stack *if* its top is the matching opener,
//!   producing a [`MatchedDelimiter`]. A closer that doesn't match the top
//!   of the stack is treated as stray text and ignored, so mismatched or
//!   otherwise invalid code doesn't throw off the rest of the scan.
//!
//! This does not understand comments: a `{` inside a `//` or `/* */`
//! comment is still treated as real structure. That's a deliberate
//! trade-off to keep the scanner simple and dependency-free; callers that
//! need better accuracy should prefer a real [`SyntaxTree`] when one is
//! available.
//!
//! [`SyntaxTree`]: https://docs.rs/vim-treesitter (not a dependency of this crate)

/// A byte offset into the scanned text.
pub type Position = usize;

/// The kind of a structural delimiter this scanner understands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DelimiterKind {
    Brace,
    Paren,
    Bracket,
    DoubleQuote,
    SingleQuote,
}

impl DelimiterKind {
    /// Whether this delimiter kind opens a string (as opposed to a
    /// brace/paren/bracket block).
    pub fn is_quote(self) -> bool {
        matches!(
            self,
            DelimiterKind::DoubleQuote | DelimiterKind::SingleQuote
        )
    }

    /// Whether this delimiter kind opens a brace/paren/bracket block (as
    /// opposed to a string).
    pub fn is_block(self) -> bool {
        !self.is_quote()
    }

    pub fn opening_char(self) -> char {
        match self {
            DelimiterKind::Brace => '{',
            DelimiterKind::Paren => '(',
            DelimiterKind::Bracket => '[',
            DelimiterKind::DoubleQuote => '"',
            DelimiterKind::SingleQuote => '\'',
        }
    }

    pub fn closing_char(self) -> char {
        match self {
            DelimiterKind::Brace => '}',
            DelimiterKind::Paren => ')',
            DelimiterKind::Bracket => ']',
            DelimiterKind::DoubleQuote => '"',
            DelimiterKind::SingleQuote => '\'',
        }
    }
}

/// A delimiter that has been opened (pushed) but not yet closed at the point
/// it was recorded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Delimiter {
    pub kind: DelimiterKind,
    /// Byte offset of the opening delimiter character itself.
    pub start: Position,
}

/// A completed pair of delimiters: the byte offsets of the opening and
/// closing delimiter characters themselves (not the content between them).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MatchedDelimiter {
    pub kind: DelimiterKind,
    /// Byte offset of the opening delimiter character.
    pub start: Position,
    /// Byte offset of the closing delimiter character.
    pub end: Position,
}

impl MatchedDelimiter {
    /// The byte range spanning both delimiter characters and everything
    /// between them (i.e. what `a{`/`a"` text objects would select).
    pub fn outer_range(&self) -> std::ops::Range<Position> {
        self.start..self.end + 1
    }

    /// The byte range strictly between the delimiter characters (i.e. what
    /// `i{`/`i"` text objects would select).
    pub fn inner_range(&self) -> std::ops::Range<Position> {
        (self.start + 1)..self.end
    }
}

/// The result of scanning a buffer's text for structural delimiters: every
/// matched pair found, plus any delimiters that were opened but never
/// closed.
#[derive(Debug, Clone, Default)]
pub struct StructuralScanner {
    matches: Vec<MatchedDelimiter>,
    unmatched: Vec<Delimiter>,
}

impl StructuralScanner {
    /// Scans `text` in a single left-to-right pass, tracking delimiter
    /// nesting with a stack.
    ///
    /// This allocates nothing beyond the resulting matches: prefer
    /// [`StructuralScanner::scan_chunks`] when the buffer's text is already
    /// available as a sequence of chunks (e.g. straight from a rope), so
    /// callers don't have to first materialize the whole buffer into one
    /// owned `String` just to scan it.
    pub fn scan(text: &str) -> Self {
        Self::scan_chunks(std::iter::once(text))
    }

    /// Like [`StructuralScanner::scan`], but scans a sequence of chunks that
    /// concatenate to the full text, without requiring them to be joined
    /// into a single contiguous string first. Byte offsets in the resulting
    /// [`Delimiter`]/[`MatchedDelimiter`] values are relative to the start
    /// of the first chunk, as if the chunks had been concatenated.
    pub fn scan_chunks<'a>(chunks: impl IntoIterator<Item = &'a str>) -> Self {
        let mut stack: Vec<Delimiter> = Vec::new();
        let mut matches = Vec::new();
        let mut escape_next = false;
        let mut base = 0usize;

        for chunk in chunks {
            for (local_idx, ch) in chunk.char_indices() {
                let idx = base + local_idx;
                let in_string = stack.last().is_some_and(|open| open.kind.is_quote());

                if in_string {
                    if escape_next {
                        escape_next = false;
                        continue;
                    }
                    match ch {
                        '\\' => escape_next = true,
                        '"' if stack.last().unwrap().kind == DelimiterKind::DoubleQuote => {
                            Self::close(&mut stack, &mut matches, idx);
                        }
                        '\'' if stack.last().unwrap().kind == DelimiterKind::SingleQuote => {
                            Self::close(&mut stack, &mut matches, idx);
                        }
                        _ => {}
                    }
                    continue;
                }

                match ch {
                    '{' => stack.push(Delimiter {
                        kind: DelimiterKind::Brace,
                        start: idx,
                    }),
                    '(' => stack.push(Delimiter {
                        kind: DelimiterKind::Paren,
                        start: idx,
                    }),
                    '[' => stack.push(Delimiter {
                        kind: DelimiterKind::Bracket,
                        start: idx,
                    }),
                    '"' => stack.push(Delimiter {
                        kind: DelimiterKind::DoubleQuote,
                        start: idx,
                    }),
                    '\'' => stack.push(Delimiter {
                        kind: DelimiterKind::SingleQuote,
                        start: idx,
                    }),
                    '}' if stack
                        .last()
                        .is_some_and(|open| open.kind == DelimiterKind::Brace) =>
                    {
                        Self::close(&mut stack, &mut matches, idx);
                    }
                    ')' if stack
                        .last()
                        .is_some_and(|open| open.kind == DelimiterKind::Paren) =>
                    {
                        Self::close(&mut stack, &mut matches, idx);
                    }
                    ']' if stack
                        .last()
                        .is_some_and(|open| open.kind == DelimiterKind::Bracket) =>
                    {
                        Self::close(&mut stack, &mut matches, idx);
                    }
                    _ => {}
                }
            }
            base += chunk.len();
        }

        Self {
            matches,
            unmatched: stack,
        }
    }

    /// Pops the stack's top delimiter, recording it as matched at `end`.
    /// Callers must only invoke this once they've confirmed the top of the
    /// stack is the expected kind.
    fn close(stack: &mut Vec<Delimiter>, matches: &mut Vec<MatchedDelimiter>, end: Position) {
        let open = stack.pop().expect("caller checked stack.last()");
        matches.push(MatchedDelimiter {
            kind: open.kind,
            start: open.start,
            end,
        });
    }

    /// Every matched delimiter pair found by the scan, in the order their
    /// closing delimiter was encountered.
    pub fn matches(&self) -> &[MatchedDelimiter] {
        &self.matches
    }

    /// Delimiters that were opened but never closed by the end of the text
    /// (e.g. an unterminated string, or unbalanced code).
    pub fn unmatched(&self) -> &[Delimiter] {
        &self.unmatched
    }

    /// The smallest matched pair (of any kind) whose range contains `byte`,
    /// i.e. the innermost delimiter pair enclosing that position. Analogous
    /// to a tree-sitter `SyntaxTree::delimiter_boundaries_at_byte`.
    pub fn innermost_at(&self, byte: Position) -> Option<MatchedDelimiter> {
        self.matches
            .iter()
            .filter(|m| m.start <= byte && byte <= m.end)
            .min_by_key(|m| m.end - m.start)
            .copied()
    }

    /// Like [`innermost_at`](Self::innermost_at), but only considers
    /// brace/paren/bracket blocks, ignoring quoted strings. Analogous to a
    /// tree-sitter `SyntaxTree::enclosing_block_at_byte`.
    pub fn enclosing_block_at(&self, byte: Position) -> Option<MatchedDelimiter> {
        self.matches
            .iter()
            .filter(|m| m.kind.is_block() && m.start <= byte && byte <= m.end)
            .min_by_key(|m| m.end - m.start)
            .copied()
    }

    /// The `(start, end)` byte offsets of the innermost delimiter pair
    /// enclosing `byte`, matching the shape tree-sitter's
    /// `delimiter_boundaries_at_byte` returns.
    pub fn delimiter_boundaries_at(&self, byte: Position) -> Option<(Position, Position)> {
        self.innermost_at(byte).map(|m| (m.start, m.end))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_a_simple_brace_pair() {
        let scan = StructuralScanner::scan("a { b } c");
        assert_eq!(
            scan.matches(),
            &[MatchedDelimiter {
                kind: DelimiterKind::Brace,
                start: 2,
                end: 6,
            }]
        );
        assert!(scan.unmatched().is_empty());
    }

    #[test]
    fn innermost_at_prefers_the_smallest_enclosing_pair() {
        let scan = StructuralScanner::scan("{ ( ) }");
        // Positions: 0:'{' 1:' ' 2:'(' 3:' ' 4:')' 5:' ' 6:'}'
        let inner = scan.innermost_at(3).unwrap();
        assert_eq!(inner.kind, DelimiterKind::Paren);
        assert_eq!((inner.start, inner.end), (2, 4));

        let outer = scan.innermost_at(1).unwrap();
        assert_eq!(outer.kind, DelimiterKind::Brace);
        assert_eq!((outer.start, outer.end), (0, 6));
    }

    #[test]
    fn braces_inside_a_string_do_not_affect_matching() {
        let scan = StructuralScanner::scan("{ \"}\" }");
        // Positions: 0:'{' 1:' ' 2:'"' 3:'}' 4:'"' 5:' ' 6:'}'
        assert_eq!(scan.matches().len(), 2);

        let brace = scan
            .matches()
            .iter()
            .find(|m| m.kind == DelimiterKind::Brace)
            .unwrap();
        assert_eq!((brace.start, brace.end), (0, 6));

        let string = scan
            .matches()
            .iter()
            .find(|m| m.kind == DelimiterKind::DoubleQuote)
            .unwrap();
        assert_eq!((string.start, string.end), (2, 4));
    }

    #[test]
    fn escaped_quotes_do_not_end_the_string() {
        // a"b -- but with an escaped quote in the middle: a\"b
        let text = "\"a\\\"b\"";
        // Chars: 0:'"' 1:'a' 2:'\\' 3:'"' 4:'b' 5:'"'
        let scan = StructuralScanner::scan(text);
        assert_eq!(scan.matches().len(), 1);
        let m = scan.matches()[0];
        assert_eq!(m.kind, DelimiterKind::DoubleQuote);
        assert_eq!((m.start, m.end), (0, 5));
    }

    #[test]
    fn a_stray_closer_without_a_matching_opener_is_ignored() {
        let scan = StructuralScanner::scan("a } b");
        assert!(scan.matches().is_empty());
        assert!(scan.unmatched().is_empty());
    }

    #[test]
    fn unclosed_delimiters_are_reported_as_unmatched() {
        let scan = StructuralScanner::scan("{ ( ");
        assert!(scan.matches().is_empty());
        let unmatched: Vec<_> = scan.unmatched().iter().map(|d| d.kind).collect();
        assert_eq!(unmatched, vec![DelimiterKind::Brace, DelimiterKind::Paren]);
    }

    #[test]
    fn enclosing_block_at_skips_quotes() {
        let scan = StructuralScanner::scan("( \"x\" )");
        // Positions: 0:'(' 1:' ' 2:'"' 3:'x' 4:'"' 5:' ' 6:')'
        let block = scan.enclosing_block_at(3).unwrap();
        assert_eq!(block.kind, DelimiterKind::Paren);
        assert_eq!((block.start, block.end), (0, 6));
    }

    #[test]
    fn delimiter_boundaries_at_matches_tree_sitter_shape() {
        let scan = StructuralScanner::scan("[1, 2]");
        assert_eq!(scan.delimiter_boundaries_at(3), Some((0, 5)));
        assert_eq!(scan.delimiter_boundaries_at(10), None);
    }

    #[test]
    fn inner_and_outer_ranges_exclude_or_include_the_delimiters() {
        let scan = StructuralScanner::scan("{abc}");
        let m = scan.matches()[0];
        assert_eq!(m.inner_range(), 1..4);
        assert_eq!(m.outer_range(), 0..5);
    }

    #[test]
    fn scan_chunks_matches_scanning_the_concatenated_text() {
        let text = "{ \"a\\\"b\" ( [1] ) }";
        let whole = StructuralScanner::scan(text);

        // Split the same text into arbitrary, mid-token chunk boundaries
        // (including one right in the middle of the escape sequence) to
        // make sure state carries across chunks correctly.
        let mut chunked_text_pieces = Vec::new();
        let mut rest = text;
        while !rest.is_empty() {
            let mut boundary = rest.len().min(3);
            while !rest.is_char_boundary(boundary) {
                boundary -= 1;
            }
            let (piece, remainder) = rest.split_at(boundary);
            chunked_text_pieces.push(piece);
            rest = remainder;
        }
        let chunked = StructuralScanner::scan_chunks(chunked_text_pieces);

        assert_eq!(whole.matches(), chunked.matches());
        assert_eq!(whole.unmatched(), chunked.unmatched());
    }

    #[test]
    fn mismatched_closers_do_not_corrupt_later_matching() {
        // The `)` doesn't match the open `{`, so it's ignored, and the
        // brace still matches its real closer afterward.
        let scan = StructuralScanner::scan("{ ) }");
        assert_eq!(scan.matches().len(), 1);
        let m = scan.matches()[0];
        assert_eq!(m.kind, DelimiterKind::Brace);
        assert_eq!((m.start, m.end), (0, 4));
    }
}
