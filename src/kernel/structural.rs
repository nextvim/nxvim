use text::{Bias, Point, Selection, SelectionGoal, ToOffset, ToPoint};
use vim_buffer::{Buffer, SelectionSet};

fn invalidate(folds: &mut Vec<display_map::Fold>, buffer: &text::Buffer, start: usize, end: usize) {
    crate::app::legacy_editor::remove_overlapping_folds(folds, buffer, start, end);
}

pub(crate) fn execute_put(
    buffer: &mut Buffer,
    selections: &mut SelectionSet,
    folds: &mut Vec<display_map::Fold>,
    text: &str,
    kind: vim_clipboard::ClipboardKind,
    count: usize,
    before: bool,
    target_line: Option<u32>,
) -> Option<super::MutationOutcome> {
    if text.is_empty() || count == 0 {
        return None;
    }
    if selections.selections.is_empty() {
        selections.add(buffer.as_text_buffer(), 0);
    }
    let payload = text.repeat(count);
    let cursor = selections.first()?.clone();
    if kind == vim_clipboard::ClipboardKind::Block {
        let point = cursor.head().to_point(buffer.as_text_buffer());
        let mut edits = Vec::new();
        for (index, line) in text.lines().enumerate() {
            let row = point.row.saturating_add(index as u32);
            if row >= buffer.as_text_buffer().row_count() {
                break;
            }
            let column = point.column.min(buffer.as_text_buffer().line_len(row));
            let offset = Point::new(row, column).to_offset(buffer.as_text_buffer());
            edits.push((offset, line.repeat(count)));
        }
        if edits.is_empty() {
            return None;
        }
        for &(offset, _) in &edits {
            invalidate(folds, buffer.as_text_buffer(), offset, offset);
        }
        let snapshot = selections.clone();
        return super::transaction(buffer, vim_buffer::EditOrigin::User, Some(snapshot), |tx| {
            for (offset, line) in edits {
                tx.insert(None, vim_buffer::ByteOffset(offset), line);
            }
        })
        .ok();
    }
    let (offset, inserted, target_offset) = match kind {
        vim_clipboard::ClipboardKind::Line => {
            let mut line = text.to_string();
            if !line.ends_with('\n') {
                line.push('\n');
            }
            let repeated = line.repeat(count);
            let requested = target_line.map(|line| line.saturating_sub(1));
            let row = requested
                .unwrap_or_else(|| cursor.head().to_point(buffer.as_text_buffer()).row)
                .min(buffer.as_text_buffer().row_count().saturating_sub(1));
            let insertion_row = if before { row } else { row.saturating_add(1) };
            if insertion_row < buffer.as_text_buffer().row_count() {
                let at = Point::new(insertion_row, 0).to_offset(buffer.as_text_buffer());
                (at, repeated, at)
            } else {
                let at = Point::new(row, buffer.as_text_buffer().line_len(row))
                    .to_offset(buffer.as_text_buffer());
                (at, format!("\n{}", repeated), at.saturating_add(1))
            }
        }
        vim_clipboard::ClipboardKind::Block => unreachable!("block registers return above"),
        vim_clipboard::ClipboardKind::Character => {
            let head = buffer.as_text_buffer().offset_for_anchor(&cursor.head());
            let at = if before {
                head
            } else {
                buffer
                    .as_text_buffer()
                    .clip_offset(head.saturating_add(1), Bias::Right)
            };
            (at, payload, at)
        }
    };
    invalidate(folds, buffer.as_text_buffer(), offset, offset);
    let snapshot = selections.clone();
    let outcome = super::transaction(buffer, vim_buffer::EditOrigin::User, Some(snapshot), |tx| {
        tx.insert(None, vim_buffer::ByteOffset(offset), inserted.as_str());
    })
    .ok()?;
    let anchor = buffer.as_text_buffer().anchor_at(target_offset, Bias::Left);
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
    Some(outcome)
}

pub(crate) fn execute_join_lines(
    buffer: &mut Buffer,
    selections: &mut SelectionSet,
    folds: &mut Vec<display_map::Fold>,
    count: usize,
) -> Option<super::MutationOutcome> {
    let cursor = selections.first()?.clone();
    let row = cursor.head().to_point(buffer.as_text_buffer()).row;
    let end_row = row
        .saturating_add(count.max(2) as u32 - 1)
        .min(buffer.as_text_buffer().row_count().saturating_sub(1));
    if end_row == row {
        return None;
    }
    let start =
        Point::new(row, buffer.as_text_buffer().line_len(row)).to_offset(buffer.as_text_buffer());
    let end = Point::new(end_row, buffer.as_text_buffer().line_len(end_row))
        .to_offset(buffer.as_text_buffer());
    let source: String = buffer
        .as_text_buffer()
        .as_rope()
        .chunks_in_range(start..end)
        .collect();
    let joined = source
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let line_start = Point::new(row, 0).to_offset(buffer.as_text_buffer());
    let current: String = buffer
        .as_text_buffer()
        .as_rope()
        .chunks_in_range(line_start..start)
        .collect();
    let replacement = if joined.is_empty() || current.ends_with(char::is_whitespace) {
        joined
    } else {
        format!(" {joined}")
    };
    invalidate(folds, buffer.as_text_buffer(), start, end);
    let snapshot = selections.clone();
    super::transaction(buffer, vim_buffer::EditOrigin::User, Some(snapshot), |tx| {
        tx.replace(
            None,
            vim_buffer::TextRange::new(vim_buffer::ByteOffset(start), vim_buffer::ByteOffset(end))
                .expect("join range"),
            replacement.as_str(),
        );
    })
    .ok()
}

pub(crate) fn execute_indent(
    buffer: &mut Buffer,
    selections: &SelectionSet,
    folds: &mut Vec<display_map::Fold>,
    count: usize,
    outdent: bool,
) -> Option<super::MutationOutcome> {
    let row = selections
        .first()?
        .head()
        .to_point(buffer.as_text_buffer())
        .row;
    let end_row = row
        .saturating_add(count.max(1) as u32)
        .min(buffer.as_text_buffer().row_count());
    let mut edits = Vec::new();
    for current in row..end_row {
        let start = Point::new(current, 0).to_offset(buffer.as_text_buffer());
        if outdent {
            let text: String = buffer
                .as_text_buffer()
                .as_rope()
                .chunks_in_range(start..buffer.as_text_buffer().len())
                .collect();
            let remove = text
                .chars()
                .take_while(|ch| *ch == ' ' || *ch == '\t')
                .take(4)
                .map(char::len_utf8)
                .sum::<usize>();
            if remove > 0 {
                edits.push((start, start + remove, ""));
            }
        } else {
            edits.push((start, start, "    "));
        }
    }
    if edits.is_empty() {
        return None;
    }
    for &(start, end, _) in &edits {
        invalidate(folds, buffer.as_text_buffer(), start, end);
    }
    let snapshot = selections.clone();
    super::transaction(buffer, vim_buffer::EditOrigin::User, Some(snapshot), |tx| {
        for &(start, end, replacement) in &edits {
            tx.replace(
                None,
                vim_buffer::TextRange::new(
                    vim_buffer::ByteOffset(start),
                    vim_buffer::ByteOffset(end),
                )
                .expect("indent range"),
                replacement,
            );
        }
    })
    .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(text: &str) -> (Buffer, SelectionSet, Vec<display_map::Fold>) {
        let buffer = Buffer::new(
            vim_buffer::BufferId::new(1).unwrap(),
            clock::ReplicaId::LOCAL,
            text,
        );
        let mut selections = SelectionSet::new();
        selections.add(buffer.as_text_buffer(), 0);
        (buffer, selections, Vec::new())
    }

    #[test]
    fn puts_character_before_and_after_cursor() {
        let (mut buffer, mut selections, mut folds) = fixture("ab");
        assert!(
            execute_put(
                &mut buffer,
                &mut selections,
                &mut folds,
                "X",
                vim_clipboard::ClipboardKind::Character,
                1,
                false,
                None
            )
            .is_some()
        );
        assert_eq!(buffer.as_text_buffer().as_rope().to_string(), "aXb");
        buffer.undo().unwrap().unwrap();
        selections.selections.clear();
        selections.add(buffer.as_text_buffer(), 0);
        assert!(
            execute_put(
                &mut buffer,
                &mut selections,
                &mut folds,
                "X",
                vim_clipboard::ClipboardKind::Character,
                1,
                true,
                None
            )
            .is_some()
        );
        assert_eq!(buffer.as_text_buffer().as_rope().to_string(), "Xab");

        let (mut block_buffer, mut block_selections, mut block_folds) = fixture("ab\ncd");
        assert!(
            execute_put(
                &mut block_buffer,
                &mut block_selections,
                &mut block_folds,
                "X\nY",
                vim_clipboard::ClipboardKind::Block,
                1,
                false,
                None
            )
            .is_some()
        );
        assert_eq!(
            block_buffer.as_text_buffer().as_rope().to_string(),
            "Xab\nYcd"
        );
    }

    #[test]
    fn joins_and_indents_with_single_transactions() {
        let (mut buffer, mut selections, mut folds) = fixture("one\n  two\nthree");
        assert!(execute_join_lines(&mut buffer, &mut selections, &mut folds, 2).is_some());
        assert_eq!(
            buffer.as_text_buffer().as_rope().to_string(),
            "one two\nthree"
        );
        assert!(execute_indent(&mut buffer, &selections, &mut folds, 1, false).is_some());
        assert_eq!(
            buffer.as_text_buffer().as_rope().to_string(),
            "    one two\nthree"
        );
        assert!(execute_indent(&mut buffer, &selections, &mut folds, 1, true).is_some());
        assert_eq!(
            buffer.as_text_buffer().as_rope().to_string(),
            "one two\nthree"
        );
    }
}
