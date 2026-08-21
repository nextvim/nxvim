use crate::renderer::Renderer;
use crate::types::Color;
use crate::Style;
use crossterm::{
    cursor::{Hide, MoveTo, SetCursorStyle, Show},
    execute,
    style::{Attribute, Print, ResetColor, SetAttribute, SetBackgroundColor, SetForegroundColor},
};
use std::io::Write;

pub struct CrosstermRenderer<W: Write> {
    writer: W,
}

impl<W: Write> CrosstermRenderer<W> {
    pub fn new(writer: W) -> Self {
        Self { writer }
    }
}

impl<W: Write> Renderer for CrosstermRenderer<W> {
    fn move_to(&mut self, x: u16, y: u16) -> std::io::Result<()> {
        execute!(self.writer, MoveTo(x, y))
    }

    fn print(&mut self, text: &str) -> std::io::Result<()> {
        execute!(self.writer, Print(text))
    }

    fn set_fg(&mut self, color: Color) -> std::io::Result<()> {
        execute!(self.writer, SetForegroundColor(color.into()))
    }

    fn set_bg(&mut self, color: Color) -> std::io::Result<()> {
        execute!(self.writer, SetBackgroundColor(color.into()))
    }

    fn reset_colors(&mut self) -> std::io::Result<()> {
        execute!(self.writer, ResetColor, SetAttribute(Attribute::Reset))
    }

    fn show_cursor(
        &mut self,
        x: u16,
        y: u16,
        shape: crate::model::CursorShape,
    ) -> std::io::Result<()> {
        let shape = match shape {
            crate::model::CursorShape::Block => SetCursorStyle::SteadyBlock,
            crate::model::CursorShape::Bar => SetCursorStyle::SteadyBar,
            crate::model::CursorShape::Underline => SetCursorStyle::SteadyUnderScore,
            crate::model::CursorShape::BlinkingBlock => SetCursorStyle::BlinkingBlock,
            crate::model::CursorShape::BlinkingBar => SetCursorStyle::BlinkingBar,
            crate::model::CursorShape::BlinkingUnderline => SetCursorStyle::BlinkingUnderScore,
        };
        execute!(self.writer, MoveTo(x, y), shape, Show)
    }

    fn hide_cursor(&mut self) -> std::io::Result<()> {
        execute!(self.writer, Hide)
    }

    fn set_style(&mut self, style: Style) -> std::io::Result<()> {
        execute!(
            self.writer,
            SetAttribute(Attribute::Reset),
            SetForegroundColor(style.fg.unwrap_or(Color::Reset).into()),
            SetBackgroundColor(style.bg.unwrap_or(Color::Reset).into()),
            SetAttribute(if style.bold {
                Attribute::Bold
            } else {
                Attribute::NoBold
            }),
            SetAttribute(if style.italic {
                Attribute::Italic
            } else {
                Attribute::NoItalic
            }),
            SetAttribute(if style.underline {
                Attribute::Underlined
            } else {
                Attribute::NoUnderline
            }),
            SetAttribute(if style.strikethrough {
                Attribute::CrossedOut
            } else {
                Attribute::NotCrossedOut
            })
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FailingWriter;

    impl Write for FailingWriter {
        fn write(&mut self, _buffer: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("intentional renderer failure"))
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn propagates_writer_errors() {
        let mut renderer = CrosstermRenderer::new(FailingWriter);

        let error = renderer.print("text").unwrap_err();

        assert_eq!(error.kind(), std::io::ErrorKind::Other);
        assert_eq!(error.to_string(), "intentional renderer failure");
    }
}
