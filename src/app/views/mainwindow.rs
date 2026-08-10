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
    app: &mut crate::app::App,
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
    let has_context = app
        .buffer_manager
        .get_buffer_display_context(buffer_id, tab_id)
        .is_some();
    if !has_context {
        if let Ok(buffer) = app.buffer_manager.get_buffer(buffer_id) {
            let snapshot = buffer.snapshot().as_inner().clone();
            let display_map = display_map::DisplayMap::new(snapshot, Some(inner_rect.width as u32));
            let display_context = crate::app::buffer_manager::BufferDisplayContext {
                display_map,
                highlights: Vec::new(),
                selections: vim_buffer::SelectionSet::new(),
            };
            app.buffer_manager.set_buffer_display_context(
                buffer_id,
                tab_id,
                display_context,
            );
        }
    } else {
        if let Some(display_context) = app
            .buffer_manager
            .get_buffer_display_context_mut(buffer_id, tab_id)
        {
            display_context.display_map.set_wrap_width(Some(inner_rect.width as u32));
        }
    }

    let mut rows = Vec::new();
    if let Some(display_context) = app
        .buffer_manager
        .get_buffer_display_context(buffer_id, tab_id)
    {
        let display_map_snapshot = display_context.display_map.snapshot();
        let row_count = display_map_snapshot.row_count();

        for i in 0..row_count {
            let line = display_map_snapshot.line_text(i);
            let buffer_row = display_map_snapshot.buffer_row_for_display_row(i);

            rows.push(vim_ui::model::DisplayRow {
                buffer_row: Some(buffer_row),
                kind: vim_ui::model::DisplayRowKind::Buffer,
                gutter: Some(vim_ui::model::GutterCell {
                    text: format!(" {:2} ", buffer_row + 1),
                    style: vim_ui::Style::default(),
                }),
                spans: vec![vim_ui::model::TextSpan::new(line, vim_ui::Style::default())],
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
