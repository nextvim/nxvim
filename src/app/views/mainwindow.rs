use vim_buffer::BufferId;
use vim_ui::{Rect, Renderer, TextView, UIContext, View, WindowId};

#[derive(Clone, Debug)]
pub struct MainWindowState {
    pub window_buffers: std::collections::HashMap<WindowId, BufferId>,
}

impl MainWindowState {
    pub fn new() -> Self {
        let mut window_buffers = std::collections::HashMap::new();
        // The initial editor window is WindowId::new(3)
        window_buffers.insert(WindowId::new(3), BufferId::new(1).unwrap());
        Self { window_buffers }
    }
}

pub struct MainWindowView {
    inner: TextView,
}

impl MainWindowView {
    pub const fn new(window_id: WindowId) -> Self {
        Self {
            inner: TextView::new(window_id),
        }
    }
}

impl View for MainWindowView {
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

    let tab_id = crate::app::buffer_manager::TabId(1);


    let mut rows = Vec::new();
    if let (Some(display_context), Ok(buffer)) = (
        app.buffer_manager.get_buffer_display_context(buffer_id, tab_id),
        app.buffer_manager.get_buffer(buffer_id),
    ) {
        let display_map_snapshot = display_context.display_map.snapshot();
        let row_count = display_map_snapshot.row_count();

        for i in 0..row_count {
            let line = display_map_snapshot.line_text(i);
            let buffer_row = display_map_snapshot.buffer_row_for_display_row(i);

            let mut spans = Vec::new();
            let mut current_text = String::new();
            let mut in_selection = false;

            for (col, ch) in line.chars().enumerate() {
                let dp = display_map::DisplayPoint::new(i, col as u32);
                let pt = display_map_snapshot.display_point_to_point(dp);
                let char_in_selection = if display_context.selections.selections.is_empty() {
                    false
                } else {
                    display_context
                        .selections
                        .is_selected(pt.row, pt.column, buffer.as_text_buffer())
                        .selected_cell
                };

                if char_in_selection != in_selection {
                    if !current_text.is_empty() {
                        let style = if in_selection {
                            vim_ui::Style::with_bg(vim_ui::Color::Blue)
                        } else {
                            vim_ui::Style::default()
                        };
                        spans.push(vim_ui::model::TextSpan::new(current_text, style));
                        current_text = String::new();
                    }
                    in_selection = char_in_selection;
                }
                current_text.push(ch);
            }

            if !current_text.is_empty() {
                let style = if in_selection {
                    vim_ui::Style::with_bg(vim_ui::Color::Blue)
                } else {
                    vim_ui::Style::default()
                };
                spans.push(vim_ui::model::TextSpan::new(current_text, style));
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
        Some(vim_ui::model::TextCursor {
            position: vim_ui::model::DisplayPosition { row: 0, column: 4 }, // after 4-character gutter
            shape: vim_ui::model::CursorShape::Block,
            visible: true,
        })
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
