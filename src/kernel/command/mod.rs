//! Command dispatch: the single `match` `Editor::execute()` routes through.
//!
//! Organized by command family per `RESCUE.md` Rule 3 — `normal`, `insert`
//! today; `visual`/`search`/`ex`/... as later milestones add them. Each
//! family module owns its own dispatch table so adding a command never
//! requires touching this file.

pub mod ex;
pub mod insert;
pub mod normal;

use vim_input::Action;

use crate::kernel::{
    Editor,
    ids::{BufferId, TabPageId, WindowId},
    mode::Mode,
    outcome::Outcome,
};

/// The buffer/window/tab a command executes against, resolved explicitly at
/// dispatch time rather than read from ambient "current" globals
/// (`RESCUE.md` Rule 4.8).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommandContext {
    pub buffer: BufferId,
    pub window: WindowId,
    pub tab: TabPageId,
}

pub fn dispatch(editor: &mut Editor, ctx: CommandContext, action: Action) -> Outcome {
    match editor.mode() {
        Mode::Normal => normal::dispatch(editor, ctx, action),
        Mode::Insert => insert::dispatch(editor, ctx, action),
        Mode::Command => ex::dispatch(editor, ctx, action),
    }
}
