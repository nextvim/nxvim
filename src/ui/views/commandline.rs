use crate::editor::Editor;
use crate::ui::layout::Rect;
use crate::ui::views::View;
use crossterm::{
    cursor::MoveTo,
    execute,
    style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor},
};
use std::io::Write;

pub struct CommandLineView {
    textview: crate::ui::views::textview::TextView,
}

impl CommandLineView {
    pub fn new() -> Self {
        CommandLineView {
            textview: crate::ui::views::textview::TextView::new(),
        }
    }
}

impl View for CommandLineView {
    fn draw(
        &self,
        w: &mut dyn Write,
        rect: Rect,
        editor: &Editor,
        buffer_manager: &mut crate::editor::buffers::BufferManager,
        doc: Option<&crate::editor::document::Document>,
        ui: &crate::ui::Ui,
    ) -> Result<Option<(u16, u16, Option<crate::ui::CursorShape>)>, Box<dyn std::error::Error>>
    {
        self.textview.draw(w, rect, editor, buffer_manager, doc, ui)
    }
}
