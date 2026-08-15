use std::collections::BTreeMap;

const CHECKPOINT_INTERVAL: u32 = 64;
const MAX_CHECKPOINT_DISTANCE: u32 = CHECKPOINT_INTERVAL * 4;
const FALLBACK_PARSE_DISTANCE: u32 = 32;
const IDLE_EXPAND_START: u32 = 1000;
const IDLE_EXPAND_END: u32 = 500;

use rope::Point;
use std::{path::Path, sync::OnceLock};
use syntect::{
    easy::ScopeRangeIterator,
    highlighting::{Highlighter, Theme, ThemeSet},
    parsing::{ParseState, ScopeStack, SyntaxSet},
};
use text::{BufferSnapshot, ToOffset, ToPoint};

/// A half-open UTF-8 byte-column range within one buffer row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HighlightSpan {
    pub start_column: u32,
    pub end_column: u32,
    pub scopes: Vec<String>,
    pub foreground: [u8; 3],
}

/// Highlighting for one covered buffer row. An empty `spans` vector still
/// records that the row was parsed and needs no styled ranges.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HighlightedRow {
    pub row: u32,
    pub spans: Vec<HighlightSpan>,
}

#[derive(Clone)]
pub struct ParseStateCheckpoint {
    pub row: u32,
    pub parse_state: ParseState,
    pub scope_stack: ScopeStack,
}

unsafe impl Send for ParseStateCheckpoint {}
unsafe impl Sync for ParseStateCheckpoint {}

impl std::fmt::Debug for ParseStateCheckpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ParseStateCheckpoint")
            .field("row", &self.row)
            .finish()
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

pub fn parse_scopes_cancellable(
    snapshot: &BufferSnapshot,
    file_path: Option<&str>,
    start_row: u32,
    end_row: u32,
    resume_checkpoint: Option<ParseStateCheckpoint>,
    existing_checkpoints: &[ParseStateCheckpoint],
    mut is_cancelled: impl FnMut() -> bool,
) -> Option<(Vec<HighlightedRow>, Vec<ParseStateCheckpoint>)> {
    let syntax_set = syntax_set();
    let syntax = file_path
        .and_then(|path| Path::new(path).extension())
        .and_then(|extension| extension.to_str())
        .and_then(|extension| syntax_set.find_syntax_by_extension(extension))
        .unwrap_or_else(|| syntax_set.find_syntax_plain_text());

    let (mut parser, mut stack) = if let Some(cp) = resume_checkpoint.as_ref() {
        (cp.parse_state.clone(), cp.scope_stack.clone())
    } else {
        (ParseState::new(syntax), ScopeStack::new())
    };

    let start_row_iter = if let Some(cp) = resume_checkpoint.as_ref() {
        cp.row
    } else {
        start_row.saturating_sub(FALLBACK_PARSE_DISTANCE)
    };
    let end_row_iter = end_row.min(snapshot.row_count());

    let mut rows = Vec::new();
    let mut checkpoints = Vec::new();
    let highlighter = Highlighter::new(highlight_theme());

    for row in start_row_iter..=end_row_iter {
        if is_cancelled() {
            return None;
        }

        let line_start_offset = Point::new(row, 0).to_offset(snapshot);
        let line_end_offset = Point::new(row, snapshot.line_len(row)).to_offset(snapshot);

        // Periodically save checkpoints (every 64 lines)
        if row > 0 && row % CHECKPOINT_INTERVAL == 0 && row >= start_row {
            checkpoints.push(ParseStateCheckpoint {
                row,
                parse_state: parser.clone(),
                scope_stack: stack.clone(),
            });
        }

        // Check for state convergence if we parsed beyond the target end range
        if row >= end_row {
            if let Some(existing_cp) = existing_checkpoints.iter().find(|cp| cp.row == row) {
                // Since ParseState might not implement PartialEq, we compare ScopeStack as convergence metric
                if existing_cp.scope_stack == stack {
                    break;
                }
            }
        }

        let mut text: String = snapshot
            .as_rope()
            .chunks_in_range(line_start_offset..line_end_offset)
            .collect();
        text.push('\n');
        let parsed = parser.parse_line(&text, &syntax_set).ok()?;

        if row >= start_row {
            let mut spans = Vec::new();
            for (range, operation) in ScopeRangeIterator::new(&parsed.ops, &text) {
                if stack.apply(&operation).is_err() {
                    return None;
                }
                if range.start == range.end {
                    continue;
                }
                let scope_style = highlighter.style_for_stack(stack.as_slice());
                let start_column = range.start.min(snapshot.line_len(row) as usize) as u32;
                let end_column = range.end.min(snapshot.line_len(row) as usize) as u32;
                if start_column == end_column {
                    continue;
                }
                spans.push(HighlightSpan {
                    start_column,
                    end_column,
                    scopes: stack.as_slice().iter().map(ToString::to_string).collect(),
                    foreground: [
                        scope_style.foreground.r,
                        scope_style.foreground.g,
                        scope_style.foreground.b,
                    ],
                });
            }

            rows.push(HighlightedRow { row, spans });
        } else {
            for (_, operation) in ScopeRangeIterator::new(&parsed.ops, &text) {
                if stack.apply(&operation).is_err() {
                    return None;
                }
            }
        }
    }

    Some((rows, checkpoints))
}

/// Per-buffer highlighting cache and incremental-parse bookkeeping. Owned by
/// the buffer's own state (see `BufferState` in the `nxvim` binary crate) so it
/// lives and dies with the buffer instead of in a separately-keyed service map.
pub struct BufferHighlightState {
    pub checkpoints: BTreeMap<u32, ParseStateCheckpoint>,
    pub rows: BTreeMap<u32, Vec<HighlightSpan>>,
    pub published_snapshot: Option<BufferSnapshot>,
}

impl BufferHighlightState {
    pub fn new() -> Self {
        Self {
            checkpoints: BTreeMap::new(),
            rows: BTreeMap::new(),
            published_snapshot: None,
        }
    }

    pub fn highlight_row(&self, row: u32) -> Option<&[HighlightSpan]> {
        self.rows.get(&row).map(|spans| spans.as_slice())
    }

    fn nearest_checkpoint(&self, target_row: u32) -> Option<ParseStateCheckpoint> {
        let mut row = target_row - (target_row % CHECKPOINT_INTERVAL);
        loop {
            if target_row - row > MAX_CHECKPOINT_DISTANCE {
                break;
            }
            if let Some(cp) = self.checkpoints.get(&row) {
                return Some(cp.clone());
            }
            if row < CHECKPOINT_INTERVAL {
                break;
            }
            row -= CHECKPOINT_INTERVAL;
        }
        None
    }
}

impl Default for BufferHighlightState {
    fn default() -> Self {
        Self::new()
    }
}

/// Incrementally (re)parses `state` so it covers `row_start..=row_end`, reusing
/// checkpoints and cached rows where possible. Call with the highlight state
/// that belongs to the buffer being highlighted (e.g. `BufferState.highlights`).
pub fn highlight_run(
    state: &mut BufferHighlightState,
    snapshot: &BufferSnapshot,
    file_path: Option<&str>,
    mut row_start: u32,
    mut row_end: u32,
    expanded: bool,
) {
    if expanded {
        row_start = row_start.saturating_sub(IDLE_EXPAND_START);
        row_end = row_end.saturating_add(IDLE_EXPAND_END);
    }

    let mut lowest_affected_row: Option<u32> = None;
    if let Some(previous) = state.published_snapshot.as_ref() {
        if previous.version != snapshot.version {
            for edit in snapshot.edits_since::<Point>(&previous.version) {
                let edit_row = edit.new.start.row;
                lowest_affected_row =
                    Some(lowest_affected_row.map_or(edit_row, |r| r.min(edit_row)));
            }
        }
    }

    if let Some(lowest) = lowest_affected_row {
        state.rows.split_off(&lowest);
        state.checkpoints.split_off(&lowest);
    }

    if (row_start..=row_end).all(|row| state.rows.contains_key(&row)) {
        state.published_snapshot = Some(snapshot.clone());
        return;
    }

    let checkpoint = state.nearest_checkpoint(row_start);
    let existing_checkpoints: Vec<ParseStateCheckpoint> =
        state.checkpoints.values().cloned().collect();

    if let Some((rows, checkpoints)) = parse_scopes_cancellable(
        snapshot,
        file_path,
        row_start,
        row_end,
        checkpoint,
        &existing_checkpoints,
        || false,
    ) {
        state
            .rows
            .retain(|row, _| *row < row_start || *row > row_end);

        state
            .rows
            .extend(rows.into_iter().map(|row| (row.row, row.spans)));
        state.published_snapshot = Some(snapshot.clone());
        state
            .checkpoints
            .extend(checkpoints.into_iter().map(|cp| (cp.row, cp)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clock::ReplicaId;
    use vim_buffer::{Buffer, BufferId};

    #[test]
    fn test_highlight_run_non_expanded() {
        let mut state = BufferHighlightState::new();
        let text = "fn main() {\n    println!(\"hello\");\n}\n".repeat(20);
        let buffer = Buffer::new(BufferId::new(1).unwrap(), ReplicaId::LOCAL, text);
        let snapshot = buffer.snapshot().as_inner().clone();

        highlight_run(&mut state, &snapshot, Some("main.rs"), 2, 5, false);

        for r in 2..=5 {
            assert!(state.highlight_row(r).is_some());
        }

        assert!(state.highlight_row(0).is_none());
        assert!(state.highlight_row(10).is_none());
    }

    #[test]
    fn test_highlight_run_expanded() {
        let mut state = BufferHighlightState::new();
        let text = "let x = 42;\n".repeat(2000);
        let buffer = Buffer::new(BufferId::new(1).unwrap(), ReplicaId::LOCAL, text);
        let snapshot = buffer.snapshot().as_inner().clone();

        highlight_run(&mut state, &snapshot, Some("main.rs"), 1100, 1200, true);

        assert!(state.highlight_row(100).is_some());
        assert!(state.highlight_row(1100).is_some());
        assert!(state.highlight_row(1700).is_some());

        assert!(state.highlight_row(50).is_none());
        assert!(state.highlight_row(1800).is_none());
    }
}
