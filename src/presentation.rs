use std::{collections::HashMap, io};

use vim_buffer::{BufferSnapshot, Point, TextRange};
use vim_input::Mode;
use vim_ui::{
    BufferId as UiBufferId, BufferPosition, BufferView, BufferViewModel, BufferedRenderer, Color,
    EditorMode, LineSource, Rect, Renderer, StatusLineView, TabLineView, UIContext, View,
};

use crate::{event::command_completions, state::AppState};

struct LinesView {
    lines: Vec<String>,
    prefix: &'static str,
}

impl View for LinesView {
    fn draw(
        &self,
        area: Rect,
        context: &dyn UIContext,
        renderer: &mut dyn Renderer,
    ) -> io::Result<()> {
        let (mut fg, mut bg) = (Color::Reset, Color::Reset);
        if let Some(style) = context
            .get_colorscheme()
            .and_then(|colorscheme| colorscheme.get_style("Normal"))
        {
            fg = style.fg.unwrap_or(fg);
            bg = style.bg.unwrap_or(bg);
        }
        renderer.set_fg(fg)?;
        renderer.set_bg(bg)?;
        for row in 0..area.height {
            renderer.move_to(area.x, area.y + row)?;
            renderer.print(&" ".repeat(area.width as usize))?;
            if let Some(line) = self.lines.get(row as usize) {
                renderer.move_to(area.x, area.y + row)?;
                let text = format!("{}{}", self.prefix, line);
                renderer.print(&text.chars().take(area.width as usize).collect::<String>())?;
            }
        }
        renderer.reset_colors()
    }

    fn cursor_screen_pos(&self, area: Rect, _context: &dyn UIContext) -> Option<(u16, u16)> {
        let width = self
            .lines
            .first()
            .map(|line| self.prefix.chars().count() + line.chars().count())?;
        Some((
            area.x + (width as u16).min(area.width.saturating_sub(1)),
            area.y,
        ))
    }
}

struct SnapshotLines(BufferSnapshot);

impl LineSource for SnapshotLines {
    fn len(&self) -> usize {
        self.0.row_count() as usize
    }

    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    fn get_line(&self, index: usize) -> Option<String> {
        let row = u32::try_from(index).ok()?;
        let len = self.0.line_len(row).ok()?;
        let start = self.0.point_to_offset(Point::new(row, 0)).ok()?;
        let end = self.0.point_to_offset(Point::new(row, len)).ok()?;
        self.0
            .text_for_range(TextRange::new(start, end)?)
            .ok()
            .map(Iterator::collect)
    }
}

struct FrameBuffer {
    lines: SnapshotLines,
    cursor: BufferPosition,
}

struct FrameContext {
    buffers: HashMap<UiBufferId, FrameBuffer>,
    active: UiBufferId,
    mode: EditorMode,
}

impl UIContext for FrameContext {
    fn get_buffer_model(&self, id: UiBufferId) -> Option<BufferViewModel<'_>> {
        let buffer = self.buffers.get(&id)?;
        Some(BufferViewModel {
            lines: &buffer.lines,
            cursor: buffer.cursor,
            selections: &[],
            mode: self.mode,
        })
    }

    fn get_active_buffer_id(&self) -> Option<UiBufferId> {
        Some(self.active)
    }
}

pub fn draw(state: &mut AppState, area: Rect, renderer: &mut BufferedRenderer) -> io::Result<()> {
    renderer.resize(area.width, area.height);
    if area.width <= 10 || area.height <= 5 {
        return Ok(());
    }

    state.sync_active_tab_to_focus();
    let tab_area = Rect::new(0, 0, area.width, 1);
    let status_area = Rect::new(0, area.height - 2, area.width, 1);
    let active_tab = state.active_tab_index;

    let mut frame_buffers = HashMap::new();
    for tab in &state.tabs {
        let buffer = state
            .buffers
            .get(tab.active_buffer_id)
            .map_err(io::Error::other)?;
        let cursor = tab.cursor_point(buffer);
        frame_buffers.insert(
            UiBufferId::new(tab.active_buffer_id.get()),
            FrameBuffer {
                lines: SnapshotLines(buffer.snapshot()),
                cursor: BufferPosition {
                    row: cursor.row as usize,
                    col: cursor.column as usize,
                },
            },
        );
    }
    let active_id = UiBufferId::new(state.tabs[active_tab].active_buffer_id.get());
    let context = FrameContext {
        buffers: frame_buffers,
        active: active_id,
        mode: ui_mode(state.mode),
    };

    for (&window_id, &tab_index) in &state.window_tabs {
        let tab = &state.tabs[tab_index.min(state.tabs.len() - 1)];
        let mut view = BufferView::new(UiBufferId::new(tab.active_buffer_id.get()), true);
        view.scroll_row = tab.scroll_row;
        view.scroll_col = tab.scroll_col;
        if let Some(window) = state.ui.window_mut(window_id) {
            window.set_title(tab.name.clone());
            window.set_view(Box::new(view));
        }
    }

    state
        .ui
        .window_mut(state.popups.command_line)
        .expect("command overlay")
        .set_view(Box::new(LinesView {
            lines: vec![state.command_line.clone()],
            prefix: ":",
        }));
    state
        .ui
        .window_mut(state.popups.autocomplete)
        .expect("autocomplete overlay")
        .set_view(Box::new(LinesView {
            lines: command_completions(&state.command_line)
                .into_iter()
                .map(str::to_owned)
                .collect(),
            prefix: "",
        }));
    state
        .ui
        .window_mut(state.popups.dialog)
        .expect("dialog overlay")
        .set_view(Box::new(LinesView {
            lines: state.dialog_message.iter().cloned().collect(),
            prefix: "",
        }));

    TabLineView::new(
        state.tabs.iter().map(|tab| tab.name.clone()).collect(),
        active_tab,
    )
    .draw(tab_area, &context, renderer)?;
    state.ui.draw(&context, renderer)?;

    let tab = &state.tabs[active_tab];
    let buffer = state
        .buffers
        .get(tab.active_buffer_id)
        .map_err(io::Error::other)?;
    let cursor = tab.cursor_point(buffer);
    StatusLineView::new(
        format!(
            " {} | file: {} | windows: {} ",
            mode_name(state.mode),
            tab.name,
            state.window_tabs.len()
        ),
        format!(" ln {}, col {} ", cursor.row + 1, cursor.column + 1),
    )
    .draw(status_area, &context, renderer)?;

    show_cursor(state, &context, renderer)
}

fn show_cursor(
    state: &AppState,
    context: &FrameContext,
    renderer: &mut BufferedRenderer,
) -> io::Result<()> {
    if state.mode == Mode::Command {
        let overlays = state.ui.computed_overlays(None);
        if let Some((_, rect)) = overlays
            .into_iter()
            .find(|(id, _)| *id == state.popups.command_line)
        {
            let column = 1 + state.command_line.chars().count() as u16;
            return renderer.show_cursor(
                rect.x + column.min(rect.width.saturating_sub(1)),
                rect.y,
                vim_ui::CursorShape::Bar,
            );
        }
    }

    let focused = state.ui.focused_window_id();
    let Some(&tab_index) = state.window_tabs.get(&focused) else {
        return renderer.hide_cursor();
    };
    let Some(rect) = state.ui.computed_layout().get_rect(focused) else {
        return renderer.hide_cursor();
    };
    let tab = &state.tabs[tab_index];
    let mut view = BufferView::new(UiBufferId::new(tab.active_buffer_id.get()), true);
    view.scroll_row = tab.scroll_row;
    view.scroll_col = tab.scroll_col;
    let view_area = if state
        .ui
        .window(focused)
        .is_some_and(|window| window.draws_border())
    {
        rect.inner(1)
    } else {
        rect
    };
    if let Some((x, y)) = view.cursor_screen_pos(view_area, context) {
        let shape = if state.mode == Mode::Insert {
            vim_ui::CursorShape::Bar
        } else {
            vim_ui::CursorShape::Block
        };
        renderer.show_cursor(x, y, shape)
    } else {
        renderer.hide_cursor()
    }
}

fn mode_name(mode: Mode) -> &'static str {
    match mode {
        Mode::Normal => "NORMAL",
        Mode::Insert => "INSERT",
        Mode::Visual | Mode::VisualLine | Mode::VisualBlock => "VISUAL",
        Mode::Command => "COMMAND",
    }
}

fn ui_mode(mode: Mode) -> EditorMode {
    match mode {
        Mode::Normal => EditorMode::Normal,
        Mode::Insert => EditorMode::Insert,
        Mode::Visual | Mode::VisualLine | Mode::VisualBlock => EditorMode::Visual,
        Mode::Command => EditorMode::Command,
    }
}
