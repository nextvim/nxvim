use std::io::{self, Stdout, Write};

use vim_buffer::{BufferSnapshot, ByteOffset, Point, TextRange};
use vim_ui::{
    BufferId as UiBufferId, BufferPosition, BufferView, BufferViewModel, BufferedRenderer,
    EditorMode, LineSource, Rect, Renderer, StatusLineView, UIContext, View,
};

use crate::{EditorFrame, ScreenSize};

pub trait Presenter {
    fn draw(&mut self, frame: EditorFrame) -> io::Result<()>;
}

#[derive(Debug, Default)]
pub struct NoopPresenter;

impl Presenter for NoopPresenter {
    fn draw(&mut self, _frame: EditorFrame) -> io::Result<()> {
        Ok(())
    }
}

pub struct TerminalPresenter {
    output: Stdout,
    renderer: BufferedRenderer,
}

impl TerminalPresenter {
    pub fn new(size: ScreenSize) -> Self {
        Self {
            output: std::io::stdout(),
            renderer: BufferedRenderer::new(size.columns, size.rows),
        }
    }
}

impl Presenter for TerminalPresenter {
    fn draw(&mut self, frame: EditorFrame) -> io::Result<()> {
        let size = frame.screen;
        self.renderer.resize(size.columns, size.rows);
        if size.columns == 0 || size.rows == 0 {
            return Ok(());
        }

        let context = FrameContext::new(frame);
        if size.columns < 5 || size.rows < 2 {
            self.renderer.move_to(0, 0);
            self.renderer.print("NXVIM");
        } else {
            let buffer_area = Rect::new(0, 0, size.columns, size.rows - 1);
            let status_area = Rect::new(0, size.rows - 1, size.columns, 1);
            let mut buffer_view = BufferView::new(context.ui_buffer_id, true);
            buffer_view.scroll_row = context.scroll_row;
            buffer_view.scroll_col = context.scroll_col;
            let status = StatusLineView::new(context.status_left(), context.status_right());

            buffer_view.draw(buffer_area, &context, &mut self.renderer);
            status.draw(status_area, &context, &mut self.renderer);
            if let Some((x, y)) = buffer_view.cursor_screen_pos(buffer_area, &context) {
                self.renderer.move_to(x, y);
            }
        }

        self.renderer.flush(&mut self.output)?;
        self.output.flush()
    }
}

struct FrameContext {
    snapshot: SnapshotLines,
    ui_buffer_id: UiBufferId,
    cursor: BufferPosition,
    scroll_row: usize,
    scroll_col: usize,
    mode: EditorMode,
    name: String,
}

impl FrameContext {
    fn new(frame: EditorFrame) -> Self {
        let cursor_col = character_column(&frame.snapshot, frame.cursor.row, frame.cursor.column);
        Self {
            ui_buffer_id: UiBufferId::new(frame.buffer_id.get()),
            snapshot: SnapshotLines(frame.snapshot),
            cursor: BufferPosition {
                row: frame.cursor.row as usize,
                col: cursor_col,
            },
            scroll_row: frame.scroll_row,
            scroll_col: frame.scroll_col,
            mode: ui_mode(frame.mode),
            name: frame.name,
        }
    }

    fn status_left(&self) -> String {
        format!(" {} | {}", mode_name(self.mode), self.name)
    }

    fn status_right(&self) -> String {
        format!("{}:{} ", self.cursor.row + 1, self.cursor.col + 1)
    }
}

impl UIContext for FrameContext {
    fn get_buffer_model(&self, id: UiBufferId) -> Option<BufferViewModel<'_>> {
        (id == self.ui_buffer_id).then_some(BufferViewModel {
            lines: &self.snapshot,
            cursor: self.cursor,
            selections: &[],
            mode: self.mode,
        })
    }

    fn get_active_buffer_id(&self) -> Option<UiBufferId> {
        Some(self.ui_buffer_id)
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
        let range = TextRange::new(start, end)?;
        self.0.text_for_range(range).ok().map(Iterator::collect)
    }
}

fn character_column(snapshot: &BufferSnapshot, row: u32, byte_column: u32) -> usize {
    let Some(start) = snapshot.point_to_offset(Point::new(row, 0)).ok() else {
        return 0;
    };
    let Some(end) = snapshot.point_to_offset(Point::new(row, byte_column)).ok() else {
        return 0;
    };
    let Some(range) = TextRange::new(ByteOffset(start.0), ByteOffset(end.0)) else {
        return 0;
    };
    snapshot
        .text_for_range(range)
        .ok()
        .map(|chunks| chunks.map(str::chars).map(Iterator::count).sum())
        .unwrap_or(0)
}

fn mode_name(mode: EditorMode) -> &'static str {
    match mode {
        EditorMode::Normal => "NORMAL",
        EditorMode::Insert => "INSERT",
        EditorMode::Visual => "VISUAL",
        EditorMode::Command => "COMMAND",
    }
}

fn ui_mode(mode: vim_input::Mode) -> EditorMode {
    match mode {
        vim_input::Mode::Normal => EditorMode::Normal,
        vim_input::Mode::Insert => EditorMode::Insert,
        vim_input::Mode::Visual | vim_input::Mode::VisualLine | vim_input::Mode::VisualBlock => {
            EditorMode::Visual
        }
        vim_input::Mode::Command => EditorMode::Command,
    }
}
