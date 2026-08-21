//! Editor-agnostic Vim key-sequence resolution.

mod action;
mod key;
mod keymap;
mod resolver;

pub use action::{Action, Mode};
pub use key::{
    set_map_leader, map_leader, BindSequence, IntoKeySequence, Key, KeyCode, KeyParseError, KeyPattern, KeySequence, Modifiers,
};
pub use keymap::{BindingContext, Keymap};
pub use resolver::{InvalidSequence, PendingInput, ResolveOutcome, ResolvedAction, Resolver};
