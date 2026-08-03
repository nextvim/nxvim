use crate::controller::actions::Mode;
use crate::editor::display::display_map::DisplayPoint;
use crate::editor::{Editor, document::BufferText};
use crate::services::search::TextSearch;
use crate::ui::colorscheme::ToCrossTerm;
use crate::ui::layout::Rect;
use crate::ui::views::View;
use text::ToPoint;

use std::io::Write;

use crossterm::{
    cursor::MoveTo,
    execute,
    style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor},
};

pub struct TextView {}

impl TextView {
    pub fn new() -> Self {
        TextView {}
    }
}

impl TextView {
    fn draw_textview<W: Write>(
        &self,
        w: &mut W,
        inner_rect: Rect,
        editor: &Editor,
        buffer_manager: &mut crate::editor::buffers::BufferManager,
        document: Option<&crate::editor::document::Document>,
        ui: &crate::ui::Ui,
    ) -> Result<Option<(u16, u16, Option<crate::ui::CursorShape>)>, Box<dyn std::error::Error>>
    {
        let mut cursor_pos = None;

        let document = document.expect("TextView requires document view state");
        let buffer = buffer_manager.find(document).unwrap();

        let display_snapshot = document.display_map.snapshot();
        let doc_buffer = &buffer.buffer;
        let row_count = display_snapshot.row_count();
        let end_line = (display_snapshot.scroll_y + inner_rect.height as u32).min(row_count);

        let gutter_width = document.gutter_width;

        let editor_fg = ui.theme_color("foreground", crossterm::style::Color::White);
        let editor_bg = ui.theme_color("background", crossterm::style::Color::Black);
        let selection_bg = ui.theme_color("selection", editor_bg);
        let gutter_fg = ui.theme_color("gutter_foreground", editor_fg);
        let gutter_bg = ui.theme_color("gutter", editor_bg);
        let find_fg = ui.theme_color("find_highlight_foreground", editor_fg);
        let find_bg = ui.theme_color("find_highlight", selection_bg);

        let mut prev_line_number = -1;
        let mut screen_row = inner_rect.y;

        // Scrollbar metrics
        let track_bg = gutter_bg;
        let handle_bg = selection_bg;

        let cursor_row = document.selections().first().map(|sel| {
            display_snapshot.point_to_display_point(sel.head().to_point(&buffer.buffer)).row()
        });
        let scrollbar = crate::ui::renderer::Scrollbar::new(
            document.show_scrollbar,
            inner_rect,
            row_count,
            display_snapshot.scroll_y,
            cursor_row,
        );

        for row in display_snapshot.scroll_y..end_line {
            {
                execute!(w, MoveTo(inner_rect.x, screen_row)).unwrap();

                // line number
                if editor.show_line_numbers && document.show_gutter {
                    let line_number = display_snapshot.buffer_row_for_display_row(row);
                    execute!(w, crossterm::style::SetForegroundColor(gutter_fg)).unwrap();
                    execute!(w, crossterm::style::SetBackgroundColor(gutter_bg)).unwrap();
                    if prev_line_number != line_number as i32 {
                        print!("{:>width$} ", (line_number + 1), width = gutter_width - 1);
                    } else {
                        print!("{}", " ".repeat(gutter_width));
                    }
                    prev_line_number = line_number as i32;
                }

                let text = display_snapshot.line_text(row) + " ";

                let mut match_ranges = Vec::<(usize, usize)>::new();
                let mut match_idx = 0usize;
                if document.show_pattern_match {
                    let mut matches = Vec::<(usize, usize, &str)>::new();
                    if let Some(ref regex) = editor.search_regex {
                        matches = text.as_str().find_pattern(regex);
                    }

                    // Convert byte-indexed matches into character-indexed ranges for rendering
                    match_ranges = matches
                        .iter()
                        .map(|(byte_start, byte_len, _)| {
                            let byte_end = *byte_start + *byte_len;
                            let start_char = text[..*byte_start].chars().count();
                            let end_char = text[..byte_end].chars().count();
                            (start_char, end_char)
                        })
                        .collect();
                }

                let mut x_scroll = display_snapshot.scroll_x;
                let mut cols_remaining = (inner_rect.width as usize).saturating_sub(gutter_width);

                let mut curr_x = inner_rect.x + gutter_width as u16;

                let mut byte_column = 0;
                for (column, ch) in text.chars().enumerate() {
                    let orig_point = display_snapshot
                        .display_point_to_point(DisplayPoint::new(row, byte_column as u32));
                    byte_column += ch.len_utf8();

                    // Determine if current column is within a search match range
                    let mut in_match = false;

                    while match_idx < match_ranges.len() && column >= match_ranges[match_idx].1 {
                        match_idx += 1;
                    }
                    if match_idx < match_ranges.len() {
                        let (s, e) = match_ranges[match_idx];
                        if column >= s && column < e {
                            in_match = true;
                        }
                    }

                    let mut fg = editor_fg;
                    let mut bg = editor_bg;

                    if editor.syntax {
                        if let Some(style_cache) = document.hl.render_row(orig_point.row) {
                            if let Some(span) = style_cache.styles.iter().find(|span| {
                                orig_point.column >= span.start && orig_point.column < span.end
                            }) {
                                fg = span.style.color;
                            }
                        }
                    }

                    // Apply search match background if not in a selection
                    if in_match {
                        fg = find_fg;
                        bg = find_bg;
                    }

                    let (selected, mut selected_line, at_cursor) = document
                        .selections()
                        .is_selected(orig_point.row, orig_point.column, &doc_buffer);
                    if selected && (editor.mode != Mode::Command) {
                        bg = selection_bg;
                    }
                    selected_line = selected_line && editor.mode == Mode::VisualLine;
                    if selected_line {
                        bg = selection_bg;
                    }

                    if at_cursor {
                        bg = selection_bg;
                        cursor_pos = Some((curr_x, screen_row));
                    }

                    if x_scroll > 0 {
                        x_scroll = x_scroll.saturating_sub(1);
                    } else {
                        let is_scrollbar = scrollbar.is_scrollbar(curr_x, screen_row);
                        let bg_color = if is_scrollbar {
                            if scrollbar.is_handle(curr_x, screen_row) { handle_bg } else { track_bg }
                        } else {
                            bg
                        };

                        execute!(w, crossterm::style::SetForegroundColor(fg)).unwrap();
                        execute!(w, crossterm::style::SetBackgroundColor(bg_color)).unwrap();

                        match ch {
                            '\t' => {
                                for _i in 0..4 {
                                    // Tab size of 4
                                    let is_scrollbar_tab = scrollbar.is_scrollbar(curr_x, screen_row);
                                    let cell_bg = if is_scrollbar_tab {
                                        if scrollbar.is_handle(curr_x, screen_row) { handle_bg } else { track_bg }
                                    } else if at_cursor
                                        && editor.mode != Mode::Insert
                                        && editor.mode != Mode::Command
                                    {
                                        editor_bg
                                    } else {
                                        bg
                                    };
                                    execute!(w, crossterm::style::SetBackgroundColor(cell_bg))
                                        .unwrap();
                                    print!(" ");
                                    curr_x += 1;
                                    cols_remaining = cols_remaining.saturating_sub(1);
                                }
                            }
                            _ => {
                                print!("{}", ch);
                                curr_x += 1;
                                cols_remaining = cols_remaining.saturating_sub(1);
                            }
                        }
                    }

                    if cols_remaining <= 0 {
                        break;
                    }
                }

                for x in 0..cols_remaining {
                    let is_scrollbar = scrollbar.is_scrollbar(curr_x, screen_row);
                    let bg_color = if is_scrollbar {
                        if scrollbar.is_handle(curr_x, screen_row) { handle_bg } else { track_bg }
                    } else {
                        editor_bg
                    };
                    execute!(w, crossterm::style::SetBackgroundColor(bg_color)).unwrap();
                    print!(" ");
                    curr_x += 1;
                }

                screen_row += 1;
                if screen_row >= inner_rect.y + inner_rect.height {
                    break;
                }
            }
        }

        while screen_row < inner_rect.y + inner_rect.height {
            execute!(w, MoveTo(inner_rect.x, screen_row)).unwrap();

            // Gutter/line numbers area for empty lines
            if editor.show_line_numbers && document.show_gutter {
                execute!(w, crossterm::style::SetBackgroundColor(gutter_bg)).unwrap();
                print!("{}", " ".repeat(gutter_width));
            }

            // The rest of the line
            let mut curr_x = inner_rect.x + gutter_width as u16;
            let mut cols_remaining = (inner_rect.width as usize).saturating_sub(gutter_width);

            for _ in 0..cols_remaining {
                let is_scrollbar = scrollbar.is_scrollbar(curr_x, screen_row);
                let bg_color = if is_scrollbar {
                    if scrollbar.is_handle(curr_x, screen_row) { handle_bg } else { track_bg }
                } else {
                    editor_bg
                };
                execute!(w, crossterm::style::SetBackgroundColor(bg_color)).unwrap();
                print!(" ");
                curr_x += 1;
            }

            screen_row += 1;
        }

        let cursor_shape = match editor.mode {
            Mode::Insert | Mode::Command => Some(crate::ui::CursorShape::Line),
            _ => Some(crate::ui::CursorShape::Block),
        };
        Ok(cursor_pos.map(|(x, y)| (x, y, cursor_shape)))
    }
}

impl View for TextView {
    fn draw(
        &self,
        mut w: &mut dyn Write,
        rect: Rect,
        editor: &Editor,
        buffer_manager: &mut crate::editor::buffers::BufferManager,
        document: Option<&crate::editor::document::Document>,
        ui: &crate::ui::Ui,
    ) -> Result<Option<(u16, u16, Option<crate::ui::CursorShape>)>, Box<dyn std::error::Error>>
    {
        self.draw_textview(&mut w, rect, editor, buffer_manager, document, ui)
    }
}
