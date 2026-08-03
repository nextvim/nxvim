pub mod commandline;
pub mod statusbar;
pub mod tabs;
pub mod textview;

use crate::editor::Editor;
use crate::ui::layout::Rect;
use std::io::Write;

pub trait View {
    fn draw(
        &self,
        w: &mut dyn Write,
        rect: Rect,
        editor: &Editor,
        buffer_manager: &mut crate::editor::buffers::BufferManager,
        doc: Option<&crate::editor::document::Document>,
        ui: &crate::ui::Ui,
    ) -> Result<Option<(u16, u16, Option<crate::ui::CursorShape>)>, Box<dyn std::error::Error>> {
        Ok(None)
    }
}
