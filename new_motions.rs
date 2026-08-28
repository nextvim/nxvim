//! `h`/`j`/`k`/`l` and friends.
//!
//! Motions never mutate text, so they don't go through
//! `kernel::transaction` — they update the window's `SelectionSet` in place
//! against the current buffer's text.
//!
//! // TODO(7.7): Search `/` and `?` must call super::marks_and_jumps::record_jump before moving cursor.

use crate::kernel::{
    Editor,
    ids::WindowId,
    outcome::{Outcome, RedrawInvalidation},
    window::Window,
};
use text::ToPoint;

fn moved(editor: &mut Editor, window: WindowId, select: bool) -> Outcome {
    let (win, buffer) = editor.window_and_buffer_mut(window);
    let primary_sel = win.selections().primary();
    let head_point = primary_sel.head().to_point(buffer.as_text_buffer());
    win.scroll_to_line(head_point.row);

    let _ = select;
    Outcome {
        invalidation: RedrawInvalidation::CurrentWindow,
        ..Outcome::default()
    }
}

pub fn move_left(editor: &mut Editor, window: WindowId, count: u32, select: bool) -> Outcome {
    let (win, buffer) = editor.window_and_buffer_mut(window);
    win.selections_mut()
        .move_left(select, count, buffer.as_text_buffer());
    moved(editor, window, select)
}

pub fn move_right(editor: &mut Editor, window: WindowId, count: u32, select: bool) -> Outcome {
    let (win, buffer) = editor.window_and_buffer_mut(window);
    win.selections_mut()
        .move_right(select, count, buffer.as_text_buffer());
    moved(editor, window, select)
}

pub fn move_up(editor: &mut Editor, window: WindowId, count: u32, select: bool) -> Outcome {
    let (win, buffer) = editor.window_and_buffer_mut(window);
    win.selections_mut()
        .move_up(select, count, buffer.as_text_buffer());
    moved(editor, window, select)
}

pub fn move_down(editor: &mut Editor, window: WindowId, count: u32, select: bool) -> Outcome {
    let (win, buffer) = editor.window_and_buffer_mut(window);
    win.selections_mut()
        .move_down(select, count, buffer.as_text_buffer());
    moved(editor, window, select)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CharSearch {
    pub ch: char,
    pub forward: bool,
    pub till: bool,
}

pub fn find_character(
    editor: &mut Editor,
    window: WindowId,
    count: u32,
    ch: char,
    forward: bool,
    till: bool,
    select: bool,
) -> Outcome {
    let (win, buffer) = editor.window_and_buffer_mut(window);
    win.selections_mut()
        .find_character(select, count, ch, forward, till, buffer.as_text_buffer());
    let search = CharSearch { ch, forward, till };
    editor.set_last_char_search(search);
    moved(editor, window, select)
}

pub fn repeat_character_search(
    editor: &mut Editor,
    window: WindowId,
    count: u32,
    reverse: bool,
    select: bool,
) -> Outcome {
    let Some(search) = editor.last_char_search() else {
        return Outcome::default();
    };
    let forward = if reverse {
        !search.forward
    } else {
        search.forward
    };
    let (win, buffer) = editor.window_and_buffer_mut(window);
    win.selections_mut().find_character(
        select,
        count,
        search.ch,
        forward,
        search.till,
        buffer.as_text_buffer(),
    );
    moved(editor, window, select)
}

pub fn move_to_word(editor: &mut Editor, window: WindowId, count: u32, select: bool) -> Outcome {
    let (win, buffer) = editor.window_and_buffer_mut(window);
    // `Action::MoveToWord` is Vim's forward `w`, but the matching-sounding
    // `SelectionSet::move_to_word` is a different thing (the word
    // *containing* the cursor) that doesn't advance if the cursor is
    // already at a word start -- `move_to_next_word` is the actual
    // forward-progressing motion (see `operators.rs`'s `motion_target`,
    // which already documents/uses this same distinction for `dw`).
    win.selections_mut()
        .move_to_next_word(select, count, buffer.as_text_buffer());
    moved(editor, window, select)
}

pub fn move_to_previous_word(
    editor: &mut Editor,
    window: WindowId,
    count: u32,
    select: bool,
) -> Outcome {
    let (win, buffer) = editor.window_and_buffer_mut(window);
    win.selections_mut()
        .move_to_previous_word(select, count, buffer.as_text_buffer());
    moved(editor, window, select)
}

pub fn move_to_word_end(
    editor: &mut Editor,
    window: WindowId,
    count: u32,
    select: bool,
) -> Outcome {
    let (win, buffer) = editor.window_and_buffer_mut(window);
    win.selections_mut()
        .move_to_word_end(select, count, buffer.as_text_buffer());
    moved(editor, window, select)
}

pub fn move_to_previous_word_end(
    editor: &mut Editor,
    window: WindowId,
    count: u32,
    select: bool,
) -> Outcome {
    let (win, buffer) = editor.window_and_buffer_mut(window);
    win.selections_mut()
        .move_to_previous_word_end(select, count, buffer.as_text_buffer());
    moved(editor, window, select)
}

pub fn move_to_big_word(
    editor: &mut Editor,
    window: WindowId,
    count: u32,
    select: bool,
) -> Outcome {
    let (win, buffer) = editor.window_and_buffer_mut(window);
    win.selections_mut()
        .move_to_big_word(select, count, buffer.as_text_buffer());
    moved(editor, window, select)
}

pub fn move_to_previous_big_word(
    editor: &mut Editor,
    window: WindowId,
    count: u32,
    select: bool,
) -> Outcome {
    let (win, buffer) = editor.window_and_buffer_mut(window);
    win.selections_mut()
        .move_to_previous_big_word(select, count, buffer.as_text_buffer());
    moved(editor, window, select)
}

pub fn move_to_big_word_end(
    editor: &mut Editor,
    window: WindowId,
    count: u32,
    select: bool,
) -> Outcome {
    let (win, buffer) = editor.window_and_buffer_mut(window);
    win.selections_mut()
        .move_to_big_word_end(select, count, buffer.as_text_buffer());
    moved(editor, window, select)
}

pub fn move_to_previous_big_word_end(
    editor: &mut Editor,
    window: WindowId,
    count: u32,
    select: bool,
) -> Outcome {
    let (win, buffer) = editor.window_and_buffer_mut(window);
    win.selections_mut()
        .move_to_previous_big_word_end(select, count, buffer.as_text_buffer());
    moved(editor, window, select)
}

pub fn move_to_start_of_document(editor: &mut Editor, window: WindowId, select: bool) -> Outcome {
    super::marks_and_jumps::record_jump(editor, window);
    let (win, buffer) = editor.window_and_buffer_mut(window);
    win.selections_mut()
        .move_to_start_of_document(select, buffer.as_text_buffer());
    moved(editor, window, select)
}

pub fn move_to_end_of_document(editor: &mut Editor, window: WindowId, select: bool) -> Outcome {
    super::marks_and_jumps::record_jump(editor, window);
    let (win, buffer) = editor.window_and_buffer_mut(window);
    win.selections_mut()
        .move_to_end_of_document(select, buffer.as_text_buffer());
    moved(editor, window, select)
}

pub fn move_to_line(editor: &mut Editor, window: WindowId, line: u32, select: bool) -> Outcome {
    super::marks_and_jumps::record_jump(editor, window);
    let (win, buffer) = editor.window_and_buffer_mut(window);
    win.selections_mut()
        .move_to_line(select, line, buffer.as_text_buffer());
    moved(editor, window, select)
}

pub fn move_to_start_of_line(editor: &mut Editor, window: WindowId, select: bool) -> Outcome {
    let (win, buffer) = editor.window_and_buffer_mut(window);
    win.selections_mut()
        .move_to_start_of_line(select, buffer.as_text_buffer());
    moved(editor, window, select)
}

pub fn move_to_start_of_line_non_space(
    editor: &mut Editor,
    window: WindowId,
    select: bool,
) -> Outcome {
    let (win, buffer) = editor.window_and_buffer_mut(window);
    win.selections_mut()
        .move_to_start_of_line_non_space(select, buffer.as_text_buffer());
    moved(editor, window, select)
}

pub fn move_to_end_of_line(editor: &mut Editor, window: WindowId, select: bool) -> Outcome {
    let (win, buffer) = editor.window_and_buffer_mut(window);
    win.selections_mut()
        .move_to_end_of_line(select, buffer.as_text_buffer());
    moved(editor, window, select)
}

pub fn move_to_start_of_previous_line(
    editor: &mut Editor,
    window: WindowId,
    select: bool,
) -> Outcome {
    let (win, buffer) = editor.window_and_buffer_mut(window);
    win.selections_mut()
        .move_to_start_of_previous_line(select, buffer.as_text_buffer());
    moved(editor, window, select)
}

pub fn move_to_end_of_previous_line(
    editor: &mut Editor,
    window: WindowId,
    select: bool,
) -> Outcome {
    let (win, buffer) = editor.window_and_buffer_mut(window);
    win.selections_mut()
        .move_to_end_of_previous_line(select, buffer.as_text_buffer());
    moved(editor, window, select)
}

pub fn move_to_start_of_next_line(editor: &mut Editor, window: WindowId, select: bool) -> Outcome {
    let (win, buffer) = editor.window_and_buffer_mut(window);
    win.selections_mut()
        .move_to_start_of_next_line(select, buffer.as_text_buffer());
    moved(editor, window, select)
}

pub fn move_to_end_of_next_line(editor: &mut Editor, window: WindowId, select: bool) -> Outcome {
    let (win, buffer) = editor.window_and_buffer_mut(window);
    win.selections_mut()
        .move_to_end_of_next_line(select, buffer.as_text_buffer());
    moved(editor, window, select)
}

pub fn move_to_previous_paragraph(
    editor: &mut Editor,
    window: WindowId,
    count: u32,
    select: bool,
) -> Outcome {
    let (win, buffer) = editor.window_and_buffer_mut(window);
    win.selections_mut()
        .move_to_previous_paragraph(select, count, buffer.as_text_buffer());
    moved(editor, window, select)
}

pub fn move_to_next_paragraph(
    editor: &mut Editor,
    window: WindowId,
    count: u32,
    select: bool,
) -> Outcome {
    let (win, buffer) = editor.window_and_buffer_mut(window);
    win.selections_mut()
        .move_to_next_paragraph(select, count, buffer.as_text_buffer());
    moved(editor, window, select)
}

pub fn move_to_previous_sentence(
    editor: &mut Editor,
    window: WindowId,
    count: u32,
    select: bool,
) -> Outcome {
    let (win, buffer) = editor.window_and_buffer_mut(window);
    win.selections_mut()
        .move_to_previous_sentence(select, count, buffer.as_text_buffer());
    moved(editor, window, select)
}

pub fn move_to_next_sentence(
    editor: &mut Editor,
    window: WindowId,
    count: u32,
    select: bool,
) -> Outcome {
    let (win, buffer) = editor.window_and_buffer_mut(window);
    win.selections_mut()
        .move_to_next_sentence(select, count, buffer.as_text_buffer());
    moved(editor, window, select)
}

pub fn move_to_matching_delimiter(editor: &mut Editor, window: WindowId, select: bool) -> Outcome {
    super::marks_and_jumps::record_jump(editor, window);
    let (win, buffer) = editor.window_and_buffer_mut(window);
    win.selections_mut()
        .move_to_matching_delimiter(select, buffer.as_text_buffer());
    moved(editor, window, select)
}

pub fn move_to_column(editor: &mut Editor, window: WindowId, column: u32, select: bool) -> Outcome {
    let (win, buffer) = editor.window_and_buffer_mut(window);
    win.selections_mut()
        .move_to_column(select, column, buffer.as_text_buffer());
    moved(editor, window, select)
}

pub fn move_to_last_non_whitespace(
    editor: &mut Editor,
    window: WindowId,
    count: u32,
    select: bool,
) -> Outcome {
    let (win, buffer) = editor.window_and_buffer_mut(window);
    win.selections_mut()
        .move_to_last_non_whitespace(select, count, buffer.as_text_buffer());
    moved(editor, window, select)
}

pub fn move_to_screen_top(editor: &mut Editor, window: WindowId, select: bool) -> Outcome {
    super::marks_and_jumps::record_jump(editor, window);
    let (win, buffer) = editor.window_and_buffer_mut(window);
    let target_line = win.scroll_top();
    win.selections_mut()
        .move_to_line(select, target_line + 1, buffer.as_text_buffer());
    moved(editor, window, select)
}

pub fn move_to_screen_middle(editor: &mut Editor, window: WindowId, select: bool) -> Outcome {
    super::marks_and_jumps::record_jump(editor, window);
    let (win, buffer) = editor.window_and_buffer_mut(window);
    let height = win.viewport_height().max(1);
    let target_line = win.scroll_top() + height / 2;
    win.selections_mut()
        .move_to_line(select, target_line + 1, buffer.as_text_buffer());
    moved(editor, window, select)
}

pub fn move_to_screen_bottom(editor: &mut Editor, window: WindowId, select: bool) -> Outcome {
    super::marks_and_jumps::record_jump(editor, window);
    let (win, buffer) = editor.window_and_buffer_mut(window);
    let height = win.viewport_height().max(1);
    let target_line = win.scroll_top() + height.saturating_sub(1);
    win.selections_mut()
        .move_to_line(select, target_line + 1, buffer.as_text_buffer());
    moved(editor, window, select)
}

/// Redraw report shared by every viewport-only mutation in this module:
/// scrolling never touches buffer text, so it's always a plain
/// `CurrentWindow` invalidation.
fn window_redraw() -> Outcome {
    Outcome {
        invalidation: RedrawInvalidation::CurrentWindow,
        ..Outcome::default()
    }
}

/// Clamps `win`'s scroll top to `win.scroll_top() + step` (`down`) or
/// `win.scroll_top() - step` (`!down`), bounded to `[0, last row]`.
fn scrolled_top(win: &Window, buffer: &vim_buffer::Buffer, step: u32, down: bool) -> u32 {
    if down {
        let max_scroll = buffer.as_text_buffer().row_count().saturating_sub(1);
        (win.scroll_top() + step).min(max_scroll)
    } else {
        win.scroll_top().saturating_sub(step)
    }
}

pub fn scroll_line_down(editor: &mut Editor, window: WindowId, count: u32) -> Outcome {
    let (win, buffer) = editor.window_and_buffer_mut(window);
    let new_scroll = scrolled_top(win, buffer, count, true);
    win.set_scroll_top(new_scroll);

    // Keep the cursor on screen; `Ctrl-e` never moves it otherwise.
    let head_row = win
        .selections()
        .primary()
        .head()
        .to_point(buffer.as_text_buffer())
        .row;
    if head_row < new_scroll {
        win.selections_mut()
            .move_to_line(false, new_scroll + 1, buffer.as_text_buffer());
    }
    window_redraw()
}

pub fn scroll_line_up(editor: &mut Editor, window: WindowId, count: u32) -> Outcome {
    let (win, buffer) = editor.window_and_buffer_mut(window);
    let new_scroll = scrolled_top(win, buffer, count, false);
    win.set_scroll_top(new_scroll);

    // Keep the cursor on screen; `Ctrl-y` never moves it otherwise.
    let head_row = win
        .selections()
        .primary()
        .head()
        .to_point(buffer.as_text_buffer())
        .row;
    let bottom_visible = new_scroll + win.viewport_height().max(1).saturating_sub(1);
    if head_row > bottom_visible {
        win.selections_mut()
            .move_to_line(false, bottom_visible + 1, buffer.as_text_buffer());
    }
    window_redraw()
}

/// Shared by `Ctrl-d`/`Ctrl-u`/`Ctrl-f`/`Ctrl-b`: unlike `Ctrl-e`/`Ctrl-y`,
/// these always move both the viewport and the cursor by `step` lines.
fn scroll_and_follow_cursor(win: &mut Window, buffer: &vim_buffer::Buffer, step: u32, down: bool) {
    let new_scroll = scrolled_top(win, buffer, step, down);
    win.set_scroll_top(new_scroll);

    let head_row = win
        .selections()
        .primary()
        .head()
        .to_point(buffer.as_text_buffer())
        .row;
    let new_cursor_row = if down {
        let max_scroll = buffer.as_text_buffer().row_count().saturating_sub(1);
        (head_row + step).min(max_scroll)
    } else {
        head_row.saturating_sub(step)
    };
    win.selections_mut()
        .move_to_line(false, new_cursor_row + 1, buffer.as_text_buffer());
    win.scroll_to_line(new_cursor_row);
}

pub fn scroll_half_page_down(editor: &mut Editor, window: WindowId, count: u32) -> Outcome {
    let (win, buffer) = editor.window_and_buffer_mut(window);
    let step = (win.viewport_height().max(1) / 2) * count;
    scroll_and_follow_cursor(win, buffer, step, true);
    window_redraw()
}

pub fn scroll_half_page_up(editor: &mut Editor, window: WindowId, count: u32) -> Outcome {
    let (win, buffer) = editor.window_and_buffer_mut(window);
    let step = (win.viewport_height().max(1) / 2) * count;
    scroll_and_follow_cursor(win, buffer, step, false);
    window_redraw()
}

pub fn scroll_forward(editor: &mut Editor, window: WindowId, count: u32) -> Outcome {
    let (win, buffer) = editor.window_and_buffer_mut(window);
    let step = win.viewport_height().max(1).saturating_sub(2) * count;
    scroll_and_follow_cursor(win, buffer, step, true);
    window_redraw()
}

pub fn scroll_backward(editor: &mut Editor, window: WindowId, count: u32) -> Outcome {
    let (win, buffer) = editor.window_and_buffer_mut(window);
    let step = win.viewport_height().max(1).saturating_sub(2) * count;
    scroll_and_follow_cursor(win, buffer, step, false);
    window_redraw()
}

pub fn center_cursor_line(editor: &mut Editor, window: WindowId) -> Outcome {
    let (win, buffer) = editor.window_and_buffer_mut(window);
    let head_row = win
        .selections()
        .primary()
        .head()
        .to_point(buffer.as_text_buffer())
        .row;
    let height = win.viewport_height().max(1);
    win.set_scroll_top(head_row.saturating_sub(height / 2));
    window_redraw()
}

pub fn cursor_line_top(editor: &mut Editor, window: WindowId) -> Outcome {
    let (win, buffer) = editor.window_and_buffer_mut(window);
    let head_row = win
        .selections()
        .primary()
        .head()
        .to_point(buffer.as_text_buffer())
        .row;
    win.set_scroll_top(head_row);
    window_redraw()
}

pub fn cursor_line_bottom(editor: &mut Editor, window: WindowId) -> Outcome {
    let (win, buffer) = editor.window_and_buffer_mut(window);
    let head_row = win
        .selections()
        .primary()
        .head()
        .to_point(buffer.as_text_buffer())
        .row;
    let height = win.viewport_height().max(1);
    win.set_scroll_top(head_row.saturating_add(1).saturating_sub(height));
    window_redraw()
}

pub fn move_page_up(editor: &mut Editor, window: WindowId, select: bool) -> Outcome {
    super::marks_and_jumps::record_jump(editor, window);
    let (win, buffer) = editor.window_and_buffer_mut(window);
    // TODO
    moved(editor, window, select)
}

pub fn move_page_down(editor: &mut Editor, window: WindowId, select: bool) -> Outcome {
    super::marks_and_jumps::record_jump(editor, window);
    let (win, buffer) = editor.window_and_buffer_mut(window);
    // TODO
    moved(editor, window, select)
}

pub fn move_to_next_function(editor: &mut Editor, window: WindowId, select: bool) -> Outcome {
    super::marks_and_jumps::record_jump(editor, window);
    let (win, buffer) = editor.window_and_buffer_mut(window);
    // TODO
    moved(editor, window, select)
}

pub fn move_to_previous_function(editor: &mut Editor, window: WindowId, select: bool) -> Outcome {
    super::marks_and_jumps::record_jump(editor, window);
    let (win, buffer) = editor.window_and_buffer_mut(window);
    // TODO
    moved(editor, window, select)
}

pub fn move_to_next_class(editor: &mut Editor, window: WindowId, select: bool) -> Outcome {
    super::marks_and_jumps::record_jump(editor, window);
    let (win, buffer) = editor.window_and_buffer_mut(window);
    // TODO
    moved(editor, window, select)
}

pub fn move_to_previous_class(editor: &mut Editor, window: WindowId, select: bool) -> Outcome {
    super::marks_and_jumps::record_jump(editor, window);
    let (win, buffer) = editor.window_and_buffer_mut(window);
    // TODO
    moved(editor, window, select)
}

pub fn move_to_next_argument(editor: &mut Editor, window: WindowId, select: bool) -> Outcome {
    super::marks_and_jumps::record_jump(editor, window);
    let (win, buffer) = editor.window_and_buffer_mut(window);
    // TODO
    moved(editor, window, select)
}

pub fn move_to_previous_argument(editor: &mut Editor, window: WindowId, select: bool) -> Outcome {
    super::marks_and_jumps::record_jump(editor, window);
    let (win, buffer) = editor.window_and_buffer_mut(window);
    // TODO
    moved(editor, window, select)
}

pub fn move_to_next_block(editor: &mut Editor, window: WindowId, select: bool) -> Outcome {
    super::marks_and_jumps::record_jump(editor, window);
    let (win, buffer) = editor.window_and_buffer_mut(window);
    // TODO
    moved(editor, window, select)
}

pub fn move_to_previous_block(editor: &mut Editor, window: WindowId, select: bool) -> Outcome {
    super::marks_and_jumps::record_jump(editor, window);
    let (win, buffer) = editor.window_and_buffer_mut(window);
    // TODO
    moved(editor, window, select)
}

pub fn move_to_block_start(editor: &mut Editor, window: WindowId, select: bool) -> Outcome {
    super::marks_and_jumps::record_jump(editor, window);
    let (win, buffer) = editor.window_and_buffer_mut(window);
    // TODO
    moved(editor, window, select)
}

pub fn move_to_block_end(editor: &mut Editor, window: WindowId, select: bool) -> Outcome {
    super::marks_and_jumps::record_jump(editor, window);
    let (win, buffer) = editor.window_and_buffer_mut(window);
    // TODO
    moved(editor, window, select)
}
