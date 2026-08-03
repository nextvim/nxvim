use crate::editor::Editor;
use crate::ui::views::{View, vim};
use std::io::Write;
use vim_ui::Rect;

pub struct TabsView;

impl View for TabsView {
    fn draw(
        &self,
        writer: &mut dyn Write,
        rect: Rect,
        _editor: &Editor,
        buffer_manager: &mut crate::editor::buffers::BufferManager,
        _doc: Option<&crate::editor::document::Document>,
        ui: &crate::ui::Ui,
    ) -> Result<Option<(u16, u16, Option<crate::ui::CursorShape>)>, Box<dyn std::error::Error>>
    {
        let active_buffer_id = ui
            .get_focused_window()
            .and_then(|window| window.doc.as_ref())
            .map(|document| document.id);
        let buffers: Vec<_> = buffer_manager.file_buffers().collect();
        let active_index = active_buffer_id
            .and_then(|id| buffers.iter().position(|buffer| buffer.id == id))
            .unwrap_or(0);
        let tabs = buffers
            .iter()
            .map(|buffer| {
                if buffer.file_path.is_empty() {
                    "[No Name]".to_string()
                } else {
                    std::path::Path::new(&buffer.file_path)
                        .file_name()
                        .unwrap_or_else(|| std::ffi::OsStr::new(&buffer.file_path))
                        .to_string_lossy()
                        .into_owned()
                }
            })
            .collect();

        let view = vim_ui::TabLineView::new(tabs, active_index);
        let context = vim::ViewContext::new(ui.colorscheme());
        vim::draw(&view, writer, rect, &context)?;
        Ok(None)
    }
}
