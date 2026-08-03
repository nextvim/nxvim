use crate::ui::layout::Rect;
use crate::ui::views::View;
use crate::{controller::controllers::ViewController, editor::Editor};
use std::io::Write;

use crossterm::{
    cursor::MoveTo,
    execute,
    style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor},
};

pub struct TabsView;

impl TabsView {
    pub fn new() -> Self {
        TabsView {}
    }
}

impl TabsView {
    fn draw_tabs<W: Write>(
        &self,
        w: &mut W,
        rect: Rect,
        editor: &Editor,
        buffer_manager: &mut crate::editor::buffers::BufferManager,
        _doc: Option<&crate::editor::document::Document>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut bar_content = "TABS".to_string();

        for (_idx, buf) in buffer_manager.file_buffers().enumerate() {
            let name = if buf.file_path.is_empty() {
                "[No Name]".to_string()
            } else {
                std::path::Path::new(&buf.file_path)
                    .file_name()
                    .unwrap_or_else(|| std::ffi::OsStr::new(&buf.file_path))
                    .to_string_lossy()
                    .into_owned()
            };

            let is_active = if let Some(doc) = _doc {
                buf.id == doc.id
            } else {
                false
            };
            let tab_text = if is_active {
                format!(" [{}] ", name)
            } else {
                format!("  {}  ", name)
            };

            bar_content.push_str(&tab_text);
        }

        let remaining = rect.width.saturating_sub(bar_content.chars().count() as u16);
        bar_content.push_str(&" ".repeat(remaining as usize));

        execute!(
            w,
            MoveTo(rect.x, rect.y),
            SetForegroundColor(Color::Black),
            SetBackgroundColor(Color::White),
            Print(bar_content),
            ResetColor,
        )?;

        Ok(())
    }
}

impl View for TabsView {
    fn draw(
        &self,
        mut w: &mut dyn Write,
        rect: Rect,
        editor: &Editor,
        buffer_manager: &mut crate::editor::buffers::BufferManager,
        _doc: Option<&crate::editor::document::Document>,
        _ui: &crate::ui::Ui,
    ) -> Result<Option<(u16, u16, Option<crate::ui::CursorShape>)>, Box<dyn std::error::Error>> {
        self.draw_tabs(&mut w, rect, editor, buffer_manager, _doc)?;
        Ok(None)
    }
}
