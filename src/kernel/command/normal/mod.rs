//! Normal-mode command family. One module per family of commands
//! (`RESCUE.md` Rule 3) — `motions`/`operators` today, `text_objects`/... as
//! later milestones add them.

pub mod marks_and_jumps;
pub mod motions;
pub mod operators;
pub mod registers_ops;
pub mod text_objects;
pub mod windows;

use vim_buffer::MutationOutcome;
use vim_input::Action;

use crate::kernel::{
    Editor, command::CommandContext, ids::WindowId, mode::VisualKind, outcome::Outcome, transaction,
};

pub fn dispatch(editor: &mut Editor, ctx: CommandContext, action: Action) -> Outcome {
    if operators::is_repeatable_change(&action) {
        editor.set_last_change(action.clone());
    }
    match action {
        Action::MoveLeft { count, select } => motions::move_left(editor, ctx.window, count, select),
        Action::MoveRight { count, select } => {
            motions::move_right(editor, ctx.window, count, select)
        }
        Action::MoveUp { count, select } => motions::move_up(editor, ctx.window, count, select),
        Action::MoveDown { count, select } => motions::move_down(editor, ctx.window, count, select),
        Action::SetToInsert => super::insert::enter(editor),
        Action::SetToReplace => super::insert::enter_replace(editor, ctx.window, false),
        Action::SetToVirtualReplace => super::insert::enter_replace(editor, ctx.window, true),
        Action::SetToVisual => super::visual::enter(editor, ctx.window, VisualKind::Char),
        Action::SetToVisualLine => super::visual::enter(editor, ctx.window, VisualKind::Line),
        Action::SetToVisualBlock => super::visual::enter(editor, ctx.window, VisualKind::Block),
        Action::ReselectLastVisual => super::visual::reselect_last_visual(editor, ctx.window),
        Action::SetToCommand => super::ex::enter(editor),
        Action::DeleteMotion { count, motion } => {
            operators::delete_motion(editor, ctx.window, count, &motion)
        }
        Action::DeleteLine { count } => operators::delete_line(editor, ctx.window, count),
        Action::ChangeMotion { count, motion } => {
            operators::change_motion(editor, ctx.window, count, &motion)
        }
        Action::ChangeLine { count } => operators::change_line(editor, ctx.window, count),
        Action::YankMotion { count, motion } => {
            operators::yank_motion(editor, ctx.window, count, &motion)
        }
        Action::YankLine { count } => operators::yank_line(editor, ctx.window, count),
        Action::UpperCaseMotion { count, motion } => {
            operators::upper_case_motion(editor, ctx.window, count, &motion)
        }
        Action::LowerCaseMotion { count, motion } => {
            operators::lower_case_motion(editor, ctx.window, count, &motion)
        }
        Action::ToggleCaseMotion { count, motion } => {
            operators::toggle_case_motion(editor, ctx.window, count, &motion)
        }
        Action::UpperCaseLine { count } => operators::upper_case_line(editor, ctx.window, count),
        Action::LowerCaseLine { count } => operators::lower_case_line(editor, ctx.window, count),
        Action::ToggleCaseLine { count } => operators::toggle_case_line(editor, ctx.window, count),
        Action::IndentMotion { count, motion } => {
            operators::indent_motion(editor, ctx.window, count, &motion)
        }
        Action::OutdentMotion { count, motion } => {
            operators::outdent_motion(editor, ctx.window, count, &motion)
        }
        Action::Indent { count } => operators::indent(editor, ctx.window, count),
        Action::Outdent { count } => operators::outdent(editor, ctx.window, count),
        Action::Repeat { count } => operators::repeat_last_change(editor, ctx.window, count),
        Action::Undo { count } => undo(editor, ctx.window, count),
        Action::Redo { count } => redo(editor, ctx.window, count),
        Action::SplitHorizontal { .. } => windows::split_horizontal(editor, ctx),
        Action::SplitVertical { .. } => windows::split_vertical(editor, ctx),
        Action::CloseWindow => windows::close_window(editor, ctx),
        Action::OnlyWindow => windows::only_window(editor, ctx),
        Action::FocusLeftWindow => windows::focus_window(
            editor,
            ctx,
            crate::kernel::window::tabpage::NavigationDirection::Left,
        ),
        Action::FocusRightWindow => windows::focus_window(
            editor,
            ctx,
            crate::kernel::window::tabpage::NavigationDirection::Right,
        ),
        Action::FocusUpWindow => windows::focus_window(
            editor,
            ctx,
            crate::kernel::window::tabpage::NavigationDirection::Up,
        ),
        Action::FocusDownWindow => windows::focus_window(
            editor,
            ctx,
            crate::kernel::window::tabpage::NavigationDirection::Down,
        ),
        Action::NextTab { count } => windows::next_tab(editor, count),
        Action::PreviousTab { count } => windows::previous_tab(editor, count),
        Action::MoveToWord { count, select } => {
            motions::move_to_word(editor, ctx.window, count, select)
        }
        Action::MoveToPreviousWord { count, select } => {
            motions::move_to_previous_word(editor, ctx.window, count, select)
        }
        Action::MoveToWordEnd { count, select } => {
            motions::move_to_word_end(editor, ctx.window, count, select)
        }
        Action::MoveToPreviousWordEnd { count, select } => {
            motions::move_to_previous_word_end(editor, ctx.window, count, select)
        }
        Action::MoveToBigWord { count, select } => {
            motions::move_to_big_word(editor, ctx.window, count, select)
        }
        Action::MoveToPreviousBigWord { count, select } => {
            motions::move_to_previous_big_word(editor, ctx.window, count, select)
        }
        Action::MoveToBigWordEnd { count, select } => {
            motions::move_to_big_word_end(editor, ctx.window, count, select)
        }
        Action::MoveToPreviousBigWordEnd { count, select } => {
            motions::move_to_previous_big_word_end(editor, ctx.window, count, select)
        }
        Action::MoveToStartOfDocument { select, .. } => {
            motions::move_to_start_of_document(editor, ctx.window, select)
        }
        Action::MoveToEndOfDocument { select, .. } => {
            motions::move_to_end_of_document(editor, ctx.window, select)
        }
        Action::MoveToLine { line, select } => {
            motions::move_to_line(editor, ctx.window, line, select)
        }
        Action::MoveToStartOfLine { select, .. } => {
            motions::move_to_start_of_line(editor, ctx.window, select)
        }
        Action::MoveToStartOfLineNonSpace { select, .. } => {
            motions::move_to_start_of_line_non_space(editor, ctx.window, select)
        }
        Action::MoveToEndOfLine { select, .. } => {
            motions::move_to_end_of_line(editor, ctx.window, select)
        }
        Action::MoveToStartOfPreviousLine { select, .. } => {
            motions::move_to_start_of_previous_line(editor, ctx.window, select)
        }
        Action::MoveToEndOfPreviousLine { select, .. } => {
            motions::move_to_end_of_previous_line(editor, ctx.window, select)
        }
        Action::MoveToStartOfNextLine { select, .. } => {
            motions::move_to_start_of_next_line(editor, ctx.window, select)
        }
        Action::MoveToEndOfNextLine { select, .. } => {
            motions::move_to_end_of_next_line(editor, ctx.window, select)
        }
        Action::MoveToPreviousParagraph { count, select } => {
            motions::move_to_previous_paragraph(editor, ctx.window, count, select)
        }
        Action::MoveToNextParagraph { count, select } => {
            motions::move_to_next_paragraph(editor, ctx.window, count, select)
        }
        Action::MoveToPreviousSentence { count, select } => {
            motions::move_to_previous_sentence(editor, ctx.window, count, select)
        }
        Action::MoveToNextSentence { count, select } => {
            motions::move_to_next_sentence(editor, ctx.window, count, select)
        }
        Action::MoveToMatchingDelimiter { select, .. } => {
            motions::move_to_matching_delimiter(editor, ctx.window, select)
        }
        Action::MoveToColumn { count } => motions::move_to_column(editor, ctx.window, count, false),
        Action::MoveToLastNonWhitespace { count, select } => {
            motions::move_to_last_non_whitespace(editor, ctx.window, count, select)
        }
        Action::MoveToScreenTop { select, .. } => {
            motions::move_to_screen_top(editor, ctx.window, select)
        }
        Action::MoveToScreenMiddle { select, .. } => {
            motions::move_to_screen_middle(editor, ctx.window, select)
        }
        Action::MoveToScreenBottom { select, .. } => {
            motions::move_to_screen_bottom(editor, ctx.window, select)
        }
        Action::ScrollLineDown { count } => motions::scroll_line_down(editor, ctx.window, count),
        Action::ScrollLineUp { count } => motions::scroll_line_up(editor, ctx.window, count),
        Action::ScrollHalfPageDown { count } => {
            motions::scroll_half_page_down(editor, ctx.window, count)
        }
        Action::ScrollHalfPageUp { count } => {
            motions::scroll_half_page_up(editor, ctx.window, count)
        }
        Action::ScrollForward { count } => motions::scroll_forward(editor, ctx.window, count),
        Action::ScrollBackward { count } => motions::scroll_backward(editor, ctx.window, count),
        Action::CenterCursorLine => motions::center_cursor_line(editor, ctx.window),
        Action::CursorLineTop => motions::cursor_line_top(editor, ctx.window),
        Action::CursorLineBottom => motions::cursor_line_bottom(editor, ctx.window),
        Action::MoveToNextCharacter {
            count,
            ch,
            till,
            select,
        } => motions::find_character(editor, ctx.window, count, ch, true, till, select),
        Action::MoveToPreviousCharacter {
            count,
            ch,
            till,
            select,
        } => motions::find_character(editor, ctx.window, count, ch, false, till, select),
        Action::RepeatCharacterSearchForward { count, select } => {
            motions::repeat_character_search(editor, ctx.window, count, false, select)
        }
        Action::RepeatCharacterSearchBackward { count, select } => {
            motions::repeat_character_search(editor, ctx.window, count, true, select)
        }
        Action::MoveWithinCharacter { ch, .. } => {
            text_objects::select(editor, ctx.window, ch, false)
        }
        Action::MoveAroundCharacter { ch, .. } => {
            text_objects::select(editor, ctx.window, ch, true)
        }
        Action::MarkSet { ch } => marks_and_jumps::set_mark(editor, ctx.window, ch),
        Action::MarkJump {
            ch,
            select,
            linewise,
        } => marks_and_jumps::jump_to_mark(editor, ctx.window, ch, select, linewise),
        Action::JumpToOlderPosition => marks_and_jumps::jump_older(editor, ctx.window),
        Action::JumpToNewerPosition => marks_and_jumps::jump_newer(editor, ctx.window),
        Action::Put { count } => registers_ops::put(editor, ctx.window, count),
        Action::PutBefore { count } => registers_ops::put_before(editor, ctx.window, count),
        Action::PutLines { line, before } => {
            registers_ops::put_lines(editor, ctx.window, line, before)
        }
        Action::SetToCommandSearchForward => super::ex::enter_search(editor, true),
        Action::SetToCommandSearchBackward => super::ex::enter_search(editor, false),
        Action::SearchForward { count } => super::search::search(editor, "", true, count, None),
        Action::SearchBackward { count } => super::search::search(editor, "", false, count, None),
        Action::SearchWordUnderForward { count } => {
            super::search::search_word_under(editor, true, count)
        }
        Action::SearchWordUnderBackward { count } => {
            super::search::search_word_under(editor, false, count)
        }
        _ => Outcome::default(),
    }
}

fn undo(editor: &mut Editor, window: WindowId, count: u32) -> Outcome {
    replay_history(editor, window, count, transaction::undo)
}

fn redo(editor: &mut Editor, window: WindowId, count: u32) -> Outcome {
    replay_history(editor, window, count, transaction::redo)
}

/// Shared loop for `undo`/`redo`: both step `count` times through
/// `vim_buffer`'s history via `kernel::transaction`, stopping early if
/// there's nothing left to replay, and restore the cursor to the selection
/// recorded alongside whichever transaction was last replayed.
fn replay_history(
    editor: &mut Editor,
    window: WindowId,
    count: u32,
    step: fn(&mut vim_buffer::Buffer) -> Result<Option<MutationOutcome>, vim_buffer::BufferError>,
) -> Outcome {
    let buffer_id = editor
        .window(window)
        .expect("dispatch only runs against a live window")
        .buffer_id();

    let mut last = None;
    for _ in 0..count.max(1) {
        let buffer = editor
            .buffers_mut()
            .get_mut(buffer_id)
            .expect("live buffer");
        match step(buffer) {
            Ok(Some(mutation)) => last = Some(mutation),
            Ok(None) | Err(_) => break,
        }
    }

    let Some(mutation) = last else {
        return Outcome::default();
    };
    if let Some(selections) = mutation.selections.clone() {
        let win = editor.windows_mut().get_mut(window).expect("live window");
        *win.selections_mut() = selections;
    }
    Outcome::from_mutation(&mutation)
}
