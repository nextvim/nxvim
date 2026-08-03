use crate::controller::actions::Mode;
use crate::editor::Editor;
use crate::editor::display::display_map::DisplayPoint;
use crate::services::search::TextSearch;
use vim_ui::Rect;
use crate::ui::views::{View, vim};
use std::io::Write;
use text::ToPoint;

pub struct TextView;

impl TextView {
    pub fn new() -> Self {
        Self
    }

    fn build_model(
        &self,
        rect: Rect,
        editor: &Editor,
        buffer_manager: &crate::editor::buffers::BufferManager,
        document: &crate::editor::document::Document,
        ui: &crate::ui::Ui,
    ) -> vim_ui::TextViewModel {
        let buffer = buffer_manager.find(document).unwrap();
        let display = document.display_map.snapshot();
        let total_rows = display.row_count();
        let end_row = (display.scroll_y + rect.height as u32).min(total_rows);
        let gutter_width = if editor.show_line_numbers && document.show_gutter {
            document.gutter_width
        } else {
            0
        };

        let editor_fg = ui.theme_color("foreground", crossterm::style::Color::White);
        let editor_bg = ui.theme_color("background", crossterm::style::Color::Black);
        let selection_bg = ui.theme_color("selection", editor_bg);
        let gutter_fg = ui.theme_color("gutter_foreground", editor_fg);
        let gutter_bg = ui.theme_color("gutter", editor_bg);
        let find_fg = ui.theme_color("find_highlight_foreground", editor_fg);
        let find_bg = ui.theme_color("find_highlight", selection_bg);
        let default_style = style(editor_fg, editor_bg, None);
        let selection_style = style(editor_fg, selection_bg, None);
        let gutter_style = style(gutter_fg, gutter_bg, None);

        let cursor_display_row = document.selections().first().map(|selection| {
            display
                .point_to_display_point(selection.head().to_point(&buffer.buffer))
                .row()
        });
        let mut cursor = None;
        let mut rows = Vec::with_capacity(rect.height as usize);
        let mut previous_buffer_row = None;

        for display_row in display.scroll_y..end_row {
            let buffer_row = display.buffer_row_for_display_row(display_row);
            let kind = if previous_buffer_row == Some(buffer_row) {
                vim_ui::DisplayRowKind::WrappedContinuation
            } else {
                vim_ui::DisplayRowKind::Buffer
            };
            let gutter = if gutter_width > 0 {
                let text = if kind == vim_ui::DisplayRowKind::Buffer {
                    format!("{:>width$} ", buffer_row + 1, width = gutter_width - 1)
                } else {
                    " ".repeat(gutter_width)
                };
                Some(vim_ui::GutterCell {
                    text,
                    style: gutter_style,
                })
            } else {
                None
            };

            let text = display.line_text(display_row) + " ";
            let match_ranges = pattern_ranges(&text, document, editor);
            let mut byte_column = 0usize;
            let mut skipped = display.scroll_x as usize;
            let content_width = (rect.width as usize).saturating_sub(gutter_width);
            let mut used = 0usize;
            let mut spans = Vec::new();

            for (character_column, character) in text.chars().enumerate() {
                let original = display
                    .display_point_to_point(DisplayPoint::new(display_row, byte_column as u32));
                byte_column += character.len_utf8();

                let syntax_style = if editor.syntax {
                    document.hl.render_row(original.row).and_then(|cache| {
                        cache.styles.iter().find(|span| {
                            original.column >= span.start && original.column < span.end
                        })
                    })
                } else {
                    None
                };
                let mut foreground = syntax_style
                    .map(|span| span.style.color)
                    .unwrap_or(editor_fg);
                let mut background = editor_bg;
                let in_match = match_ranges
                    .iter()
                    .any(|(start, end)| character_column >= *start && character_column < *end);
                if in_match {
                    foreground = find_fg;
                    background = find_bg;
                }

                let (selected, selected_line, at_cursor) = document.selections().is_selected(
                    original.row,
                    original.column,
                    &buffer.buffer,
                );
                if (selected && editor.mode != Mode::Command)
                    || (selected_line && editor.mode == Mode::VisualLine)
                    || at_cursor
                {
                    background = selection_bg;
                }

                let expanded = if character == '\t' {
                    "    ".to_string()
                } else {
                    character.to_string()
                };
                for cell in expanded.chars() {
                    if skipped > 0 {
                        skipped -= 1;
                        continue;
                    }
                    if used >= content_width {
                        break;
                    }
                    if at_cursor && cursor.is_none() {
                        cursor = Some(vim_ui::TextCursor {
                            position: vim_ui::DisplayPosition {
                                row: display_row - display.scroll_y,
                                column: (gutter_width + used) as u32,
                            },
                            shape: match editor.mode {
                                Mode::Insert | Mode::Command => vim_ui::CursorShape::Bar,
                                _ => vim_ui::CursorShape::Block,
                            },
                            visible: true,
                        });
                    }
                    let attributes = syntax_style.map(|span| &span.style);
                    spans.push(vim_ui::TextSpan::new(
                        cell.to_string(),
                        style(foreground, background, attributes),
                    ));
                    used += 1;
                }
                if used >= content_width {
                    break;
                }
            }

            rows.push(vim_ui::DisplayRow {
                buffer_row: Some(buffer_row),
                kind,
                gutter,
                spans,
                fill_style: default_style,
            });
            previous_buffer_row = Some(buffer_row);
        }

        while rows.len() < rect.height as usize {
            rows.push(vim_ui::DisplayRow {
                buffer_row: None,
                kind: vim_ui::DisplayRowKind::Virtual,
                gutter: (gutter_width > 0).then(|| vim_ui::GutterCell {
                    text: " ".repeat(gutter_width),
                    style: gutter_style,
                }),
                spans: Vec::new(),
                fill_style: default_style,
            });
        }

        let scrollbar = document.show_scrollbar.then(|| vim_ui::ScrollbarModel {
            total_rows: total_rows.max(1),
            first_visible_row: display.scroll_y.min(total_rows.saturating_sub(1)),
            visible_rows: (rect.height as u32).min(total_rows.max(1)),
            cursor_row: cursor_display_row.filter(|row| *row < total_rows),
            track_style: style(editor_fg, gutter_bg, None),
            thumb_style: selection_style,
            cursor_style: Some(selection_style),
        });

        vim_ui::TextViewModel {
            viewport_width: rect.width,
            viewport_height: rect.height,
            rows,
            selections: Vec::new(),
            cursor,
            scrollbar,
            default_style,
        }
    }
}

impl View for TextView {
    fn draw(
        &self,
        writer: &mut dyn Write,
        rect: Rect,
        editor: &Editor,
        buffer_manager: &mut crate::editor::buffers::BufferManager,
        document: Option<&crate::editor::document::Document>,
        ui: &crate::ui::Ui,
    ) -> Result<Option<(u16, u16, Option<crate::ui::CursorShape>)>, Box<dyn std::error::Error>>
    {
        let document = document.expect("TextView requires document view state");
        let model = self.build_model(rect, editor, buffer_manager, document, ui);
        model.validate()?;
        let cursor = model.cursor;
        let window_id = vim_ui::WindowId::new(1);
        let context = vim::ViewContext::new(ui.colorscheme()).with_text_model(window_id, model);
        let view = vim_ui::TextView::new(window_id);
        vim::draw(&view, writer, rect, &context)?;

        Ok(cursor.filter(|cursor| cursor.visible).map(|cursor| {
            (
                rect.x + cursor.position.column as u16,
                rect.y + cursor.position.row as u16,
                Some(match cursor.shape {
                    vim_ui::CursorShape::Block => crate::ui::CursorShape::Block,
                    vim_ui::CursorShape::Bar => crate::ui::CursorShape::Line,
                    vim_ui::CursorShape::Underline => crate::ui::CursorShape::Block,
                }),
            )
        }))
    }
}

fn pattern_ranges(
    text: &str,
    document: &crate::editor::document::Document,
    editor: &Editor,
) -> Vec<(usize, usize)> {
    if !document.show_pattern_match {
        return Vec::new();
    }
    editor
        .search_regex
        .as_ref()
        .map(|regex| {
            text.find_pattern(regex)
                .into_iter()
                .map(|(byte_start, byte_len, _)| {
                    let byte_end = byte_start + byte_len;
                    (
                        text[..byte_start].chars().count(),
                        text[..byte_end].chars().count(),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

fn style(
    foreground: crossterm::style::Color,
    background: crossterm::style::Color,
    attributes: Option<&crate::ui::colorscheme::Style>,
) -> vim_ui::Style {
    vim_ui::Style {
        fg: Some(vim::color(foreground)),
        bg: Some(vim::color(background)),
        bold: attributes.is_some_and(|style| style.bold),
        italic: attributes.is_some_and(|style| style.italic),
        underline: attributes.is_some_and(|style| style.underline),
        strikethrough: attributes.is_some_and(|style| style.strikethrough),
    }
}
