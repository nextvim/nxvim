use vim_ui::Rect;

use super::views::View;
use crate::controller::controllers::ViewController;
use crate::editor::Editor;
use crate::editor::buffers::VimBuffers;
use crate::editor::document::VimDocument;
use vim_buffer::BufferId;
use vim_ui::Renderer as _;

use std::io::Write;

#[repr(usize)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WindowId {
    MainWindow = 1,
    StatusBar = 2,
    Tabs = 3,
    CommandLine = 4,
}

pub struct Window {
    pub id: usize,
    pub title: String,
    pub draw_border: bool,
    pub draw_title: bool,
    pub view: Option<Box<dyn View>>,
    pub controller: Option<Box<dyn ViewController>>,
    /// Canonical Vim-backed window selection and document state.
    pub vim_buffer_id: Option<BufferId>,
    pub doc: Option<VimDocument>,
    pub docs: std::collections::HashMap<BufferId, VimDocument>,
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
            vim_buffer_id: None,
            doc: None,
            docs: std::collections::HashMap::new(),
            cursor_x: None,
            cursor_y: None,
            cursor_shape: None,
            visible: true,
        }
    }

    pub fn set_view(&mut self, view: Box<dyn View>) {
        self.view = Some(view);
    }

    pub fn set_controller(&mut self, controller: Box<dyn ViewController>) {
        self.controller = Some(controller);
    }

    /// Select the active Vim-backed buffer and restore its document state.
    pub fn set_vim_buffer(&mut self, buffer_id: BufferId, buffers: &VimBuffers) -> bool {
        if let Some(current_id) = self.vim_buffer_id {
            if let Some(doc) = self.doc.take() {
                self.docs.insert(current_id, doc);
            }
        }

        let Some(_buffer) = buffers.get(buffer_id).ok() else {
            return false;
        };
        self.vim_buffer_id = Some(buffer_id);
        self.doc = self
            .docs
            .remove(&buffer_id)
            .or_else(|| buffers.document(buffer_id).ok());
        self.doc.is_some()
    }

    pub fn vim_bnext(&mut self, buffers: &VimBuffers) -> bool {
        let current = self.vim_buffer_id;
        let files: Vec<_> = buffers.file_buffers().map(|entry| entry.id).collect();
        if files.is_empty() {
            return false;
        }
        let next = current
            .and_then(|id| files.iter().position(|candidate| *candidate == id))
            .map(|position| files[(position + 1) % files.len()])
            .unwrap_or(files[0]);
        self.set_vim_buffer(next, buffers)
    }

    pub fn vim_bprev(&mut self, buffers: &VimBuffers) -> bool {
        let current = self.vim_buffer_id;
        let files: Vec<_> = buffers.file_buffers().map(|entry| entry.id).collect();
        if files.is_empty() {
            return false;
        }
        let previous = current
            .and_then(|id| files.iter().position(|candidate| *candidate == id))
            .map(|position| {
                files[if position == 0 {
                    files.len() - 1
                } else {
                    position - 1
                }]
            })
            .unwrap_or(files[files.len() - 1]);
        self.set_vim_buffer(previous, buffers)
    }

    pub fn draw<W: Write>(
        &mut self,
        w: &mut W,
        rect: Rect,
        editor: &Editor,
        buffers: &mut crate::editor::buffers::VimBuffers,
        ui: &crate::ui::Ui,
    ) -> std::io::Result<()> {
        if !self.visible || rect.width == 0 || rect.height == 0 {
            return Ok(());
        }

        if self.draw_border {
            let is_focused = ui.focused_window_id() == Some(self.id);
            let foreground = if is_focused {
                vim_ui::Color::Magenta
            } else {
                vim_ui::Color::DarkGrey
            };
            let mut renderer = vim_ui::CrosstermRenderer::new(&mut *w);
            renderer.draw_window_frame(
                vim_ui::Rect::new(rect.x, rect.y, rect.width, rect.height),
                self.draw_title.then_some(self.title.as_str()),
                vim_ui::Style::with_fg(foreground),
            )?;
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
            if let Ok(Some((cx, cy, shape))) =
                view.draw(w, inner_rect, editor, buffers, self.doc.as_ref(), ui)
            {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vim_buffer_selection_uses_canonical_window_document() {
        let mut buffers = VimBuffers::new();
        let first = buffers.create("first");
        let second = buffers.create("second");
        let mut window = Window::new(WindowId::MainWindow as usize, "Editor".into());

        assert!(window.set_vim_buffer(first, &buffers));
        assert_eq!(window.vim_buffer_id, Some(first));
        assert_eq!(
            window.doc.as_ref().map(|doc| doc.id),
            Some(first.get() as usize)
        );

        assert!(window.set_vim_buffer(second, &buffers));
        assert_eq!(window.vim_buffer_id, Some(second));
        assert_eq!(
            window.doc.as_ref().map(|doc| doc.id),
            Some(second.get() as usize)
        );
        assert!(window.docs.contains_key(&first));
    }
}
