//! Minimal read-only render path.

pub mod layout;

use std::io::{self, Write};

use crossterm::{
    cursor, queue,
    style::Print,
    terminal::{Clear, ClearType},
};
use text::Point;
use vim_ui::Rect;

use crate::kernel::Editor;

/// Renders all windows in the active tab page according to the layout tree.
pub fn render(out: &mut impl Write, editor: &Editor, status: &str, screen: Rect) -> io::Result<()> {
    let active_win_id = editor.current_context().window;
    let tab = editor.tabs().active();
    let rects = layout::layout(tab, screen);

    queue!(
        out,
        cursor::Hide,
        cursor::MoveTo(0, 0),
        Clear(ClearType::All)
    )?;

    for (&win_id, &rect) in &rects {
        let window = editor.window(win_id).expect("window must exist");
        let buffer = editor.buffer(window.buffer_id()).expect("buffer must exist");
        let snapshot = buffer.snapshot();
        let full_text: String = snapshot.chunks().collect();

        for (i, line) in full_text.split('\n').enumerate() {
            let row = rect.y + i as u16;
            if row >= rect.y + rect.height {
                break;
            }
            let display_line = if line.len() > rect.width as usize {
                &line[..rect.width as usize]
            } else {
                line
            };
            queue!(out, cursor::MoveTo(rect.x, row), Print(display_line))?;
        }
    }

    // Print status line at the bottom of the screen
    let status_row = screen.height.saturating_sub(1);
    queue!(out, cursor::MoveTo(0, status_row), Print(status))?;

    // Position cursor in the active window
    let active_window = editor.current_window();
    let active_buffer = editor.current_buffer();
    let text_buffer = active_buffer.as_text_buffer();
    let head = active_window.selections().primary().head();
    let point: Point = text_buffer.summary_for_anchor(&head);

    if let Some(&rect) = rects.get(&active_win_id) {
        let cursor_x = rect.x + point.column as u16;
        let cursor_y = rect.y + point.row as u16;
        if cursor_x < rect.x + rect.width && cursor_y < rect.y + rect.height {
            queue!(
                out,
                cursor::MoveTo(cursor_x, cursor_y),
                cursor::Show
            )?;
        }
    }

    out.flush()
}
