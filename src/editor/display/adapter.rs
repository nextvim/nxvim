//! Renderer-facing views of the Vim buffer and document state.
//!
//! These adapters are deliberately read-only. They keep the renderer coupled to
//! `text` while allowing the editing model to be migrated to `vim-buffer` one
//! boundary at a time.

use crate::editor::document::VimDocument;
use text::{BufferSnapshot, Point};
use vim_buffer::{BufferError, BufferSnapshot as VimBufferSnapshot};
use vim_input::Mode;

/// An owned `text` snapshot suitable for existing renderer infrastructure.
///
/// `vim-buffer::BufferSnapshot` retains Vim metadata such as the buffer id and
/// changedtick. The renderer only needs the immutable text snapshot, so this
/// adapter makes that projection explicit instead of requiring renderer code
/// to reach through `as_inner()`.
#[derive(Clone)]
pub struct RendererBufferSnapshot {
    inner: BufferSnapshot,
}

impl RendererBufferSnapshot {
    pub fn new(snapshot: &VimBufferSnapshot) -> Self {
        Self {
            inner: snapshot.as_inner().clone(),
        }
    }

    pub fn as_inner(&self) -> &BufferSnapshot {
        &self.inner
    }

    pub fn into_inner(self) -> BufferSnapshot {
        self.inner
    }
}

impl From<&VimBufferSnapshot> for RendererBufferSnapshot {
    fn from(snapshot: &VimBufferSnapshot) -> Self {
        Self::new(snapshot)
    }
}

/// Immutable document state needed by a renderer during a draw/update pass.
///
/// This is intentionally a value object: callers can retain it while the Vim
/// document or buffer is being mutated elsewhere, just as they can retain a
/// normal text snapshot.
#[derive(Clone)]
pub struct RendererDocument {
    pub id: usize,
    pub snapshot: RendererBufferSnapshot,
    pub cursor: Point,
    pub folds: Vec<crate::editor::display::fold_map::Fold>,
    pub mode: Mode,
    pub show_gutter: bool,
    pub gutter_width: usize,
}

impl RendererDocument {
    pub fn snapshot(&self) -> &BufferSnapshot {
        self.snapshot.as_inner()
    }

    pub fn validate(&self) -> Result<(), BufferError> {
        let row_count = self.snapshot().row_count();
        if self.cursor.row >= row_count
            || self.cursor.column > self.snapshot().line_len(self.cursor.row)
        {
            return Err(BufferError::InvalidPoint(vim_buffer::Point::new(
                self.cursor.row,
                self.cursor.column,
            )));
        }
        Ok(())
    }
}

impl VimDocument {
    /// Build the renderer-facing projection for a Vim document.
    pub fn renderer_adapter(&self, snapshot: &VimBufferSnapshot) -> RendererDocument {
        RendererDocument {
            id: self.id,
            snapshot: RendererBufferSnapshot::from(snapshot),
            cursor: self.selections().point,
            folds: self
                .folds
                .iter()
                .map(|fold| crate::editor::display::fold_map::Fold {
                    start: fold.start,
                    end: fold.end,
                })
                .collect(),
            mode: self.mode(),
            show_gutter: self.show_gutter,
            gutter_width: self.gutter_width,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vim_buffer::{Buffer, BufferId};

    #[test]
    fn adapts_vim_snapshot_without_losing_text() {
        let buffer = Buffer::new(
            BufferId::new(1).unwrap(),
            clock::ReplicaId::LOCAL,
            "hello\nworld",
        );
        let adapted = RendererBufferSnapshot::from(&buffer.snapshot());

        assert_eq!(adapted.as_inner().row_count(), 2);
        assert_eq!(
            adapted.as_inner().text_for_range(0..5).collect::<String>(),
            "hello"
        );
    }

    #[test]
    fn adapts_document_render_state() {
        let buffer = Buffer::new(BufferId::new(1).unwrap(), clock::ReplicaId::LOCAL, "hello");
        let document = VimDocument::new(1, &buffer).unwrap();
        let adapted = document.renderer_adapter(&buffer.snapshot());

        assert_eq!(adapted.id, 1);
        assert_eq!(adapted.cursor, Point::new(0, 0));
        assert_eq!(adapted.mode, Mode::Normal);
        adapted.validate().unwrap();
    }
}
