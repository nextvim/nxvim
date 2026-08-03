use crate::renderer::Renderer;
use crate::types::Color;
use crossterm::{
    cursor::MoveTo,
    execute,
    style::{Print, ResetColor, SetBackgroundColor, SetForegroundColor},
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
        execute!(self.writer, ResetColor)
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
