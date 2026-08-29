//! What `Editor::execute()` reports back to its caller.
//!
//! `Effect` grows real variants (clipboard, fs, script) once a milestone
//! needs one; today nothing produces one. `RedrawInvalidation` and
//! `Outcome::events` grew real shape in the "Operators + undo + events"
//! milestone: a redraw needs to know *what* changed, and every mutating
//! command must report the same `TextChanged` event, not a bespoke one.

use vim_buffer::{BufferId, MutationOutcome, TextRange};

use crate::kernel::events::EditorEvent;

/// What redraw work, if any, a command's effect requires.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RedrawInvalidation {
    #[default]
    None,
    /// App-owned presentation state changed for every window.
    All,
    /// Only the cursor moved; the window's content is unchanged.
    CurrentWindow,
    /// Text changed within `range` of `buffer`; a real redraw only needs to
    /// re-layout/re-paint that span, not the whole window.
    Range { buffer: BufferId, range: TextRange },
}

/// A side effect `app::` must carry out on the kernel's behalf. No variants
/// yet — nothing produces one until a milestone needs fs/clipboard/script
/// effects.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    Quit,
    FileSaved {
        path: std::path::PathBuf,
        bytes_written: u64,
    },
    FileSaveFailed {
        message: String,
    },
    OptionMessage {
        message: String,
    },
    ClipboardWrite {
        text: String,
        primary: bool,
    },
    ConfirmSubstitute {
        buffer: BufferId,
        match_range: TextRange,
        match_text: String,
        replacement: String,
    },
}

/// The result of one `Editor::execute()` call.
#[derive(Clone, Debug, Default)]
pub struct Outcome {
    /// Whether buffer text changed (always via `kernel::transaction`).
    pub mutated: bool,
    /// Whether `kernel::Mode` changed as a result of this command.
    pub mode_changed: bool,
    pub invalidation: RedrawInvalidation,
    pub effects: Vec<Effect>,
    pub events: Vec<EditorEvent>,
}

impl Outcome {
    /// Builds the `Outcome` every mutating command should report: a
    /// `Range` invalidation spanning the edited bytes and one
    /// `TextChanged` event. Kept in one place so `kernel::transaction`'s
    /// contract (undo grouping, events, redraw invalidation are uniform no
    /// matter what triggered the edit) is enforced by construction rather
    /// than by convention at each call site.
    pub fn from_mutation(mutation: &MutationOutcome) -> Self {
        let invalidation = match (mutation.edits.first(), mutation.edits.last()) {
            (Some(first), Some(last)) => RedrawInvalidation::Range {
                buffer: mutation.buffer,
                range: TextRange {
                    start: first.new_range.start,
                    end: last.new_range.end,
                },
            },
            _ => RedrawInvalidation::None,
        };
        Self {
            mutated: !mutation.edits.is_empty(),
            mode_changed: false,
            invalidation,
            effects: Vec::new(),
            events: vec![EditorEvent::TextChanged {
                buffer: mutation.buffer,
                tick: mutation.changedtick,
            }],
        }
    }
}
