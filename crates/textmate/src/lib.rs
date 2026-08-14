use std::{
    collections::HashMap,
    sync::{Arc, atomic::AtomicU64},
};

use vim_buffer::BufferId;
use background_worker::TaskId;
use std::{path::Path, sync::OnceLock, cmp::Ordering};
use rope::Point;
use syntect::{
    easy::ScopeRangeIterator,
    highlighting::{Highlighter, Theme, ThemeSet},
    parsing::{ParseState, ScopeStack, SyntaxSet},
};
use text::{BufferSnapshot, ToOffset, Anchor, ToPoint};

/// A contiguous byte-column range with the TextMate scopes active for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HighlightSpan {
    pub start: Anchor,
    pub end: Anchor,
    pub scopes: Vec<String>,
    pub foreground: [u8; 3],
}

#[derive(Clone)]
pub struct ParseStateCheckpoint {
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CachedHighlightChunk {
    pub start: Anchor,
    pub end: Anchor,
    pub cached_start_offset: usize,
    pub cached_end_offset: usize,
    pub spans: Vec<HighlightSpan>,
}

/// Background-computed highlighting data for one buffer version and interval.
#[derive(Clone)]
pub struct HighlightSnapshot {
    pub changedtick: u64,
    pub start: Anchor,
    pub end: Anchor,
    pub style_cache: Vec<CachedHighlightChunk>,
    pub checkpoints: Vec<ParseStateCheckpoint>,
}

impl std::fmt::Debug for HighlightSnapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HighlightSnapshot")
            .field("changedtick", &self.changedtick)
            .field("start", &self.start)
            .field("end", &self.end)
            .field("style_cache", &self.style_cache)
            .field("checkpoints", &self.checkpoints)
            .finish()
    }
}

impl PartialEq for HighlightSnapshot {
    fn eq(&self, other: &Self) -> bool {
        self.changedtick == other.changedtick
            && self.start == other.start
            && self.end == other.end
            && self.style_cache == other.style_cache
    }
}

impl Eq for HighlightSnapshot {}

impl HighlightSnapshot {
    pub fn spans_for_row(&self, row: u32, snapshot: &BufferSnapshot) -> Option<Vec<HighlightSpan>> {
        let row_start_offset = Point::new(row, 0).to_offset(snapshot);
        let row_end_offset = Point::new(row, snapshot.line_len(row)).to_offset(snapshot);

        let mut row_spans = Vec::new();
        for chunk in &self.style_cache {
            if chunk.cached_start_offset <= row_end_offset && chunk.cached_end_offset >= row_start_offset {
                for span in &chunk.spans {
                    let span_start = span.start.to_offset(snapshot);
                    let span_end = span.end.to_offset(snapshot);
                    if span_start <= row_end_offset && span_end >= row_start_offset {
                        row_spans.push(span.clone());
                    }
                }
            }
        }

        if row_spans.is_empty() {
            None
        } else {
            Some(row_spans)
        }
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
        (cp.parse_state, cp.scope_stack, cp.anchor.to_offset(snapshot))
    } else {
        (ParseState::new(syntax), ScopeStack::new(), 0)
    };

    let start_row = snapshot.offset_to_point(current_offset).row;
    let end_row = end.to_point(snapshot).row.min(snapshot.row_count());

    let mut style_cache = Vec::new();
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
                anchor: snapshot.anchor_before(line_start_offset),
                cached_offset: line_start_offset,
                parse_state: parser.clone(),
                scope_stack: stack.clone(),
            });
        }

        // Check for state convergence if we parsed beyond the target end range
        if line_start_offset >= end_offset {
            if let Some(existing_cp) = existing_checkpoints.iter().find(|cp| cp.cached_offset == line_start_offset) {
                // Since ParseState might not implement PartialEq, we compare ScopeStack as convergence metric
                if existing_cp.scope_stack == stack {
                    break;
                }
            }
        }

        let mut text: String = snapshot.as_rope().chunks_in_range(line_start_offset..line_end_offset).collect();
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
                let span_start = (line_start_offset + range.start).min(snapshot.len());
                let span_end = (line_start_offset + range.end).min(snapshot.len());
                spans.push(HighlightSpan {
                    start: snapshot.anchor_before(span_start),
                    end: snapshot.anchor_after(span_end),
                    scopes: stack.as_slice().iter().map(ToString::to_string).collect(),
                    foreground: [
                        scope_style.foreground.r,
                        scope_style.foreground.g,
                        scope_style.foreground.b,
                    ],
                });
            }

            style_cache.push(CachedHighlightChunk {
                start: snapshot.anchor_before(line_start_offset),
                end: snapshot.anchor_after(line_end_offset),
                cached_start_offset: line_start_offset,
                cached_end_offset: line_end_offset,
                spans,
            });
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
        style_cache,
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
    style_cache: Vec<CachedHighlightChunk>,
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

            for chunk in &mut self.style_cache {
                chunk.cached_start_offset = chunk.start.to_offset(snapshot);
                chunk.cached_end_offset = chunk.end.to_offset(snapshot);
            }
            self.style_cache.sort_by_key(|chunk| chunk.cached_start_offset);
        }
    }

    fn nearest_checkpoint(&self, target_offset: usize) -> Option<ParseStateCheckpoint> {
        let idx = self.checkpoints.partition_point(|cp| cp.cached_offset <= target_offset);
        if idx > 0 {
            Some(self.checkpoints[idx - 1].clone())
        } else {
            None
        }
    }
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

    pub fn should_highlight(
        &mut self,
        buffer_id: u64,
        changedtick: u64,
        start: Anchor,
        end: Anchor,
        snapshot: &BufferSnapshot,
    ) -> bool {
        let state = self.buffers.entry(buffer_id).or_insert_with(|| BufferHighlightState {
            changedtick: None,
            pending_tasks: HashMap::new(),
            completed_ranges: Vec::new(),
            checkpoints: Vec::new(),
            style_cache: Vec::new(),
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
                style_cache: Vec::new(),
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

    pub fn nearest_checkpoint(&mut self, buffer_id: u64, target_offset: usize, snapshot: &BufferSnapshot, changedtick: u64) -> Option<ParseStateCheckpoint> {
        let state = self.buffers.get_mut(&buffer_id)?;
        state.ensure_cache_resolved(changedtick, snapshot);
        state.nearest_checkpoint(target_offset)
    }

    pub fn existing_checkpoints(&mut self, buffer_id: u64, snapshot: &BufferSnapshot, changedtick: u64) -> Vec<ParseStateCheckpoint> {
        if let Some(state) = self.buffers.get_mut(&buffer_id) {
            state.ensure_cache_resolved(changedtick, snapshot);
            state.checkpoints.clone()
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

        state.ensure_cache_resolved(completed.changedtick, snapshot);

        let completed_start = completed.start.to_offset(snapshot);
        let completed_end = completed.end.to_offset(snapshot);

        state.style_cache.retain(|chunk| {
            chunk.cached_end_offset <= completed_start || chunk.cached_start_offset >= completed_end
        });

        state.checkpoints.retain(|cp| {
            cp.cached_offset < completed_start || cp.cached_offset > completed_end
        });

        let Some(highlights) = completed.highlights else {
            return false;
        };

        state.style_cache.extend(highlights.style_cache);
        state.checkpoints.extend(highlights.checkpoints);

        state.style_cache.sort_by_key(|c| c.cached_start_offset);
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

        let row_start_offset = Point::new(row, 0).to_offset(snapshot);
        let row_end_offset = Point::new(row, snapshot.line_len(row)).to_offset(snapshot);

        let mut row_spans = Vec::new();
        let start_idx = state.style_cache.partition_point(|chunk| chunk.cached_end_offset < row_start_offset);
        for chunk in &state.style_cache[start_idx..] {
            if chunk.cached_start_offset > row_end_offset {
                break;
            }
            for span in &chunk.spans {
                let span_start = span.start.to_offset(snapshot);
                let span_end = span.end.to_offset(snapshot);
                if span_start <= row_end_offset && span_end >= row_start_offset {
                    row_spans.push(span.clone());
                }
            }
        }

        if row_spans.is_empty() {
            None
        } else {
            Some(row_spans)
        }
    }

    pub fn all_spans(&self, buffer_id: BufferId) -> Vec<HighlightSpan> {
        if let Some(state) = self.buffers.get(&buffer_id.get()) {
            state.style_cache.iter().flat_map(|chunk| chunk.spans.clone()).collect()
        } else {
            Vec::new()
        }
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
                style_cache: vec![CachedHighlightChunk {
                    start,
                    end,
                    cached_start_offset: 0,
                    cached_end_offset: 0,
                    spans,
                }],
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
            start,
            end,
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
                    style_cache: Vec::new(),
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
            parse_scopes_cancellable(snapshot, 1, Some("test.rs"), start, end, None, &[], || false)
                .unwrap();

        assert!(
            highlights
                .spans_for_row(2, snapshot)
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
        let end = snapshot.anchor_after(7);   // "two" ends at offset 7
        let highlights =
            parse_scopes_cancellable(snapshot, 0, None, start, end, None, &[], || false).unwrap();

        assert_eq!(highlights.start.to_point(snapshot).row, 1);
        assert_eq!(highlights.style_cache.len(), 1);
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
        assert!(parse_scopes_cancellable(snapshot, 0, None, start, end, None, &[], || true).is_none());
    }
}
