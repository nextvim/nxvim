use super::layout::Rect;
use super::renderer::Renderer;
use super::views::View;
use crate::controller::controllers::ViewController;
use crate::editor::Editor;

use crossterm::{
    cursor::MoveTo,
    execute,
    style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor},
};
use std::io::Write;


#[repr(usize)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WindowId {
    MainWindow = 1,
    StatusBar = 2,
    Tabs = 3,
    CommandLine = 4,
    Any = 0,
}

pub struct Window {
    pub id: usize,
    pub title: String,
    pub draw_border: bool,
    pub draw_title: bool,
    pub view: Option<Box<dyn View>>,
    pub controller: Option<Box<dyn ViewController>>,
    pub buffer_id: Option<usize>,
    pub doc: Option<crate::editor::document::Document>,
    pub docs: std::collections::HashMap<usize, crate::editor::document::Document>,
    pub cursor_x: Option<u16>,
    pub cursor_y: Option<u16>,
    pub cursor_shape: Option<crate::ui::CursorShape>,
    pub visible: bool,
}

impl Window {
    pub fn new(id: usize, title: String) -> Self {
        Self {
            id,
            title,
            draw_border: true,
            draw_title: true,
            view: None,
            controller: None,
            buffer_id: None,
            doc: None,
            docs: std::collections::HashMap::new(),
            cursor_x: None,
            cursor_y: None,
            cursor_shape: None,
            visible: true,
        }
    }

    pub fn show(&mut self) {
        self.visible = true;
    }

    pub fn hide(&mut self) {
        self.visible = false;
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    pub fn set_view(&mut self, view: Box<dyn View>) {
        self.view = Some(view);
    }

    pub fn set_controller(&mut self, controller: Box<dyn ViewController>) {
        self.controller = Some(controller);
    }

    pub fn set_buffer(&mut self, buffer_id: usize, buffer_manager: &crate::editor::buffers::BufferManager) {
        if let Some(current_id) = self.buffer_id {
            if let Some(doc) = self.doc.take() {
                self.docs.insert(current_id, doc);
            }
        }
        self.buffer_id = Some(buffer_id);
        if let Some(doc) = self.docs.remove(&buffer_id) {
            self.doc = Some(doc);
        } else if let Some(buf) = buffer_manager.buffers.iter().find(|b| b.id == buffer_id) {
            self.doc = Some(crate::editor::document::Document::new_with_buffer(
                buf.id,
                &buf.buffer,
                &buf.file_path,
            ));
        }
    }

    pub fn bnext(&mut self, buffer_manager: &crate::editor::buffers::BufferManager) {
        if let Some(current_id) = self.buffer_id {
            let files: Vec<&crate::editor::buffers::TextBuffer> = buffer_manager.file_buffers().collect();
            if !files.is_empty() {
                if let Some(pos) = files.iter().position(|b| b.id == current_id) {
                    let next_idx = (pos + 1) % files.len();
                    let next_buf = files[next_idx];
                    self.set_buffer(next_buf.id, buffer_manager);
                } else {
                    let next_buf = files[0];
                    self.set_buffer(next_buf.id, buffer_manager);
                }
            }
        }
    }

    pub fn bprev(&mut self, buffer_manager: &crate::editor::buffers::BufferManager) {
        if let Some(current_id) = self.buffer_id {
            let files: Vec<&crate::editor::buffers::TextBuffer> = buffer_manager.file_buffers().collect();
            if !files.is_empty() {
                if let Some(pos) = files.iter().position(|b| b.id == current_id) {
                    let prev_idx = if pos == 0 {
                        files.len() - 1
                    } else {
                        pos - 1
                    };
                    let prev_buf = files[prev_idx];
                    self.set_buffer(prev_buf.id, buffer_manager);
                } else {
                    let prev_buf = files[files.len() - 1];
                    self.set_buffer(prev_buf.id, buffer_manager);
                }
            }
        }
    }

    pub fn draw<W: Write>(
        &mut self,
        w: &mut W,
        rect: Rect,
        editor: &Editor,
        buffer_manager: &mut crate::editor::buffers::BufferManager,
        ui: &crate::ui::Ui,
    ) -> std::io::Result<()> {
        if !self.visible || rect.width == 0 || rect.height == 0 {
            return Ok(());
        }

        if self.draw_border {
            let is_focused = ui.focused_window_id == Some(self.id);
            ui.renderer.draw_border(w, rect, is_focused, ui)?;
        }

        if self.draw_title {
            let is_focused = ui.focused_window_id == Some(self.id);
            ui.renderer.draw_title(w, rect, &self.title, is_focused, ui)?;
        }

        // Draw inner view content
        if let Some(ref mut view) = self.view {
            let inner_rect = if self.draw_border {
                Rect {
                    x: rect.x.saturating_add(1),
                    y: rect.y.saturating_add(1),
                    width: rect.width.saturating_sub(2),
                    height: rect.height.saturating_sub(2),
                }
            } else {
                rect
            };
            let doc_to_pass = self.doc.as_ref();
            if let Ok(Some((cx, cy, shape))) = view.draw(w, inner_rect, editor, buffer_manager, doc_to_pass, ui) {
                self.cursor_x = Some(cx);
                self.cursor_y = Some(cy);
                self.cursor_shape = shape;
            } else {
                self.cursor_x = None;
                self.cursor_y = None;
                self.cursor_shape = None;
            }
        } else {
            self.cursor_x = None;
            self.cursor_y = None;
            self.cursor_shape = None;
        }

        Ok(())
    }
}
