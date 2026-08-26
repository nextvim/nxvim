use super::{BufferId, TabPageId, WindowId};
use vim_input::Mode;

/// Typed summary of one committed buffer transaction. The underlying
/// `vim-buffer` outcome remains authoritative for history and revisions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MutationOutcome {
    pub buffer: BufferId,
    pub changed_ranges: Vec<vim_buffer::TextRange>,
    pub changed_tick: vim_buffer::ChangedTick,
    pub cursor_changed: bool,
    pub selection_changed: bool,
    pub metadata_changed: bool,
    pub transaction: Option<text::TransactionId>,
}

impl MutationOutcome {
    pub(crate) fn from_buffer(outcome: vim_buffer::MutationOutcome) -> Self {
        Self {
            buffer: BufferId::new(outcome.buffer.get()).expect("buffer IDs are non-zero"),
            changed_ranges: outcome.edits.iter().map(|edit| edit.new_range).collect(),
            changed_tick: outcome.changedtick,
            cursor_changed: outcome.selections.is_some(),
            selection_changed: outcome.selections.is_some(),
            metadata_changed: outcome.modified_changed,
            transaction: outcome.transaction,
        }
    }

    /// Converts mutation facts into presentation work without choosing a
    /// particular window or renderer.
    pub fn invalidations(&self) -> Vec<RedrawInvalidation> {
        let ranges = self.changed_ranges.clone();
        let mut invalidations = vec![
            RedrawInvalidation::buffer(
                RedrawInvalidationKind::TextRows,
                self.buffer,
                ranges.clone(),
            ),
            RedrawInvalidation::buffer(
                RedrawInvalidationKind::DisplayMapTransforms,
                self.buffer,
                ranges.clone(),
            ),
            RedrawInvalidation::buffer(
                RedrawInvalidationKind::SyntaxHighlighting,
                self.buffer,
                ranges.clone(),
            ),
            RedrawInvalidation::buffer(RedrawInvalidationKind::Gutter, self.buffer, ranges),
        ];
        if self.cursor_changed {
            invalidations.push(RedrawInvalidation::buffer(
                RedrawInvalidationKind::Cursor,
                self.buffer,
                Vec::new(),
            ));
        }
        if self.selection_changed {
            invalidations.push(RedrawInvalidation::buffer(
                RedrawInvalidationKind::Selection,
                self.buffer,
                Vec::new(),
            ));
        }
        if self.metadata_changed {
            invalidations.push(RedrawInvalidation::global(
                RedrawInvalidationKind::Statusline,
            ));
        }
        invalidations
    }
}

/// Redraw work requested by a kernel command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RedrawRequest {
    None,
    View,
    Layout,
    Full,
}

impl Default for RedrawRequest {
    fn default() -> Self {
        Self::None
    }
}

/// The independently invalidatable parts of the presentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RedrawInvalidationKind {
    TextRows,
    DisplayMapTransforms,
    SyntaxHighlighting,
    Cursor,
    Selection,
    Gutter,
    Statusline,
    Tabline,
    Overlays,
    CompleteLayout,
}

/// Typed redraw work. IDs and ranges are owned so invalidations can cross the
/// controller and background-task boundaries without retaining editor borrows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RedrawInvalidation {
    pub kind: RedrawInvalidationKind,
    pub buffer: Option<BufferId>,
    pub window: Option<WindowId>,
    pub ranges: Vec<vim_buffer::TextRange>,
}

impl RedrawInvalidation {
    pub fn buffer(
        kind: RedrawInvalidationKind,
        buffer: BufferId,
        ranges: Vec<vim_buffer::TextRange>,
    ) -> Self {
        Self {
            kind,
            buffer: Some(buffer),
            window: None,
            ranges,
        }
    }

    pub fn window(kind: RedrawInvalidationKind, window: WindowId) -> Self {
        Self {
            kind,
            buffer: None,
            window: Some(window),
            ranges: Vec::new(),
        }
    }

    pub fn global(kind: RedrawInvalidationKind) -> Self {
        Self {
            kind,
            buffer: None,
            window: None,
            ranges: Vec::new(),
        }
    }
}

/// Semantic effects produced by kernel commands. Effects carry stable IDs or
/// owned payloads so they can cross callbacks without retaining editor borrows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandEffect {
    CursorMoved {
        window: WindowId,
    },
    BufferMutated {
        buffer: BufferId,
    },
    MutationCommitted(MutationOutcome),
    WindowChanged {
        window: WindowId,
    },
    TabChanged {
        tab: TabPageId,
    },
    OptionChanged {
        name: String,
    },
    EventEmitted {
        name: String,
        payload: Option<String>,
    },
    ModeChanged {
        from: Mode,
        to: Mode,
    },
    Message(String),
    QuitRequested,
    BackgroundWorkRequested {
        kind: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutcome {
    pub effects: Vec<CommandEffect>,
    pub redraw: RedrawRequest,
    pub invalidations: Vec<RedrawInvalidation>,
}

impl CommandOutcome {
    pub fn no_redraw() -> Self {
        Self {
            effects: Vec::new(),
            redraw: RedrawRequest::None,
            invalidations: Vec::new(),
        }
    }

    pub fn cursor_moved(window: WindowId) -> Self {
        Self {
            effects: vec![CommandEffect::CursorMoved { window }],
            redraw: RedrawRequest::View,
            invalidations: vec![RedrawInvalidation::window(
                RedrawInvalidationKind::Cursor,
                window,
            )],
        }
    }

    pub fn buffer_mutated(buffer: BufferId) -> Self {
        Self {
            effects: vec![CommandEffect::BufferMutated { buffer }],
            redraw: RedrawRequest::View,
            invalidations: vec![
                RedrawInvalidation::buffer(RedrawInvalidationKind::TextRows, buffer, Vec::new()),
                RedrawInvalidation::buffer(
                    RedrawInvalidationKind::DisplayMapTransforms,
                    buffer,
                    Vec::new(),
                ),
                RedrawInvalidation::buffer(
                    RedrawInvalidationKind::SyntaxHighlighting,
                    buffer,
                    Vec::new(),
                ),
                RedrawInvalidation::buffer(RedrawInvalidationKind::Gutter, buffer, Vec::new()),
            ],
        }
    }

    pub fn mutation_committed(mutation: MutationOutcome) -> Self {
        Self {
            invalidations: mutation.invalidations(),
            effects: vec![CommandEffect::MutationCommitted(mutation)],
            redraw: RedrawRequest::View,
        }
    }

    pub fn with_effect(effect: CommandEffect, redraw: RedrawRequest) -> Self {
        Self {
            effects: vec![effect],
            redraw,
            invalidations: Vec::new(),
        }
    }

    pub fn merge(&mut self, mut other: Self) {
        self.redraw = self.redraw.max(other.redraw);
        self.invalidations.append(&mut other.invalidations);
        self.effects.append(&mut other.effects);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_preserves_effect_order_and_strongest_redraw() {
        let mut outcome = CommandOutcome::with_effect(
            CommandEffect::Message("first".to_string()),
            RedrawRequest::View,
        );
        outcome.merge(CommandOutcome::with_effect(
            CommandEffect::QuitRequested,
            RedrawRequest::Full,
        ));

        assert_eq!(
            outcome.effects,
            vec![
                CommandEffect::Message("first".to_string()),
                CommandEffect::QuitRequested,
            ]
        );
        assert_eq!(outcome.redraw, RedrawRequest::Full);
    }
}
