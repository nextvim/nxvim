pub mod grammars;
pub mod tree_sitter;

use std::{
    collections::HashMap,
    sync::{Arc, atomic::AtomicU64},
};

use background_worker::TaskId;

pub use grammars::Grammar;
pub use tree_sitter::{SyntaxNode, SyntaxTree, TreeSitterParser};

use vim_buffer::{BufferId, BufferSnapshot, ChangedTick};

#[derive(Debug)]
pub struct ParseTaskResult {
    pub buffer_id: BufferId,
    pub changedtick: ChangedTick,
    pub grammar: Grammar,
    pub result: Result<SyntaxTree, String>,
}

struct BufferSyntaxState {
    grammar: Grammar,
    requested_changedtick: ChangedTick,
    applied_changedtick: Option<ChangedTick>,
    pending_task_id: Option<TaskId>,
    latest_task_id: Arc<AtomicU64>,
    syntax_tree: Option<SyntaxTree>,
    error: Option<String>,
}

/// Owns exactly one syntax tree and parse sequence per editor buffer.
pub struct TreeSitterService {
    buffers: HashMap<BufferId, BufferSyntaxState>,
}

impl TreeSitterService {
    pub fn new() -> Self {
        Self {
            buffers: HashMap::new(),
        }
    }

    pub fn should_parse(&self, buffer_id: BufferId, changedtick: ChangedTick, grammar: Grammar) -> bool {
        self.buffers.get(&buffer_id).is_none_or(|state| {
            state.grammar != grammar || state.requested_changedtick != changedtick
        })
    }

    pub fn begin_parse(
        &mut self,
        buffer_id: BufferId,
        changedtick: ChangedTick,
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

    pub fn set_pending_task(&mut self, buffer_id: BufferId, task_id: TaskId) {
        if let Some(state) = self.buffers.get_mut(&buffer_id) {
            state.pending_task_id = Some(task_id);
        }
    }

    pub fn apply_task_result(
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

    pub fn syntax_tree(&self, buffer_id: BufferId) -> Option<&SyntaxTree> {
        self.buffers.get(&buffer_id)?.syntax_tree.as_ref()
    }

    pub fn grammar(&self, buffer_id: BufferId) -> Option<Grammar> {
        Some(self.buffers.get(&buffer_id)?.grammar)
    }

    pub fn error(&self, buffer_id: BufferId) -> Option<&str> {
        self.buffers.get(&buffer_id)?.error.as_deref()
    }

    pub fn is_parsing(&self, buffer_id: BufferId) -> bool {
        self.buffers
            .get(&buffer_id)
            .is_some_and(|state| state.pending_task_id.is_some())
    }

    pub fn parsed_changedtick(&self, buffer_id: BufferId) -> Option<ChangedTick> {
        self.buffers.get(&buffer_id)?.applied_changedtick
    }

    pub fn remove_buffer(&mut self, buffer_id: BufferId) {
        self.buffers.remove(&buffer_id);
    }

    pub fn initialize_from_parsed(
        &mut self,
        buffer_id: BufferId,
        changedtick: ChangedTick,
        grammar: Grammar,
        syntax_tree: SyntaxTree,
    ) {
        self.buffers.insert(
            buffer_id,
            BufferSyntaxState {
                grammar,
                requested_changedtick: changedtick,
                applied_changedtick: Some(changedtick),
                pending_task_id: None,
                latest_task_id: Arc::new(AtomicU64::new(0)),
                syntax_tree: Some(syntax_tree),
                error: None,
            },
        );
    }
}

impl Default for TreeSitterService {
    fn default() -> Self {
        Self::new()
    }
}

pub fn parse_snapshot(
    snapshot: BufferSnapshot,
    grammar: Grammar,
) -> ParseTaskResult {
    parse_snapshot_cancellable(snapshot, grammar, || false)
}

pub fn parse_snapshot_cancellable(
    snapshot: BufferSnapshot,
    grammar: Grammar,
    mut is_cancelled: impl FnMut() -> bool,
) -> ParseTaskResult {
    let buffer_id = snapshot.id();
    let changedtick = snapshot.changedtick();
    let result = TreeSitterParser::new(grammar)
        .and_then(|mut parser| parser.parse_cancellable(snapshot.as_inner(), None, &mut is_cancelled))
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
    use text::Buffer as TextBuffer;
    use vim_buffer::Buffer;

    fn parsed(buffer_id: BufferId, changedtick: ChangedTick, source: &str) -> ParseTaskResult {
        let mut buffer = Buffer::new(
            ReplicaId::LOCAL,
            buffer_id,
            source,
        );
        while buffer.changedtick() != changedtick {
            buffer.increment_changedtick();
        }
        parse_snapshot(
            buffer.snapshot().clone(),
            Grammar::Rust,
        )
    }

    #[test]
    fn parse_state_is_shared_by_buffer_id() {
        let mut service = TreeSitterService::new();
        let buf7 = BufferId::new(7).unwrap();
        let buf8 = BufferId::new(8).unwrap();
        let tick1 = ChangedTick::INITIAL;
        assert!(service.should_parse(buf7, tick1, Grammar::Rust));
        service.begin_parse(buf7, tick1, Grammar::Rust);
        service.set_pending_task(buf7, TaskId(1));
        assert!(!service.should_parse(buf7, tick1, Grammar::Rust));
        assert!(service.should_parse(buf8, tick1, Grammar::Rust));

        assert!(service.apply_task_result(TaskId(1), parsed(buf7, tick1, "fn main() {}")));
        assert_eq!(
            service
                .buffers
                .get(&buf7)
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
        let buf7 = BufferId::new(7).unwrap();
        let tick1 = ChangedTick::INITIAL;
        let tick2 = ChangedTick::new(1).unwrap_or(ChangedTick::INITIAL); // Let's just increment or use custom changedtick if helper is simple
        let tick2 = {
            let mut b = Buffer::new(ReplicaId::LOCAL, buf7, "");
            b.increment_changedtick();
            b.changedtick()
        };
        service.begin_parse(buf7, tick1, Grammar::Rust);
        service.set_pending_task(buf7, TaskId(1));
        service.begin_parse(buf7, tick2, Grammar::Rust);
        service.set_pending_task(buf7, TaskId(2));

        assert!(!service.apply_task_result(TaskId(1), parsed(buf7, tick1, "fn old() {}")));
        assert!(service.buffers.get(&buf7).unwrap().syntax_tree.is_none());
        assert!(service.apply_task_result(TaskId(2), parsed(buf7, tick2, "fn current() {}")));
        assert_eq!(
            service.buffers.get(&buf7).unwrap().applied_changedtick,
            Some(tick2)
        );
    }
}
