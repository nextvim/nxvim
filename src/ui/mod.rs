pub mod colorscheme;

pub mod views;
pub mod window;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorShape {
    Block,
    Line,
}

pub use window::WindowId;

use crate::controller::controllers;
use crate::editor::Editor;

use std::collections::HashMap;
use std::io::Write;

pub struct Ui {
    core: vim_ui::Ui,
    screen_rows: u32,
    screen_cols: u32,
    cached_layouts: Vec<(usize, vim_ui::Rect)>,
    windows: HashMap<usize, window::Window>,
    needs_clear: bool,
    colorscheme: colorscheme::ColorScheme,
}

impl Ui {
    pub fn new() -> Self {
        let mut windows = HashMap::new();

        // Create initial default window
        let main_win_id = WindowId::MainWindow as usize;
        let mut main_win = window::Window::new(main_win_id, "Editor".to_string());
        main_win.set_view(Box::new(views::textview::TextView::new()));
        main_win.set_controller(Box::new(controllers::textview::TextViewController::new()));
        main_win.draw_border = true;
        windows.insert(main_win_id, main_win);

        // Create tabs window
        let tabs_win_id = WindowId::Tabs as usize;
        let mut tabs_win = window::Window::new(tabs_win_id, "Tabs".to_string());
        tabs_win.set_view(Box::new(views::tabs::TabsView {}));
        tabs_win.draw_border = false;
        windows.insert(tabs_win_id, tabs_win);

        // Create status bar window
        let statusbar_win_id = WindowId::StatusBar as usize;
        let mut statusbar_win = window::Window::new(statusbar_win_id, "Status Bar".to_string());
        statusbar_win.set_view(Box::new(views::statusbar::StatusBarView {}));
        statusbar_win.draw_border = false;
        // statusbar_win.hide();
        windows.insert(statusbar_win_id, statusbar_win);

        // Create command bar window
        let commandline_win_id = WindowId::CommandLine as usize;
        let mut commandline_win = window::Window::new(commandline_win_id, "Command".to_string());
        commandline_win.set_view(Box::new(views::textview::TextView::new()));
        commandline_win.set_controller(Box::new(
            controllers::commandline::CommandLineController::new(),
        ));
        commandline_win.draw_border = false;
        windows.insert(commandline_win_id, commandline_win);

        let mut core = vim_ui::Ui::new(vim_ui::Rect::new(0, 0, 0, 0));
        let status_id = core.create_window("Status Bar");
        let tabs_id = core.create_window("Tabs");
        let command_id = core.create_window("Command");
        debug_assert_eq!(status_id.get(), WindowId::StatusBar as u64);
        debug_assert_eq!(tabs_id.get(), WindowId::Tabs as u64);
        debug_assert_eq!(command_id.get(), WindowId::CommandLine as u64);
        core.set_layout(Self::root_layout(vim_ui::LayoutNode::Leaf {
            window_id: vim_ui::WindowId::new(main_win_id as u64),
        }))
        .expect("initial nxvim layout must be valid");

        let colorscheme = colorscheme::ColorScheme::load_default();

        Self {
            core,
            screen_rows: 0,
            screen_cols: 0,
            cached_layouts: Vec::new(),
            windows,
            needs_clear: true,
            colorscheme,
        }
    }

    fn root_layout(editor_layout: vim_ui::LayoutNode) -> vim_ui::LayoutNode {
        vim_ui::LayoutNode::Split {
            axis: vim_ui::SplitAxis::Rows,
            constraints: vec![
                vim_ui::SizeConstraint::Fixed(1),
                vim_ui::SizeConstraint::Percentage(1.0),
                vim_ui::SizeConstraint::Fixed(1),
                vim_ui::SizeConstraint::Fixed(1),
            ],
            children: vec![
                vim_ui::LayoutNode::Leaf {
                    window_id: vim_ui::WindowId::new(WindowId::Tabs as u64),
                },
                editor_layout,
                vim_ui::LayoutNode::Leaf {
                    window_id: vim_ui::WindowId::new(WindowId::StatusBar as u64),
                },
                vim_ui::LayoutNode::Leaf {
                    window_id: vim_ui::WindowId::new(WindowId::CommandLine as u64),
                },
            ],
        }
    }

    fn layout(&mut self, screen_cols: u32, screen_rows: u32) -> bool {
        if self.screen_cols == screen_cols && self.screen_rows == screen_rows {
            return false;
        }
        self.screen_rows = screen_rows;
        self.screen_cols = screen_cols;
        self.core.resize(vim_ui::Rect::new(
            0,
            0,
            screen_cols as u16,
            screen_rows as u16,
        ));
        self.sync_cached_layout();
        true
    }

    fn sync_cached_layout(&mut self) {
        self.cached_layouts = self
            .core
            .computed_layout()
            .windows
            .iter()
            .map(|(id, rect)| {
                (
                    id.get() as usize,
                    vim_ui::Rect {
                        x: rect.x,
                        y: rect.y,
                        width: rect.width,
                        height: rect.height,
                    },
                )
            })
            .collect();
    }

    pub fn find_neighbor(&self, direction: vim_ui::NavigationDirection) -> Option<usize> {
        self.core
            .find_neighbor(direction)
            .map(|id| id.get() as usize)
    }

    pub fn set_focused_window(&mut self, window_id: usize) {
        let _ = self.core.focus(vim_ui::WindowId::new(window_id as u64));
    }

    pub fn focus_window(&mut self, window_id: usize) {
        self.set_focused_window(window_id);
    }

    pub fn window(&self, window_id: usize) -> Option<&window::Window> {
        self.windows.get(&window_id)
    }

    pub fn window_mut(&mut self, window_id: usize) -> Option<&mut window::Window> {
        self.windows.get_mut(&window_id)
    }

    pub fn create_popup(
        &mut self,
        title: impl Into<String>,
        config: vim_ui::FloatingConfig,
        modal: bool,
    ) -> usize {
        let title = title.into();
        let id = self.core.create_floating_window(title.clone(), config);
        if modal {
            self.core.overlay_manager_mut().set_modal(Some(id));
        }
        let mut window = window::Window::new(id.get() as usize, title);
        window.draw_border = config.border;
        window.draw_title = config.border;
        self.windows.insert(id.get() as usize, window);
        id.get() as usize
    }

    pub fn focused_window_id(&self) -> Option<usize> {
        Some(self.core.focused_window_id().get() as usize)
    }

    pub fn last_focused_window_id(&self) -> Option<usize> {
        self.core
            .focus_manager()
            .previous_id()
            .map(|id| id.get() as usize)
    }

    pub fn colorscheme(&self) -> &colorscheme::ColorScheme {
        &self.colorscheme
    }

    pub fn set_colorscheme(&mut self, colorscheme: colorscheme::ColorScheme) {
        self.colorscheme = colorscheme;
        self.clear_highlights();
    }

    pub fn set_vim_window_buffer(
        &mut self,
        window_id: usize,
        buffer_id: vim_buffer::BufferId,
        buffers: &crate::editor::buffers::VimBuffers,
    ) -> bool {
        let Some(window) = self.windows.get_mut(&window_id) else {
            return false;
        };
        window.set_vim_buffer(buffer_id, buffers)
    }

    pub fn focused_vim_buffer_id(&self) -> Option<vim_buffer::BufferId> {
        self.get_focused_window()
            .and_then(|window| window.vim_buffer_id)
    }

    pub fn take_window_controller(
        &mut self,
        window_id: usize,
    ) -> Option<Box<dyn crate::controller::controllers::ViewController>> {
        self.windows
            .get_mut(&window_id)
            .and_then(|window| window.controller.take())
    }

    pub fn restore_window_controller(
        &mut self,
        window_id: usize,
        controller: Option<Box<dyn crate::controller::controllers::ViewController>>,
    ) {
        if let Some(window) = self.windows.get_mut(&window_id) {
            window.controller = controller;
        }
    }

    pub fn cancel_document_parse_tasks(&mut self) {
        for window in self.windows.values_mut() {
            if let Some(document) = &mut window.doc {
                document
                    .latest_parse_task_id
                    .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }
        }
    }

    pub fn restore_last_focused_window(&mut self) {
        if let Some(last_id) = self.last_focused_window_id() {
            self.set_focused_window(last_id);
        }
    }

    pub fn split_focused_window(
        &mut self,
        axis: vim_ui::SplitAxis,
        file_path: Option<String>,
        buffers: &mut crate::editor::buffers::VimBuffers,
    ) {
        let Some(focused_id) = self.focused_window_id().filter(|id| {
            *id != WindowId::Tabs as usize
                && *id != WindowId::StatusBar as usize
                && *id != WindowId::CommandLine as usize
        }) else {
            return;
        };

        let Ok(core_id) = self.core.split_focused(axis) else {
            return;
        };
        let new_win_id = core_id.get() as usize;

        let mut new_win = window::Window::new(new_win_id, String::new());
        new_win.set_view(Box::new(views::textview::TextView::new()));
        new_win.set_controller(Box::new(controllers::textview::TextViewController::new()));
        new_win.draw_border = true;

        if let Some(focused_win) = self.windows.get(&focused_id) {
            new_win.title = focused_win.title.clone();
            if let Some(buf_id) = focused_win.vim_buffer_id {
                new_win.set_vim_buffer(buf_id, buffers);
            }
        }

        if let Some(p) = file_path {
            if let Ok(new_buf) = buffers.add_buffer_for_path(&p) {
                new_win.set_vim_buffer(new_buf.id, buffers);
            }
        }

        self.windows.insert(new_win_id, new_win);
        self.sync_cached_layout();
    }

    pub fn close_window(&mut self, window_id: usize) {
        if window_id == WindowId::Tabs as usize
            || window_id == WindowId::StatusBar as usize
            || window_id == WindowId::CommandLine as usize
        {
            return;
        }

        let core_id = vim_ui::WindowId::new(window_id as u64);
        if self.core.overlay_manager().is_floating(core_id) {
            if self.core.close_window(core_id).is_ok() {
                self.windows.remove(&window_id);
            }
            return;
        }

        let editor_window_count = self
            .windows
            .keys()
            .filter(|&&id| {
                id != WindowId::Tabs as usize
                    && id != WindowId::StatusBar as usize
                    && id != WindowId::CommandLine as usize
                    && !self
                        .core
                        .overlay_manager()
                        .is_floating(vim_ui::WindowId::new(id as u64))
            })
            .count();

        if editor_window_count <= 1 {
            return;
        }

        if self
            .core
            .close_window(vim_ui::WindowId::new(window_id as u64))
            .is_ok()
        {
            self.windows.remove(&window_id);
            self.sync_cached_layout();
        }
    }

    pub fn only_windows(&mut self) {
        let Some(focused_id) = self.focused_window_id().filter(|id| {
            *id != WindowId::Tabs as usize
                && *id != WindowId::StatusBar as usize
                && *id != WindowId::CommandLine as usize
        }) else {
            return;
        };

        let to_remove: Vec<usize> = self
            .windows
            .keys()
            .cloned()
            .filter(|&id| {
                id != focused_id
                    && id != WindowId::Tabs as usize
                    && id != WindowId::StatusBar as usize
                    && id != WindowId::CommandLine as usize
                    && !self
                        .core
                        .overlay_manager()
                        .is_floating(vim_ui::WindowId::new(id as u64))
            })
            .collect();

        for id in to_remove {
            if self
                .core
                .close_window(vim_ui::WindowId::new(id as u64))
                .is_ok()
            {
                self.windows.remove(&id);
            }
        }
        self.sync_cached_layout();
    }

    pub fn adjust_focused_window_size(&mut self, axis: vim_ui::SplitAxis, amount: f32) {
        let Some(focused_id) = self.focused_window_id().filter(|id| {
            *id != WindowId::Tabs as usize
                && *id != WindowId::StatusBar as usize
                && *id != WindowId::CommandLine as usize
        }) else {
            return;
        };
        if self
            .core
            .adjust_window_size(vim_ui::WindowId::new(focused_id as u64), axis, amount)
            .unwrap_or(false)
        {
            self.sync_cached_layout();
        }
    }

    pub fn get_focused_window(&self) -> Option<&window::Window> {
        self.focused_window_id()
            .and_then(|id| self.windows.get(&id))
    }

    pub fn get_focused_window_mut(&mut self) -> Option<&mut window::Window> {
        let id = self.focused_window_id()?;
        self.windows.get_mut(&id)
    }

    pub fn clear_highlights(&mut self) {
        for win in self.windows.values_mut() {
            if let Some(ref mut doc) = win.doc {
                doc.hl.clear();
                doc.should_sync = true;
            }
            for doc in win.docs.values_mut() {
                doc.hl.clear();
                doc.should_sync = true;
            }
        }
    }

    pub fn update(
        &mut self,
        editor: &mut Editor,
        buffers: &mut crate::editor::buffers::VimBuffers,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if !editor.buffers_to_redraw.is_empty() {
            for window in self.windows.values_mut() {
                if let Some(ref mut doc) = window.doc {
                    if editor.buffers_to_redraw.contains(&doc.id) {
                        doc.should_sync = true;
                    }
                }
                for (buf_id, doc) in &mut window.docs {
                    if editor.buffers_to_redraw.contains(&(buf_id.get() as usize)) {
                        doc.should_sync = true;
                    }
                }
            }
            editor.buffers_to_redraw.clear();
            editor.should_redraw = true;
        }

        // Handle terminal resize.
        let (screen_cols, screen_rows) = {
            let (cols, rows) = crossterm::terminal::size().unwrap();
            (cols as u32, rows as u32)
        };

        let old_cols = self.screen_cols;
        let old_rows = self.screen_rows;

        // Recompute layout if needed.
        // Update window rects.
        if self.layout(screen_cols, screen_rows) {
            for window in self.windows.values_mut() {
                if let Some(ref mut doc) = window.doc {
                    doc.should_sync = true;
                }
            }
            editor.should_redraw = true;
            if old_cols != screen_cols || old_rows != screen_rows {
                self.needs_clear = true;
            }
        }

        let computed = self.cached_layouts.clone();
        for &(window_id, rect) in &computed {
            if let Some(window) = self.windows.get_mut(&window_id) {
                let mut controller = window.controller.take();
                if let Some(ref mut c) = controller {
                    let adjusted_rect = if window.draw_border {
                        vim_ui::Rect {
                            x: rect.x.saturating_add(1),
                            y: rect.y.saturating_add(1),
                            width: rect.width.saturating_sub(2),
                            height: rect.height.saturating_sub(2),
                        }
                    } else {
                        rect
                    };
                    c.update(editor, buffers, self, window_id, adjusted_rect)?;
                }
                if let Some(window) = self.windows.get_mut(&window_id) {
                    window.controller = controller;
                }
            }
        }

        Ok(())
    }

    pub fn draw<W: Write>(
        &mut self,
        stdout: &mut W,
        editor: &mut Editor,
        buffers: &mut crate::editor::buffers::VimBuffers,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut buffered_stdout = std::io::BufWriter::with_capacity(128 * 1024, stdout);
        let stdout = &mut buffered_stdout;

        // Start synchronized update to prevent terminal from rendering intermediate states
        _ = write!(stdout, "\x1b[?2026h");

        if self.needs_clear {
            _ = crossterm::execute!(
                stdout,
                crossterm::terminal::Clear(crossterm::terminal::ClearType::All)
            );
            self.needs_clear = false;
        }

        {
            use vim_ui::Renderer as _;
            vim_ui::CrosstermRenderer::new(&mut *stdout).hide_cursor()?;
        }

        let computed = self.cached_layouts.clone();
        for &(win_id, rect) in &computed {
            if let Some(mut win) = self.windows.remove(&win_id) {
                win.draw(stdout, rect, editor, buffers, self)?;
                self.windows.insert(win_id, win);
            }
        }

        let focused_cursor = self
            .focused_window_id()
            .and_then(|id| self.windows.get(&id))
            .and_then(|window| Some((window.cursor_x?, window.cursor_y?)));
        let overlays = self.core.computed_overlays(focused_cursor);
        for (id, rect) in overlays {
            let id = id.get() as usize;
            if let Some(mut window) = self.windows.remove(&id) {
                window.draw(
                    stdout,
                    vim_ui::Rect {
                        x: rect.x,
                        y: rect.y,
                        width: rect.width,
                        height: rect.height,
                    },
                    editor,
                    buffers,
                    self,
                )?;
                self.windows.insert(id, window);
            }
        }

        use vim_ui::Renderer as _;
        let mut renderer = vim_ui::CrosstermRenderer::new(&mut *stdout);
        let cursor = self
            .focused_window_id()
            .and_then(|id| self.windows.get(&id))
            .and_then(|window| Some((window.cursor_x?, window.cursor_y?, window.cursor_shape?)));
        if let Some((x, y, shape)) = cursor {
            let shape = match shape {
                CursorShape::Block => vim_ui::CursorShape::Block,
                CursorShape::Line => vim_ui::CursorShape::Bar,
            };
            renderer.show_cursor(x, y, shape)?;
        } else {
            renderer.hide_cursor()?;
        }

        // End synchronized update
        _ = write!(stdout, "\x1b[?2026l");

        buffered_stdout.flush()?;

        Ok(())
    }

    pub fn theme_color(
        &self,
        name: &str,
        default: crossterm::style::Color,
    ) -> crossterm::style::Color {
        self.colorscheme
            .ui
            .get(name)
            .map(|s| s.color)
            .unwrap_or(default)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn popup_lifecycle_is_owned_by_overlay_manager() {
        let mut ui = Ui::new();
        ui.layout(100, 30);
        let id = ui.create_popup(
            "Completion",
            vim_ui::FloatingConfig {
                relative_to: vim_ui::RelativeTo::Editor,
                anchor: vim_ui::Anchor::TopLeft,
                row: 2,
                col: 3,
                width: 20,
                height: 5,
                zindex: 50,
                border: true,
            },
            true,
        );

        assert!(
            ui.core
                .overlay_manager()
                .is_floating(vim_ui::WindowId::new(id as u64))
        );
        assert_eq!(
            ui.core.computed_overlays(None)[0].1,
            vim_ui::Rect::new(3, 2, 20, 5)
        );
        assert!(ui.window(id).is_some());

        ui.close_window(id);
        assert!(ui.window(id).is_none());
        assert!(
            !ui.core
                .overlay_manager()
                .is_floating(vim_ui::WindowId::new(id as u64))
        );
    }

    #[test]
    fn test_find_neighbor() {
        let mut ui = Ui::new();
        let mut buffers = crate::editor::buffers::VimBuffers::new();
        buffers.create("first");
        ui.layout(100, 30);
        ui.split_focused_window(vim_ui::SplitAxis::Columns, None, &mut buffers);

        assert_eq!(ui.focused_window_id(), Some(5));
        assert_eq!(ui.find_neighbor(vim_ui::NavigationDirection::Left), Some(1));
        ui.focus_window(1);
        assert_eq!(
            ui.find_neighbor(vim_ui::NavigationDirection::Right),
            Some(5)
        );

        ui.focus_window(5);
        ui.close_window(5);
        assert_eq!(ui.focused_window_id(), Some(1));
        assert!(ui.window(5).is_none());
        assert!(ui.core.window(vim_ui::WindowId::new(5)).is_none());
    }
}
