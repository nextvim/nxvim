//! Editor-agnostic Vim key-sequence resolution.

mod action;
mod key;
mod keymap;
mod mapping;
mod resolver;

pub use action::{Action, Mode};
pub use key::{
    BindSequence, IntoKeySequence, Key, KeyCode, KeyParseError, KeyPattern, KeySequence, Modifiers,
    map_leader, set_map_leader,
};
pub use keymap::{BindingContext, Keymap};
pub use mapping::{
    Mapping, MappingExpansion, MappingFlags, MappingId, MappingMatch, MappingMode, MappingOrigin,
    MappingScope, MappingScriptContext, MappingStore, SharedMappingStore,
};
pub use resolver::{InvalidSequence, PendingInput, ResolveOutcome, ResolvedAction, Resolver};
