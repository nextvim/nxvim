//! Command dispatch: the single `match` `Editor::execute()` routes through.
//!
//! Organized by command family per `RESCUE.md` Rule 3 — `normal`, `insert`,
//! `visual` today; `search`/... as later milestones add them. Each family
//! module owns its own dispatch table so adding a command never requires
//! touching this file.

pub mod ex;
pub mod insert;
pub mod normal;
pub mod search;
pub mod substitute;
pub mod visual;

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
        Mode::Insert | Mode::Replace | Mode::VirtualReplace => {
            insert::dispatch(editor, ctx, action)
        }
        Mode::Visual(_) => visual::dispatch(editor, ctx, action),
        Mode::Command(_) => ex::dispatch(editor, ctx, action),
    }
}
