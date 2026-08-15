use std::any::Any;
use text::{Point, ToPoint};
use vim_ui::{Rect, Renderer, View, WindowState};

use crate::model::BufferState;
use crate::view::globals::RenderGlobals;

/// Renders one window's buffer content. Owns a small, cheap `vim_ui::TextView`
/// model rebuilt each frame by `refresh` from the three data tiers (window
/// state, buffer state, and render globals) — `draw` just renders it.
#[derive(Default)]
pub struct TextView {
    inner: vim_ui::TextView,
}

impl TextView {
    pub fn new() -> Self {
        Self {
            inner: vim_ui::TextView::new(),
        }
    }

    pub fn model(&self) -> Option<&vim_ui::TextViewModel> {
        self.inner.model()
    }

    pub fn refresh(
        &mut self,
        buffer: &vim_buffer::Buffer,
        window_state: &WindowState,
        buffer_state: &BufferState,
        inner_rect: Rect,
        active: bool,
        globals: &RenderGlobals,
    ) {
        let model = build_text(
            buffer,
            window_state,
            inner_rect,
            active,
            globals.mode,
            Some(&buffer_state.highlights),
            globals.search_pattern,
            globals.search_regex,
        );
        self.inner.set_model(model);
    }
}

impl View for TextView {
    fn draw(&self, area: Rect, renderer: &mut dyn Renderer) -> std::io::Result<()> {
        self.inner.draw(area, renderer)
    }

    fn cursor_screen_pos(&self, area: Rect) -> Option<(u16, u16)> {
        self.inner.cursor_screen_pos(area)
    }

    fn cursor_shape(&self) -> vim_ui::CursorShape {
        self.inner.cursor_shape()
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

pub fn build_text(
    buffer: &vim_buffer::Buffer,
    window: &WindowState,
    inner_rect: Rect,
    active: bool,
    mode: vim_input::Mode,
    highlights: Option<&textmate::BufferHighlightState>,
    _search_pattern: Option<&str>,
    _search_regex: Option<&onig::Regex>,
) -> vim_ui::TextViewModel {
    let mut rows = Vec::new();
    let mut saved_cursor = None;
    let display_map_snapshot = window.display_map.snapshot();
    let row_count = display_map_snapshot.row_count();
    let scroll_y = display_map_snapshot.scroll_y;
    let scroll_x = display_map_snapshot.scroll_x;
    let start_row = scroll_y;
    let end_row = (scroll_y + inner_rect.height as u32).min(row_count);

    let resolved_selections = if window.selections.selections.is_empty() {
        None
    } else {
        Some(vim_buffer::ResolvedSelectionSet::new(
            &window.selections,
            buffer.as_text_buffer(),
        ))
    };

    let mut prev_row = 0;
    for row in start_row..end_row {
        let line = display_map_snapshot.line_text(row);
        let buffer_row = display_map_snapshot.buffer_row_for_display_row(row);
        let mut spans = Vec::<vim_ui::model::TextSpan>::new();
        let line_chars: Vec<char> = line.chars().skip(scroll_x as usize).collect();
        let line_len = line_chars.len();

        let line_highlights = highlights.and_then(|h| h.highlight_row(buffer_row));
        let mut highlight_index = 0;

        let mut gutter_text = if window.show_gutter {
            format!(" {:2} ", buffer_row + 1)
        } else {
            String::new()
        };
        if prev_row == buffer_row + 1 {
            gutter_text = " ".repeat(gutter_text.len());
        }
        prev_row = buffer_row + 1;
        let cursor_offset = gutter_text.len() as u32;

        let mut gutter_style = vim_ui::Style::default();

        for column in 0..=line_len {
            let is_eol = column == line_len;
            let is_tab = !is_eol && line_chars[column] == '\t';
            let character = if is_eol { ' ' } else { line_chars[column] };
            let display_point = display_map::DisplayPoint::new(row, (column as u32) + scroll_x);
            let point = display_map_snapshot.display_point_to_point(display_point);
            let selection_state = if let Some(ref resolved) = resolved_selections {
                resolved.is_selected(point.row, point.column)
            } else {
                vim_buffer::SelectionCellState::default()
            };

            if active && selection_state.at_cursor_head {
                saved_cursor = Some(vim_ui::model::TextCursor {
                    position: vim_ui::model::DisplayPosition {
                        row: row - start_row,
                        column: column as u32 + cursor_offset,
                    },
                    shape: cursor_shape(mode),
                    visible: true,
                });
            }

            if is_eol && !selection_state.selected_cell && !selection_state.at_cursor_head {
                continue;
            }

            let mut style = vim_ui::Style::default();
            if selection_state.selected_cell || selection_state.at_cursor_head {
                style.bg = Some(vim_ui::Color::Blue);
            } else if let Some(line_highlights) = line_highlights {
                if let Some(span) =
                    highlight_at_column(line_highlights, &mut highlight_index, point.column)
                {
                    style.fg = Some(vim_ui::Color::Rgb(
                        span.foreground[0],
                        span.foreground[1],
                        span.foreground[2],
                    ));
                }
            }

            if let Some(span) = spans.last_mut().filter(|span| span.style == style) {
                if is_tab {
                    // TODO
                    span.text.push(' ');
                } else {
                    span.text.push(character);
                }
            } else {
                spans.push(vim_ui::model::TextSpan::new(character.to_string(), style));
            }
        }

        rows.push(vim_ui::model::DisplayRow {
            buffer_row: Some(buffer_row),
            kind: vim_ui::model::DisplayRowKind::Buffer,
            gutter: if window.show_gutter {
                Some(vim_ui::model::GutterCell {
                    text: gutter_text,
                    style: vim_ui::Style::default(),
                })
            } else {
                None
            },
            spans,
            fill_style: vim_ui::Style::default(),
        });
    }

    if rows.is_empty() {
        rows.push(empty_row(window.show_gutter));
    }

    let cursor = if active {
        saved_cursor.or_else(|| fallback_cursor(buffer, window, inner_rect, mode))
    } else {
        None
    };

    let cursor_row = if !window.selections.selections.is_empty() {
        let cursor_anchor = window.selections.primary().head();
        let display_snapshot = window.display_map.snapshot();
        let original_buffer = display_snapshot.buffer_snapshot();
        let display_cursor = if original_buffer.version == buffer.snapshot().as_inner().version {
            display_snapshot.anchor_to_display_point(cursor_anchor)
        } else {
            let point = cursor_anchor.to_point(buffer.snapshot().as_inner());
            let max_row = original_buffer.row_count().saturating_sub(1);
            let row = point.row.min(max_row);
            let column = if row < original_buffer.row_count() {
                point.column.min(original_buffer.line_len(row))
            } else {
                0
            };
            display_snapshot.point_to_display_point(Point { row, column })
        };
        Some(display_cursor.row())
    } else {
        None
    };

    vim_ui::TextViewModel {
        viewport_width: inner_rect.width,
        viewport_height: inner_rect.height,
        rows,
        selections: vec![],
        cursor,
        scrollbar: Some(vim_ui::model::ScrollbarModel {
            total_rows: row_count,
            first_visible_row: scroll_y,
            visible_rows: inner_rect.height as u32,
            cursor_row,
            track_style: vim_ui::Style {
                bg: Some(vim_ui::Color::DarkGrey),
                ..Default::default()
            },
            thumb_style: vim_ui::Style {
                bg: Some(vim_ui::Color::Grey),
                ..Default::default()
            },
            cursor_style: Some(vim_ui::Style {
                bg: Some(vim_ui::Color::Red),
                ..Default::default()
            }),
        }),
        default_style: vim_ui::Style::default(),
    }
}

fn highlight_at_column<'a>(
    spans: &'a [textmate::HighlightSpan],
    index: &mut usize,
    byte_column: u32,
) -> Option<&'a textmate::HighlightSpan> {
    while *index < spans.len() && spans[*index].end_column <= byte_column {
        *index += 1;
    }
    spans
        .get(*index)
        .filter(|span| span.start_column <= byte_column && byte_column < span.end_column)
}

fn fallback_cursor(
    buffer: &vim_buffer::Buffer,
    window: &WindowState,
    inner_rect: Rect,
    mode: vim_input::Mode,
) -> Option<vim_ui::model::TextCursor> {
    if window.selections.selections.is_empty() {
        let offset = if window.show_gutter { 4 } else { 0 };
        return Some(text_cursor(0, offset, mode));
    }

    let cursor_anchor = window.selections.primary().head();
    let display_snapshot = window.display_map.snapshot();
    let original_buffer = display_snapshot.buffer_snapshot();
    let display_cursor = if original_buffer.version == buffer.snapshot().as_inner().version {
        display_snapshot.anchor_to_display_point(cursor_anchor)
    } else {
        let point = cursor_anchor.to_point(buffer.snapshot().as_inner());
        let max_row = original_buffer.row_count().saturating_sub(1);
        let row = point.row.min(max_row);
        let column = if row < original_buffer.row_count() {
            point.column.min(original_buffer.line_len(row))
        } else {
            0
        };
        display_snapshot.point_to_display_point(Point { row, column })
    };
    let scroll_y = window.display_map.scroll_y;
    let scroll_x = window.display_map.scroll_x;
    if display_cursor.row() < scroll_y
        || display_cursor.row() >= scroll_y + inner_rect.height as u32
    {
        return None;
    }
    let offset = if window.show_gutter {
        let buffer_row = display_snapshot.buffer_row_for_display_row(display_cursor.row());
        format!(" {:2} ", buffer_row + 1).len() as u32
    } else {
        0
    };
    Some(text_cursor(
        display_cursor.row() - scroll_y,
        display_cursor.column().saturating_sub(scroll_x) + offset,
        mode,
    ))
}

fn text_cursor(row: u32, column: u32, mode: vim_input::Mode) -> vim_ui::model::TextCursor {
    vim_ui::model::TextCursor {
        position: vim_ui::model::DisplayPosition { row, column },
        shape: cursor_shape(mode),
        visible: true,
    }
}

#[cfg(test)]
mod tests {
    use super::highlight_at_column;

    fn span(start_column: u32, end_column: u32) -> textmate::HighlightSpan {
        textmate::HighlightSpan {
            start_column,
            end_column,
            foreground: [1, 2, 3],
        }
    }

    #[test]
    fn span_cursor_starts_at_a_horizontally_scrolled_column() {
        let spans = vec![span(0, 3), span(3, 8), span(10, 12)];
        let mut index = 0;

        assert_eq!(highlight_at_column(&spans, &mut index, 6), Some(&spans[1]));
        assert_eq!(index, 1);
        assert_eq!(highlight_at_column(&spans, &mut index, 9), None);
        assert_eq!(highlight_at_column(&spans, &mut index, 10), Some(&spans[2]));
    }

    #[test]
    fn span_cursor_uses_utf8_byte_columns() {
        let spans = vec![span(4, 9)];
        let mut index = 0;

        // `café`: the final character occupies byte columns 7..9.
        assert_eq!(highlight_at_column(&spans, &mut index, 7), Some(&spans[0]));
        assert_eq!(highlight_at_column(&spans, &mut index, 8), Some(&spans[0]));
        assert_eq!(highlight_at_column(&spans, &mut index, 9), None);
    }
}

fn cursor_shape(mode: vim_input::Mode) -> vim_ui::model::CursorShape {
    if mode == vim_input::Mode::Insert {
        vim_ui::model::CursorShape::BlinkingBar
    } else {
        vim_ui::model::CursorShape::Block
    }
}

fn empty_row(show_gutter: bool) -> vim_ui::model::DisplayRow {
    vim_ui::model::DisplayRow {
        buffer_row: Some(0),
        kind: vim_ui::model::DisplayRowKind::Buffer,
        gutter: if show_gutter {
            Some(vim_ui::model::GutterCell {
                text: "  1 ".to_string(),
                style: vim_ui::Style::default(),
            })
        } else {
            None
        },
        spans: vec![vim_ui::model::TextSpan::new(
            String::new(),
            vim_ui::Style::default(),
        )],
        fill_style: vim_ui::Style::default(),
    }
}
