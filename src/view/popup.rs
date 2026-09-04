//! Popup window rendering module.
//!
//! Renders popup outer box frames (border, title, close button [X], padding)
//! and popup buffer text content onto a `ScreenBuffer`.

use crate::app::view_sync::PopupViewSnapshot;
use vim_ui::renderer::{Cell, ScreenBuffer};
use vim_ui::types::Color;

/// Renders a single popup window onto a ScreenBuffer cell grid.
pub fn render_popup(popup: &PopupViewSnapshot, buffer: &mut ScreenBuffer) {
    let rect = popup.rect;
    let outer_x = rect.outer_col.saturating_sub(1) as u16;
    let outer_y = rect.outer_line.saturating_sub(1) as u16;
    let outer_w = rect.outer_width as u16;
    let outer_h = rect.outer_height as u16;

    if outer_w == 0 || outer_h == 0 {
        return;
    }

    let border_chars = popup.border_chars.unwrap_or([
        '┌', '─', '┐', '│', '┘', '─', '└', '│',
    ]);
    // 0: top-left, 1: top, 2: top-right, 3: right, 4: bottom-right, 5: bottom, 6: bottom-left, 7: left

    let bg_color = Color::Reset;
    let border_fg = Color::Blue;
    let text_fg = Color::Reset;

    // 1. Draw frame (border, padding, background)
    for r in 0..outer_h {
        for c in 0..outer_w {
            let px = outer_x + c;
            let py = outer_y + r;

            let is_top = r == 0 && popup.border.top;
            let is_bottom = r == outer_h - 1 && popup.border.bottom;
            let is_left = c == 0 && popup.border.left;
            let is_right = c == outer_w - 1 && popup.border.right;

            let symbol = if is_top && is_left {
                border_chars[0]
            } else if is_top && is_right {
                border_chars[2]
            } else if is_bottom && is_left {
                border_chars[6]
            } else if is_bottom && is_right {
                border_chars[4]
            } else if is_top {
                border_chars[1]
            } else if is_bottom {
                border_chars[5]
            } else if is_left {
                border_chars[7]
            } else if is_right {
                border_chars[3]
            } else {
                ' '
            };

            let fg = if is_top || is_bottom || is_left || is_right {
                border_fg
            } else {
                text_fg
            };

            buffer.set_cell(px, py, Cell { symbol, fg, bg: bg_color });
        }
    }

    // 2. Draw title on top border if title exists and border.top is enabled
    if popup.border.top {
        if let Some(ref title) = popup.title {
            let title_text = format!(" {title} ");
            let title_len = title_text.chars().count() as u16;
            if title_len < outer_w {
                let start_c = (outer_w - title_len) / 2;
                for (i, ch) in title_text.chars().enumerate() {
                    let px = outer_x + start_c + i as u16;
                    buffer.set_cell(
                        px,
                        outer_y,
                        Cell {
                            symbol: ch,
                            fg: Color::Yellow,
                            bg: bg_color,
                        },
                    );
                }
            }
        }
    }

    // 3. Draw close button '[X]' at top-right corner if close_button is enabled
    if popup.close_button && outer_w >= 4 && popup.border.top {
        let close_str = "[X]";
        let start_c = outer_w.saturating_sub(4);
        for (i, ch) in close_str.chars().enumerate() {
            let px = outer_x + start_c + i as u16;
            buffer.set_cell(
                px,
                outer_y,
                Cell {
                    symbol: ch,
                    fg: Color::Red,
                    bg: bg_color,
                },
            );
        }
    }

    // 4. Render popup buffer text inside core content box
    let core_x = rect.core_col.saturating_sub(1) as u16;
    let core_y = rect.core_line.saturating_sub(1) as u16;
    let core_w = rect.core_width as u16;
    let core_h = rect.core_height as u16;

    let row_count = popup.snapshot.row_count();
    let start_row = popup.scroll_top;

    for r in 0..core_h {
        let buffer_row = start_row + r as u32;
        if buffer_row >= row_count {
            break;
        }

        let start_offset = popup.snapshot.point_to_offset(text::Point::new(buffer_row, 0));
        let line_len = popup.snapshot.line_len(buffer_row);
        let end_offset = popup.snapshot.point_to_offset(text::Point::new(buffer_row, line_len));
        let line_text = popup.snapshot.as_rope().slice(start_offset..end_offset).to_string();


        for (c, ch) in line_text.chars().take(core_w as usize).enumerate() {
            let px = core_x + c as u16;
            let py = core_y + r;
            buffer.set_cell(
                px,
                py,
                Cell {
                    symbol: ch,
                    fg: text_fg,
                    bg: bg_color,
                },
            );
        }
    }
}

/// Renders all projected popup snapshots in z-index order onto the ScreenBuffer.
pub fn render_popups(popups: &[PopupViewSnapshot], buffer: &mut ScreenBuffer) {
    for popup in popups {
        render_popup(popup, buffer);
    }
}
