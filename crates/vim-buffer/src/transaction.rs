use crate::{
    Buffer, BufferError, ByteOffset, Edit, EditOrigin, EditSummary, MutationOutcome, PlannedEdit,
    SelectionId, SelectionSet, TextExtent, TextRange,
};
use std::{cmp::Ordering, collections::HashSet, sync::Arc};

pub struct Transaction<'a> {
    buffer: &'a mut Buffer,
    origin: EditOrigin,
    edits: Vec<PlannedEdit>,
    join_previous: bool,
}

impl<'a> Transaction<'a> {
    pub fn new(buffer: &'a mut Buffer, origin: EditOrigin) -> Self {
        Self {
            buffer,
            origin,
            edits: Vec::new(),
            join_previous: false,
        }
    }

    pub fn push(&mut self, edit: PlannedEdit) {
        self.edits.push(edit);
    }

    pub fn replace(
        &mut self,
        selection: Option<SelectionId>,
        range: TextRange,
        replacement: impl Into<Arc<str>>,
    ) {
        self.push(PlannedEdit {
            selection,
            edit: Edit::replace(range, replacement),
        });
    }

    pub fn insert(
        &mut self,
        selection: Option<SelectionId>,
        offset: ByteOffset,
        text: impl Into<Arc<str>>,
    ) {
        self.push(PlannedEdit {
            selection,
            edit: Edit::insert(offset, text),
        });
    }

    pub fn delete(&mut self, selection: Option<SelectionId>, range: TextRange) {
        self.push(PlannedEdit {
            selection,
            edit: Edit::delete(range),
        });
    }

    pub fn planned_edits(&self) -> &[PlannedEdit] {
        &self.edits
    }

    /// Merge this transaction into the preceding Zed transaction, matching the
    /// primitive behavior required by Vim's `:undojoin`.
    pub fn join_previous(&mut self) {
        self.join_previous = true;
    }

    pub fn commit(
        mut self,
        selections: Option<SelectionSet>,
    ) -> Result<MutationOutcome, BufferError> {
        if !self.buffer.options().modifiable && self.origin != EditOrigin::Reload {
            return Err(BufferError::Unmodifiable(self.buffer.id()));
        }
        if !self.buffer.is_loaded() {
            return Err(BufferError::InvalidLifecycleTransition);
        }

        let previous_transaction = self.buffer.last_transaction_id();
        let before = self.buffer.snapshot();
        let before_marks = self.buffer.marks().clone();
        let old_revision = before.revision().clone();
        let modified_before = self.buffer.is_modified();

        self.edits.sort_by(compare_planned_edits);
        let mut seen = HashSet::with_capacity(self.edits.len());
        self.edits
            .retain(|planned| seen.insert((planned.edit.range, planned.edit.replacement.clone())));
        let mut previous_non_empty_end = None;
        let mut backend_edits = Vec::with_capacity(self.edits.len());
        let mut summaries = Vec::with_capacity(self.edits.len());
        let mut cumulative_delta = 0isize;

        for planned in self.edits {
            let old = before.validate_range(planned.edit.range)?;
            if previous_non_empty_end.is_some_and(|previous_end| old.start < previous_end) {
                return Err(BufferError::OverlappingEdits);
            }
            if !old.is_empty() {
                previous_non_empty_end = Some(old.end);
            }

            let replacement = text::LineEnding::normalize_arc(planned.edit.replacement);
            let old_summary = before
                .as_inner()
                .text_summary_for_range::<text::TextSummary, _>(old.clone());
            let new_start = shift_offset(old.start, cumulative_delta);
            let new_end = new_start + replacement.len();

            summaries.push(EditSummary {
                old_range: planned.edit.range,
                new_range: TextRange {
                    start: ByteOffset(new_start),
                    end: ByteOffset(new_end),
                },
                old_extent: TextExtent {
                    bytes: old_summary.len,
                    lines: old_summary.lines.row,
                    last_line_bytes: old_summary.lines.column,
                },
                new_extent: extent_for_text(&replacement),
            });

            cumulative_delta += replacement.len() as isize - (old.end - old.start) as isize;
            backend_edits.push((old, replacement));
        }

        if backend_edits.is_empty() {
            return Ok(MutationOutcome {
                buffer: self.buffer.id(),
                old_revision: old_revision.clone(),
                new_revision: old_revision,
                changedtick: self.buffer.changedtick(),
                transaction: None,
                edits: Arc::from([]),
                origin: self.origin,
                selections,
                modified_changed: false,
            });
        }

        let mut transaction = self.buffer.apply_text_edits(backend_edits);
        if self.join_previous
            && let (Some(source), Some(destination)) = (transaction, previous_transaction)
        {
            self.buffer.merge_transaction(source, destination);
            transaction = Some(destination);
        }
        self.buffer.increment_changedtick();
        if let Some(transaction) = transaction {
            self.buffer.finish_change_metadata(
                transaction,
                selections.clone(),
                before_marks,
                &before,
                &summaries,
            );
        }
        let new_revision = self.buffer.revision();
        let modified_changed = modified_before != self.buffer.is_modified();

        Ok(MutationOutcome {
            buffer: self.buffer.id(),
            old_revision,
            new_revision,
            changedtick: self.buffer.changedtick(),
            transaction,
            edits: summaries.into(),
            origin: self.origin,
            selections,
            modified_changed,
        })
    }
}

fn compare_planned_edits(left: &PlannedEdit, right: &PlannedEdit) -> Ordering {
    left.edit
        .range
        .start
        .cmp(&right.edit.range.start)
        .then_with(|| left.edit.range.end.cmp(&right.edit.range.end))
        .then_with(|| {
            left.selection
                .map(SelectionId::get)
                .cmp(&right.selection.map(SelectionId::get))
        })
}

fn shift_offset(offset: usize, delta: isize) -> usize {
    if delta >= 0 {
        offset + delta as usize
    } else {
        offset - delta.unsigned_abs()
    }
}

fn extent_for_text(text: &str) -> TextExtent {
    let mut lines = 0;
    let mut last_line_bytes = 0;
    for byte in text.bytes() {
        if byte == b'\n' {
            lines += 1;
            last_line_bytes = 0;
        } else {
            last_line_bytes += 1;
        }
    }
    TextExtent {
        bytes: text.len(),
        lines,
        last_line_bytes,
    }
}
