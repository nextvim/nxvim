use std::{
    collections::HashMap,
    sync::{Arc, atomic::AtomicU64},
};

use vim_buffer::BufferId;
use background_worker::TaskId;
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

#[derive(Debug)]
pub(crate) struct HighlightTaskResult {
    pub buffer_id: u64,
    pub changedtick: u64,
    pub start_row: u32,
    pub end_row: u32,
    pub highlights: Option<HighlightSnapshot>,
}

#[derive(Clone, Copy)]
struct HighlightRange {
    changedtick: u64,
    start_row: u32,
    end_row: u32,
}

struct BufferHighlightState {
    changedtick: Option<u64>,
    pending_tasks: HashMap<TaskId, HighlightRange>,
    completed_ranges: Vec<HighlightRange>,
    style_cache: HashMap<u32, Vec<HighlightSpan>>,
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

    pub(crate) fn should_highlight(
        &self,
        buffer_id: u64,
        changedtick: u64,
        start_row: u32,
        end_row: u32,
    ) -> bool {
        self.buffers.get(&buffer_id).is_none_or(|state| {
            state.changedtick != Some(changedtick)
                || (!state.completed_ranges.iter().any(|completed| {
                    completed.changedtick == changedtick
                        && completed.start_row <= start_row
                        && completed.end_row >= end_row
                }) && !state.pending_tasks.values().any(|pending| {
                    pending.changedtick == changedtick
                        && pending.start_row <= start_row
                        && pending.end_row >= end_row
                }))
        })
    }

    pub(crate) fn begin_highlight(&mut self, buffer_id: u64, changedtick: u64) -> Arc<AtomicU64> {
        let state = self
            .buffers
            .entry(buffer_id)
            .or_insert_with(|| BufferHighlightState {
                changedtick: None,
                pending_tasks: HashMap::new(),
                completed_ranges: Vec::new(),
                style_cache: HashMap::new(),
            });
        if state.changedtick != Some(changedtick) {
            state.changedtick = Some(changedtick);
            state.pending_tasks.clear();
            state.completed_ranges.clear();
        }
        Arc::new(AtomicU64::new(0))
    }

    pub(crate) fn set_pending_task(
        &mut self,
        buffer_id: u64,
        task_id: TaskId,
        changedtick: u64,
        start_row: u32,
        end_row: u32,
    ) {
        if let Some(state) = self.buffers.get_mut(&buffer_id) {
            state.pending_tasks.insert(
                task_id,
                HighlightRange {
                    changedtick,
                    start_row,
                    end_row,
                },
            );
        }
    }

    pub(crate) fn apply_task_result(
        &mut self,
        task_id: TaskId,
        completed: HighlightTaskResult,
    ) -> bool {
        let Some(state) = self.buffers.get_mut(&completed.buffer_id) else {
            return false;
        };
        let Some(pending) = state.pending_tasks.remove(&task_id) else {
            return false;
        };
        if state.changedtick != Some(completed.changedtick)
            || pending.changedtick != completed.changedtick
            || pending.start_row != completed.start_row
            || pending.end_row != completed.end_row
        {
            return false;
        }
        let Some(highlights) = completed.highlights else {
            return false;
        };
        let start_row = highlights.start_row;
        let end_row = highlights.end_row();
        for (offset, spans) in highlights.rows.into_iter().enumerate() {
            state.style_cache.insert(start_row + offset as u32, spans);
        }
        state.completed_ranges.retain(|cached| {
            cached.changedtick == completed.changedtick
                && (cached.end_row <= start_row || cached.start_row >= end_row)
        });
        state.completed_ranges.push(HighlightRange {
            changedtick: completed.changedtick,
            start_row,
            end_row,
        });
        true
    }

    pub fn spans(&self, buffer_id: BufferId, row: u32) -> Option<&[HighlightSpan]> {
        self.buffers
            .get(&buffer_id.get())?
            .style_cache
            .get(&row)
            .map(Vec::as_slice)
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

    fn result(changedtick: u64, spans: Vec<HighlightSpan>) -> HighlightTaskResult {
        HighlightTaskResult {
            buffer_id: 1,
            changedtick,
            start_row: 0,
            end_row: 1,
            highlights: Some(HighlightSnapshot {
                changedtick,
                start_row: 0,
                rows: vec![spans],
            }),
        }
    }

    #[test]
    fn retains_style_cache_while_new_version_is_pending() {
        let mut service = HighlightService::new();
        service.begin_highlight(1, 7);
        service.set_pending_task(1, TaskId(1), 7, 0, 1);
        let span = HighlightSpan {
            start: 0,
            end: 1,
            scopes: vec!["source".to_owned()],
            foreground: [1, 2, 3],
        };
        assert!(service.apply_task_result(TaskId(1), result(7, vec![span.clone()])));

        service.begin_highlight(1, 8);
        service.set_pending_task(1, TaskId(2), 8, 0, 1);

        assert_eq!(
            service.spans(BufferId::new(1).unwrap(), 0),
            Some([span].as_slice())
        );
        assert!(service.is_highlighting());
    }

    #[test]
    fn applies_finished_result_while_another_task_is_pending() {
        let mut service = HighlightService::new();
        service.begin_highlight(1, 7);
        service.set_pending_task(1, TaskId(1), 7, 0, 32);
        service.set_pending_task(1, TaskId(2), 7, 32, 64);

        assert!(service.apply_task_result(
            TaskId(1),
            HighlightTaskResult {
                buffer_id: 1,
                changedtick: 7,
                start_row: 0,
                end_row: 32,
                highlights: Some(HighlightSnapshot {
                    changedtick: 7,
                    start_row: 0,
                    rows: vec![Vec::new(); 32],
                }),
            },
        ));
        assert!(service.is_highlighting());
        assert!(
            service
                .buffers
                .get(&1)
                .unwrap()
                .completed_ranges
                .iter()
                .any(|completed| completed.start_row == 0 && completed.end_row == 32)
        );
    }

    #[test]
    fn preserves_multiline_parse_state() {
        use clock::ReplicaId;
        use text::{Buffer, BufferId};
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
        use clock::ReplicaId;
        use text::{Buffer, BufferId};
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
        use clock::ReplicaId;
        use text::{Buffer, BufferId};
        let buffer = Buffer::new(
            ReplicaId::LOCAL,
            BufferId::new(1).unwrap(),
            "one\ntwo".to_owned(),
        );
        assert!(parse_scopes_cancellable(buffer.snapshot(), 0, None, 0, 2, || true).is_none());
    }
}
