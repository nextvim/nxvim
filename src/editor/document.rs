//! Document state backed by `vim-buffer`.
//!
//! This document owns Vim-facing state while rendering remains outside the
//! `vim-buffer` crate. Fold ranges are stored as document coordinates so a
//! renderer can consume them without becoming part of the editing model.

use crate::editor::display::display_map::DisplayMap;
use crate::editor::display::highlight::Highlights;
use crate::editor::selections::VimSelectionCollection;
use std::sync::{Arc, atomic::AtomicU64};
use text::{Point, Selection, SelectionGoal};
use vim_buffer::{
    Buffer, BufferError, BufferSnapshot, ByteOffset, EditOrigin, MutationOutcome, Revision,
    SelectionSet, TextRange, VimSelection,
};
use vim_input::{Action, Mode};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VimFold {
    pub start: Point,
    pub end: Point,
}

pub struct VimDocument {
    pub id: usize,
    selections: VimSelectionCollection,
    mode: Mode,
    pub should_sync: bool,
    pub folds: Vec<VimFold>,
    pub show_gutter: bool,
    pub gutter_width: usize,
    pub show_pattern_match: bool,
    pub show_scrollbar: bool,
    pub display_map: DisplayMap,
    pub hl: Highlights,
    pub latest_hl_task_id: Arc<AtomicU64>,
    pub latest_wrap_task_id: Arc<AtomicU64>,
    pub latest_parse_task_id: Arc<AtomicU64>,
    pub latest_index_task_id: Arc<AtomicU64>,
    pub current_hl_task_id: u64,
    pub current_wrap_task_id: u64,
    pub current_parse_task_id: u64,
    pub current_index_task_id: u64,
    sync_revision: Option<Revision>,
}

impl VimDocument {
    pub fn new(id: usize, buffer: &Buffer) -> Result<Self, BufferError> {
        Self::new_with_file_path(id, buffer, "")
    }

    pub fn new_with_buffer(
        id: usize,
        buffer: &Buffer,
        file_path: &str,
    ) -> Result<Self, BufferError> {
        Self::new_with_file_path(id, buffer, file_path)
    }

    pub fn new_with_file_path(
        id: usize,
        buffer: &Buffer,
        file_path: &str,
    ) -> Result<Self, BufferError> {
        let snapshot = buffer.snapshot();
        let mut selections = VimSelectionCollection::new();
        selections.add_caret(&snapshot, 0)?;
        Ok(Self {
            id,
            selections,
            mode: Mode::Normal,
            should_sync: true,
            folds: Vec::new(),
            show_gutter: true,
            gutter_width: 0,
            show_pattern_match: true,
            show_scrollbar: true,
            display_map: DisplayMap::new(snapshot.as_inner().clone(), None),
            hl: Highlights::new(file_path),
            latest_hl_task_id: Arc::new(AtomicU64::new(0)),
            latest_wrap_task_id: Arc::new(AtomicU64::new(0)),
            latest_parse_task_id: Arc::new(AtomicU64::new(0)),
            latest_index_task_id: Arc::new(AtomicU64::new(0)),
            current_hl_task_id: 0,
            current_wrap_task_id: 0,
            current_parse_task_id: 0,
            current_index_task_id: 0,
            sync_revision: None,
        })
    }

    pub fn clear(&mut self, buffer: &Buffer) -> Result<(), BufferError> {
        let snapshot = buffer.snapshot();
        self.selections = VimSelectionCollection::new();
        self.selections.add_caret(&snapshot, 0)?;
        self.mode = Mode::Normal;
        self.folds.clear();
        self.gutter_width = 0;
        self.display_map = DisplayMap::new(snapshot.as_inner().clone(), None);
        self.hl.clear();
        self.current_hl_task_id = 0;
        self.current_wrap_task_id = 0;
        self.current_parse_task_id = 0;
        self.current_index_task_id = 0;
        self.sync_revision = None;
        self.should_sync = true;
        Ok(())
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// Compatibility name used by the former document API.
    pub fn current_mode(&self) -> Mode {
        self.mode()
    }

    pub fn set_mode(&mut self, mode: Mode) {
        self.mode = mode;
    }

    pub fn should_sync(&self) -> bool {
        self.should_sync
    }

    pub fn mark_synced(&mut self) {
        self.should_sync = false;
        self.sync_revision = None;
    }

    pub fn mark_synced_at(&mut self, buffer: &Buffer) {
        self.should_sync = false;
        self.sync_revision = Some(buffer.snapshot().revision().clone());
    }

    pub fn needs_sync(&self, buffer: &Buffer) -> bool {
        self.should_sync
            || self
                .sync_revision
                .as_ref()
                .map_or(true, |revision| revision != buffer.snapshot().revision())
    }

    pub fn sync_revision(&self) -> Option<&Revision> {
        self.sync_revision.as_ref()
    }

    pub fn snapshot(&self, buffer: &Buffer) -> BufferSnapshot {
        buffer.snapshot()
    }

    pub fn selections(&self) -> &VimSelectionCollection {
        &self.selections
    }

    pub fn selections_mut(&mut self) -> &mut VimSelectionCollection {
        &mut self.selections
    }

    pub fn selection_set(&self) -> Result<SelectionSet, BufferError> {
        self.selections.selection_set()
    }

    /// Returns the primary Vim selection.
    pub fn selection(&self) -> Option<VimSelection> {
        self.selections.first().cloned()
    }

    /// Adds a caret selection at the start of the buffer.
    pub fn add_selection(&mut self, buffer: &Buffer) -> Result<VimSelection, BufferError> {
        self.selections.add_caret(&buffer.snapshot(), 0)
    }

    pub fn clear_selections(&mut self, buffer: &Buffer) {
        self.selections.clear_selections(&buffer.snapshot());
    }

    pub fn has_selection(&self, buffer: &Buffer) -> bool {
        self.selections.has_selection(&buffer.snapshot())
    }

    pub fn selection_text(&self, buffer: &Buffer) -> Result<String, BufferError> {
        self.selections.text(&buffer.snapshot())
    }

    pub fn set_mark(
        &mut self,
        buffer: &mut Buffer,
        name: char,
        offset: ByteOffset,
    ) -> Result<(), BufferError> {
        buffer.set_mark(name, offset)
    }

    pub fn mark_offset(&self, buffer: &Buffer, name: char) -> Option<ByteOffset> {
        buffer.resolve_mark(name)
    }

    pub fn delete_mark(&mut self, buffer: &mut Buffer, name: char) -> Result<bool, BufferError> {
        buffer.delete_mark(name)
    }

    pub fn new_line(&self, buffer: &Buffer) -> &'static str {
        match buffer.snapshot().line_ending() {
            text::LineEnding::Unix => "\n",
            text::LineEnding::Windows => "\r\n",
        }
    }

    pub fn enter_mode(&mut self, buffer: &Buffer, mode: Mode) {
        if self.mode == mode {
            self.clear_selections(buffer);
            return;
        }
        self.mode = mode;
        self.should_sync = true;
    }

    pub fn sync(&mut self, _buffer: &Buffer) {
        self.should_sync = true;
    }

    pub fn select_similar(&mut self, buffer: &Buffer) -> Result<bool, BufferError> {
        let snapshot = buffer.snapshot();
        if !self.has_selection(buffer) {
            self.selections.move_to_word(&snapshot, false)?;
            self.selections.move_to_word_end(&snapshot, true)?;
            return Ok(true);
        }
        let text = self.selection_text(buffer)?;
        self.selections.move_to_next_match(&snapshot, &text)
    }

    pub fn fold(&mut self, start: Point, end: Point) {
        let fold = VimFold { start, end };
        if start < end && !self.folds.contains(&fold) {
            self.folds.push(fold);
            self.should_sync = true;
        }
    }

    /// Compatibility wrapper for the former syntax-aware unfold API.
    pub fn unfold(
        &mut self,
        buffer: &Buffer,
        _count: u32,
        _editor: &crate::editor::Editor,
        _syntax_tree: Option<&crate::services::treesitter::SyntaxTree>,
    ) -> Result<usize, BufferError> {
        self.unfold_at(buffer)
    }

    /// Adds a fold around the syntax block containing the primary caret.
    pub fn fold_from_syntax(
        &mut self,
        buffer: &Buffer,
        _count: u32,
        editor: &crate::editor::Editor,
        syntax_tree: Option<&crate::services::treesitter::SyntaxTree>,
    ) -> Result<bool, BufferError> {
        let Some(tree) = syntax_tree else {
            return Ok(false);
        };
        let snapshot = buffer.snapshot();
        let Some(selection) = self.selection() else {
            return Ok(false);
        };
        let head = selection.head_offset(&snapshot)?.0;
        let Some(block) = tree.enclosing_block_at_byte(head) else {
            return Ok(false);
        };
        let start = snapshot.offset_to_point(ByteOffset(block.byte_range.start))?;
        let end = snapshot.offset_to_point(ByteOffset(block.byte_range.end))?;
        if editor.fold_multiline_only && start.row >= end.row {
            return Ok(false);
        }
        self.fold(start, end);
        Ok(true)
    }

    pub fn unfold_at(&mut self, buffer: &Buffer) -> Result<usize, BufferError> {
        let snapshot = buffer.snapshot();
        let mut removed = 0;
        self.folds.retain(|fold| {
            let contains = self
                .selections
                .selections
                .iter()
                .filter_map(|selection| selection.head_offset(&snapshot).ok())
                .filter_map(|offset| snapshot.offset_to_point(offset).ok())
                .any(|point| {
                    (point >= fold.start && point <= fold.end) || point.row == fold.start.row
                });
            if contains {
                removed += 1;
            }
            !contains
        });
        if removed > 0 {
            self.should_sync = true;
        }
        Ok(removed)
    }

    /// Move selections out of folded ranges before applying a motion.
    pub fn snap_selections_to_folds(
        &mut self,
        buffer: &Buffer,
        moving_right: bool,
        is_move_right: bool,
    ) -> Result<(), BufferError> {
        if self.folds.is_empty() {
            return Ok(());
        }
        let snapshot = buffer.snapshot();
        let current = self.selections.selections.clone();
        for selection in current {
            let head_offset = selection.head_offset(&snapshot)?;
            let head = snapshot.offset_to_point(head_offset)?;
            let Some(fold) = self
                .folds
                .iter()
                .find(|fold| head >= fold.start && head < fold.end)
            else {
                continue;
            };
            let target = if moving_right {
                if is_move_right && head == fold.start {
                    fold.start
                } else {
                    fold.end
                }
            } else {
                fold.start
            };
            if target == head {
                continue;
            }
            let tail_offset = snapshot.as_inner().offset_for_anchor(&selection.anchor());
            let tail = snapshot.offset_to_point(ByteOffset(tail_offset))?;
            let anchor = if tail == head { target } else { tail };
            let moved = VimSelection::new(
                Selection {
                    id: selection.id().get(),
                    start: snapshot
                        .as_inner()
                        .anchor_before(snapshot.point_to_offset(anchor)?.0),
                    end: snapshot
                        .as_inner()
                        .anchor_before(snapshot.point_to_offset(target)?.0),
                    reversed: target < anchor,
                    goal: SelectionGoal::None,
                },
                selection.kind(),
                selection.is_inclusive(),
            );
            self.selections.replace(moved);
        }
        Ok(())
    }

    /// Applies the core editing and motion actions against Vim buffer state.
    ///
    /// Higher-level actions (yank registers, syntax-aware text objects, and
    /// paste) remain owned by their respective services, but the document
    /// boundary is now the single entry point for ordinary editing actions.
    pub fn apply_action(
        &mut self,
        buffer: &mut Buffer,
        action: &Action,
    ) -> Result<(), BufferError> {
        let snapshot = buffer.snapshot();
        match action {
            Action::Clear => self.clear_selections(buffer),
            Action::SetToNormal => self.enter_mode(buffer, Mode::Normal),
            Action::SetToInsert => self.enter_mode(buffer, Mode::Insert),
            Action::SetToVisual => self.enter_mode(buffer, Mode::Visual),
            Action::SetToVisualLine => self.enter_mode(buffer, Mode::VisualLine),
            Action::SetToVisualBlock => self.enter_mode(buffer, Mode::VisualBlock),
            Action::SetToCommand
            | Action::SetToCommandSearchForward
            | Action::SetToCommandSearchBackward => self.enter_mode(buffer, Mode::Command),
            Action::InsertText(text) => {
                let selection = self
                    .selection()
                    .ok_or(BufferError::InvalidLifecycleTransition)?;
                let head = selection.head_offset(&snapshot)?;
                let anchor = snapshot.as_inner().offset_for_anchor(&selection.anchor());
                if head.0 != anchor {
                    self.delete(
                        buffer,
                        TextRange {
                            start: ByteOffset(head.0.min(anchor)),
                            end: ByteOffset(head.0.max(anchor)),
                        },
                    )?;
                }
                self.insert(buffer, head, text.clone())?;
            }
            Action::Undo { count } => {
                self.undo(buffer, *count)?;
            }
            Action::Redo { count } => {
                self.redo(buffer, *count)?;
            }
            Action::MoveLeft { count, select } => {
                self.selections.move_left(&snapshot, *count, *select)?;
            }
            Action::MoveRight { count, select } => {
                self.selections.move_right(&snapshot, *count, *select)?;
            }
            Action::MoveUp { count, select } => {
                self.selections.move_up(&snapshot, *count, *select)?;
            }
            Action::MoveDown { count, select } => {
                self.selections.move_down(&snapshot, *count, *select)?;
            }
            Action::MoveToStartOfDocument { select, .. } => {
                self.selections
                    .move_to_start_of_document(&snapshot, *select)?;
            }
            Action::MoveToEndOfDocument { select, .. } => {
                self.selections
                    .move_to_end_of_document(&snapshot, *select)?;
            }
            Action::MoveToStartOfLine { select, .. } => {
                self.selections.move_to_start_of_line(&snapshot, *select)?;
            }
            Action::MoveToEndOfLine { select, .. } => {
                self.selections.move_to_end_of_line(&snapshot, *select)?;
            }
            Action::MoveToWord { select, count } => {
                for _ in 0..*count {
                    self.selections.move_to_word(&snapshot, *select)?;
                }
            }
            Action::MoveToWordEnd { select, count } => {
                for _ in 0..*count {
                    self.selections.move_to_word_end(&snapshot, *select)?;
                }
            }
            Action::DeleteChar { count } | Action::Delete { count } => {
                let selection = self
                    .selection()
                    .ok_or(BufferError::InvalidLifecycleTransition)?;
                let head = selection.head_offset(&snapshot)?;
                let end = ByteOffset((head.0 + *count as usize).min(snapshot.len_bytes()));
                self.delete(buffer, TextRange { start: head, end })?;
            }
            Action::DeleteLine { count } => {
                self.delete_current_line(buffer, *count)?;
            }
            _ => {}
        }
        self.should_sync = true;
        Ok(())
    }

    pub fn delete_current_line(
        &mut self,
        buffer: &mut Buffer,
        count: u32,
    ) -> Result<MutationOutcome, BufferError> {
        let snapshot = buffer.snapshot();
        let row = self
            .selections
            .first()
            .and_then(|selection| selection.head_offset(&snapshot).ok())
            .and_then(|offset| snapshot.offset_to_point(offset).ok())
            .map_or(0, |point| point.row);
        let count = count.max(1);
        let start = snapshot.point_to_offset(Point::new(row, 0))?;
        let end_row = (row + count - 1).min(snapshot.row_count().saturating_sub(1));
        let mut end = snapshot.point_to_offset(Point::new(end_row, snapshot.line_len(end_row)?))?;
        if end_row + 1 < snapshot.row_count() {
            end = ByteOffset(end.0 + 1);
        }
        self.delete(buffer, TextRange { start, end })
    }

    pub fn undo(&mut self, buffer: &mut Buffer, count: u32) -> Result<usize, BufferError> {
        let mut applied = 0;
        for _ in 0..count {
            if buffer.undo()?.is_some() {
                applied += 1;
            } else {
                break;
            }
        }
        self.should_sync = applied > 0;
        Ok(applied)
    }

    pub fn redo(&mut self, buffer: &mut Buffer, count: u32) -> Result<usize, BufferError> {
        let mut applied = 0;
        for _ in 0..count {
            if buffer.redo()?.is_some() {
                applied += 1;
            } else {
                break;
            }
        }
        self.should_sync = applied > 0;
        Ok(applied)
    }

    pub fn insert(
        &mut self,
        buffer: &mut Buffer,
        offset: ByteOffset,
        text: impl Into<std::sync::Arc<str>>,
    ) -> Result<MutationOutcome, BufferError> {
        self.replace(
            buffer,
            TextRange {
                start: offset,
                end: offset,
            },
            text,
        )
    }

    pub fn replace(
        &mut self,
        buffer: &mut Buffer,
        range: TextRange,
        text: impl Into<std::sync::Arc<str>>,
    ) -> Result<MutationOutcome, BufferError> {
        let snapshot = buffer.snapshot();
        let selections = self.selection_set().ok();
        let mut transaction = buffer.transaction(EditOrigin::User);
        transaction.replace(None, range, text);
        let outcome = transaction.commit(selections)?;
        self.should_sync = true;
        let _ = snapshot;
        Ok(outcome)
    }

    pub fn delete(
        &mut self,
        buffer: &mut Buffer,
        range: TextRange,
    ) -> Result<MutationOutcome, BufferError> {
        let selections = self.selection_set().ok();
        let mut transaction = buffer.transaction(EditOrigin::User);
        transaction.delete(None, range);
        let outcome = transaction.commit(selections)?;
        self.should_sync = true;
        Ok(outcome)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buffer() -> Buffer {
        Buffer::new(
            vim_buffer::BufferId::new(1).unwrap(),
            clock::ReplicaId::default(),
            "hello",
        )
    }

    #[test]
    fn starts_with_a_normal_mode_caret() {
        let buffer = buffer();
        let document = VimDocument::new(7, &buffer).unwrap();
        assert_eq!(document.id, 7);
        assert_eq!(document.mode(), Mode::Normal);
        assert_eq!(document.selections().selections.len(), 1);
    }

    #[test]
    fn edit_uses_vim_buffer_transaction() {
        let mut buffer = buffer();
        let mut document = VimDocument::new(1, &buffer).unwrap();
        document
            .insert(&mut buffer, ByteOffset(5), " world")
            .unwrap();
        assert_eq!(
            buffer.snapshot().chunks().collect::<String>(),
            "hello world"
        );
        assert!(buffer.changedtick().get() > 0);
    }

    #[test]
    fn folds_snap_selections_and_unfold_at_the_caret() {
        let buffer = Buffer::new(
            vim_buffer::BufferId::new(2).unwrap(),
            clock::ReplicaId::default(),
            "one\ntwo\nthree",
        );
        let mut document = VimDocument::new(2, &buffer).unwrap();
        document
            .selections_mut()
            .clear_selections(&buffer.snapshot());
        document
            .selections_mut()
            .add_caret(&buffer.snapshot(), 1)
            .unwrap();
        document.fold(Point::new(0, 0), Point::new(2, 0));
        assert_eq!(document.folds.len(), 1);

        document
            .snap_selections_to_folds(&buffer, true, false)
            .unwrap();
        let head = document
            .selections()
            .first()
            .unwrap()
            .head_offset(&buffer.snapshot())
            .unwrap();
        assert_eq!(head.0, 8);

        assert_eq!(document.unfold_at(&buffer).unwrap(), 1);
        assert!(document.folds.is_empty());
        assert!(document.should_sync());
    }

    #[test]
    fn migrated_document_api_covers_modes_motion_and_line_deletion() {
        let mut buffer = Buffer::new(
            vim_buffer::BufferId::new(3).unwrap(),
            clock::ReplicaId::default(),
            "one\ntwo\nthree",
        );
        let mut document = VimDocument::new(3, &buffer).unwrap();
        assert_eq!(document.new_line(&buffer), "\n");
        document.enter_mode(&buffer, Mode::Insert);
        assert_eq!(document.current_mode(), Mode::Insert);
        document
            .apply_action(
                &mut buffer,
                &Action::MoveDown {
                    count: 1,
                    select: false,
                },
            )
            .unwrap();
        assert_eq!(document.selections().point.row, 1);
        document.delete_current_line(&mut buffer, 1).unwrap();
        assert_eq!(buffer.snapshot().chunks().collect::<String>(), "one\nthree");
    }

    #[test]
    fn marks_and_revision_sync_delegate_to_vim_buffer() {
        let mut buffer = buffer();
        let mut document = VimDocument::new(9, &buffer).unwrap();
        document.set_mark(&mut buffer, 'a', ByteOffset(2)).unwrap();
        assert_eq!(document.mark_offset(&buffer, 'a').unwrap().0, 2);
        assert!(document.delete_mark(&mut buffer, 'a').unwrap());
        assert!(document.mark_offset(&buffer, 'a').is_none());

        document.mark_synced_at(&buffer);
        assert!(!document.needs_sync(&buffer));
        document.insert(&mut buffer, ByteOffset(0), "x").unwrap();
        assert!(document.needs_sync(&buffer));
    }
}
