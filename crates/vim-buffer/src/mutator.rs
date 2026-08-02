use crate::{
    BufferError, BufferId, BufferManager, CallbackContext, CallbackRegistry, EditOrigin,
    ManagerOutcome, MutationOutcome, PlannedEdit, SelectionSet, VimEvent,
};
use std::{collections::VecDeque, path::PathBuf};

#[derive(Clone, Debug)]
pub enum Action {
    Create {
        initial_text: String,
    },
    Load {
        path: PathBuf,
    },
    SetCurrent {
        buffer: BufferId,
    },
    Unload {
        buffer: BufferId,
        force: bool,
    },
    Delete {
        buffer: BufferId,
        force: bool,
    },
    Wipe {
        buffer: BufferId,
        force: bool,
    },
    ApplyEdits {
        buffer: BufferId,
        origin: EditOrigin,
        edits: Vec<PlannedEdit>,
        selections: Option<SelectionSet>,
        join_previous: bool,
    },
    Undo {
        buffer: BufferId,
        count: u32,
    },
    Redo {
        buffer: BufferId,
        count: u32,
    },
    Save {
        buffer: BufferId,
        path: Option<PathBuf>,
        force: bool,
    },
    SetOptions {
        buffer: BufferId,
        options: crate::BufferOptions,
    },
}

#[derive(Clone, Debug)]
pub enum ActionOutcome {
    Manager(ManagerOutcome),
    Mutation(Option<MutationOutcome>),
    Save(crate::SaveOutcome),
    Options(Option<crate::OptionsOutcome>),
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
        manager: &mut BufferManager,
        action: Action,
    ) -> Result<ActionOutcome, BufferError> {
        match action {
            Action::Create { initial_text } => {
                let id = manager.create(initial_text).id();
                let outcome = ManagerOutcome::Added(id);
                self.dispatch(manager, VimEvent::BufNew, id, None)?;
                self.dispatch(manager, VimEvent::BufAdd, id, None)?;
                Ok(ActionOutcome::Manager(outcome))
            }
            Action::Load { path } => {
                let (id, outcome) = manager.load(path)?;
                if matches!(outcome, ManagerOutcome::Loaded(_)) {
                    self.dispatch(manager, VimEvent::BufNew, id, None)?;
                    self.dispatch(manager, VimEvent::BufAdd, id, None)?;
                    self.dispatch(manager, VimEvent::BufReadPre, id, None)?;
                    self.dispatch(manager, VimEvent::BufReadPost, id, None)?;
                }
                Ok(ActionOutcome::Manager(outcome))
            }
            Action::SetCurrent { buffer } => self.set_current(manager, buffer),
            Action::Unload { buffer, force } => {
                self.destroy(manager, buffer, force, DestructiveAction::Unload)
            }
            Action::Delete { buffer, force } => {
                self.destroy(manager, buffer, force, DestructiveAction::Delete)
            }
            Action::Wipe { buffer, force } => {
                self.destroy(manager, buffer, force, DestructiveAction::Wipe)
            }
            Action::Undo { buffer, count } => {
                let mut last = None;
                for _ in 0..count {
                    let Some(outcome) = manager.get_mut(buffer)?.undo()? else {
                        break;
                    };
                    self.dispatch(manager, VimEvent::TextChanged, buffer, Some(&outcome))?;
                    last = Some(outcome);
                }
                Ok(ActionOutcome::Mutation(last))
            }
            Action::Redo { buffer, count } => {
                let mut last = None;
                for _ in 0..count {
                    let Some(outcome) = manager.get_mut(buffer)?.redo()? else {
                        break;
                    };
                    self.dispatch(manager, VimEvent::TextChanged, buffer, Some(&outcome))?;
                    last = Some(outcome);
                }
                Ok(ActionOutcome::Mutation(last))
            }
            Action::Save {
                buffer,
                path,
                force,
            } => {
                self.dispatch(manager, VimEvent::BufWritePre, buffer, None)?;
                let outcome = match path {
                    Some(path) => manager.save_as(buffer, path, force)?,
                    None => manager.save(buffer, force)?,
                };
                self.dispatch(manager, VimEvent::BufWritePost, buffer, None)?;
                Ok(ActionOutcome::Save(outcome))
            }
            Action::SetOptions { buffer, options } => {
                let outcome = manager.get_mut(buffer)?.set_options(options)?;
                if outcome.is_some() {
                    self.dispatch(manager, VimEvent::OptionSet, buffer, None)?;
                }
                Ok(ActionOutcome::Options(outcome))
            }
            Action::ApplyEdits {
                buffer,
                origin,
                edits,
                selections,
                join_previous,
            } => self.apply_edits(manager, buffer, origin, edits, selections, join_previous),
        }
    }

    /// Commits a batch of edits to the buffer identified by `buffer` and
    /// synchronously dispatches the corresponding Vim text-change callback.
    ///
    /// Every edit is resolved against the same pre-edit snapshot and the batch
    /// is atomic: validation failure leaves the buffer unchanged.
    pub fn apply_edits(
        &mut self,
        manager: &mut BufferManager,
        buffer: BufferId,
        origin: EditOrigin,
        edits: impl IntoIterator<Item = PlannedEdit>,
        selections: Option<SelectionSet>,
        join_previous: bool,
    ) -> Result<ActionOutcome, BufferError> {
        let mut transaction = manager.get_mut(buffer)?.transaction(origin);
        for edit in edits {
            transaction.push(edit);
        }
        if join_previous {
            transaction.join_previous();
        }
        let outcome = transaction.commit(selections)?;
        if !outcome.edits.is_empty() {
            self.dispatch(manager, text_changed_event(origin), buffer, Some(&outcome))?;
        }
        Ok(ActionOutcome::Mutation(Some(outcome)))
    }

    pub fn execute_queued(
        &mut self,
        manager: &mut BufferManager,
    ) -> Result<Vec<ActionOutcome>, BufferError> {
        let mut outcomes = Vec::new();
        while let Some(action) = self.queued.pop_front() {
            outcomes.push(self.execute(manager, action)?);
        }
        Ok(outcomes)
    }

    fn set_current(
        &mut self,
        manager: &mut BufferManager,
        buffer: BufferId,
    ) -> Result<ActionOutcome, BufferError> {
        manager.get(buffer)?;
        let old = manager.current();
        if old == Some(buffer) {
            return Ok(ActionOutcome::Manager(manager.set_current(buffer)?));
        }
        if let Some(old) = old {
            self.dispatch(manager, VimEvent::BufLeave, old, None)?;
        }
        let outcome = manager.set_current(buffer)?;
        if let Some(old) = old {
            self.dispatch(manager, VimEvent::BufHidden, old, None)?;
        }
        self.dispatch(manager, VimEvent::BufEnter, buffer, None)?;
        Ok(ActionOutcome::Manager(outcome))
    }

    fn destroy(
        &mut self,
        manager: &mut BufferManager,
        buffer: BufferId,
        force: bool,
        action: DestructiveAction,
    ) -> Result<ActionOutcome, BufferError> {
        let target = manager.get(buffer)?;
        if target.is_modified() && !force {
            return Err(BufferError::ModifiedBuffer(buffer));
        }
        let was_loaded = target.is_loaded();
        let was_current = manager.current() == Some(buffer);
        let snapshot = target.snapshot();
        let file = target
            .path()
            .map(|path| path.to_string_lossy().into_owned());

        if was_current {
            self.dispatch_snapshot(VimEvent::BufLeave, buffer, &snapshot, file.as_deref(), None);
        }
        let outcome = match action {
            DestructiveAction::Unload => manager.unload(buffer, force)?,
            DestructiveAction::Delete => manager.delete(buffer, force)?,
            DestructiveAction::Wipe => manager.wipe(buffer, force)?,
        };
        if was_loaded {
            self.dispatch_snapshot(
                VimEvent::BufUnload,
                buffer,
                &snapshot,
                file.as_deref(),
                None,
            );
        }
        if matches!(action, DestructiveAction::Delete | DestructiveAction::Wipe) {
            self.dispatch_snapshot(
                VimEvent::BufDelete,
                buffer,
                &snapshot,
                file.as_deref(),
                None,
            );
        }
        if matches!(action, DestructiveAction::Wipe) {
            self.dispatch_snapshot(
                VimEvent::BufWipeout,
                buffer,
                &snapshot,
                file.as_deref(),
                None,
            );
        }
        if was_current {
            let replacement = manager
                .current()
                .expect("manager selects a replacement for the current buffer");
            self.dispatch(manager, VimEvent::BufEnter, replacement, None)?;
        }
        Ok(ActionOutcome::Manager(outcome))
    }

    fn dispatch(
        &mut self,
        manager: &BufferManager,
        event: VimEvent,
        buffer: BufferId,
        outcome: Option<&MutationOutcome>,
    ) -> Result<(), BufferError> {
        let buffer = manager.get(buffer)?;
        let snapshot = buffer.snapshot();
        let file = buffer
            .path()
            .map(|path| path.to_string_lossy().into_owned());
        self.dispatch_snapshot(event, buffer.id(), &snapshot, file.as_deref(), outcome);
        Ok(())
    }

    fn dispatch_snapshot(
        &mut self,
        event: VimEvent,
        buffer: BufferId,
        snapshot: &crate::BufferSnapshot,
        file: Option<&str>,
        outcome: Option<&MutationOutcome>,
    ) {
        self.callbacks.dispatch(
            event,
            &CallbackContext {
                buffer,
                snapshot,
                outcome,
                file,
                matched: file,
            },
        );
    }

    pub fn queued_actions(&self) -> usize {
        self.queued.len()
    }
}

fn text_changed_event(origin: EditOrigin) -> VimEvent {
    if origin == EditOrigin::InsertMode {
        VimEvent::TextChangedI
    } else {
        VimEvent::TextChanged
    }
}

#[derive(Clone, Copy)]
enum DestructiveAction {
    Unload,
    Delete,
    Wipe,
}
