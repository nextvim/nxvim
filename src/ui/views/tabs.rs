use crate::editor::Editor;
use crate::editor::buffers::VimBuffers;
use crate::editor::document::VimDocument;
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
        buffers: &mut VimBuffers,
        _doc: Option<&VimDocument>,
        ui: &crate::ui::Ui,
    ) -> Result<Option<(u16, u16, Option<crate::ui::CursorShape>)>, Box<dyn std::error::Error>>
    {
        let active_buffer_id = ui
            .get_focused_window()
            .and_then(|window| window.vim_buffer_id);
        let entries: Vec<_> = buffers.file_buffers().collect();
        let active_index = active_buffer_id
            .and_then(|id| entries.iter().position(|entry| entry.id == id))
            .unwrap_or(0);
        let tabs = entries
            .iter()
            .map(|entry| {
                if entry.file_path.is_empty() {
                    "[No Name]".to_string()
                } else {
                    std::path::Path::new(&entry.file_path)
                        .file_name()
                        .unwrap_or_else(|| std::ffi::OsStr::new(&entry.file_path))
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
