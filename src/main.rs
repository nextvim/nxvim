// mod scripting;
// mod services;

use std::io::{self, Write, stdout};
use std::time::Duration;

use crossterm::{
    cursor::Show,
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{
        EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode, size,
    },
};

use vim_buffer::{BufferId, BufferManager, BufferSnapshot, ByteOffset, Point, TextRange};
use vim_ui::{
    BufferId as UiBufferId, BufferPosition, BufferView, BufferViewModel, BufferedRenderer, Color,
    EditorMode, LineSource, Rect, Renderer, StatusLineView, TabLineView, UIContext, View,
};

/// RAII Terminal Session Guard to ensure Raw Mode and Alternate Screen
/// are cleanly restored on normal return, early exits, or panics.
pub struct TerminalSession {
    restored: bool,
}

impl TerminalSession {
    pub fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        if let Err(error) = execute!(stdout(), EnterAlternateScreen, Show) {
            let _ = disable_raw_mode();
            return Err(error);
        }
        Ok(Self { restored: false })
    }

    pub fn size(&self) -> io::Result<Rect> {
        let (columns, rows) = size()?;
        Ok(Rect::new(0, 0, columns, rows))
    }

    pub fn restore(&mut self) -> io::Result<()> {
        if self.restored {
            return Ok(());
        }
        let screen_result = execute!(stdout(), Show, LeaveAlternateScreen);
        let raw_result = disable_raw_mode();
        self.restored = true;
        screen_result.and(raw_result)
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

/// Simple TabPage structure supporting tabbed page routing.
struct TabPage {
    name: String,
    active_buffer_id: BufferId,
    cursor_row: usize,
    cursor_col: usize,
    scroll_row: usize,
    scroll_col: usize,
}

/// Active Application State driving the rebuild model.
struct AppState {
    buffers: BufferManager,
    tabs: Vec<TabPage>,
    active_tab_index: usize,
    mode: EditorMode,
    running: bool,
    command_line: String,
}

impl AppState {
    fn active_tab(&self) -> &TabPage {
        &self.tabs[self.active_tab_index]
    }
}

/// Custom CommandLine window view.
struct CommandLineView {
    pub text: String,
    pub active: bool,
}

impl View for CommandLineView {
    fn draw(
        &self,
        area: Rect,
        context: &dyn UIContext,
        renderer: &mut dyn Renderer,
    ) -> std::io::Result<()> {
        let mut bg = Color::Reset;
        let mut fg = Color::Reset;

        if let Some(cs) = context.get_colorscheme() {
            if let Some(style) = cs.get_style("Normal") {
                if let Some(style_bg) = style.bg {
                    bg = style_bg;
                }
                if let Some(style_fg) = style.fg {
                    fg = style_fg;
                }
            }
        }

        renderer.set_bg(bg)?;
        renderer.set_fg(fg)?;

        // Clear line
        renderer.move_to(area.x, area.y)?;
        renderer.print(&" ".repeat(area.width as usize))?;

        renderer.move_to(area.x, area.y)?;
        if self.active {
            renderer.print(&format!(":{}", self.text))?;
        } else if !self.text.is_empty() {
            renderer.print(&self.text)?;
        }
        renderer.reset_colors()?;
        Ok(())
    }

    fn cursor_screen_pos(&self, area: Rect, _context: &dyn UIContext) -> Option<(u16, u16)> {
        if self.active {
            let char_count = self.text.chars().count();
            if char_count + 1 < area.width as usize {
                return Some((area.x + 1 + char_count as u16, area.y));
            }
        }
        None
    }
}

/// Bridges the `vim-buffer` snapshot to `vim-ui`'s LineSource.
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

/// Holds rendering snapshots for the UI.
struct FrameContext {
    snapshot: SnapshotLines,
    ui_buffer_id: UiBufferId,
    cursor: BufferPosition,
    mode: EditorMode,
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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Enter safe terminal session
    let mut terminal = TerminalSession::enter()?;
    let screen = terminal.size()?;

    // 2. Setup model state
    let mut buffers = BufferManager::new();

    // Create 3 sample buffers to populate tabs
    let buf1 = buffers.create("Welcome to nxvim!\nThis is a complete rebuild of the editor from the ground up.\nNow powered by vim-ui and vim-buffer.\n\nEnjoy editing!\n");
    let buf1_id = buf1.id();

    let buf2 = buffers.create("This is buffer 2.\nYou can cycle through tabs using Tab / Shift-Tab.\nEditing characters is supported in INSERT mode.");
    let buf2_id = buf2.id();

    let buf3 = buffers.create("This is buffer 3.\nIt represents another open file.");
    let buf3_id = buf3.id();

    let tabs = vec![
        TabPage {
            name: "nxvim_welcome".to_string(),
            active_buffer_id: buf1_id,
            cursor_row: 0,
            cursor_col: 0,
            scroll_row: 0,
            scroll_col: 0,
        },
        TabPage {
            name: "buffer_two".to_string(),
            active_buffer_id: buf2_id,
            cursor_row: 0,
            cursor_col: 0,
            scroll_row: 0,
            scroll_col: 0,
        },
        TabPage {
            name: "buffer_three".to_string(),
            active_buffer_id: buf3_id,
            cursor_row: 0,
            cursor_col: 0,
            scroll_row: 0,
            scroll_col: 0,
        },
    ];

    let mut state = AppState {
        buffers,
        tabs,
        active_tab_index: 0,
        mode: EditorMode::Normal,
        running: true,
        command_line: String::new(),
    };

    // 3. Initialize BufferedRenderer
    let mut renderer = BufferedRenderer::new(screen.width, screen.height);
    let mut stdout_handle = stdout();

    while state.running {
        // Draw the interface
        let current_size = terminal.size()?;
        renderer.resize(current_size.width, current_size.height);

        if current_size.width > 10 && current_size.height > 5 {
            // Screen structure:
            // Row 0: TabLine (1 cell high)
            // Rows 1..height-2: Active BufferView (editor space)
            // Row height-2: StatusLine (1 cell high)
            // Row height-1: CommandLine (1 cell high)
            let tab_area = Rect::new(0, 0, current_size.width, 1);
            let editor_area = Rect::new(0, 1, current_size.width, current_size.height - 3);
            let status_area = Rect::new(0, current_size.height - 2, current_size.width, 1);
            let command_area = Rect::new(0, current_size.height - 1, current_size.width, 1);

            let tab_names: Vec<String> = state.tabs.iter().map(|t| t.name.clone()).collect();
            let tab_view = TabLineView::new(tab_names, state.active_tab_index);

            let active_tab = state.active_tab();
            let active_buf = state.buffers.get(active_tab.active_buffer_id)?;
            let snapshot = active_buf.snapshot();

            let context = FrameContext {
                ui_buffer_id: UiBufferId::new(active_tab.active_buffer_id.get()),
                snapshot: SnapshotLines(snapshot),
                cursor: BufferPosition {
                    row: active_tab.cursor_row,
                    col: active_tab.cursor_col,
                },
                mode: state.mode,
            };

            let mut buffer_view = BufferView::new(context.ui_buffer_id, true);
            buffer_view.scroll_row = active_tab.scroll_row;
            buffer_view.scroll_col = active_tab.scroll_col;

            let mode_str = match state.mode {
                EditorMode::Normal => "NORMAL",
                EditorMode::Insert => "INSERT",
                EditorMode::Visual => "VISUAL",
                EditorMode::Command => "COMMAND",
            };
            let left_status = format!(" {} | file: {} ", mode_str, active_tab.name);
            let right_status = format!(
                " ln {}, col {} ",
                active_tab.cursor_row + 1,
                active_tab.cursor_col + 1
            );
            let status_view = StatusLineView::new(left_status, right_status);
            let command_view = CommandLineView {
                text: state.command_line.clone(),
                active: state.mode == EditorMode::Command,
            };

            // Draw views
            tab_view.draw(tab_area, &context, &mut renderer)?;
            buffer_view.draw(editor_area, &context, &mut renderer)?;
            status_view.draw(status_area, &context, &mut renderer)?;
            command_view.draw(command_area, &context, &mut renderer)?;

            // Set final cursor position
            if state.mode == EditorMode::Command {
                if let Some((x, y)) = command_view.cursor_screen_pos(command_area, &context) {
                    renderer.show_cursor(x, y, vim_ui::CursorShape::Bar)?;
                } else {
                    renderer.hide_cursor()?;
                }
            } else {
                if let Some((x, y)) = buffer_view.cursor_screen_pos(editor_area, &context) {
                    let cursor_shape = match state.mode {
                        EditorMode::Insert => vim_ui::CursorShape::Bar,
                        _ => vim_ui::CursorShape::Block,
                    };
                    renderer.show_cursor(x, y, cursor_shape)?;
                } else {
                    renderer.hide_cursor()?;
                }
            }
        }

        renderer.flush(&mut stdout_handle)?;
        stdout_handle.flush()?;

        // Wait/Read event
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press || key.kind == KeyEventKind::Repeat {
                    handle_key_event(&mut state, key, current_size.height as usize - 3)?;
                }
            }
        }
    }

    // Explicit restore terminal session
    terminal.restore()?;
    Ok(())
}

fn handle_key_event(
    state: &mut AppState,
    key: KeyEvent,
    viewport_height: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    // Global exits
    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        state.running = false;
        return Ok(());
    }

    let active_tab_index = state.active_tab_index;
    let tabs = &mut state.tabs;
    let buffers = &mut state.buffers;
    let mode = state.mode;

    match mode {
        EditorMode::Normal => {
            let active_tab = &mut tabs[active_tab_index];
            match key.code {
                KeyCode::Esc | KeyCode::Char('q') => {
                    state.running = false;
                }
                KeyCode::Char('i') => {
                    state.mode = EditorMode::Insert;
                }
                KeyCode::Char(':') => {
                    state.mode = EditorMode::Command;
                    state.command_line.clear();
                }
                KeyCode::Tab => {
                    // Cycle tabs forward
                    state.active_tab_index = (active_tab_index + 1) % tabs.len();
                }
                KeyCode::BackTab => {
                    // Cycle tabs backward
                    state.active_tab_index = (active_tab_index + tabs.len() - 1) % tabs.len();
                }
                KeyCode::Char('h') | KeyCode::Left => {
                    active_tab.cursor_col = active_tab.cursor_col.saturating_sub(1);
                    ensure_cursor_visible(
                        active_tab,
                        buffers.get(active_tab.active_buffer_id)?.snapshot(),
                        viewport_height,
                    );
                }
                KeyCode::Char('l') | KeyCode::Right => {
                    let buf = buffers.get(active_tab.active_buffer_id)?;
                    let snapshot = buf.snapshot();
                    let line_len = get_line_char_count(&snapshot, active_tab.cursor_row);
                    if active_tab.cursor_col + 1 < line_len || line_len == 0 {
                        active_tab.cursor_col += 1;
                    }
                    ensure_cursor_visible(active_tab, snapshot, viewport_height);
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    active_tab.cursor_row = active_tab.cursor_row.saturating_sub(1);
                    let buf = buffers.get(active_tab.active_buffer_id)?;
                    let snapshot = buf.snapshot();
                    let line_len = get_line_char_count(&snapshot, active_tab.cursor_row);
                    if active_tab.cursor_col >= line_len {
                        active_tab.cursor_col = line_len.saturating_sub(1);
                    }
                    ensure_cursor_visible(active_tab, snapshot, viewport_height);
                }
                KeyCode::Char('j') | KeyCode::Down => {
                    let buf = buffers.get(active_tab.active_buffer_id)?;
                    let snapshot = buf.snapshot();
                    let total_lines = snapshot.row_count() as usize;
                    if active_tab.cursor_row + 1 < total_lines {
                        active_tab.cursor_row += 1;
                    }
                    let line_len = get_line_char_count(&snapshot, active_tab.cursor_row);
                    if active_tab.cursor_col >= line_len {
                        active_tab.cursor_col = line_len.saturating_sub(1);
                    }
                    ensure_cursor_visible(active_tab, snapshot, viewport_height);
                }
                _ => {}
            }
        }
        EditorMode::Insert => {
            let active_tab = &mut tabs[active_tab_index];
            match key.code {
                KeyCode::Esc => {
                    state.mode = EditorMode::Normal;
                }
                KeyCode::Enter => {
                    // Insert newline
                    let buf = buffers.get_mut(active_tab.active_buffer_id)?;
                    let snapshot = buf.snapshot();
                    if let Some(offset) =
                        get_byte_offset(&snapshot, active_tab.cursor_row, active_tab.cursor_col)
                    {
                        let mut trans = buf.transaction(vim_buffer::EditOrigin::InsertMode);
                        trans.insert(None, offset, "\n");
                        trans.commit(None)?;
                        active_tab.cursor_row += 1;
                        active_tab.cursor_col = 0;
                        ensure_cursor_visible(active_tab, buf.snapshot(), viewport_height);
                    }
                }
                KeyCode::Backspace => {
                    if active_tab.cursor_col > 0 {
                        let buf = buffers.get_mut(active_tab.active_buffer_id)?;
                        let snapshot = buf.snapshot();
                        if let Some(offset_to_del) = get_byte_offset(
                            &snapshot,
                            active_tab.cursor_row,
                            active_tab.cursor_col - 1,
                        ) {
                            if let Some(offset_curr) = get_byte_offset(
                                &snapshot,
                                active_tab.cursor_row,
                                active_tab.cursor_col,
                            ) {
                                let mut trans = buf.transaction(vim_buffer::EditOrigin::InsertMode);
                                trans.delete(
                                    None,
                                    TextRange::new(offset_to_del, offset_curr).unwrap(),
                                );
                                trans.commit(None)?;
                                active_tab.cursor_col -= 1;
                                ensure_cursor_visible(active_tab, buf.snapshot(), viewport_height);
                            }
                        }
                    } else if active_tab.cursor_row > 0 {
                        // Delete line break
                        let buf = buffers.get_mut(active_tab.active_buffer_id)?;
                        let snapshot = buf.snapshot();
                        let prev_row = active_tab.cursor_row - 1;
                        let prev_row_len = get_line_char_count(&snapshot, prev_row);
                        if let Some(offset_to_del) =
                            get_byte_offset(&snapshot, prev_row, prev_row_len)
                        {
                            if let Some(offset_curr) =
                                get_byte_offset(&snapshot, active_tab.cursor_row, 0)
                            {
                                let mut trans = buf.transaction(vim_buffer::EditOrigin::InsertMode);
                                trans.delete(
                                    None,
                                    TextRange::new(offset_to_del, offset_curr).unwrap(),
                                );
                                trans.commit(None)?;
                                active_tab.cursor_row = prev_row;
                                active_tab.cursor_col = prev_row_len;
                                ensure_cursor_visible(active_tab, buf.snapshot(), viewport_height);
                            }
                        }
                    }
                }
                KeyCode::Char(c) => {
                    let buf = buffers.get_mut(active_tab.active_buffer_id)?;
                    let snapshot = buf.snapshot();
                    if let Some(offset) =
                        get_byte_offset(&snapshot, active_tab.cursor_row, active_tab.cursor_col)
                    {
                        let mut trans = buf.transaction(vim_buffer::EditOrigin::InsertMode);
                        trans.insert(None, offset, c.to_string());
                        trans.commit(None)?;
                        active_tab.cursor_col += 1;
                        ensure_cursor_visible(active_tab, buf.snapshot(), viewport_height);
                    }
                }
                _ => {}
            }
        }
        EditorMode::Command => {
            let active_tab = &mut tabs[active_tab_index];
            match key.code {
                KeyCode::Esc => {
                    state.mode = EditorMode::Normal;
                    state.command_line.clear();
                }
                KeyCode::Enter => {
                    let cmd = state.command_line.clone();
                    if cmd == "q" || cmd == "quit" {
                        state.running = false;
                    } else if cmd == "tabnew" {
                        // Create a new tab page
                        let new_buf = buffers.create(format!(
                            "This is a new tab page buffer.\nTab count: {}\n",
                            tabs.len() + 1
                        ));
                        tabs.push(TabPage {
                            name: format!("tab_{}", tabs.len() + 1),
                            active_buffer_id: new_buf.id(),
                            cursor_row: 0,
                            cursor_col: 0,
                            scroll_row: 0,
                            scroll_col: 0,
                        });
                        state.active_tab_index = tabs.len() - 1;
                        state.command_line = "New tab page created.".to_string();
                    } else if cmd.is_empty() {
                        state.command_line.clear();
                    } else {
                        state.command_line = format!("Unknown command: {}", cmd);
                    }
                    state.mode = EditorMode::Normal;
                }
                KeyCode::Backspace => {
                    state.command_line.pop();
                }
                KeyCode::Char(c) => {
                    state.command_line.push(c);
                }
                _ => {}
            }
        }
        _ => {}
    }
    Ok(())
}

fn ensure_cursor_visible(tab: &mut TabPage, snapshot: BufferSnapshot, viewport_height: usize) {
    if viewport_height == 0 {
        return;
    }
    // Vertical scroll logic
    if tab.cursor_row < tab.scroll_row {
        tab.scroll_row = tab.cursor_row;
    } else if tab.cursor_row >= tab.scroll_row + viewport_height {
        tab.scroll_row = tab.cursor_row + 1 - viewport_height;
    }

    // Keep scroll_col within limits
    let line_len = get_line_char_count(&snapshot, tab.cursor_row);
    if tab.cursor_col > line_len {
        tab.cursor_col = line_len;
    }
}

fn get_line_char_count(snapshot: &BufferSnapshot, row: usize) -> usize {
    let Ok(row_u32) = u32::try_from(row) else {
        return 0;
    };
    if row_u32 >= snapshot.row_count() {
        return 0;
    }
    let Ok(len) = snapshot.line_len(row_u32) else {
        return 0;
    };
    let Ok(start) = snapshot.point_to_offset(Point::new(row_u32, 0)) else {
        return 0;
    };
    let Ok(end) = snapshot.point_to_offset(Point::new(row_u32, len)) else {
        return 0;
    };
    let Some(range) = TextRange::new(start, end) else {
        return 0;
    };
    snapshot
        .text_for_range(range)
        .ok()
        .map(|chunks| chunks.map(str::chars).map(Iterator::count).sum())
        .unwrap_or(0)
}

fn get_byte_offset(snapshot: &BufferSnapshot, row: usize, char_col: usize) -> Option<ByteOffset> {
    let row_u32 = u32::try_from(row).ok()?;
    if row_u32 >= snapshot.row_count() {
        return None;
    }
    // Walk characters to compute byte offset
    let len = snapshot.line_len(row_u32).ok()?;
    let start = snapshot.point_to_offset(Point::new(row_u32, 0)).ok()?;
    let end = snapshot.point_to_offset(Point::new(row_u32, len)).ok()?;
    let range = TextRange::new(start, end)?;
    let line_text: String = snapshot.text_for_range(range).ok()?.collect();

    let mut byte_idx = 0;
    for (i, c) in line_text.chars().enumerate() {
        if i == char_col {
            break;
        }
        byte_idx += c.len_utf8();
    }
    snapshot
        .point_to_offset(Point::new(row_u32, byte_idx as u32))
        .ok()
}
