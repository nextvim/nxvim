use text::{Point, ToPoint};
use vim_buffer::BufferId;
use vim_ui::{Rect, Renderer, UIContext, View, WindowId};

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
    app: &crate::app::App,
    win_id: WindowId,
    buffer_id: BufferId,
    active_id: WindowId,
    width: u16,
    height: u16,
) -> vim_ui::model::TextViewModel {
    let win_rect = app
        .ui
        .computed_layout()
        .get_rect(win_id)
        .unwrap_or(Rect::new(0, 0, width, height));

    let inner_rect = if let Some(win) = app.ui.window(win_id) {
        if win.draws_border() {
            win_rect.inner(1)
        } else {
            win_rect
        }
    } else {
        win_rect
    };

    let tab_id = crate::app::buffer_manager::TabId(win_id.get());

    let mut rows = Vec::new();
    let mut saved_cursor = None;
    if let (Some(display_context), Ok(buffer)) = (
        app.buffer_manager
            .get_buffer_display_context(buffer_id, tab_id),
        app.buffer_manager.get_buffer(buffer_id),
    ) {
        let display_map_snapshot = display_context.display_map.snapshot();
        let row_count = display_map_snapshot.row_count();
        let scroll_y = display_map_snapshot.scroll_y;
        let scroll_x = display_map_snapshot.scroll_x;
        let start_row = scroll_y;
        let end_row = (scroll_y + inner_rect.height as u32).min(row_count);

        for i in start_row..end_row {
            let line = display_map_snapshot.line_text(i);
            let buffer_row = display_map_snapshot.buffer_row_for_display_row(i);

            let mut spans = Vec::<vim_ui::model::TextSpan>::new();

            let line_chars: Vec<char> = line.chars().skip(scroll_x as usize).collect();
            let line_len = line_chars.len();
            for col in 0..=line_len {
                let is_eol = col == line_len;
                let ch = if is_eol { ' ' } else { line_chars[col] };
                let dp = display_map::DisplayPoint::new(i, (col as u32) + scroll_x);
                let pt = display_map_snapshot.display_point_to_point(dp);
                let selection_state = if display_context.selections.selections.is_empty() {
                    vim_buffer::SelectionCellState::default()
                } else {
                    display_context.selections.is_selected(
                        pt.row,
                        pt.column,
                        buffer.as_text_buffer(),
                    )
                };

                if win_id == active_id && selection_state.at_cursor_head {
                    let screen_row = i - start_row;
                    let screen_col = col as u32;
                    let cursor_shape = if app.controller.mode() == vim_input::Mode::Insert {
                        vim_ui::model::CursorShape::BlinkingBar
                    } else {
                        vim_ui::model::CursorShape::Block
                    };
                    saved_cursor = Some(vim_ui::model::TextCursor {
                        position: vim_ui::model::DisplayPosition {
                            row: screen_row,
                            column: screen_col + 4,
                        },
                        shape: cursor_shape,
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
                    span.text.push(ch);
                } else {
                    spans.push(vim_ui::model::TextSpan::new(ch.to_string(), style));
                }
            }

            rows.push(vim_ui::model::DisplayRow {
                buffer_row: Some(buffer_row),
                kind: vim_ui::model::DisplayRowKind::Buffer,
                gutter: Some(vim_ui::model::GutterCell {
                    text: format!(" {:2} ", buffer_row + 1),
                    style: vim_ui::Style::default(),
                }),
                spans,
                fill_style: vim_ui::Style::default(),
            });
        }
    }

    if rows.is_empty() {
        rows.push(vim_ui::model::DisplayRow {
            buffer_row: Some(0),
            kind: vim_ui::model::DisplayRowKind::Buffer,
            gutter: Some(vim_ui::model::GutterCell {
                text: "  1 ".to_string(),
                style: vim_ui::Style::default(),
            }),
            spans: vec![vim_ui::model::TextSpan::new(
                "".to_string(),
                vim_ui::Style::default(),
            )],
            fill_style: vim_ui::Style::default(),
        });
    }

    let cursor = if win_id == active_id {
        if saved_cursor.is_some() {
            saved_cursor
        } else if let (Some(display_context), Ok(buffer)) = (
            app.buffer_manager
                .get_buffer_display_context(buffer_id, tab_id),
            app.buffer_manager.get_buffer(buffer_id),
        ) {
            let cursor_shape = if app.controller.mode() == vim_input::Mode::Insert {
                vim_ui::model::CursorShape::BlinkingBar
            } else {
                vim_ui::model::CursorShape::Block
            };
            if display_context.selections.selections.is_empty() {
                Some(vim_ui::model::TextCursor {
                    position: vim_ui::model::DisplayPosition { row: 0, column: 4 },
                    shape: cursor_shape,
                    visible: true,
                })
            } else {
                let cursor_anchor = display_context.selections.primary().head();
                let display_snapshot = display_context.display_map.snapshot();
                let original_buffer = display_snapshot.buffer_snapshot();
                let display_cursor =
                    if original_buffer.version == buffer.snapshot().as_inner().version {
                        display_snapshot.anchor_to_display_point(cursor_anchor)
                    } else {
                        let point = cursor_anchor.to_point(buffer.snapshot().as_inner());
                        let max_row = original_buffer.row_count().saturating_sub(1);
                        let row = point.row.min(max_row);
                        let col = if row < original_buffer.row_count() {
                            point.column.min(original_buffer.line_len(row))
                        } else {
                            0
                        };
                        let clipped_point = Point { row, column: col };
                        display_snapshot.point_to_display_point(clipped_point)
                    };
                let scroll_y = display_context.display_map.scroll_y;
                let scroll_x = display_context.display_map.scroll_x;
                if display_cursor.row() >= scroll_y
                    && display_cursor.row() < scroll_y + inner_rect.height as u32
                {
                    let screen_row = display_cursor.row() - scroll_y;
                    let screen_col = display_cursor.column().saturating_sub(scroll_x) + 4;
                    Some(vim_ui::model::TextCursor {
                        position: vim_ui::model::DisplayPosition {
                            row: screen_row,
                            column: screen_col,
                        },
                        shape: cursor_shape,
                        visible: true,
                    })
                } else {
                    None
                }
            }
        } else {
            let cursor_shape = if app.controller.mode() == vim_input::Mode::Insert {
                vim_ui::model::CursorShape::BlinkingBar
            } else {
                vim_ui::model::CursorShape::Block
            };
            Some(vim_ui::model::TextCursor {
                position: vim_ui::model::DisplayPosition { row: 0, column: 4 },
                shape: cursor_shape,
                visible: true,
            })
        }
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
