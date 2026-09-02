use std::collections::{BTreeMap, BTreeSet};

const CHECKPOINT_INTERVAL: u32 = 64;
const MAX_LOOKBACK: u32 = 128;

use rope::Point;
use std::collections::HashMap;
use std::{path::Path, sync::OnceLock};
use syntect::{
    easy::ScopeRangeIterator,
    highlighting::{Theme, ThemeSet},
    parsing::{ParseState, Scope, ScopeStack, SyntaxSet},
};
use text::{BufferSnapshot, ToOffset};

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

pub const SCOPE_MAPPINGS: &[(&str, &str)] = &[
    ("comment", "comment"),
    ("string", "string"),
    ("constant", "constant"),
    ("keyword", "keyword"),
    ("storage", "keyword"),
    ("entity.name.function", "function"),
    ("variable", "variable"),
    ("support.type", "type"),
];

pub fn map_scope_to_style(
    scopes: &[syntect::parsing::Scope],
    colorscheme: &vim_colorscheme::ColorScheme,
) -> vim_colorscheme::Style {
    let mut resolved_style = vim_colorscheme::Style {
        fg: None,
        bg: colorscheme.background,
        bold: false,
        italic: false,
        underline: false,
        strikethrough: false,
    };

    'outer: for scope in scopes.iter().rev() {
        let scope_str = scope.to_string();

        if let Some(last_scope) = scope_str.split('.').last() {
            for &(pattern, key) in SCOPE_MAPPINGS {
                if last_scope.contains(pattern) {
                    if let Some(style) = colorscheme.get_style(key) {
                        resolved_style = style.clone();
                        break 'outer;
                    }
                }
            }
        }

        for &(pattern, key) in SCOPE_MAPPINGS {
            if scope_str.contains(pattern) {
                if let Some(style) = colorscheme.get_style(key) {
                    resolved_style = style.clone();
                    break 'outer;
                }
            }
        }
    }

    resolved_style
}

fn color_to_rgb_array(color: vim_colorscheme::Color) -> [u8; 3] {
    match color {
        vim_colorscheme::Color::Rgb(r, g, b) => [r, g, b],
        vim_colorscheme::Color::Black => [0, 0, 0],
        vim_colorscheme::Color::White => [255, 255, 255],
        vim_colorscheme::Color::Grey => [128, 128, 128],
        vim_colorscheme::Color::DarkGrey => [64, 64, 64],
        vim_colorscheme::Color::Red => [255, 0, 0],
        vim_colorscheme::Color::Green => [0, 255, 0],
        vim_colorscheme::Color::Blue => [0, 0, 255],
        vim_colorscheme::Color::Yellow => [255, 255, 0],
        vim_colorscheme::Color::Magenta => [255, 0, 255],
        vim_colorscheme::Color::Cyan => [0, 255, 255],
        vim_colorscheme::Color::Reset => [255, 255, 255],
    }
}

pub fn load_colorscheme(colorscheme: &vim_colorscheme::ColorScheme) -> Highlighter<'static> {
    let is_dark = colorscheme.is_dark();
    let mut theme = get_theme(is_dark).clone();

    let mut colors = Vec::new();
    let mut add_color = |c: vim_colorscheme::Color| match c {
        vim_colorscheme::Color::Rgb(r, g, b) => {
            colors.push(syntect::highlighting::Color { r, g, b, a: 255 });
        }
        vim_colorscheme::Color::Black => {
            colors.push(syntect::highlighting::Color {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            });
        }
        vim_colorscheme::Color::White => {
            colors.push(syntect::highlighting::Color {
                r: 255,
                g: 255,
                b: 255,
                a: 255,
            });
        }
        vim_colorscheme::Color::Grey => {
            colors.push(syntect::highlighting::Color {
                r: 128,
                g: 128,
                b: 128,
                a: 255,
            });
        }
        vim_colorscheme::Color::DarkGrey => {
            colors.push(syntect::highlighting::Color {
                r: 64,
                g: 64,
                b: 64,
                a: 255,
            });
        }
        vim_colorscheme::Color::Red => {
            colors.push(syntect::highlighting::Color {
                r: 255,
                g: 0,
                b: 0,
                a: 255,
            });
        }
        vim_colorscheme::Color::Green => {
            colors.push(syntect::highlighting::Color {
                r: 0,
                g: 255,
                b: 0,
                a: 255,
            });
        }
        vim_colorscheme::Color::Blue => {
            colors.push(syntect::highlighting::Color {
                r: 0,
                g: 0,
                b: 255,
                a: 255,
            });
        }
        vim_colorscheme::Color::Yellow => {
            colors.push(syntect::highlighting::Color {
                r: 255,
                g: 255,
                b: 0,
                a: 255,
            });
        }
        vim_colorscheme::Color::Magenta => {
            colors.push(syntect::highlighting::Color {
                r: 255,
                g: 0,
                b: 255,
                a: 255,
            });
        }
        vim_colorscheme::Color::Cyan => {
            colors.push(syntect::highlighting::Color {
                r: 0,
                g: 255,
                b: 255,
                a: 255,
            });
        }
        vim_colorscheme::Color::Reset => {}
    };

    if let Some(c) = colorscheme.foreground {
        add_color(c);
    }
    if let Some(c) = colorscheme.background {
        add_color(c);
    }
    if let Some(c) = colorscheme.cursor {
        add_color(c);
    }
    if let Some(c) = colorscheme.selection {
        add_color(c);
    }

    for style in colorscheme.styles.values() {
        if let Some(c) = style.fg {
            add_color(c);
        }
        if let Some(c) = style.bg {
            add_color(c);
        }
    }

    let find_nearest = |c: syntect::highlighting::Color| -> syntect::highlighting::Color {
        if colors.is_empty() {
            return c;
        }
        let mut best_color = colors[0];
        let mut min_distance = f32::MAX;
        for &candidate in &colors {
            let dr = c.r as f32 - candidate.r as f32;
            let dg = c.g as f32 - candidate.g as f32;
            let db = c.b as f32 - candidate.b as f32;
            let distance = dr * dr + dg * dg + db * db;
            if distance < min_distance {
                min_distance = distance;
                best_color = candidate;
            }
        }
        best_color
    };

    let update_color = |opt_color: &mut Option<syntect::highlighting::Color>| {
        if let Some(c) = opt_color {
            *c = find_nearest(*c);
        }
    };

    let to_syntect_color = |c: vim_colorscheme::Color| -> Option<syntect::highlighting::Color> {
        match c {
            vim_colorscheme::Color::Rgb(r, g, b) => {
                Some(syntect::highlighting::Color { r, g, b, a: 255 })
            }
            vim_colorscheme::Color::Black => Some(syntect::highlighting::Color {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            }),
            vim_colorscheme::Color::White => Some(syntect::highlighting::Color {
                r: 255,
                g: 255,
                b: 255,
                a: 255,
            }),
            vim_colorscheme::Color::Grey => Some(syntect::highlighting::Color {
                r: 128,
                g: 128,
                b: 128,
                a: 255,
            }),
            vim_colorscheme::Color::DarkGrey => Some(syntect::highlighting::Color {
                r: 64,
                g: 64,
                b: 64,
                a: 255,
            }),
            vim_colorscheme::Color::Red => Some(syntect::highlighting::Color {
                r: 255,
                g: 0,
                b: 0,
                a: 255,
            }),
            vim_colorscheme::Color::Green => Some(syntect::highlighting::Color {
                r: 0,
                g: 255,
                b: 0,
                a: 255,
            }),
            vim_colorscheme::Color::Blue => Some(syntect::highlighting::Color {
                r: 0,
                g: 0,
                b: 255,
                a: 255,
            }),
            vim_colorscheme::Color::Yellow => Some(syntect::highlighting::Color {
                r: 255,
                g: 255,
                b: 0,
                a: 255,
            }),
            vim_colorscheme::Color::Magenta => Some(syntect::highlighting::Color {
                r: 255,
                g: 0,
                b: 255,
                a: 255,
            }),
            vim_colorscheme::Color::Cyan => Some(syntect::highlighting::Color {
                r: 0,
                g: 255,
                b: 255,
                a: 255,
            }),
            vim_colorscheme::Color::Reset => None,
        }
    };

    if let Some(c) = colorscheme.foreground.and_then(to_syntect_color) {
        theme.settings.foreground = Some(c);
    } else {
        update_color(&mut theme.settings.foreground);
    }

    if let Some(c) = colorscheme.background.and_then(to_syntect_color) {
        theme.settings.background = Some(c);
    } else {
        update_color(&mut theme.settings.background);
    }

    if let Some(c) = colorscheme.cursor.and_then(to_syntect_color) {
        theme.settings.caret = Some(c);
    } else {
        update_color(&mut theme.settings.caret);
    }

    if let Some(c) = colorscheme.selection.and_then(to_syntect_color) {
        theme.settings.selection = Some(c);
    } else {
        update_color(&mut theme.settings.selection);
    }

    update_color(&mut theme.settings.line_highlight);
    update_color(&mut theme.settings.selection_border);
    update_color(&mut theme.settings.inactive_selection);
    update_color(&mut theme.settings.find_highlight);
    update_color(&mut theme.settings.find_highlight_foreground);
    update_color(&mut theme.settings.guide);
    update_color(&mut theme.settings.active_guide);
    update_color(&mut theme.settings.stack_guide);
    update_color(&mut theme.settings.gutter);
    update_color(&mut theme.settings.gutter_foreground);
    update_color(&mut theme.settings.shadow);
    update_color(&mut theme.settings.accent);

    for scope in &mut theme.scopes {
        update_color(&mut scope.style.foreground);
        update_color(&mut scope.style.background);
    }

    let static_theme: &'static Theme = Box::leak(Box::new(theme));
    Highlighter::new(static_theme)
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
    visible_range: Option<std::ops::RangeInclusive<u32>>,
    resume_checkpoint: Option<ParseStateCheckpoint>,
    existing_checkpoints: &[ParseStateCheckpoint],
    highlighter: Option<&Highlighter>,
    colorscheme: &vim_colorscheme::ColorScheme,
    mut is_cancelled: impl FnMut() -> bool,
) -> Option<(
    Vec<HighlightedRow>,
    Vec<ParseStateCheckpoint>,
    BTreeSet<u32>,
)> {
    let map_differently = true;
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
        start_row.saturating_sub(MAX_LOOKBACK)
    };
    let convergence_row = existing_checkpoints
        .iter()
        .filter(|checkpoint| checkpoint.row >= end_row)
        .map(|checkpoint| checkpoint.row)
        .min()
        .unwrap_or(end_row);
    let end_row_iter = convergence_row.min(snapshot.row_count());

    let mut rows = Vec::new();
    let mut checkpoints = Vec::new();
    let mut unresolved_rows = BTreeSet::new();
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
        if row > 0 && row % CHECKPOINT_INTERVAL == 0 {
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
            let is_visible = visible_range.as_ref().map_or(true, |r| r.contains(&row));
            if is_visible {
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
                        let mut foreground_color = None;
                        if map_differently {
                            let style = map_scope_to_style(stack.as_slice(), colorscheme);
                            if let Some(col) = style.fg {
                                foreground_color = Some(color_to_rgb_array(col));
                            }
                        }
                        let foreground = if let Some(fg) = foreground_color {
                            fg
                        } else {
                            let scope_style = highlighter.style_for_stack(stack.as_slice());
                            [
                                scope_style.foreground.r,
                                scope_style.foreground.g,
                                scope_style.foreground.b,
                            ]
                        };
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
                rows.push(HighlightedRow {
                    row,
                    spans: Vec::new(),
                });
                unresolved_rows.insert(row);
            }
        } else {
            for (_, operation) in ScopeRangeIterator::new(&parsed.ops, &text) {
                if stack.apply(&operation).is_err() {
                    return None;
                }
            }
        }
    }

    Some((rows, checkpoints, unresolved_rows))
}

pub struct BufferHighlightState {
    pub checkpoints: BTreeMap<u32, ParseStateCheckpoint>,
    pub rows: BTreeMap<u32, Vec<HighlightSpan>>,
    pub published_snapshot: Option<BufferSnapshot>,
    pub unresolved_rows: BTreeSet<u32>,
}

impl BufferHighlightState {
    pub fn new() -> Self {
        Self {
            checkpoints: BTreeMap::new(),
            rows: BTreeMap::new(),
            published_snapshot: None,
            unresolved_rows: BTreeSet::new(),
        }
    }

    pub fn highlight_row(&self, row: u32) -> Option<&[HighlightSpan]> {
        if self.unresolved_rows.contains(&row) {
            None
        } else {
            self.rows.get(&row).map(|spans| spans.as_slice())
        }
    }

    pub fn invalidate(&mut self) {
        self.checkpoints.clear();
        self.rows.clear();
        self.published_snapshot = None;
        self.unresolved_rows.clear();
    }

    fn nearest_checkpoint(&self, target_row: u32) -> Option<ParseStateCheckpoint> {
        if let Some((&row, cp)) = self.checkpoints.range(..=target_row).next_back() {
            if target_row - row <= MAX_LOOKBACK {
                return Some(cp.clone());
            }
        }
        None
    }

    pub fn scope_path_at_position(
        &self,
        snapshot: &BufferSnapshot,
        file_path: Option<&str>,
        row: u32,
        column: u32,
    ) -> Vec<String> {
        let syntax_set = syntax_set();
        let syntax = file_path
            .and_then(|path| Path::new(path).extension())
            .and_then(|extension| extension.to_str())
            .and_then(|extension| syntax_set.find_syntax_by_extension(extension))
            .unwrap_or_else(|| syntax_set.find_syntax_plain_text());

        let checkpoint = self.nearest_checkpoint(row);
        let (mut parser, mut stack) = if let Some(cp) = checkpoint.as_ref() {
            (cp.parse_state.clone(), cp.scope_stack.clone())
        } else {
            (ParseState::new(syntax), ScopeStack::new())
        };

        let start_row_iter = if let Some(cp) = checkpoint.as_ref() {
            cp.row
        } else {
            row.saturating_sub(MAX_LOOKBACK)
        };

        let mut text = String::new();
        for r in start_row_iter..=row {
            let line_start_offset = Point::new(r, 0).to_offset(snapshot);
            let line_end_offset = Point::new(r, snapshot.line_len(r)).to_offset(snapshot);

            text.clear();
            for chunk in snapshot
                .as_rope()
                .chunks_in_range(line_start_offset..line_end_offset)
            {
                text.push_str(chunk);
            }
            text.push('\n');
            let Ok(parsed) = parser.parse_line(&text, &syntax_set) else {
                return Vec::new();
            };

            if r == row {
                for (range, operation) in ScopeRangeIterator::new(&parsed.ops, &text) {
                    if stack.apply(&operation).is_err() {
                        return Vec::new();
                    }
                    if column as usize >= range.start && (column as usize) < range.end {
                        return stack.as_slice().iter().map(|s| s.to_string()).collect();
                    }
                }
                return stack.as_slice().iter().map(|s| s.to_string()).collect();
            } else {
                for (_, operation) in ScopeRangeIterator::new(&parsed.ops, &text) {
                    if stack.apply(&operation).is_err() {
                        return Vec::new();
                    }
                }
            }
        }
        Vec::new()
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
    colorscheme: &vim_colorscheme::ColorScheme,
) {
    let visible_start = row_start;
    let visible_end = row_end;
    let row_start = row_start.saturating_sub(expand_before);
    let row_end = row_end.saturating_add(expand_after);

    // Keep the previous checkpoints as convergence sentinels. Checkpoints at
    // and after an edit are never valid resume points, but their scope stacks
    // still tell reparsing when it has reached the old stable parser state.
    let existing_checkpoints: Vec<ParseStateCheckpoint> =
        state.checkpoints.values().cloned().collect();
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
        state.unresolved_rows.split_off(&lowest);
    }

    let visible_all_resolved = (visible_start..=visible_end)
        .all(|row| state.rows.contains_key(&row) && !state.unresolved_rows.contains(&row));
    let expanded_all_parsed = (row_start..=row_end).all(|row| state.rows.contains_key(&row));

    if visible_all_resolved && expanded_all_parsed {
        state.published_snapshot = Some(snapshot.clone());
        return;
    }

    let checkpoint = state.nearest_checkpoint(row_start);

    if let Some((rows, checkpoints, unresolved)) = parse_scopes_cancellable(
        snapshot,
        file_path,
        row_start,
        row_end,
        Some(visible_start..=visible_end),
        checkpoint,
        &existing_checkpoints,
        highlighter,
        colorscheme,
        || false,
    ) {
        state
            .rows
            .retain(|row, _| *row < row_start || *row > row_end);
        state
            .unresolved_rows
            .retain(|row| *row < row_start || *row > row_end);

        state
            .rows
            .extend(rows.into_iter().map(|row| (row.row, row.spans)));
        state.unresolved_rows.extend(unresolved);
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
    use vim_buffer::{Buffer, BufferId, ByteOffset, Edit, EditOrigin, PlannedEdit};

    #[test]
    fn test_map_scope_to_style_last_segment() {
        let mut colorscheme = vim_colorscheme::ColorScheme::new(vim_colorscheme::Metadata {
            name: "test".to_string(),
            r#type: Some("dark".to_string()),
            author: None,
            description: None,
            github: None,
        });

        let mut keyword_style = vim_colorscheme::Style::default();
        keyword_style.bold = true;
        colorscheme.insert_style("keyword", keyword_style);

        let mut comment_style = vim_colorscheme::Style::default();
        comment_style.italic = true;
        colorscheme.insert_style("comment", comment_style);

        // A scope string where the last part is 'keyword' but 'comment' is in the full scope string
        let scope = syntect::parsing::Scope::new("comment.line.keyword").unwrap();
        let style = map_scope_to_style(&[scope], &colorscheme);

        // Since 'keyword' is the last segment, it should match first, so bold should be true
        assert!(style.bold);
        assert!(!style.italic);
    }

    #[test]
    fn test_highlight_run_non_expanded() {
        let mut state = BufferHighlightState::new();
        let text = "fn main() {\n    println!(\"hello\");\n}\n".repeat(20);
        let buffer = Buffer::new(BufferId::new(1).unwrap(), ReplicaId::LOCAL, text);
        let snapshot = buffer.snapshot().as_inner().clone();
        let colorscheme = vim_colorscheme::ColorScheme::load_default();

        highlight_run(
            &mut state,
            &snapshot,
            Some("main.rs"),
            2,
            5,
            0,
            0,
            None,
            &colorscheme,
        );

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
        let colorscheme = vim_colorscheme::ColorScheme::load_default();

        highlight_run(
            &mut state,
            &snapshot,
            Some("main.rs"),
            1100,
            1200,
            1000,
            500,
            None,
            &colorscheme,
        );

        assert!(state.highlight_row(100).is_none());
        assert!(state.highlight_row(1100).is_some());
        assert!(state.highlight_row(1700).is_none());

        assert!(state.rows.contains_key(&100));
        assert!(state.rows.contains_key(&1700));
        assert!(state.highlight_row(50).is_none());
        assert!(state.highlight_row(1800).is_none());
    }

    #[test]
    fn edit_invalidates_rows_and_checkpoints_from_earliest_affected_row() {
        let mut state = BufferHighlightState::new();
        let text = "fn first() {}\nfn second() {}\nfn third() {}\nfn fourth() {}\n";
        let mut buffer = Buffer::new(BufferId::new(1).unwrap(), ReplicaId::LOCAL, text);
        let colorscheme = vim_colorscheme::ColorScheme::load_default();
        let before = buffer.snapshot().as_inner().clone();

        highlight_run(
            &mut state,
            &before,
            Some("main.rs"),
            0,
            3,
            0,
            0,
            None,
            &colorscheme,
        );
        assert!(state.highlight_row(3).is_some());

        let edit_offset = text.find("fn third").unwrap();
        let mut transaction = buffer.transaction(EditOrigin::User);
        transaction.push(PlannedEdit {
            selection: None,
            edit: Edit::insert(ByteOffset(edit_offset), "// changed\n"),
        });
        transaction.commit(None).unwrap();
        let after = buffer.snapshot().as_inner().clone();

        // Requesting an already-cached earlier row still processes snapshot
        // invalidation before taking the cache-hit fast path.
        highlight_run(
            &mut state,
            &after,
            Some("main.rs"),
            0,
            0,
            0,
            0,
            None,
            &colorscheme,
        );

        assert!(state.highlight_row(0).is_some());
        assert!(state.highlight_row(1).is_some());
        assert!(state.highlight_row(2).is_none());
        assert!(state.checkpoints.keys().all(|row| *row < 2));
        assert_eq!(
            state.published_snapshot.as_ref().unwrap().version,
            after.version
        );
    }

    #[test]
    fn fully_cached_range_is_a_no_op() {
        let mut state = BufferHighlightState::new();
        let buffer = Buffer::new(
            BufferId::new(1).unwrap(),
            ReplicaId::LOCAL,
            "let value = 1;\n".repeat(20),
        );
        let snapshot = buffer.snapshot().as_inner().clone();
        let colorscheme = vim_colorscheme::ColorScheme::load_default();
        highlight_run(
            &mut state,
            &snapshot,
            Some("main.rs"),
            3,
            8,
            0,
            0,
            None,
            &colorscheme,
        );
        let rows_before = state.rows.clone();
        let checkpoint_rows_before = state.checkpoints.keys().copied().collect::<Vec<_>>();

        highlight_run(
            &mut state,
            &snapshot,
            Some("main.rs"),
            3,
            8,
            0,
            0,
            None,
            &colorscheme,
        );

        assert_eq!(state.rows, rows_before);
        assert_eq!(
            state.checkpoints.keys().copied().collect::<Vec<_>>(),
            checkpoint_rows_before
        );
    }

    #[test]
    fn nearest_checkpoint_bounds_lookback_and_converges_at_next_checkpoint() {
        let buffer = Buffer::new(
            BufferId::new(1).unwrap(),
            ReplicaId::LOCAL,
            "let value = 1;\n".repeat(260),
        );
        let snapshot = buffer.snapshot().as_inner().clone();
        let colorscheme = vim_colorscheme::ColorScheme::load_default();
        let mut state = BufferHighlightState::new();
        highlight_run(
            &mut state,
            &snapshot,
            Some("main.rs"),
            0,
            255,
            0,
            0,
            None,
            &colorscheme,
        );

        let resume = state.nearest_checkpoint(130).unwrap();
        assert_eq!(resume.row, 128);
        let existing = state.checkpoints.values().cloned().collect::<Vec<_>>();
        let mut visited = 0;
        let (rows, _, _) = parse_scopes_cancellable(
            &snapshot,
            Some("main.rs"),
            130,
            130,
            None,
            Some(resume),
            &existing,
            None,
            &colorscheme,
            || {
                visited += 1;
                false
            },
        )
        .unwrap();

        assert_eq!(visited, 65, "only checkpoint 128 through sentinel 192");
        assert_eq!(rows.first().unwrap().row, 130);
        assert_eq!(rows.last().unwrap().row, 191);
    }

    #[test]
    fn edit_reparse_stops_when_scope_stack_reconverges() {
        let text = "let value = 1;\n".repeat(200);
        let mut buffer = Buffer::new(BufferId::new(1).unwrap(), ReplicaId::LOCAL, text.clone());
        let colorscheme = vim_colorscheme::ColorScheme::load_default();
        let mut state = BufferHighlightState::new();
        let before = buffer.snapshot().as_inner().clone();
        highlight_run(
            &mut state,
            &before,
            Some("main.rs"),
            0,
            192,
            0,
            0,
            None,
            &colorscheme,
        );

        let edit_offset = text.lines().take(10).map(|line| line.len() + 1).sum();
        let mut transaction = buffer.transaction(EditOrigin::User);
        transaction.push(PlannedEdit {
            selection: None,
            edit: Edit::insert(ByteOffset(edit_offset), "let inserted = 2;\n"),
        });
        transaction.commit(None).unwrap();
        let after = buffer.snapshot().as_inner().clone();
        highlight_run(
            &mut state,
            &after,
            Some("main.rs"),
            10,
            20,
            0,
            0,
            None,
            &colorscheme,
        );

        assert!(state.rows.contains_key(&63));
        assert!(state.highlight_row(63).is_none());
        assert!(!state.rows.contains_key(&64));
        assert!(state.checkpoints.contains_key(&64));
    }
}
