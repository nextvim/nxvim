use crate::editor::Editor;
use crate::editor::buffers::VimBuffers;
use crate::editor::document::VimDocument;
use crate::ui::views::{View, vim};
use std::io::Write;
use vim_ui::Rect;

pub struct StatusBarView;

impl StatusBarView {
    fn status_parts(
        &self,
        editor: &Editor,
        buffers: &VimBuffers,
        doc: Option<&VimDocument>,
    ) -> (String, String) {
        let mut left = editor.last_action.to_string();
        if let Some(doc) = doc {
            let buffer_id = vim_buffer::BufferId::new(doc.id as u64);
            let modified = buffer_id
                .and_then(|id| buffers.get(id).ok())
                .is_some_and(|buffer| buffer.is_modified());
            left.push_str(if modified { " [+]" } else { " [OK]" });
            left.push_str(&format!(" [{:?}]", doc.mode()));
            if doc.current_index_task_id
                < doc
                    .latest_index_task_id
                    .load(std::sync::atomic::Ordering::SeqCst)
            {
                left.push_str(" [Indexing...]");
            }
        }
        (left, editor.pending_keys.clone())
    }
}

impl View for StatusBarView {
    fn draw(
        &self,
        writer: &mut dyn Write,
        rect: Rect,
        editor: &Editor,
        buffers: &mut VimBuffers,
        _doc: Option<&VimDocument>,
        ui: &crate::ui::Ui,
    ) -> Result<Option<(u16, u16, Option<crate::ui::CursorShape>)>, Box<dyn std::error::Error>>
    {
        let active_doc = ui
            .focused_window_id()
            .and_then(|id| ui.window(id))
            .and_then(|win| win.doc.as_ref());
        let (left, right) = self.status_parts(editor, buffers, active_doc);
        let view = vim_ui::StatusLineView::new(left, right);
        let context = vim::ViewContext::new(ui.colorscheme());
        vim::draw(&view, writer, rect, &context)?;
        Ok(None)
    }
}
