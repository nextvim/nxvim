use std::collections::BTreeMap;

const CHECKPOINT_INTERVAL: u32 = 64;
const MAX_CHECKPOINT_DISTANCE: u32 = CHECKPOINT_INTERVAL * 4;
const FALLBACK_PARSE_DISTANCE: u32 = 32;

use rope::Point;
use std::collections::HashMap;
use std::{path::Path, sync::OnceLock};
use syntect::{
    easy::ScopeRangeIterator,
    highlighting::{Theme, ThemeSet},
    parsing::{ParseState, Scope, ScopeStack, SyntaxSet},
};
use text::{BufferSnapshot, ToOffset, ToPoint};

/// A half-open UTF-8 byte-column range within one buffer row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HighlightSpan {
    pub start_column: u32,
    pub end_column: u32,
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

pub fn get_theme(dark: bool) -> &'static Theme {
    static DARK_THEME: OnceLock<Theme> = OnceLock::new();
    static LIGHT_THEME: OnceLock<Theme> = OnceLock::new();
    if dark {
        DARK_THEME.get_or_init(|| {
            let themes = ThemeSet::load_defaults().themes;
            themes
                .get("base16-ocean.dark")
                .cloned()
                .or_else(|| themes.into_values().next())
                .expect("syntect must provide a default highlight theme")
        })
    } else {
        LIGHT_THEME.get_or_init(|| {
            let themes = ThemeSet::load_defaults().themes;
            themes
                .get("base16-ocean.light")
                .cloned()
                .or_else(|| themes.into_values().next())
                .expect("syntect must provide a default highlight theme")
        })
    }
}

fn highlight_theme() -> &'static Theme {
    get_theme(true)
}

pub use syntect::highlighting::Highlighter;

pub fn load_colorscheme(colorscheme: &vim_colorscheme::ColorScheme) -> Highlighter<'static> {
    let is_dark = colorscheme.is_dark();
    Highlighter::new(get_theme(is_dark))
}

pub fn global_highlighter() -> &'static Highlighter<'static> {
    static HIGHLIGHTER: OnceLock<Highlighter<'static>> = OnceLock::new();
    HIGHLIGHTER.get_or_init(|| Highlighter::new(highlight_theme()))
}

pub fn parse_scopes_cancellable(
    snapshot: &BufferSnapshot,
    file_path: Option<&str>,
    start_row: u32,
    end_row: u32,
    resume_checkpoint: Option<ParseStateCheckpoint>,
    existing_checkpoints: &[ParseStateCheckpoint],
    highlighter: Option<&Highlighter>,
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
    let fallback_highlighter;
    let highlighter = match highlighter {
        Some(h) => h,
        None => {
            fallback_highlighter = Highlighter::new(highlight_theme());
            &fallback_highlighter
        }
    };

    // Style resolution (`style_for_stack`) walks the theme's scope selectors
    // and is far more expensive than a hash lookup. The same scope stack
    // recurs constantly within and across lines (e.g. every plain-text run,
    // every identifier of the same kind), so memoize it for the duration of
    // this parse. Lookups borrow `stack.as_slice()` directly and only
    // allocate on first sight of a given stack.
    let mut style_cache: HashMap<Vec<Scope>, [u8; 3]> = HashMap::new();

    // Reused across rows to avoid a fresh allocation for every line's text.
    let mut text = String::new();

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

        text.clear();
        for chunk in snapshot
            .as_rope()
            .chunks_in_range(line_start_offset..line_end_offset)
        {
            text.push_str(chunk);
        }
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
                let start_column = range.start.min(snapshot.line_len(row) as usize) as u32;
                let end_column = range.end.min(snapshot.line_len(row) as usize) as u32;
                if start_column == end_column {
                    continue;
                }
                let foreground = if let Some(cached) = style_cache.get(stack.as_slice()) {
                    *cached
                } else {
                    let scope_style = highlighter.style_for_stack(stack.as_slice());
                    let foreground = [
                        scope_style.foreground.r,
                        scope_style.foreground.g,
                        scope_style.foreground.b,
                    ];
                    style_cache.insert(stack.as_slice().to_vec(), foreground);
                    foreground
                };
                spans.push(HighlightSpan {
                    start_column,
                    end_column,
                    foreground,
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
///
/// `expand_before`/`expand_after` widen the requested range by an explicit
/// row count (used for idle speculative prefetch). Callers driving idle
/// prefetch should ramp these up gradually across repeated calls rather than
/// requesting a large margin all at once: every row outside the previously
/// cached range is parsed synchronously in this call, so a large one-shot
/// margin (e.g. 1000+ rows) can visibly stall the caller's thread.
pub fn highlight_run(
    state: &mut BufferHighlightState,
    snapshot: &BufferSnapshot,
    file_path: Option<&str>,
    row_start: u32,
    row_end: u32,
    expand_before: u32,
    expand_after: u32,
    highlighter: Option<&Highlighter>,
) {
    let row_start = row_start.saturating_sub(expand_before);
    let row_end = row_end.saturating_add(expand_after);

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
        highlighter,
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

        highlight_run(&mut state, &snapshot, Some("main.rs"), 2, 5, 0, 0, None);

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

        highlight_run(
            &mut state,
            &snapshot,
            Some("main.rs"),
            1100,
            1200,
            1000,
            500,
            None,
        );

        assert!(state.highlight_row(100).is_some());
        assert!(state.highlight_row(1100).is_some());
        assert!(state.highlight_row(1700).is_some());

        assert!(state.highlight_row(50).is_none());
        assert!(state.highlight_row(1800).is_none());
    }
}
