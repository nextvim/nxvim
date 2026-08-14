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

/// Background-computed highlighting data for one buffer version and interval.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HighlightSnapshot {
    pub changedtick: u64,
    pub start: Anchor,
    pub end: Anchor,
    pub spans: Vec<HighlightSpan>,
}

impl HighlightSnapshot {
    pub fn contains_range(&self, start: Anchor, end: Anchor, snapshot: &BufferSnapshot) -> bool {
        self.start.cmp(&start, snapshot).is_le() && self.end.cmp(&end, snapshot).is_ge()
    }

    pub fn spans_for_row(&self, row: u32, snapshot: &BufferSnapshot) -> Option<Vec<HighlightSpan>> {
        let mut row_spans = Vec::new();
        for span in &self.spans {
            if span.start.to_point(snapshot).row == row {
                row_spans.push(span.clone());
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
    mut is_cancelled: impl FnMut() -> bool,
) -> Option<HighlightSnapshot> {
    let syntax_set = syntax_set();
    let syntax = file_path
        .and_then(|path| Path::new(path).extension())
        .and_then(|extension| extension.to_str())
        .and_then(|extension| syntax_set.find_syntax_by_extension(extension))
        .unwrap_or_else(|| syntax_set.find_syntax_plain_text());

    let start_point = start.to_point(snapshot);
    let end_point = end.to_point(snapshot);
    let start_row = start_point.row.min(snapshot.row_count());
    let mut end_row = end_point.row;
    if end_point.column > 0 {
        end_row += 1;
    }
    let end_row = end_row.max(start_row).min(snapshot.row_count());

    let mut parser = ParseState::new(syntax);
    let mut stack = ScopeStack::new();
    let highlighter = Highlighter::new(highlight_theme());
    let mut spans = Vec::new();

    for row in start_row..end_row {
        if is_cancelled() {
            return None;
        }

        let line_start_offset = Point::new(row, 0).to_offset(snapshot);
        let line_end_offset = Point::new(row, snapshot.line_len(row)).to_offset(snapshot);
        let mut text: String = snapshot.as_rope().chunks_in_range(line_start_offset..line_end_offset).collect();
        text.push('\n');
        let parsed = parser.parse_line(&text, &syntax_set).ok()?;

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
    }

    Some(HighlightSnapshot {
        changedtick,
        start,
        end,
        spans,
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
    style_cache: Vec<HighlightSpan>,
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
        &self,
        buffer_id: u64,
        changedtick: u64,
        start: Anchor,
        end: Anchor,
        snapshot: &BufferSnapshot,
    ) -> bool {
        self.buffers.get(&buffer_id).is_none_or(|state| {
            state.changedtick != Some(changedtick)
                || (!state.completed_ranges.iter().any(|completed| {
                    completed.changedtick == changedtick
                        && completed.start.cmp(&start, snapshot).is_le()
                        && completed.end.cmp(&end, snapshot).is_ge()
                }) && !state.pending_tasks.values().any(|pending| {
                    pending.changedtick == changedtick
                        && pending.start.cmp(&start, snapshot).is_le()
                        && pending.end.cmp(&end, snapshot).is_ge()
                }))
        })
    }

    pub fn begin_highlight(&mut self, buffer_id: u64, changedtick: u64) -> Arc<AtomicU64> {
        let state = self
            .buffers
            .entry(buffer_id)
            .or_insert_with(|| BufferHighlightState {
                changedtick: None,
                pending_tasks: HashMap::new(),
                completed_ranges: Vec::new(),
                style_cache: Vec::new(),
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
        state.style_cache.retain(|span| {
            span.end.cmp(&completed.start, snapshot).is_le()
                || span.start.cmp(&completed.end, snapshot).is_ge()
        });
        state.style_cache.extend(highlights.spans);
        state.completed_ranges.retain(|cached| {
            cached.changedtick == completed.changedtick
                && (cached.end.cmp(&completed.start, snapshot).is_le()
                    || cached.start.cmp(&completed.end, snapshot).is_ge())
        });
        state.completed_ranges.push(HighlightRange {
            changedtick: completed.changedtick,
            start: completed.start,
            end: completed.end,
        });
        true
    }

    pub fn spans(
        &self,
        buffer_id: BufferId,
        row: u32,
        snapshot: &BufferSnapshot,
    ) -> Option<Vec<HighlightSpan>> {
        let state = self.buffers.get(&buffer_id.get())?;
        let mut row_spans = Vec::new();
        for span in &state.style_cache {
            if span.start.to_point(snapshot).row == row {
                row_spans.push(span.clone());
            }
        }
        if row_spans.is_empty() {
            None
        } else {
            Some(row_spans)
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
                spans,
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
            service.spans(vim_buffer::BufferId::new(1).unwrap(), 0, snapshot),
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
                    spans: Vec::new(),
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
            parse_scopes_cancellable(snapshot, 1, Some("test.rs"), start, end, || false)
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
            parse_scopes_cancellable(snapshot, 0, None, start, end, || false).unwrap();

        assert_eq!(highlights.start.to_point(snapshot).row, 1);
        assert_eq!(highlights.spans.len(), 1);
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
        assert!(parse_scopes_cancellable(snapshot, 0, None, start, end, || true).is_none());
    }
}
