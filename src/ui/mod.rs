pub mod colorscheme;
pub mod layout;
pub mod popup;
pub mod renderer;
pub mod views;
pub mod window;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorShape {
    Block,
    Line,
    UnderScore,
}

impl CursorShape {
    pub fn to_ansi_sequence(&self) -> &'static str {
        match self {
            CursorShape::Block => "\x1b[2 q",
            CursorShape::Line => "\x1b[6 q",
            CursorShape::UnderScore => "\x1b[4 q",
        }
    }
}

pub use window::WindowId;

use crate::controller::controllers;
use crate::editor::Editor;
use crossterm::{
    cursor::MoveTo,
    event::{self, Event, KeyCode, KeyEvent, MouseEventKind},
    execute,
    terminal::{Clear, ClearType},
};

use std::collections::HashMap;
use std::io::Write;

pub struct Ui {
    pub layout: layout::LayoutNode,
    pub editor_layout: layout::LayoutNode,
    pub screen_rows: u32,
    pub screen_cols: u32,
    pub last_parent_rect: Option<layout::Rect>,
    pub cached_layouts: Vec<(usize, layout::Rect)>,
    pub windows: HashMap<usize, window::Window>,
    pub focused_window_id: Option<usize>,
    pub last_focused_window_id: Option<usize>,
    pub needs_clear: bool,
    pub colorscheme: colorscheme::ColorScheme,
    pub popup_stack: Vec<popup::Popup>,
    pub renderer: renderer::Renderer,
    next_window_id: usize,
}

impl Ui {
    pub fn new() -> Self {
        let layout = layout::LayoutNode::Split {
            direction: layout::SplitDirection::Vertical,
            constraints: vec![
                layout::SizeConstraint::Fixed(1),        // Tabs (1 row)
                layout::SizeConstraint::Percentage(1.0), // Editor
                layout::SizeConstraint::Fixed(1),        // Statusbar (1 row)
                layout::SizeConstraint::Fixed(1),        // CommandLine (1 row)
            ],
            children: vec![
                layout::LayoutNode::Leaf {
                    window_id: WindowId::Tabs as usize,
                }, // Tabs
                layout::LayoutNode::Leaf {
                    window_id: WindowId::MainWindow as usize,
                }, // Editor
                layout::LayoutNode::Leaf {
                    window_id: WindowId::StatusBar as usize,
                }, // Statusbar
                layout::LayoutNode::Leaf {
                    window_id: WindowId::CommandLine as usize,
                }, // CommandLine
            ],
        };

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
        commandline_win.set_view(Box::new(views::commandline::CommandLineView::new()));
        commandline_win.set_controller(Box::new(
            controllers::commandline::CommandLineController::new(),
        ));
        commandline_win.draw_border = false;
        windows.insert(commandline_win_id, commandline_win);

        let editor_layout = layout::LayoutNode::Leaf {
            window_id: main_win_id,
        };

        let colorscheme = colorscheme::ColorScheme::load_default();

        let mut popup_stack = Vec::<popup::Popup>::new();
        // let mut sample_popup = popup::Popup::new(99, 2, 2, 20, 8);
        // sample_popup.window.set_view(Box::new(views::tabs::TabsView {}));
        // sample_popup.hide();
        // popup_stack.push(sample_popup);

        Self {
            layout,
            editor_layout,
            screen_rows: 0,
            screen_cols: 0,
            last_parent_rect: None,
            cached_layouts: Vec::new(),
            windows,
            focused_window_id: Some(main_win_id),
            last_focused_window_id: None,
            needs_clear: true,
            colorscheme,
            popup_stack,
            renderer: renderer::Renderer::new(),
            next_window_id: 5,
        }
    }

    fn layout(&mut self, screen_cols: u32, screen_rows: u32) -> bool {
        self.layout = layout::LayoutNode::Split {
            direction: layout::SplitDirection::Vertical,
            constraints: vec![
                layout::SizeConstraint::Fixed(1),        // Tabs (1 row)
                layout::SizeConstraint::Percentage(1.0), // Editor
                layout::SizeConstraint::Fixed(1),        // Statusbar (1 row)
                layout::SizeConstraint::Fixed(1),        // CommandLine (1 row)
            ],
            children: vec![
                layout::LayoutNode::Leaf {
                    window_id: WindowId::Tabs as usize,
                }, // Tabs
                self.editor_layout.clone(), // Editor
                layout::LayoutNode::Leaf {
                    window_id: WindowId::StatusBar as usize,
                }, // Statusbar
                layout::LayoutNode::Leaf {
                    window_id: WindowId::CommandLine as usize,
                }, // CommandLine
            ],
        };

        if self.screen_cols == screen_cols && self.screen_rows == screen_rows && self.last_parent_rect.is_some() {
            return false;
        }
        self.screen_rows = screen_rows;
        self.screen_cols = screen_cols;

        let parent_rect = layout::Rect {
            x: 0,
            y: 0,
            width: screen_cols as u16,
            height: screen_rows as u16,
        };
        let visible_check = |id| self.windows.get(&id).map(|w| w.visible).unwrap_or(true);
        self.cached_layouts = self.layout.compute_layout(parent_rect, &visible_check);
        self.last_parent_rect = Some(parent_rect);

        return true;
    }

    pub fn create_window(&mut self, id: usize) -> &mut window::Window {
        let actual_id = if id == WindowId::Any as usize {
            let nid = self.next_window_id;
            self.next_window_id += 1;
            nid
        } else {
            id
        };
        let win = window::Window::new(actual_id, String::new());
        self.windows.insert(actual_id, win);
        self.windows.get_mut(&actual_id).unwrap()
    }

    pub fn create_popup(&mut self, id: usize, x: u16, y: u16, width: u16, height: u16) -> &mut popup::Popup {
        let actual_id = if id == WindowId::Any as usize {
            let nid = self.next_window_id;
            self.next_window_id += 1;
            nid
        } else {
            id
        };
        let popup = popup::Popup::new(actual_id, x, y, width, height);
        self.popup_stack.push(popup);
        self.popup_stack.last_mut().unwrap()
    }

    pub fn find_neighbor(&self, direction: layout::NavigationDirection) -> Option<usize> {
        let focused_id = self.focused_window_id?;
        let focused_rect = self
            .cached_layouts
            .iter()
            .find(|&&(id, _)| id == focused_id)
            .map(|&(_, r)| r)?;

        let mut best_candidate = None;
        let mut min_dist = i32::MAX;

        for &(id, rect) in &self.cached_layouts {
            if id == focused_id {
                continue;
            }

            let (is_in_direction, dist) = match direction {
                layout::NavigationDirection::Left => {
                    let is_left = rect.x + rect.width <= focused_rect.x;
                    let dx = focused_rect.x as i32 - (rect.x + rect.width) as i32;
                    let dy = (rect.y as i32 + rect.height as i32 / 2)
                        - (focused_rect.y as i32 + focused_rect.height as i32 / 2);
                    (is_left, dx * 10 + dy.abs())
                }
                layout::NavigationDirection::Right => {
                    let is_right = rect.x >= focused_rect.x + focused_rect.width;
                    let dx = rect.x as i32 - (focused_rect.x + focused_rect.width) as i32;
                    let dy = (rect.y as i32 + rect.height as i32 / 2)
                        - (focused_rect.y as i32 + focused_rect.height as i32 / 2);
                    (is_right, dx * 10 + dy.abs())
                }
                layout::NavigationDirection::Up => {
                    let is_above = rect.y + rect.height <= focused_rect.y;
                    let dy = focused_rect.y as i32 - (rect.y + rect.height) as i32;
                    let dx = (rect.x as i32 + rect.width as i32 / 2)
                        - (focused_rect.x as i32 + focused_rect.width as i32 / 2);
                    (is_above, dy * 10 + dx.abs())
                }
                layout::NavigationDirection::Down => {
                    let is_below = rect.y >= focused_rect.y + focused_rect.height;
                    let dy = rect.y as i32 - (focused_rect.y + focused_rect.height) as i32;
                    let dx = (rect.x as i32 + rect.width as i32 / 2)
                        - (focused_rect.x as i32 + focused_rect.width as i32 / 2);
                    (is_below, dy * 10 + dx.abs())
                }
            };

            if is_in_direction && dist < min_dist {
                min_dist = dist;
                best_candidate = Some(id);
            }
        }

        best_candidate
    }

    pub fn set_focused_window(&mut self, window_id: usize) {
        if self.focused_window_id != Some(window_id) {
            if self.focused_window_id != Some(WindowId::CommandLine as usize) {
                self.last_focused_window_id = self.focused_window_id;
            }
            self.focused_window_id = Some(window_id);
        }
    }

    pub fn focus_window(&mut self, window_id: usize) {
        self.set_focused_window(window_id);
    }

    pub fn restore_last_focused_window(&mut self) {
        if let Some(last_id) = self.last_focused_window_id {
            self.set_focused_window(last_id);
        }
    }

    pub fn split_focused_window(
        &mut self,
        direction: layout::SplitDirection,
        file_path: Option<String>,
        buffer_manager: &mut crate::editor::buffers::BufferManager,
    ) {
        let focused_id = match self.focused_window_id {
            Some(id) if id != WindowId::Tabs as usize
                && id != WindowId::StatusBar as usize
                && id != WindowId::CommandLine as usize => id,
            _ => return,
        };

        let new_win_id = self.next_window_id;
        self.next_window_id += 1;

        let mut new_win = window::Window::new(new_win_id, String::new());
        new_win.set_view(Box::new(views::textview::TextView::new()));
        new_win.set_controller(Box::new(controllers::textview::TextViewController::new()));
        new_win.draw_border = true;

        if let Some(focused_win) = self.windows.get(&focused_id) {
            new_win.title = focused_win.title.clone();
            if let Some(buf_id) = focused_win.buffer_id {
                new_win.set_buffer(buf_id, buffer_manager);
            }
        }

        if let Some(p) = file_path {
            if let Ok(new_buf) = buffer_manager.add_buffer_for_path(&p) {
                new_win.set_buffer(new_buf.id, buffer_manager);
            }
        }

        self.windows.insert(new_win_id, new_win);

        self.editor_layout.split_leaf(focused_id, new_win_id, direction);
        self.last_parent_rect = None;

        self.set_focused_window(new_win_id);
    }

    pub fn close_window(&mut self, window_id: usize) {
        if window_id == WindowId::Tabs as usize
            || window_id == WindowId::StatusBar as usize
            || window_id == WindowId::CommandLine as usize
        {
            return;
        }

        let editor_window_count = self
            .windows
            .keys()
            .filter(|&&id| {
                id != WindowId::Tabs as usize
                    && id != WindowId::StatusBar as usize
                    && id != WindowId::CommandLine as usize
            })
            .count();

        if editor_window_count <= 1 {
            return;
        }

        let (_, sibling_id) = self.editor_layout.remove_leaf(window_id);
        self.windows.remove(&window_id);
        self.last_parent_rect = None;

        if self.focused_window_id == Some(window_id) {
            if let Some(sib) = sibling_id {
                self.set_focused_window(sib);
            } else {
                let fallback = self.windows.keys().find(|&&id| {
                    id != WindowId::Tabs as usize
                        && id != WindowId::StatusBar as usize
                        && id != WindowId::CommandLine as usize
                });
                if let Some(&f_id) = fallback {
                    self.set_focused_window(f_id);
                }
            }
        }
    }

    pub fn only_windows(&mut self) {
        let focused_id = match self.focused_window_id {
            Some(id) if id != WindowId::Tabs as usize
                && id != WindowId::StatusBar as usize
                && id != WindowId::CommandLine as usize => id,
            _ => return,
        };

        self.editor_layout = layout::LayoutNode::Leaf { window_id: focused_id };
        self.last_parent_rect = None;

        let to_remove: Vec<usize> = self
            .windows
            .keys()
            .cloned()
            .filter(|&id| {
                id != focused_id
                    && id != WindowId::Tabs as usize
                    && id != WindowId::StatusBar as usize
                    && id != WindowId::CommandLine as usize
            })
            .collect();

        for id in to_remove {
            self.windows.remove(&id);
        }
    }

    pub fn adjust_focused_window_size(&mut self, direction: layout::SplitDirection, amount: f32) {
        let focused_id = match self.focused_window_id {
            Some(id) if id != WindowId::Tabs as usize
                && id != WindowId::StatusBar as usize
                && id != WindowId::CommandLine as usize => id,
            _ => return,
        };

        if self.editor_layout.adjust_size(focused_id, direction, amount) {
            self.last_parent_rect = None; // Force layout recompute
        }
    }

    pub fn get_focused_window(&self) -> Option<&window::Window> {
        self.focused_window_id.and_then(|id| self.windows.get(&id))
    }

    pub fn get_focused_window_mut(&mut self) -> Option<&mut window::Window> {
        self.focused_window_id
            .and_then(|id| self.windows.get_mut(&id))
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
        buffer_manager: &mut crate::editor::buffers::BufferManager,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if !editor.buffers_to_redraw.is_empty() {
            for window in self.windows.values_mut() {
                if let Some(ref mut doc) = window.doc {
                    if editor.buffers_to_redraw.contains(&doc.id) {
                        doc.should_sync = true;
                    }
                }
                for (buf_id, doc) in &mut window.docs {
                    if editor.buffers_to_redraw.contains(buf_id) {
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
                        layout::Rect {
                            x: rect.x.saturating_add(1),
                            y: rect.y.saturating_add(1),
                            width: rect.width.saturating_sub(2),
                            height: rect.height.saturating_sub(2),
                        }
                    } else {
                        rect
                    };
                    c.update(editor, buffer_manager, self, window_id, adjusted_rect)?;
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
        buffer_manager: &mut crate::editor::buffers::BufferManager,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut buffered_stdout = std::io::BufWriter::with_capacity(128 * 1024, stdout);
        let stdout = &mut buffered_stdout;

        // Start synchronized update to prevent terminal from rendering intermediate states
        _ = write!(stdout, "\x1b[?2026h");

        if self.needs_clear {
            _ = crossterm::execute!(stdout, crossterm::terminal::Clear(crossterm::terminal::ClearType::All));
            self.needs_clear = false;
        }

        _ = crossterm::execute!(stdout, crossterm::cursor::Hide);

        let computed = self.cached_layouts.clone();
        for &(win_id, rect) in &computed {
            if let Some(mut win) = self.windows.remove(&win_id) {
                win.draw(
                    stdout,
                    rect,
                    editor,
                    buffer_manager,
                    self,
                )?;
                self.windows.insert(win_id, win);
            }
        }

        // Draw popups
        let ui_ptr = self as *const Ui;
        for popup in &mut self.popup_stack {
            unsafe {
                if popup.is_visible() {
                    popup.window.draw(
                        stdout,
                        popup.rect,
                        editor,
                        buffer_manager,
                        &*ui_ptr,
                    )?;
                }
            }
        }

        // Put it back permanently
        if let Some(id) = self.focused_window_id {
            if let Some(win) = self.windows.get(&id) {
                if let (Some(cx), Some(cy)) = (win.cursor_x, win.cursor_y) {
                    _ = crossterm::execute!(stdout, crossterm::cursor::MoveTo(cx, cy));
                    if let Some(shape) = win.cursor_shape {
                        _ = write!(stdout, "{}", shape.to_ansi_sequence());
                    }
                    _ = crossterm::execute!(stdout, crossterm::cursor::Show);
                } else {
                    _ = crossterm::execute!(stdout, crossterm::cursor::Hide);
                }
            } else if let Some(p) = self.popup_stack.iter().find(|p| p.window.id == id) {
                if let (Some(cx), Some(cy)) = (p.window.cursor_x, p.window.cursor_y) {
                    _ = crossterm::execute!(stdout, crossterm::cursor::MoveTo(cx, cy));
                    if let Some(shape) = p.window.cursor_shape {
                        _ = write!(stdout, "{}", shape.to_ansi_sequence());
                    }
                    _ = crossterm::execute!(stdout, crossterm::cursor::Show);
                } else {
                    _ = crossterm::execute!(stdout, crossterm::cursor::Hide);
                }
            }
        }

        // End synchronized update
        _ = write!(stdout, "\x1b[?2026l");

        buffered_stdout.flush()?;

        Ok(())
    }

    pub fn theme_color(&self, name: &str, default: crossterm::style::Color) -> crossterm::style::Color {
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
    fn test_create_window() {
        let mut ui = Ui::new();
        let win_id = 999;
        assert!(ui.windows.get(&win_id).is_none());

        {
            let win = ui.create_window(win_id);
            assert_eq!(win.id, win_id);
            win.title = "Test Window".to_string();
        }

        let win = ui.windows.get(&win_id).unwrap();
        assert_eq!(win.id, win_id);
        assert_eq!(win.title, "Test Window");
    }

    #[test]
    fn test_create_popup() {
        let mut ui = Ui::new();
        let popup_id = 999;
        {
            let popup = ui.create_popup(popup_id, 10, 10, 30, 15);
            assert_eq!(popup.window.id, popup_id);
            assert_eq!(popup.rect.x, 10);
            assert_eq!(popup.rect.y, 10);
            assert_eq!(popup.rect.width, 30);
            assert_eq!(popup.rect.height, 15);
        }

        assert_eq!(ui.popup_stack.len(), 2);
        assert_eq!(ui.popup_stack[1].window.id, popup_id);

        // Test auto id allocation
        let popup_auto = ui.create_popup(0, 0, 0, 5, 5);
        assert_ne!(popup_auto.window.id, 0);
        assert_ne!(popup_auto.window.id, popup_id);
    }

    #[test]
    fn test_find_neighbor() {
        let mut ui = Ui::new();
        ui.focused_window_id = Some(1);
        ui.cached_layouts = vec![
            (
                1,
                layout::Rect {
                    x: 10,
                    y: 10,
                    width: 10,
                    height: 10,
                },
            ), // Focused
            (
                2,
                layout::Rect {
                    x: 0,
                    y: 10,
                    width: 10,
                    height: 10,
                },
            ), // Left
            (
                3,
                layout::Rect {
                    x: 20,
                    y: 10,
                    width: 10,
                    height: 10,
                },
            ), // Right
            (
                4,
                layout::Rect {
                    x: 10,
                    y: 0,
                    width: 10,
                    height: 10,
                },
            ), // Up
            (
                5,
                layout::Rect {
                    x: 10,
                    y: 20,
                    width: 10,
                    height: 10,
                },
            ), // Down
        ];

        assert_eq!(ui.find_neighbor(layout::NavigationDirection::Left), Some(2));
        assert_eq!(
            ui.find_neighbor(layout::NavigationDirection::Right),
            Some(3)
        );
        assert_eq!(ui.find_neighbor(layout::NavigationDirection::Up), Some(4));
        assert_eq!(ui.find_neighbor(layout::NavigationDirection::Down), Some(5));
    }
}
