use std::collections::HashMap;
use sum_tree::Bias;

use text::{Selection, SelectionGoal, ToOffset, ToPoint};
use vim_input::Action;
use vim_scanner::StructuralScanner;

use crate::kernel::{
    Editor,
    ids::WindowId,
    outcome::{Outcome, RedrawInvalidation},
    window::FoldRange,
};

pub fn close(editor: &mut Editor, window: WindowId, count: u32) -> Outcome {
    if count == 0 {
        return Outcome::default();
    }

    let buffer_id = editor.window(window).expect("live window").buffer_id();
    let (folds, selections) = {
        let buffer = editor.buffer(buffer_id).expect("live buffer");
        let text = buffer.as_text_buffer();
        let scan = StructuralScanner::scan_chunks(text.as_rope().chunks());
        let mut folds = Vec::new();
        let mut selections = Vec::new();

        for selection in &editor.window(window).unwrap().selections().selections {
            let head = selection.head().to_offset(text);
            let mut at = head;
            let mut found = None;
            for _ in 0..count {
                found = scan.enclosing_block_at(at);
                let Some(block) = found else { break };
                at = block.start.saturating_sub(1);
            }
            let Some(block) = found else { continue };
            let inner = block.inner_range();
            if inner.start >= inner.end {
                continue;
            }
            folds.push(FoldRange {
                start: text.anchor_at(inner.start, Bias::Left),
                end: text.anchor_at(inner.end, Bias::Right),
            });
            let anchor = text.anchor_at(block.start, Bias::Left);
            selections.push(Selection {
                id: selection.id,
                start: anchor,
                end: anchor,
                reversed: false,
                goal: SelectionGoal::None,
            });
        }
        (folds, selections)
    };

    if folds.is_empty() {
        return Outcome::default();
    }
    let win = editor.windows_mut().get_mut(window).unwrap();
    for fold in folds {
        if !win.folds().contains(&fold) {
            win.folds_mut().push(fold);
        }
    }
    for selection in selections {
        if let Some(existing) = win
            .selections_mut()
            .selections
            .iter_mut()
            .find(|existing| existing.id == selection.id)
        {
            *existing = selection;
        }
    }

    Outcome {
        invalidation: RedrawInvalidation::CurrentWindow,
        ..Outcome::default()
    }
}

pub fn open(editor: &mut Editor, window: WindowId, count: u32) -> Outcome {
    if count == 0 {
        return Outcome::default();
    }
    let buffer_id = editor.window(window).expect("live window").buffer_id();
    let heads = {
        let buffer = editor.buffer(buffer_id).unwrap();
        editor
            .window(window)
            .unwrap()
            .selections()
            .selections
            .iter()
            .map(|selection| {
                let head = selection.head();
                (
                    head.to_offset(buffer.as_text_buffer()),
                    head.to_point(buffer.as_text_buffer()).row,
                )
            })
            .collect::<Vec<_>>()
    };
    let buffer = editor.buffer(buffer_id).unwrap().snapshot().into_inner();
    let before = editor.window(window).unwrap().folds().len();
    editor
        .windows_mut()
        .get_mut(window)
        .unwrap()
        .folds_mut()
        .retain(|fold| {
            let start = fold.start.to_offset(&buffer);
            let end = fold.end.to_offset(&buffer);
            let start_row = buffer.offset_to_point(start).row;
            !heads
                .iter()
                .any(|(head, row)| (*head >= start && *head <= end) || *row == start_row)
        });
    let changed = editor.window(window).unwrap().folds().len() != before;
    if changed {
        Outcome {
            invalidation: RedrawInvalidation::CurrentWindow,
            ..Outcome::default()
        }
    } else {
        Outcome::default()
    }
}

pub fn dispatch(editor: &mut Editor, window: WindowId, action: Action) -> Outcome {
    match action {
        Action::Fold { count } => close(editor, window, count),
        Action::Unfold { count } => open(editor, window, count),
        _ => Outcome::default(),
    }
}

/// Prevents a cursor from resting in text hidden by a closed fold.
pub fn snap_cursors(
    editor: &mut Editor,
    window: WindowId,
    action: &Action,
    previous_heads: &HashMap<usize, usize>,
) {
    let Some(win) = editor.window(window) else {
        return;
    };
    if win.folds().is_empty() {
        return;
    }
    let buffer_id = win.buffer_id();
    let replacements = {
        let buffer = editor
            .buffer(buffer_id)
            .expect("window names a live buffer");
        let text = buffer.as_text_buffer();
        let win = editor.window(window).unwrap();
        let mut replacements = Vec::new();

        for selection in &win.selections().selections {
            let head = selection.head().to_offset(text);
            for fold in win.folds() {
                let start = fold.start.to_offset(text);
                let end = fold.end.to_offset(text);
                if head < start || head >= end {
                    continue;
                }
                let forward = previous_heads.get(&selection.id).map_or_else(
                    || action_moves_forward(action),
                    |previous| head >= *previous,
                );
                let target = if forward { end } else { start };
                let anchor = if forward {
                    text.anchor_at(target, Bias::Right)
                } else {
                    text.anchor_at(target, Bias::Left)
                };
                let mut replacement = selection.clone();
                replacement.start = anchor;
                replacement.end = anchor;
                replacement.reversed = false;
                replacement.goal = SelectionGoal::None;
                replacements.push(replacement);
                break;
            }
        }
        replacements
    };

    let win = editor.windows_mut().get_mut(window).unwrap();
    for replacement in replacements {
        if let Some(selection) = win
            .selections_mut()
            .selections
            .iter_mut()
            .find(|selection| selection.id == replacement.id)
        {
            *selection = replacement;
        }
    }
}

fn action_moves_forward(action: &Action) -> bool {
    matches!(
        action,
        Action::MoveRight { .. }
            | Action::MoveDown { .. }
            | Action::MoveToWord { .. }
            | Action::MoveToWordEnd { .. }
            | Action::MoveToBigWord { .. }
            | Action::MoveToBigWordEnd { .. }
            | Action::MoveToEndOfLine { .. }
            | Action::MoveToEndOfDocument { .. }
            | Action::MoveToEndOfNextLine { .. }
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn close_and_open_fold_at_cursor() {
        let mut editor = Editor::new("{\n  one\n  two\n}\n");
        let window = editor.current_context().window;

        let outcome = editor.execute(Action::Fold { count: 1 });
        assert_eq!(outcome.invalidation, RedrawInvalidation::CurrentWindow);
        let folds = editor.window(window).unwrap().folds();
        assert_eq!(folds.len(), 1);

        editor.execute(Action::Unfold { count: 1 });
        assert!(editor.window(window).unwrap().folds().is_empty());
    }

    #[test]
    fn motions_jump_across_hidden_fold_text() {
        let mut editor = Editor::new("{\n  one\n  two\n}\n");
        let window = editor.current_context().window;
        editor.execute(Action::Fold { count: 1 });
        let (fold_start, fold_end) = {
            let text = editor.current_buffer().as_text_buffer();
            let fold = &editor.window(window).unwrap().folds()[0];
            (fold.start.to_offset(text), fold.end.to_offset(text))
        };

        editor.execute(Action::MoveRight {
            count: 1,
            select: false,
        });
        let forward = editor
            .window(window)
            .unwrap()
            .selections()
            .primary()
            .head()
            .to_offset(editor.current_buffer().as_text_buffer());
        assert_eq!(forward, fold_end);

        editor.execute(Action::MoveLeft {
            count: 1,
            select: false,
        });
        let backward = editor
            .window(window)
            .unwrap()
            .selections()
            .primary()
            .head()
            .to_offset(editor.current_buffer().as_text_buffer());
        assert_eq!(backward, fold_start);
    }

    #[test]
    fn editing_a_folded_range_removes_the_fold() {
        let mut editor = Editor::new("{\n  one\n}\n");
        let window = editor.current_context().window;
        editor.execute(Action::Fold { count: 1 });
        assert_eq!(editor.window(window).unwrap().folds().len(), 1);

        editor.execute(Action::SetToInsert);
        editor.execute(Action::InsertText("x".into()));
        assert!(editor.window(window).unwrap().folds().is_empty());
    }
}
