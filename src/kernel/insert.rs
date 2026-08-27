use text::{Bias, Point, Selection, SelectionGoal, ToOffset, ToPoint};
use unicode_width::UnicodeWidthChar;
use vim_buffer::{Buffer, SelectionSet};

fn display_width(text: &str, column: usize) -> usize {
    let mut width = 0;
    for ch in text.chars() {
        width += if ch == '\t' {
            4 - ((column + width) % 4)
        } else {
            ch.width().unwrap_or(0).max(1)
        };
    }
    width
}

/// Opens lines above or below the primary cursor in one insert-mode
/// transaction and places the cursor at the first opened line.
pub(crate) fn execute_open_line(
    buffer: &mut Buffer,
    selections: &mut SelectionSet,
    folds: &mut Vec<display_map::Fold>,
    count: usize,
    above: bool,
) -> Option<super::MutationOutcome> {
    if selections.selections.is_empty() {
        selections.add(buffer.as_text_buffer(), 0);
    }
    let count = count.max(1);
    let cursor = selections.primary().clone();
    let point = cursor.head().to_point(buffer.as_text_buffer());
    let insertion_point = if above {
        Point {
            row: point.row,
            column: 0,
        }
    } else {
        Point {
            row: point.row,
            column: buffer.as_text_buffer().line_len(point.row),
        }
    };
    let insertion_offset = insertion_point.to_offset(buffer.as_text_buffer());
    crate::app::legacy_editor::remove_overlapping_folds(
        folds,
        buffer.as_text_buffer(),
        insertion_offset,
        insertion_offset,
    );
    let text = "\n".repeat(count);
    let selection_snapshot = selections.clone();
    let outcome = super::transaction(
        buffer,
        vim_buffer::EditOrigin::InsertMode,
        Some(selection_snapshot),
        |tx| {
            tx.replace(
                None,
                vim_buffer::TextRange {
                    start: vim_buffer::ByteOffset(insertion_offset),
                    end: vim_buffer::ByteOffset(insertion_offset),
                },
                text.as_str(),
            );
        },
    )
    .ok()?;

    let target_offset = if above {
        insertion_offset
    } else {
        insertion_offset.saturating_add(1)
    };
    let target = buffer.as_text_buffer().anchor_at(target_offset, Bias::Left);
    selections.clear(buffer.as_text_buffer());
    let primary = selections
        .first()
        .expect("clear retains a primary cursor")
        .clone();
    selections.update(
        buffer.as_text_buffer(),
        &Selection {
            id: primary.id,
            start: target.clone(),
            end: target,
            reversed: false,
            goal: SelectionGoal::None,
        },
    );
    Some(outcome)
}

fn virtual_replace_end(buffer: &text::Buffer, start: usize, width: usize) -> usize {
    let point = start.to_point(buffer);
    let available: String = buffer
        .as_rope()
        .chunks_in_range(start..buffer.len())
        .collect();
    let mut consumed_width = 0;
    let mut offset = start;
    for ch in available.chars() {
        if ch == '\n' || consumed_width >= width {
            break;
        }
        let character_width = if ch == '\t' {
            4 - ((point.column as usize + consumed_width) % 4)
        } else {
            ch.width().unwrap_or(0).max(1)
        };
        consumed_width += character_width;
        offset += ch.len_utf8();
    }
    offset
}

/// Inserts text for all selections in one kernel transaction. Existing
/// selections are replaced directly; Replace mode first extends each selection
/// by the inserted character count. Cursor anchors are normalized after commit.
pub(crate) fn execute_insert_text(
    buffer: &mut Buffer,
    selections: &mut SelectionSet,
    folds: &mut Vec<display_map::Fold>,
    text: &str,
    replace: bool,
    virtual_replace: bool,
    join_previous: bool,
) -> Option<super::MutationOutcome> {
    if selections.selections.is_empty() {
        selections.add(buffer.as_text_buffer(), 0);
    }
    if replace && !virtual_replace {
        selections.move_right(
            true,
            text.chars().count().min(u32::MAX as usize) as u32,
            buffer.as_text_buffer(),
        );
        selections.collapse_overlapping_cursors(buffer.as_text_buffer());
    }

    let cursors = selections.selections.clone();
    let edits: Vec<_> = cursors
        .iter()
        .map(|cursor| {
            let a = buffer.as_text_buffer().offset_for_anchor(&cursor.tail());
            let b = buffer.as_text_buffer().offset_for_anchor(&cursor.head());
            let start = a.min(b);
            let end = a.max(b);
            if virtual_replace && start == end {
                (
                    start,
                    virtual_replace_end(
                        buffer.as_text_buffer(),
                        start,
                        display_width(
                            text,
                            start.to_point(buffer.as_text_buffer()).column as usize,
                        ),
                    ),
                )
            } else {
                (start, end)
            }
        })
        .collect();
    for &(start, end) in &edits {
        crate::app::legacy_editor::remove_overlapping_folds(
            folds,
            buffer.as_text_buffer(),
            start,
            end,
        );
    }

    let selection_snapshot = selections.clone();
    let outcome = super::transaction(
        buffer,
        vim_buffer::EditOrigin::InsertMode,
        Some(selection_snapshot),
        |tx| {
            if join_previous {
                tx.join_previous();
            }
            for &(start, end) in &edits {
                tx.replace(
                    None,
                    vim_buffer::TextRange {
                        start: vim_buffer::ByteOffset(start),
                        end: vim_buffer::ByteOffset(end),
                    },
                    text,
                );
            }
        },
    )
    .ok();

    for (cursor, &(start, _)) in cursors.iter().zip(&edits) {
        let new_offset = buffer
            .as_text_buffer()
            .clip_offset(start.saturating_add(text.len()), Bias::Left);
        let anchor = buffer.as_text_buffer().anchor_at(new_offset, Bias::Left);
        selections.update(
            buffer.as_text_buffer(),
            &Selection {
                id: cursor.id,
                start: anchor.clone(),
                end: anchor,
                reversed: false,
                goal: SelectionGoal::None,
            },
        );
    }
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_line_uses_one_transaction_and_positions_insert_cursor() {
        let mut buffer = Buffer::new(
            vim_buffer::BufferId::new(1).unwrap(),
            clock::ReplicaId::LOCAL,
            "one\ntwo",
        );
        let mut selections = SelectionSet::new();
        selections.add(buffer.as_text_buffer(), 1);
        let mutation = execute_open_line(&mut buffer, &mut selections, &mut Vec::new(), 2, false);
        assert!(mutation.is_some());
        assert_eq!(
            buffer.as_text_buffer().as_rope().to_string(),
            "one\n\n\ntwo"
        );
        assert_eq!(
            buffer
                .as_text_buffer()
                .offset_for_anchor(&selections.first().unwrap().head()),
            4
        );
        buffer.undo().unwrap().unwrap();
        assert_eq!(buffer.as_text_buffer().as_rope().to_string(), "one\ntwo");
    }

    #[test]
    fn virtual_replace_consumes_display_cells_not_bytes_or_characters() {
        let mut buffer = Buffer::new(
            vim_buffer::BufferId::new(1).unwrap(),
            clock::ReplicaId::LOCAL,
            "abc",
        );
        let mut selections = SelectionSet::new();
        selections.add(buffer.as_text_buffer(), 0);
        assert!(
            execute_insert_text(
                &mut buffer,
                &mut selections,
                &mut Vec::new(),
                "界",
                true,
                true,
                false,
            )
            .is_some()
        );
        assert_eq!(buffer.as_text_buffer().as_rope().to_string(), "界c");
    }

    #[test]
    fn joined_insert_transactions_undo_as_one_session() {
        let mut buffer = Buffer::new(
            vim_buffer::BufferId::new(1).unwrap(),
            clock::ReplicaId::LOCAL,
            "",
        );
        let mut selections = SelectionSet::new();
        let mut folds = Vec::new();

        assert!(
            execute_insert_text(
                &mut buffer,
                &mut selections,
                &mut folds,
                "a",
                false,
                false,
                false,
            )
            .is_some()
        );
        assert!(
            execute_insert_text(
                &mut buffer,
                &mut selections,
                &mut folds,
                "b",
                false,
                false,
                true,
            )
            .is_some()
        );
        assert_eq!(buffer.as_text_buffer().as_rope().to_string(), "ab");

        buffer.undo().unwrap().unwrap();
        assert_eq!(buffer.as_text_buffer().as_rope().to_string(), "");
    }
}
