use std::{path::Path, sync::OnceLock};

use rope::Point;
use syntect::{
    easy::ScopeRangeIterator,
    highlighting::{Highlighter, Theme, ThemeSet},
    parsing::{ParseState, ScopeStack, SyntaxSet},
};
use text::{BufferSnapshot, ToOffset};

/// A contiguous byte-column range with the TextMate scopes active for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HighlightSpan {
    pub start: u32,
    pub end: u32,
    pub scopes: Vec<String>,
    pub foreground: [u8; 3],
}

/// Background-computed highlighting data for one buffer version and row interval.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HighlightSnapshot {
    pub changedtick: u64,
    pub start_row: u32,
    pub rows: Vec<Vec<HighlightSpan>>,
}

impl HighlightSnapshot {
    pub fn end_row(&self) -> u32 {
        self.start_row.saturating_add(self.rows.len() as u32)
    }

    pub fn contains_rows(&self, start_row: u32, end_row: u32) -> bool {
        self.start_row <= start_row && self.end_row() >= end_row
    }

    pub fn spans_for_row(&self, row: u32) -> Option<&[HighlightSpan]> {
        let index = row.checked_sub(self.start_row)?;
        self.rows.get(index as usize).map(Vec::as_slice)
    }
}

fn syntax_set() -> &'static SyntaxSet {
    static SYNTAX_SET: OnceLock<SyntaxSet> = OnceLock::new();
    SYNTAX_SET.get_or_init(SyntaxSet::load_defaults_newlines)
}

fn highlight_theme() -> &'static Theme {
    static THEME: OnceLock<Theme> = OnceLock::new();
    THEME.get_or_init(|| {
        let themes = ThemeSet::load_defaults().themes;
        themes
            .get("base16-ocean.dark")
            .cloned()
            .or_else(|| themes.into_values().next())
            .expect("syntect must provide a default highlight theme")
    })
}

/// Parses TextMate scopes for `[start_row, end_row)`. Callers include a lookbehind
/// window in `start_row` so multiline state can settle before visible rows.
pub fn parse_scopes_cancellable(
    snapshot: &BufferSnapshot,
    changedtick: u64,
    file_path: Option<&str>,
    start_row: u32,
    end_row: u32,
    mut is_cancelled: impl FnMut() -> bool,
) -> Option<HighlightSnapshot> {
    let syntax_set = syntax_set();
    let syntax = file_path
        .and_then(|path| Path::new(path).extension())
        .and_then(|extension| extension.to_str())
        .and_then(|extension| syntax_set.find_syntax_by_extension(extension))
        .unwrap_or_else(|| syntax_set.find_syntax_plain_text());
    let start_row = start_row.min(snapshot.row_count());
    let end_row = end_row.max(start_row).min(snapshot.row_count());
    let mut parser = ParseState::new(syntax);
    let mut stack = ScopeStack::new();
    let highlighter = Highlighter::new(highlight_theme());
    let mut rows = Vec::with_capacity(end_row.saturating_sub(start_row) as usize);

    for row in start_row..end_row {
        if is_cancelled() {
            return None;
        }

        let start = Point::new(row, 0).to_offset(snapshot);
        let end = Point::new(row, snapshot.line_len(row)).to_offset(snapshot);
        let mut text: String = snapshot.as_rope().chunks_in_range(start..end).collect();
        text.push('\n');
        let parsed = parser.parse_line(&text, &syntax_set).ok()?;
        let mut spans = Vec::new();

        for (range, operation) in ScopeRangeIterator::new(&parsed.ops, &text) {
            if stack.apply(&operation).is_err() {
                return None;
            }
            if range.start == range.end {
                continue;
            }
            let scope_style = highlighter.style_for_stack(stack.as_slice());
            spans.push(HighlightSpan {
                start: u32::try_from(range.start).ok()?,
                end: u32::try_from(range.end).ok()?,
                scopes: stack.as_slice().iter().map(ToString::to_string).collect(),
                foreground: [
                    scope_style.foreground.r,
                    scope_style.foreground.g,
                    scope_style.foreground.b,
                ],
            });
        }
        rows.push(spans);
    }

    Some(HighlightSnapshot {
        changedtick,
        start_row,
        rows,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use clock::ReplicaId;
    use text::{Buffer, BufferId};

    #[test]
    fn preserves_multiline_parse_state() {
        let buffer = Buffer::new(
            ReplicaId::LOCAL,
            BufferId::new(1).unwrap(),
            "fn main() {\n/* comment\nstill comment */\n}".to_owned(),
        );
        let highlights =
            parse_scopes_cancellable(buffer.snapshot(), 1, Some("test.rs"), 0, 4, || false)
                .unwrap();

        assert_eq!(highlights.rows.len(), 4);
        assert!(
            highlights
                .spans_for_row(2)
                .unwrap()
                .iter()
                .any(|span| { span.scopes.iter().any(|scope| scope.contains("comment")) })
        );
    }

    #[test]
    fn stops_at_requested_row() {
        let buffer = Buffer::new(
            ReplicaId::LOCAL,
            BufferId::new(1).unwrap(),
            "one\ntwo\nthree".to_owned(),
        );
        let highlights =
            parse_scopes_cancellable(buffer.snapshot(), 0, None, 1, 2, || false).unwrap();

        assert_eq!(highlights.start_row, 1);
        assert_eq!(highlights.rows.len(), 1);
    }

    #[test]
    fn cancellation_discards_partial_snapshot() {
        let buffer = Buffer::new(
            ReplicaId::LOCAL,
            BufferId::new(1).unwrap(),
            "one\ntwo".to_owned(),
        );
        assert!(parse_scopes_cancellable(buffer.snapshot(), 0, None, 0, 2, || true).is_none());
    }
}
