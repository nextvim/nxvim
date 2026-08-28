//! Minimal read-only render path.
//!
//! Draws the current buffer's visible lines and cursor position to the
//! terminal. This is a projection only — it reads `kernel::Editor` and never
//! mutates it, matching `RESCUE.md` Rule 4.7 (mutation and rendering stay
//! decoupled). Grows into the full statusline/tabline/command-line pipeline
//! (porting from `src_/view/textview.rs` etc.) once those milestones land.

use std::io::{self, Write};

use crossterm::{
    cursor, queue,
    style::Print,
    terminal::{Clear, ClearType},
};
use text::Point;

use crate::kernel::Editor;

pub fn render(out: &mut impl Write, editor: &Editor, status: &str) -> io::Result<()> {
    let buffer = editor.current_buffer();
    let window = editor.current_window();
    let snapshot = buffer.snapshot();
    let text_buffer = buffer.as_text_buffer();

    queue!(
        out,
        cursor::Hide,
        cursor::MoveTo(0, 0),
        Clear(ClearType::All)
    )?;

    let full_text: String = snapshot.chunks().collect();
    let mut last_row = 0u16;
    for (row, line) in full_text.split('\n').enumerate() {
        last_row = row as u16;
        queue!(out, cursor::MoveTo(0, last_row), Print(line))?;
    }

    // Temporary debug status line (mode + last resolved action) so motion
    // wiring can be visually confirmed before a real statusline exists.
    queue!(out, cursor::MoveTo(0, last_row + 2), Print(status))?;

    let head = window.selections().primary().head();
    let point: Point = text_buffer.summary_for_anchor(&head);
    queue!(
        out,
        cursor::MoveTo(point.column as u16, point.row as u16),
        cursor::Show
    )?;

    out.flush()
}
