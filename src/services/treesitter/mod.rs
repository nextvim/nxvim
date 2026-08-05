pub mod grammars;
pub mod tree_sitter;

use std::{
    collections::HashMap,
    sync::{Arc, atomic::AtomicU64},
};

use super::background::TaskId;
pub use grammars::Grammar;
pub use tree_sitter::{SyntaxNode, SyntaxTree, TreeSitterParser};

#[derive(Debug)]
pub(crate) struct ParseTaskResult {
    pub buffer_id: u64,
    pub changedtick: u64,
    pub grammar: Grammar,
    pub result: Result<SyntaxTree, String>,
}

struct BufferSyntaxState {
    grammar: Grammar,
    requested_changedtick: u64,
    applied_changedtick: Option<u64>,
    pending_task_id: Option<TaskId>,
    latest_task_id: Arc<AtomicU64>,
    syntax_tree: Option<SyntaxTree>,
    error: Option<String>,
}

/// Owns exactly one syntax tree and parse sequence per editor buffer.
pub struct TreeSitterService {
    buffers: HashMap<u64, BufferSyntaxState>,
}

impl TreeSitterService {
    pub fn new() -> Self {
        Self {
            buffers: HashMap::new(),
        }
    }

    pub(crate) fn should_parse(&self, buffer_id: u64, changedtick: u64, grammar: Grammar) -> bool {
        self.buffers.get(&buffer_id).is_none_or(|state| {
            state.grammar != grammar || state.requested_changedtick != changedtick
        })
    }

    pub(crate) fn begin_parse(
        &mut self,
        buffer_id: u64,
        changedtick: u64,
        grammar: Grammar,
    ) -> Arc<AtomicU64> {
        let state = self
            .buffers
            .entry(buffer_id)
            .or_insert_with(|| BufferSyntaxState {
                grammar,
                requested_changedtick: changedtick,
                applied_changedtick: None,
                pending_task_id: None,
                latest_task_id: Arc::new(AtomicU64::new(0)),
                syntax_tree: None,
                error: None,
            });
        if state.grammar != grammar {
            state.grammar = grammar;
            state.syntax_tree = None;
            state.applied_changedtick = None;
        }
        state.requested_changedtick = changedtick;
        state.error = None;
        state.latest_task_id.clone()
    }

    pub(crate) fn set_pending_task(&mut self, buffer_id: u64, task_id: TaskId) {
        if let Some(state) = self.buffers.get_mut(&buffer_id) {
            state.pending_task_id = Some(task_id);
        }
    }

    pub(crate) fn apply_task_result(
        &mut self,
        task_id: TaskId,
        completed: ParseTaskResult,
    ) -> bool {
        let Some(state) = self.buffers.get_mut(&completed.buffer_id) else {
            return false;
        };
        if state.pending_task_id != Some(task_id)
            || state.requested_changedtick != completed.changedtick
            || state.grammar != completed.grammar
        {
            return false;
        }
        state.pending_task_id = None;
        match completed.result {
            Ok(tree) => {
                state.syntax_tree = Some(tree);
                state.applied_changedtick = Some(completed.changedtick);
                state.error = None;
            }
            Err(error) => {
                state.error = Some(error);
            }
        }
        true
    }

    pub fn syntax_tree(&self, buffer_id: vim_buffer::BufferId) -> Option<&SyntaxTree> {
        self.buffers.get(&buffer_id.get())?.syntax_tree.as_ref()
    }

    pub fn grammar(&self, buffer_id: vim_buffer::BufferId) -> Option<Grammar> {
        Some(self.buffers.get(&buffer_id.get())?.grammar)
    }

    pub fn error(&self, buffer_id: vim_buffer::BufferId) -> Option<&str> {
        self.buffers.get(&buffer_id.get())?.error.as_deref()
    }

    pub fn is_parsing(&self, buffer_id: vim_buffer::BufferId) -> bool {
        self.buffers
            .get(&buffer_id.get())
            .is_some_and(|state| state.pending_task_id.is_some())
    }

    pub fn parsed_changedtick(&self, buffer_id: vim_buffer::BufferId) -> Option<u64> {
        self.buffers.get(&buffer_id.get())?.applied_changedtick
    }

    pub fn remove_buffer(&mut self, buffer_id: vim_buffer::BufferId) {
        self.buffers.remove(&buffer_id.get());
    }
}

impl Default for TreeSitterService {
    fn default() -> Self {
        Self::new()
    }
}

pub(crate) fn parse_snapshot(
    buffer_id: u64,
    changedtick: u64,
    grammar: Grammar,
    snapshot: text::BufferSnapshot,
) -> ParseTaskResult {
    parse_snapshot_cancellable(buffer_id, changedtick, grammar, snapshot, || false)
}

pub(crate) fn parse_snapshot_cancellable(
    buffer_id: u64,
    changedtick: u64,
    grammar: Grammar,
    snapshot: text::BufferSnapshot,
    mut is_cancelled: impl FnMut() -> bool,
) -> ParseTaskResult {
    let result = TreeSitterParser::new(grammar)
        .and_then(|mut parser| parser.parse_cancellable(&snapshot, None, &mut is_cancelled))
        .map_err(|error| error.to_string());
    ParseTaskResult {
        buffer_id,
        changedtick,
        grammar,
        result,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clock::ReplicaId;
    use text::{Buffer, BufferId as TextBufferId};

    fn parsed(buffer_id: u64, changedtick: u64, source: &str) -> ParseTaskResult {
        let buffer = Buffer::new(
            ReplicaId::LOCAL,
            TextBufferId::new(buffer_id).unwrap(),
            source,
        );
        parse_snapshot(
            buffer_id,
            changedtick,
            Grammar::Rust,
            buffer.snapshot().clone(),
        )
    }

    #[test]
    fn parse_state_is_shared_by_buffer_id() {
        let mut service = TreeSitterService::new();
        assert!(service.should_parse(7, 1, Grammar::Rust));
        service.begin_parse(7, 1, Grammar::Rust);
        service.set_pending_task(7, TaskId(1));
        assert!(!service.should_parse(7, 1, Grammar::Rust));
        assert!(service.should_parse(8, 1, Grammar::Rust));

        assert!(service.apply_task_result(TaskId(1), parsed(7, 1, "fn main() {}")));
        assert_eq!(
            service
                .buffers
                .get(&7)
                .unwrap()
                .syntax_tree
                .as_ref()
                .unwrap()
                .root_kind(),
            "source_file"
        );
    }

    #[test]
    fn stale_parse_does_not_replace_the_current_tree() {
        let mut service = TreeSitterService::new();
        service.begin_parse(7, 1, Grammar::Rust);
        service.set_pending_task(7, TaskId(1));
        service.begin_parse(7, 2, Grammar::Rust);
        service.set_pending_task(7, TaskId(2));

        assert!(!service.apply_task_result(TaskId(1), parsed(7, 1, "fn old() {}")));
        assert!(service.buffers.get(&7).unwrap().syntax_tree.is_none());
        assert!(service.apply_task_result(TaskId(2), parsed(7, 2, "fn current() {}")));
        assert_eq!(
            service.buffers.get(&7).unwrap().applied_changedtick,
            Some(2)
        );
    }
}
