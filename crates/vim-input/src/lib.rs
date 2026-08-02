//! Editor-agnostic Vim key-sequence resolution.

mod action;
mod key;
mod keymap;
mod resolver;

pub use action::{Action, Mode};
pub use key::{
    BindSequence, IntoKeySequence, Key, KeyCode, KeyParseError, KeyPattern, KeySequence, Modifiers,
};
pub use keymap::{BindingContext, Keymap};
pub use resolver::{InvalidSequence, PendingInput, ResolveOutcome, ResolvedAction, Resolver};
