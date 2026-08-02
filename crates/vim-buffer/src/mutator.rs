use crate::{
    BufferError, BufferId, BufferManager, CallbackRegistry, EditOrigin, MutationOutcome,
    SelectionSet,
};
use std::collections::VecDeque;

#[derive(Clone, Debug)]
pub enum Action {
    ApplyEdits {
        buffer: BufferId,
        origin: EditOrigin,
        selections: Option<SelectionSet>,
    },
    Undo {
        buffer: BufferId,
        count: u32,
    },
    Redo {
        buffer: BufferId,
        count: u32,
    },
}

#[derive(Default)]
pub struct Mutator {
    callbacks: CallbackRegistry,
    queued: VecDeque<Action>,
}

impl Mutator {
    pub fn callbacks_mut(&mut self) -> &mut CallbackRegistry {
        &mut self.callbacks
    }

    pub fn queue(&mut self, action: Action) {
        self.queued.push_back(action);
    }

    pub fn execute(
        &mut self,
        _manager: &mut BufferManager,
        _action: Action,
    ) -> Result<Option<MutationOutcome>, BufferError> {
        Err(BufferError::NotImplemented("action execution"))
    }

    pub fn queued_actions(&self) -> usize {
        self.queued.len()
    }
}
