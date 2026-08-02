use crate::{BufferId, BufferSnapshot, MutationOutcome};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum VimEvent {
    BufAdd,
    BufNew,
    BufReadPre,
    BufReadPost,
    BufEnter,
    BufLeave,
    BufHidden,
    BufUnload,
    BufDelete,
    BufWipeout,
    BufWritePre,
    BufWritePost,
    TextChanged,
    TextChangedI,
    OptionSet,
}

pub struct CallbackContext<'a> {
    pub buffer: BufferId,
    pub snapshot: &'a BufferSnapshot,
    pub outcome: Option<&'a MutationOutcome>,
    pub file: Option<&'a str>,
    pub matched: Option<&'a str>,
}

pub trait Callback: Send {
    fn call(&mut self, event: VimEvent, context: &CallbackContext<'_>);
}

#[derive(Default)]
pub struct CallbackRegistry {
    callbacks: Vec<Box<dyn Callback + Send>>,
    dispatching: bool,
}

impl CallbackRegistry {
    pub fn register(&mut self, callback: impl Callback + 'static) {
        self.callbacks.push(Box::new(callback));
    }

    pub fn is_dispatching(&self) -> bool {
        self.dispatching
    }

    pub fn dispatch(&mut self, event: VimEvent, context: &CallbackContext<'_>) {
        self.dispatching = true;
        for callback in &mut self.callbacks {
            callback.call(event, context);
        }
        self.dispatching = false;
    }
}
