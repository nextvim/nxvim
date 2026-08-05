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

/// Background-computed highlighting data for one buffer version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HighlightSnapshot {
    pub changedtick: u64,
    pub rows: Vec<Vec<HighlightSpan>>,
}

impl HighlightSnapshot {
    pub fn spans_for_row(&self, row: u32) -> Option<&[HighlightSpan]> {
        self.rows.get(row as usize).map(Vec::as_slice)
    }
}

fn highlight_theme() -> &'static Theme {
    static THEME: OnceLock<Theme> = OnceLock::new();
    THEME.get_or_init(|| {
        ThemeSet::load_defaults()
            .themes
            .get("base16-ocean.dark")
            .cloned()
            .or_else(|| ThemeSet::load_defaults().themes.into_values().next())
            .expect("syntect must provide a default highlight theme")
    })
}

/// Parses TextMate scopes for the whole snapshot. Scope parsing is stateful across
/// lines, so this intentionally starts at row zero and is run off the UI thread.
pub fn parse_scopes_cancellable(
    snapshot: &BufferSnapshot,
    changedtick: u64,
    file_path: Option<&str>,
    mut is_cancelled: impl FnMut() -> bool,
) -> Option<HighlightSnapshot> {
    let syntax_set = SyntaxSet::load_defaults_newlines();
    let syntax = file_path
        .and_then(|path| Path::new(path).extension())
        .and_then(|extension| extension.to_str())
        .and_then(|extension| syntax_set.find_syntax_by_extension(extension))
        .unwrap_or_else(|| syntax_set.find_syntax_plain_text());
    let mut parser = ParseState::new(syntax);
    let mut stack = ScopeStack::new();
    let highlighter = Highlighter::new(highlight_theme());
    let mut rows = Vec::with_capacity(snapshot.row_count() as usize);

    for row in 0..snapshot.row_count() {
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

    Some(HighlightSnapshot { changedtick, rows })
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
            parse_scopes_cancellable(buffer.snapshot(), 1, Some("test.rs"), || false).unwrap();

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
    fn cancellation_discards_partial_snapshot() {
        let buffer = Buffer::new(
            ReplicaId::LOCAL,
            BufferId::new(1).unwrap(),
            "one\ntwo".to_owned(),
        );
        assert!(parse_scopes_cancellable(buffer.snapshot(), 0, None, || true).is_none());
    }
}
