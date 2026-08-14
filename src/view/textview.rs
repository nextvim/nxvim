use text::{Point, ToPoint};
use vim_ui::{Rect, Renderer, UIContext, View, WindowId};

use crate::model::WindowState;

pub struct TextView {
    inner: vim_ui::TextView,
}

impl TextView {
    pub const fn new(window_id: WindowId) -> Self {
        Self {
            inner: vim_ui::TextView::new(window_id),
        }
    }
}

impl View for TextView {
    fn draw(
        &self,
        area: Rect,
        context: &dyn UIContext,
        renderer: &mut dyn Renderer,
    ) -> std::io::Result<()> {
        self.inner.draw(area, context, renderer)
    }

    fn cursor_screen_pos(&self, area: Rect, context: &dyn UIContext) -> Option<(u16, u16)> {
        self.inner.cursor_screen_pos(area, context)
    }
}

pub fn build_text(
    buffer: &vim_buffer::Buffer,
    window: &WindowState,
    inner_rect: Rect,
    active: bool,
    mode: vim_input::Mode,
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
        Some(vim_buffer::ResolvedSelectionSet::new(&window.selections, buffer.as_text_buffer()))
    };

    for row in start_row..end_row {
        let line = display_map_snapshot.line_text(row);
        let buffer_row = display_map_snapshot.buffer_row_for_display_row(row);
        let mut spans = Vec::<vim_ui::model::TextSpan>::new();
        let line_chars: Vec<char> = line.chars().skip(scroll_x as usize).collect();
        let line_len = line_chars.len();

        let gutter_text = format!(" {:2} ", buffer_row + 1);
        let cursor_offset = gutter_text.len() as u32;

        for column in 0..=line_len {
            let is_eol = column == line_len;
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
            }

            if let Some(span) = spans.last_mut().filter(|span| span.style == style) {
                span.text.push(character);
            } else {
                spans.push(vim_ui::model::TextSpan::new(character.to_string(), style));
            }
        }

        rows.push(vim_ui::model::DisplayRow {
            buffer_row: Some(buffer_row),
            kind: vim_ui::model::DisplayRowKind::Buffer,
            gutter: Some(vim_ui::model::GutterCell {
                text: gutter_text,
                style: vim_ui::Style::default(),
            }),
            spans,
            fill_style: vim_ui::Style::default(),
        });
    }

    if rows.is_empty() {
        rows.push(empty_row());
    }

    let cursor = if active {
        saved_cursor.or_else(|| fallback_cursor(buffer, window, inner_rect, mode))
    } else {
        None
    };

    vim_ui::TextViewModel {
        viewport_width: inner_rect.width,
        viewport_height: inner_rect.height,
        rows,
        selections: vec![],
        cursor,
        scrollbar: None,
        default_style: vim_ui::Style::default(),
    }
}

fn fallback_cursor(
    buffer: &vim_buffer::Buffer,
    window: &WindowState,
    inner_rect: Rect,
    mode: vim_input::Mode,
) -> Option<vim_ui::model::TextCursor> {
    if window.selections.selections.is_empty() {
        return Some(text_cursor(0, 4, mode));
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
    Some(text_cursor(
        display_cursor.row() - scroll_y,
        display_cursor.column().saturating_sub(scroll_x) + 4,
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

fn cursor_shape(mode: vim_input::Mode) -> vim_ui::model::CursorShape {
    if mode == vim_input::Mode::Insert {
        vim_ui::model::CursorShape::BlinkingBar
    } else {
        vim_ui::model::CursorShape::Block
    }
}

fn empty_row() -> vim_ui::model::DisplayRow {
    vim_ui::model::DisplayRow {
        buffer_row: Some(0),
        kind: vim_ui::model::DisplayRowKind::Buffer,
        gutter: Some(vim_ui::model::GutterCell {
            text: "  1 ".to_string(),
            style: vim_ui::Style::default(),
        }),
        spans: vec![vim_ui::model::TextSpan::new(
            String::new(),
            vim_ui::Style::default(),
        )],
        fill_style: vim_ui::Style::default(),
    }
}
