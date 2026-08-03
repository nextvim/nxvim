pub mod statusbar;
pub mod tabs;
pub mod textview;
mod vim;

use crate::editor::Editor;
use std::io::Write;
use vim_ui::Rect;

pub trait View {
    fn draw(
        &self,
        w: &mut dyn Write,
        rect: Rect,
        editor: &Editor,
        buffers: &mut crate::editor::buffers::VimBuffers,
        doc: Option<&crate::editor::document::VimDocument>,
        ui: &crate::ui::Ui,
    ) -> Result<Option<(u16, u16, Option<crate::ui::CursorShape>)>, Box<dyn std::error::Error>>
    {
        Ok(None)
    }
}
