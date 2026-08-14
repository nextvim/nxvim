use std::{
    collections::HashMap,
    sync::{Arc, atomic::AtomicU64},
};

use background_worker::TaskId;
use rope::Point;
use std::{cmp::Ordering, path::Path, sync::OnceLock};
use syntect::{
    easy::ScopeRangeIterator,
    highlighting::{Highlighter, Theme, ThemeSet},
    parsing::{ParseState, ScopeStack, SyntaxSet},
};
use text::{Anchor, BufferSnapshot, ToOffset, ToPoint};
use vim_buffer::BufferId;

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
    pub changedtick: u64,
    pub anchor: Anchor,
    pub cached_offset: usize,
    pub parse_state: ParseState,
    pub scope_stack: ScopeStack,
}

unsafe impl Send for ParseStateCheckpoint {}
unsafe impl Sync for ParseStateCheckpoint {}

impl std::fmt::Debug for ParseStateCheckpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ParseStateCheckpoint")
            .field("anchor", &self.anchor)
            .field("cached_offset", &self.cached_offset)
            .finish()
    }
}

/// Background-computed highlighting data for one buffer version and interval.
#[derive(Clone)]
pub struct HighlightSnapshot {
    pub changedtick: u64,
    pub start: Anchor,
    pub end: Anchor,
    pub rows: Vec<HighlightedRow>,
    pub checkpoints: Vec<ParseStateCheckpoint>,
}

impl std::fmt::Debug for HighlightSnapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HighlightSnapshot")
            .field("changedtick", &self.changedtick)
            .field("start", &self.start)
            .field("end", &self.end)
            .field("rows", &self.rows)
            .field("checkpoints", &self.checkpoints)
            .finish()
    }
}

impl PartialEq for HighlightSnapshot {
    fn eq(&self, other: &Self) -> bool {
        self.changedtick == other.changedtick
            && self.start == other.start
            && self.end == other.end
            && self.rows == other.rows
    }
}

impl Eq for HighlightSnapshot {}

impl HighlightSnapshot {
    pub fn spans_for_row(&self, row: u32) -> Option<&[HighlightSpan]> {
        self.rows
            .binary_search_by_key(&row, |highlighted| highlighted.row)
            .ok()
            .map(|index| self.rows[index].spans.as_slice())
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

/// Parses TextMate scopes for `[start, end)`. Callers include a lookbehind
/// window in `start` so multiline state can settle before visible rows.
pub fn parse_scopes_cancellable(
    snapshot: &BufferSnapshot,
    changedtick: u64,
    file_path: Option<&str>,
    start: Anchor,
    end: Anchor,
    resume_checkpoint: Option<ParseStateCheckpoint>,
    existing_checkpoints: &[ParseStateCheckpoint],
    mut is_cancelled: impl FnMut() -> bool,
) -> Option<HighlightSnapshot> {
    let syntax_set = syntax_set();
    let syntax = file_path
        .and_then(|path| Path::new(path).extension())
        .and_then(|extension| extension.to_str())
        .and_then(|extension| syntax_set.find_syntax_by_extension(extension))
        .unwrap_or_else(|| syntax_set.find_syntax_plain_text());

    let start_offset = start.to_offset(snapshot);
    let end_offset = end.to_offset(snapshot);

    let (mut parser, mut stack, current_offset) = if let Some(cp) = resume_checkpoint {
        (
            cp.parse_state,
            cp.scope_stack,
            cp.anchor.to_offset(snapshot),
        )
    } else {
        (ParseState::new(syntax), ScopeStack::new(), 0)
    };

    let start_row = snapshot.offset_to_point(current_offset).row;
    let end_row = end.to_point(snapshot).row.min(snapshot.row_count());

    let mut rows = Vec::new();
    let mut checkpoints = Vec::new();
    let highlighter = Highlighter::new(highlight_theme());

    for row in start_row..=end_row {
        if is_cancelled() {
            return None;
        }

        let line_start_offset = Point::new(row, 0).to_offset(snapshot);
        let line_end_offset = Point::new(row, snapshot.line_len(row)).to_offset(snapshot);

        // Periodically save checkpoints (every 64 lines)
        if row > 0 && row % 64 == 0 && line_start_offset >= start_offset {
            checkpoints.push(ParseStateCheckpoint {
                changedtick,
                anchor: snapshot.anchor_before(line_start_offset),
                cached_offset: line_start_offset,
                parse_state: parser.clone(),
                scope_stack: stack.clone(),
            });
        }

        // Check for state convergence if we parsed beyond the target end range
        if line_start_offset >= end_offset {
            if let Some(existing_cp) = existing_checkpoints
                .iter()
                .find(|cp| cp.cached_offset == line_start_offset)
            {
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

        if line_start_offset >= start_offset {
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

    Some(HighlightSnapshot {
        changedtick,
        start,
        end,
        rows,
        checkpoints,
    })
}

#[derive(Debug)]
pub struct HighlightTaskResult {
    pub buffer_id: u64,
    pub changedtick: u64,
    pub start: Anchor,
    pub end: Anchor,
    pub highlights: Option<HighlightSnapshot>,
}

#[derive(Clone, Copy)]
struct HighlightRange {
    changedtick: u64,
    start: Anchor,
    end: Anchor,
}

struct BufferHighlightState {
    changedtick: Option<u64>,
    pending_tasks: HashMap<TaskId, HighlightRange>,
    completed_ranges: Vec<HighlightRange>,
    checkpoints: Vec<ParseStateCheckpoint>,
    rows: HashMap<u32, Vec<HighlightSpan>>,
    published_snapshot: Option<BufferSnapshot>,
    valid_checkpoint_prefix_end: usize,
    cache_changedtick: Option<u64>,
}

impl BufferHighlightState {
    fn ensure_cache_resolved(&mut self, changedtick: u64, snapshot: &BufferSnapshot) {
        if self.cache_changedtick != Some(changedtick) {
            self.cache_changedtick = Some(changedtick);

            for cp in &mut self.checkpoints {
                cp.cached_offset = cp.anchor.to_offset(snapshot);
            }
            self.checkpoints.sort_by_key(|cp| cp.cached_offset);
        }
    }

    fn nearest_checkpoint(
        &self,
        target_offset: usize,
        changedtick: u64,
    ) -> Option<ParseStateCheckpoint> {
        self.checkpoints.iter().rev().find_map(|checkpoint| {
            (checkpoint.cached_offset <= target_offset
                && (checkpoint.changedtick == changedtick
                    || checkpoint.cached_offset < self.valid_checkpoint_prefix_end))
                .then(|| checkpoint.clone())
        })
    }
}

fn project_rows(
    rows: &mut HashMap<u32, Vec<HighlightSpan>>,
    old: std::ops::Range<Point>,
    new: std::ops::Range<Point>,
    snapshot: &BufferSnapshot,
) {
    let fallback = rows
        .get(&old.start.row)
        .and_then(|spans| {
            spans
                .iter()
                .find(|span| {
                    span.start_column <= old.start.column && old.start.column <= span.end_column
                })
                .or_else(|| spans.last())
        })
        .cloned();
    let old_end_spans = (old.end.row != old.start.row)
        .then(|| rows.get(&old.end.row).cloned())
        .flatten();
    let row_delta = new.end.row as i64 - old.end.row as i64;
    let mut projected =
        HashMap::with_capacity(rows.len() + new.end.row.saturating_sub(new.start.row) as usize);

    for (row, mut spans) in rows.drain() {
        if row < old.start.row {
            projected.insert(row, spans);
        } else if row > old.end.row {
            let shifted = (row as i64 + row_delta).max(0) as u32;
            projected.insert(shifted, spans);
        } else if row == old.start.row {
            if old.start.row == old.end.row && new.start.row == new.end.row {
                project_same_line_spans(
                    &mut spans,
                    old.start.column..old.end.column,
                    new.end.column,
                );
            } else {
                spans.retain(|span| span.start_column < old.start.column);
                for span in &mut spans {
                    span.end_column = span.end_column.min(old.start.column);
                }
                if let Some(style) = fallback.as_ref() {
                    let mut inserted = style.clone();
                    inserted.start_column = old.start.column;
                    inserted.end_column = if new.end.row == new.start.row {
                        new.end.column
                    } else {
                        snapshot.line_len(new.start.row)
                    };
                    if inserted.start_column < inserted.end_column {
                        spans.push(inserted);
                    }
                }
            }
            projected.insert(new.start.row, spans);
        }
    }

    if old.start.row != old.end.row && new.end.row == new.start.row {
        if let Some(tail) = old_end_spans {
            let target = projected.entry(new.start.row).or_default();
            for mut span in tail {
                if span.end_column <= old.end.column {
                    continue;
                }
                span.start_column = new
                    .end
                    .column
                    .saturating_add(span.start_column.saturating_sub(old.end.column));
                span.end_column = new
                    .end
                    .column
                    .saturating_add(span.end_column.saturating_sub(old.end.column));
                target.push(span);
            }
            target.sort_by_key(|span| span.start_column);
        }
    }

    if new.end.row > new.start.row {
        for row in new.start.row + 1..=new.end.row {
            let spans = fallback
                .as_ref()
                .and_then(|style| {
                    let line_len = snapshot.line_len(row);
                    (line_len > 0).then(|| {
                        let mut span = style.clone();
                        span.start_column = 0;
                        span.end_column = line_len;
                        vec![span]
                    })
                })
                .unwrap_or_default();
            projected.insert(row, spans);
        }
    }

    *rows = projected;
}

fn project_same_line_spans(
    spans: &mut Vec<HighlightSpan>,
    old: std::ops::Range<u32>,
    new_end: u32,
) {
    let delta = new_end as i64 - old.end as i64;
    for span in spans.iter_mut() {
        if span.end_column <= old.start {
            continue;
        }
        if span.start_column >= old.end {
            span.start_column = (span.start_column as i64 + delta).max(0) as u32;
            span.end_column = (span.end_column as i64 + delta).max(0) as u32;
        } else {
            span.end_column = (span.end_column as i64 + delta).max(old.start as i64) as u32;
        }
    }
    spans.retain(|span| span.start_column < span.end_column);
}

pub struct HighlightService {
    buffers: HashMap<u64, BufferHighlightState>,
}

impl HighlightService {
    pub fn new() -> Self {
        Self {
            buffers: HashMap::new(),
        }
    }

    /// Projects the bounded published window through edits since its source
    /// snapshot. This preserves visual continuity; projected rows are fallback
    /// styling and are replaced by the next authoritative worker result.
    pub fn project_edits(&mut self, buffer_id: u64, changedtick: u64, snapshot: &BufferSnapshot) {
        let Some(state) = self.buffers.get_mut(&buffer_id) else {
            return;
        };
        let Some(previous) = state.published_snapshot.as_ref() else {
            return;
        };
        if previous.version == snapshot.version {
            return;
        }

        let mut edits: Vec<_> = snapshot.edits_since::<Point>(&previous.version).collect();
        if let Some(first_dirty_row) = edits.iter().map(|edit| edit.new.start.row).min() {
            let dirty_offset = Point::new(first_dirty_row, 0).to_offset(snapshot);
            state.valid_checkpoint_prefix_end = state.valid_checkpoint_prefix_end.min(dirty_offset);
        }
        state.changedtick = Some(changedtick);
        state.pending_tasks.clear();
        state.completed_ranges.clear();
        edits.reverse();
        for edit in edits {
            project_rows(&mut state.rows, edit.old, edit.new, snapshot);
        }
        state.published_snapshot = Some(snapshot.clone());
    }

    pub fn should_highlight(
        &mut self,
        buffer_id: u64,
        changedtick: u64,
        start: Anchor,
        end: Anchor,
        snapshot: &BufferSnapshot,
    ) -> bool {
        let state = self
            .buffers
            .entry(buffer_id)
            .or_insert_with(|| BufferHighlightState {
                changedtick: None,
                pending_tasks: HashMap::new(),
                completed_ranges: Vec::new(),
                checkpoints: Vec::new(),
                rows: HashMap::new(),
                published_snapshot: None,
                valid_checkpoint_prefix_end: usize::MAX,
                cache_changedtick: None,
            });
        state.ensure_cache_resolved(changedtick, snapshot);

        let start_offset = start.to_offset(snapshot);
        let end_offset = end.to_offset(snapshot);

        if state.changedtick != Some(changedtick) {
            return true;
        }

        let is_covered = |ranges: &[HighlightRange]| {
            ranges.iter().any(|r| {
                let r_start = r.start.to_offset(snapshot);
                let r_end = r.end.to_offset(snapshot);
                r.changedtick == changedtick && r_start <= start_offset && r_end >= end_offset
            })
        };

        let pending_ranges: Vec<HighlightRange> = state.pending_tasks.values().cloned().collect();
        !is_covered(&state.completed_ranges) && !is_covered(&pending_ranges)
    }

    pub fn begin_highlight(&mut self, buffer_id: u64, changedtick: u64) -> Arc<AtomicU64> {
        let state = self
            .buffers
            .entry(buffer_id)
            .or_insert_with(|| BufferHighlightState {
                changedtick: None,
                pending_tasks: HashMap::new(),
                completed_ranges: Vec::new(),
                checkpoints: Vec::new(),
                rows: HashMap::new(),
                published_snapshot: None,
                valid_checkpoint_prefix_end: usize::MAX,
                cache_changedtick: None,
            });
        if state.changedtick != Some(changedtick) {
            state.changedtick = Some(changedtick);
            state.pending_tasks.clear();
            state.completed_ranges.clear();
        }
        Arc::new(AtomicU64::new(0))
    }

    pub fn set_pending_task(
        &mut self,
        buffer_id: u64,
        task_id: TaskId,
        changedtick: u64,
        start: Anchor,
        end: Anchor,
    ) {
        if let Some(state) = self.buffers.get_mut(&buffer_id) {
            state.pending_tasks.insert(
                task_id,
                HighlightRange {
                    changedtick,
                    start,
                    end,
                },
            );
        }
    }

    pub fn nearest_checkpoint(
        &mut self,
        buffer_id: u64,
        target_offset: usize,
        snapshot: &BufferSnapshot,
        changedtick: u64,
    ) -> Option<ParseStateCheckpoint> {
        let state = self.buffers.get_mut(&buffer_id)?;
        state.ensure_cache_resolved(changedtick, snapshot);
        state.nearest_checkpoint(target_offset, changedtick)
    }

    pub fn existing_checkpoints(
        &mut self,
        buffer_id: u64,
        snapshot: &BufferSnapshot,
        changedtick: u64,
    ) -> Vec<ParseStateCheckpoint> {
        if let Some(state) = self.buffers.get_mut(&buffer_id) {
            state.ensure_cache_resolved(changedtick, snapshot);
            state
                .checkpoints
                .iter()
                .filter(|checkpoint| {
                    checkpoint.changedtick == changedtick
                        || checkpoint.cached_offset < state.valid_checkpoint_prefix_end
                })
                .cloned()
                .collect()
        } else {
            Vec::new()
        }
    }

    pub fn apply_task_result(
        &mut self,
        task_id: TaskId,
        completed: HighlightTaskResult,
        snapshot: &BufferSnapshot,
    ) -> bool {
        let Some(state) = self.buffers.get_mut(&completed.buffer_id) else {
            return false;
        };
        let Some(pending) = state.pending_tasks.remove(&task_id) else {
            return false;
        };
        if state.changedtick != Some(completed.changedtick)
            || pending.changedtick != completed.changedtick
            || pending.start.cmp(&completed.start, snapshot) != Ordering::Equal
            || pending.end.cmp(&completed.end, snapshot) != Ordering::Equal
        {
            return false;
        }

        let Some(highlights) = completed.highlights else {
            return false;
        };

        state.ensure_cache_resolved(completed.changedtick, snapshot);

        let completed_start = completed.start.to_offset(snapshot);
        let completed_end = completed.end.to_offset(snapshot);

        let completed_start_row = completed.start.to_point(snapshot).row;
        let completed_end_row = completed.end.to_point(snapshot).row;
        state
            .rows
            .retain(|row, _| *row < completed_start_row || *row > completed_end_row);

        state
            .checkpoints
            .retain(|cp| cp.cached_offset < completed_start || cp.cached_offset > completed_end);

        state.rows = highlights
            .rows
            .into_iter()
            .map(|row| (row.row, row.spans))
            .collect();
        state.published_snapshot = Some(snapshot.clone());
        state.checkpoints.extend(highlights.checkpoints);
        state.checkpoints.sort_by_key(|cp| cp.cached_offset);

        state.completed_ranges.retain(|cached| {
            let cached_start = cached.start.to_offset(snapshot);
            let cached_end = cached.end.to_offset(snapshot);
            cached.changedtick == completed.changedtick
                && (cached_end <= completed_start || cached_start >= completed_end)
        });
        state.completed_ranges.push(HighlightRange {
            changedtick: completed.changedtick,
            start: completed.start,
            end: completed.end,
        });
        true
    }

    pub fn spans(
        &mut self,
        buffer_id: BufferId,
        row: u32,
        snapshot: &BufferSnapshot,
        changedtick: u64,
    ) -> Option<Vec<HighlightSpan>> {
        let state = self.buffers.get_mut(&buffer_id.get())?;
        state.ensure_cache_resolved(changedtick, snapshot);
        state.rows.get(&row).cloned()
    }

    /// Returns the currently published rows without cloning the cache.
    /// Task results update the map completely before the single-threaded UI
    /// observes it on the next redraw.
    pub fn published_rows(&self, buffer_id: BufferId) -> Option<&HashMap<u32, Vec<HighlightSpan>>> {
        self.buffers.get(&buffer_id.get()).map(|state| &state.rows)
    }

    pub fn is_highlighting(&self) -> bool {
        self.buffers
            .values()
            .any(|state| !state.pending_tasks.is_empty())
    }

    pub fn remove_buffer(&mut self, buffer_id: BufferId) {
        self.buffers.remove(&buffer_id.get());
    }
}

impl Default for HighlightService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clock::ReplicaId;
    use text::{Buffer, BufferId};

    fn result(
        changedtick: u64,
        spans: Vec<HighlightSpan>,
        start: Anchor,
        end: Anchor,
    ) -> HighlightTaskResult {
        HighlightTaskResult {
            buffer_id: 1,
            changedtick,
            start,
            end,
            highlights: Some(HighlightSnapshot {
                changedtick,
                start,
                end,
                rows: vec![HighlightedRow { row: 0, spans }],
                checkpoints: Vec::new(),
            }),
        }
    }

    #[test]
    fn retains_style_cache_while_new_version_is_pending() {
        let buffer = Buffer::new(
            ReplicaId::LOCAL,
            BufferId::new(1).unwrap(),
            "one\ntwo".to_owned(),
        );
        let snapshot = buffer.snapshot();
        let start = snapshot.anchor_before(0);
        let end = snapshot.anchor_after(snapshot.len());

        let mut service = HighlightService::new();
        service.begin_highlight(1, 7);
        service.set_pending_task(1, TaskId(1), 7, start, end);
        let span = HighlightSpan {
            start_column: 0,
            end_column: 3,
            scopes: vec!["source".to_owned()],
            foreground: [1, 2, 3],
        };
        assert!(service.apply_task_result(
            TaskId(1),
            result(7, vec![span.clone()], start, end),
            snapshot
        ));

        service.begin_highlight(1, 8);
        service.set_pending_task(1, TaskId(2), 8, start, end);

        assert_eq!(
            service.spans(vim_buffer::BufferId::new(1).unwrap(), 0, snapshot, 8),
            Some(vec![span])
        );
        assert!(service.is_highlighting());
    }

    #[test]
    fn projects_same_line_insertions_without_clearing_published_style() {
        let mut buffer = Buffer::new(
            ReplicaId::LOCAL,
            BufferId::new(1).unwrap(),
            "one".to_owned(),
        );
        let old_snapshot = buffer.snapshot().clone();
        let start = old_snapshot.anchor_before(0);
        let end = old_snapshot.anchor_after(old_snapshot.len());
        let span = HighlightSpan {
            start_column: 0,
            end_column: 3,
            scopes: vec!["source".to_owned()],
            foreground: [1, 2, 3],
        };

        let mut service = HighlightService::new();
        service.begin_highlight(1, 7);
        service.set_pending_task(1, TaskId(1), 7, start, end);
        assert!(service.apply_task_result(
            TaskId(1),
            result(7, vec![span], start, end),
            &old_snapshot,
        ));

        buffer.edit([(1..1, "x")]);
        let new_snapshot = buffer.snapshot();
        service.project_edits(1, 8, new_snapshot);

        let projected = &service
            .published_rows(vim_buffer::BufferId::new(1).unwrap())
            .unwrap()[&0][0];
        assert_eq!(projected.start_column, 0);
        assert_eq!(projected.end_column, 4);
    }

    #[test]
    fn projects_rows_across_newline_insertions() {
        let mut buffer = Buffer::new(
            ReplicaId::LOCAL,
            BufferId::new(1).unwrap(),
            "one\ntwo".to_owned(),
        );
        let old_snapshot = buffer.snapshot().clone();
        let start = old_snapshot.anchor_before(0);
        let end = old_snapshot.anchor_after(old_snapshot.len());
        let highlights =
            parse_scopes_cancellable(&old_snapshot, 7, None, start, end, None, &[], || false)
                .unwrap();

        let mut service = HighlightService::new();
        service.begin_highlight(1, 7);
        service.set_pending_task(1, TaskId(1), 7, start, end);
        assert!(service.apply_task_result(
            TaskId(1),
            HighlightTaskResult {
                buffer_id: 1,
                changedtick: 7,
                start,
                end,
                highlights: Some(highlights),
            },
            &old_snapshot,
        ));

        buffer.edit([(0..0, "new\n")]);
        let new_snapshot = buffer.snapshot();
        service.project_edits(1, 8, new_snapshot);

        let rows = service
            .published_rows(vim_buffer::BufferId::new(1).unwrap())
            .unwrap();
        assert!(rows.contains_key(&0));
        assert!(rows.contains_key(&1));
        assert!(rows.contains_key(&2));
    }

    #[test]
    fn failed_result_preserves_published_rows() {
        let buffer = Buffer::new(
            ReplicaId::LOCAL,
            BufferId::new(1).unwrap(),
            "one".to_owned(),
        );
        let snapshot = buffer.snapshot();
        let start = snapshot.anchor_before(0);
        let end = snapshot.anchor_after(snapshot.len());
        let span = HighlightSpan {
            start_column: 0,
            end_column: 3,
            scopes: vec!["source".to_owned()],
            foreground: [1, 2, 3],
        };

        let mut service = HighlightService::new();
        service.begin_highlight(1, 7);
        service.set_pending_task(1, TaskId(1), 7, start, end);
        assert!(service.apply_task_result(
            TaskId(1),
            result(7, vec![span.clone()], start, end),
            snapshot,
        ));

        service.set_pending_task(1, TaskId(2), 7, start, end);
        assert!(!service.apply_task_result(
            TaskId(2),
            HighlightTaskResult {
                buffer_id: 1,
                changedtick: 7,
                start,
                end,
                highlights: None,
            },
            snapshot,
        ));

        assert_eq!(
            service.published_rows(vim_buffer::BufferId::new(1).unwrap()),
            Some(&HashMap::from([(0, vec![span])])),
        );
    }

    #[test]
    fn applies_finished_result_while_another_task_is_pending() {
        let buffer = Buffer::new(
            ReplicaId::LOCAL,
            BufferId::new(1).unwrap(),
            "one\ntwo".to_owned(),
        );
        let snapshot = buffer.snapshot();
        let start1 = snapshot.anchor_before(0);
        let end1 = snapshot.anchor_after(3);
        let start2 = snapshot.anchor_before(3);
        let end2 = snapshot.anchor_after(snapshot.len());

        let mut service = HighlightService::new();
        service.begin_highlight(1, 7);
        service.set_pending_task(1, TaskId(1), 7, start1, end1);
        service.set_pending_task(1, TaskId(2), 7, start2, end2);

        assert!(service.apply_task_result(
            TaskId(1),
            HighlightTaskResult {
                buffer_id: 1,
                changedtick: 7,
                start: start1,
                end: end1,
                highlights: Some(HighlightSnapshot {
                    changedtick: 7,
                    start: start1,
                    end: end1,
                    rows: Vec::new(),
                    checkpoints: Vec::new(),
                }),
            },
            snapshot
        ));
        assert!(service.is_highlighting());
        assert!(
            service
                .buffers
                .get(&1)
                .unwrap()
                .completed_ranges
                .iter()
                .any(|completed| {
                    completed.start.cmp(&start1, snapshot) == Ordering::Equal
                        && completed.end.cmp(&end1, snapshot) == Ordering::Equal
                })
        );
    }

    #[test]
    fn preserves_multiline_parse_state() {
        let buffer = Buffer::new(
            ReplicaId::LOCAL,
            BufferId::new(1).unwrap(),
            "fn main() {\n/* comment\nstill comment */\n}".to_owned(),
        );
        let snapshot = buffer.snapshot();
        let start = snapshot.anchor_before(0);
        let end = snapshot.anchor_after(snapshot.len());
        let highlights =
            parse_scopes_cancellable(snapshot, 1, Some("test.rs"), start, end, None, &[], || {
                false
            })
            .unwrap();

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
        let snapshot = buffer.snapshot();
        let start = snapshot.anchor_before(4); // "two" starts at offset 4
        let end = snapshot.anchor_after(7); // "two" ends at offset 7
        let highlights =
            parse_scopes_cancellable(snapshot, 0, None, start, end, None, &[], || false).unwrap();

        assert_eq!(highlights.start.to_point(snapshot).row, 1);
        assert_eq!(highlights.rows.len(), 1);
    }

    #[test]
    fn empty_rows_are_explicitly_covered() {
        let buffer = Buffer::new(
            ReplicaId::LOCAL,
            BufferId::new(1).unwrap(),
            "one\n\nthree".to_owned(),
        );
        let snapshot = buffer.snapshot();
        let start = snapshot.anchor_before(0);
        let end = snapshot.anchor_after(snapshot.len());
        let highlights =
            parse_scopes_cancellable(snapshot, 0, None, start, end, None, &[], || false).unwrap();

        assert_eq!(highlights.spans_for_row(1), Some([].as_slice()));
    }

    #[test]
    fn span_columns_are_row_local_utf8_byte_boundaries() {
        let source = "let café = \"é\";";
        let buffer = Buffer::new(
            ReplicaId::LOCAL,
            BufferId::new(1).unwrap(),
            source.to_owned(),
        );
        let snapshot = buffer.snapshot();
        let start = snapshot.anchor_before(0);
        let end = snapshot.anchor_after(snapshot.len());
        let highlights =
            parse_scopes_cancellable(snapshot, 0, Some("test.rs"), start, end, None, &[], || {
                false
            })
            .unwrap();

        let spans = highlights.spans_for_row(0).unwrap();
        assert!(!spans.is_empty());
        assert!(spans.iter().all(|span| {
            let start = span.start_column as usize;
            let end = span.end_column as usize;
            start < end
                && end <= source.len()
                && source.is_char_boundary(start)
                && source.is_char_boundary(end)
        }));
    }

    #[test]
    fn cancellation_discards_partial_snapshot() {
        let buffer = Buffer::new(
            ReplicaId::LOCAL,
            BufferId::new(1).unwrap(),
            "one\ntwo".to_owned(),
        );
        let snapshot = buffer.snapshot();
        let start = snapshot.anchor_before(0);
        let end = snapshot.anchor_after(snapshot.len());
        assert!(
            parse_scopes_cancellable(snapshot, 0, None, start, end, None, &[], || true).is_none()
        );
    }
}
