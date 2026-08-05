use std::{
    collections::HashMap,
    sync::{Arc, atomic::AtomicU64},
};

use vim_buffer::BufferId;

use super::background::TaskId;
use crate::display::highlight::{HighlightSnapshot, HighlightSpan};

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
}
