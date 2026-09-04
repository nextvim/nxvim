//! Normal-mode command family. One module per family of commands
//! (`RESCUE.md` Rule 3) — `motions`/`operators` today, `text_objects`/... as
//! later milestones add them.

pub mod folds;
pub mod marks_and_jumps;
pub mod motions;
pub mod operators;
pub mod registers_ops;
pub mod text_objects;
pub mod windows;

use vim_buffer::MutationOutcome;
use vim_input::Action;

use crate::kernel::{
    Editor,
    command::CommandContext,
    ids::WindowId,
    mode::VisualKind,
    outcome::{Outcome, RedrawInvalidation},
    transaction,
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
        Action::MovePageUp { count, select } => {
            motions::move_page_up(editor, ctx.window, count, select)
        }
        Action::MovePageDown { count, select } => {
            motions::move_page_down(editor, ctx.window, count, select)
        }
        action @ (Action::Fold { .. } | Action::Unfold { .. }) => {
            folds::dispatch(editor, ctx.window, action)
        }
        Action::SetToInsert => super::insert::enter(editor),
        Action::SetToAppend => super::insert::enter_append(editor, ctx.window),
        Action::SetToAppendEndOfLine => super::insert::enter_append_eol(editor, ctx.window),
        Action::SetToOpenLineBelow { count } => super::insert::enter_open_line(editor, ctx.window, count, false),
        Action::SetToOpenLineAbove { count } => super::insert::enter_open_line(editor, ctx.window, count, true),
        Action::SetToInsertStartOfLineNonSpace => super::insert::enter_insert_start_non_space(editor, ctx.window),
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
        Action::ToggleCase { count } => {
            if editor.mode().is_visual() {
                operators::toggle_case_motion(
                    editor,
                    ctx.window,
                    count,
                    &Action::MoveRight {
                        count: 0,
                        select: true,
                    },
                )
            } else {
                Outcome::default()
            }
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
        Action::ResizeLeft => windows::resize_window(
            editor,
            ctx,
            crate::kernel::window::tabpage::NavigationDirection::Left,
            1,
        ),
        Action::ResizeRight => windows::resize_window(
            editor,
            ctx,
            crate::kernel::window::tabpage::NavigationDirection::Right,
            1,
        ),
        Action::ResizeUp => windows::resize_window(
            editor,
            ctx,
            crate::kernel::window::tabpage::NavigationDirection::Up,
            1,
        ),
        Action::ResizeDown => windows::resize_window(
            editor,
            ctx,
            crate::kernel::window::tabpage::NavigationDirection::Down,
            1,
        ),
        Action::ResizeEqual => windows::resize_equal(editor, ctx),
        Action::MoveWindowLeft => windows::move_window(
            editor,
            ctx,
            crate::kernel::window::tabpage::NavigationDirection::Left,
        ),
        Action::MoveWindowRight => windows::move_window(
            editor,
            ctx,
            crate::kernel::window::tabpage::NavigationDirection::Right,
        ),
        Action::MoveWindowUp => windows::move_window(
            editor,
            ctx,
            crate::kernel::window::tabpage::NavigationDirection::Up,
        ),
        Action::MoveWindowDown => windows::move_window(
            editor,
            ctx,
            crate::kernel::window::tabpage::NavigationDirection::Down,
        ),
        Action::CarriageReturn => {
            let win_type = editor
                .window(ctx.window)
                .map(|w| w.window_type())
                .unwrap_or(crate::kernel::window::WindowType::Normal);
            match win_type {
                crate::kernel::window::WindowType::Quickfix => {
                    let current_row = if let Some(win) = editor.window(ctx.window) {
                        let head = win.selections().primary().head();
                        if let Some(buf) = editor.buffer(ctx.buffer) {
                            let pt: text::Point = buf.as_text_buffer().summary_for_anchor(&head);
                            pt.row
                        } else {
                            0
                        }
                    } else {
                        0
                    };
                    let items = editor.quickfix_list().to_vec();
                    if let Some(item) = items.get(current_row as usize) {
                        let item_clone = item.clone();
                        super::ex::jump_to_quickfix_item(editor, ctx, &item_clone)
                    } else {
                        Outcome::default()
                    }
                }
                crate::kernel::window::WindowType::LocationList => {
                    let current_row = if let Some(win) = editor.window(ctx.window) {
                        let head = win.selections().primary().head();
                        if let Some(buf) = editor.buffer(ctx.buffer) {
                            let pt: text::Point = buf.as_text_buffer().summary_for_anchor(&head);
                            pt.row
                        } else {
                            0
                        }
                    } else {
                        0
                    };
                    let items = if let Some(win) = editor.window(ctx.window) {
                        win.location_list().to_vec()
                    } else {
                        Vec::new()
                    };
                    if let Some(item) = items.get(current_row as usize) {
                        let item_clone = item.clone();
                        super::ex::jump_to_quickfix_item(editor, ctx, &item_clone)
                    } else {
                        Outcome::default()
                    }
                }
                crate::kernel::window::WindowType::Normal => {
                    motions::move_to_start_of_next_line(editor, ctx.window, false)
                }
            }
        }
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
        Action::ScrollColumnLeft { count } => {
            motions::scroll_column_left(editor, ctx.window, count)
        }
        Action::ScrollColumnRight { count } => {
            motions::scroll_column_right(editor, ctx.window, count)
        }
        Action::ScrollHalfPageLeft { count } => {
            motions::scroll_half_page_left(editor, ctx.window, count)
        }
        Action::ScrollHalfPageRight { count } => {
            motions::scroll_half_page_right(editor, ctx.window, count)
        }
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
        Action::Clear => clear_selections(editor, ctx),
        Action::DeleteChar { count } => operators::delete_char(editor, ctx.window, count),
        Action::DeleteCharBefore { count } => {
            operators::delete_char_before(editor, ctx.window, count)
        }
        Action::ChangeCase { count } => operators::change_case(editor, ctx.window, count),
        Action::SelectSimilar => select_similar(editor, ctx),
        Action::Sequence { actions, .. } => {
            let mut outcome = Outcome::default();
            for act in actions {
                let sub_outcome = dispatch(editor, ctx, *act);
                outcome.mutated |= sub_outcome.mutated;
                outcome.mode_changed |= sub_outcome.mode_changed;
                if sub_outcome.invalidation != RedrawInvalidation::None {
                    outcome.invalidation = sub_outcome.invalidation;
                }
                outcome.effects.extend(sub_outcome.effects);
                outcome.events.extend(sub_outcome.events);
            }
            outcome
        }
        _ => Outcome::default(),
    }
}

fn clear_selections(editor: &mut Editor, ctx: CommandContext) -> Outcome {
    let (win, buffer) = editor.window_and_buffer_mut(ctx.window);
    let text_buf = buffer.as_text_buffer();
    let had_multicursor_or_selection =
        win.selections().selections.len() > 1 || win.selections().has_selection(text_buf);
    win.selections_mut().clear(text_buf);
    if had_multicursor_or_selection {
        Outcome {
            invalidation: RedrawInvalidation::CurrentWindow,
            ..Outcome::default()
        }
    } else {
        Outcome::default()
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

fn select_similar(editor: &mut Editor, ctx: CommandContext) -> Outcome {
    use text::{Point, Selection, SelectionGoal, ToOffset, ToPoint};
    use vim_buffer::{BufferText, TextSearch};
    use vim_regex::{CompileOptions, EditorOptions, Regex};
    use crate::kernel::buffer::registers::{Register, RegisterKind, RegisterName};

    let ignorecase = editor.global_options().ignorecase;
    let (win, buffer) = editor.window_and_buffer_mut(ctx.window);
    let text_buf = buffer.as_text_buffer();
    let primary = win.selections().primary().clone();
    let primary_start_off = primary.start.to_offset(text_buf);
    let primary_end_off = primary.end.to_offset(text_buf);

    if primary_start_off == primary_end_off {
        let point = primary.head().to_point(text_buf);
        let row_text = text_buf.row_text(point.row);
        if let Some((start_col, len, word_str)) = row_text.find_word(point.column as usize) {
            let word = word_str.to_string();
            let start_pt = Point::new(point.row, start_col as u32);
            let end_pt = Point::new(point.row, (start_col + len) as u32);
            let start_anchor = text_buf.anchor_before(start_pt.to_offset(text_buf));
            let end_anchor = text_buf.anchor_before(end_pt.to_offset(text_buf));

            let selected = Selection {
                id: primary.id,
                start: start_anchor,
                end: end_anchor,
                reversed: false,
                goal: SelectionGoal::None,
            };
            let _ = win.selections_mut().replace_primary(selected);
            win.selections_mut().point = end_pt;

            let escaped = super::search::regex_escape(&word);
            let pattern = format!("\\<{}\\>", escaped);
            let _ = editor.registers_mut().set(
                RegisterName::Search,
                Register {
                    text: pattern,
                    kind: RegisterKind::Character,
                },
            );

            return Outcome {
                invalidation: RedrawInvalidation::CurrentWindow,
                ..Outcome::default()
            };
        }
        Outcome::default()
    } else {
        let low = primary_start_off.min(primary_end_off);
        let high = primary_start_off.max(primary_end_off);
        let target_text: String = text_buf.as_rope().chunks_in_range(low..high).collect();
        if target_text.is_empty() {
            return Outcome::default();
        }

        let is_word = target_text
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_');
        let pattern = if is_word {
            format!("\\<{}\\>", super::search::regex_escape(&target_text))
        } else {
            super::search::regex_escape(&target_text)
        };

        let compile_opts = CompileOptions {
            editor: EditorOptions {
                ignore_case: ignorecase,
                smart_case: false,
                ..EditorOptions::default()
            },
            ..CompileOptions::default()
        };

        let Ok(regex) = Regex::compile(&pattern, compile_opts) else {
            return Outcome::default();
        };

        let last_sel = win
            .selections()
            .selections
            .iter()
            .max_by_key(|s| s.end.to_offset(text_buf).max(s.start.to_offset(text_buf)))
            .cloned()
            .unwrap_or(primary.clone());

        let max_off = last_sel
            .end
            .to_offset(text_buf)
            .max(last_sel.start.to_offset(text_buf));
        let last_point = text_buf.anchor_before(max_off).to_point(text_buf);

        let row_count = buffer.snapshot().row_count();
        if let Some((next_row, next_col, len)) = super::search::find_next_occurrence(
            buffer,
            &regex,
            last_point.row,
            last_point.column,
            true,
            row_count,
        ) {
            let next_start_pt = Point::new(next_row, next_col);
            let next_end_pt = Point::new(next_row, next_col + len as u32);
            let next_start_anchor = text_buf.anchor_before(next_start_pt.to_offset(text_buf));
            let next_end_anchor = text_buf.anchor_before(next_end_pt.to_offset(text_buf));

            let next_id = win.selections().id;
            win.selections_mut().id += 1;
            let new_selection = Selection {
                id: next_id,
                start: next_start_anchor,
                end: next_end_anchor,
                reversed: false,
                goal: SelectionGoal::None,
            };

            win.selections_mut().selections.push(new_selection);
            win.selections_mut().collapse_overlapping_cursors(text_buf);

            return Outcome {
                invalidation: RedrawInvalidation::CurrentWindow,
                ..Outcome::default()
            };
        }
        Outcome::default()
    }
}
