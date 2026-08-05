use crate::rect::Rect;
use crate::renderer::Renderer;
use crate::types::Color;
use crate::window::{UIContext, View};
use vim_formatter::{CompiledFormat, FormatDialect, FormatResolver, RenderItem, parse};

#[derive(Default)]
struct StatusLineResolver;

impl FormatResolver for StatusLineResolver {}

pub struct StatusLineView {
    format: CompiledFormat,
    resolver: StatusLineResolver,
}

impl StatusLineView {
    pub fn new(left: String, right: String) -> Self {
        let source = format!(
            " {} %=% {} ",
            left.replace('%', "%%"),
            right.replace('%', "%%")
        );
        let ast = parse(&source, FormatDialect::StatusLine)
            .expect("generated statusline format must be valid");
        let format =
            CompiledFormat::compile(&ast).expect("generated statusline format must compile");

        Self {
            format,
            resolver: StatusLineResolver,
        }
    }

    fn render_text(&self, width: usize) -> std::io::Result<String> {
        let items = self
            .format
            .render(&self.resolver, width)
            .map_err(std::io::Error::other)?;
        let mut text = String::new();
        for item in items {
            if let RenderItem::Text { text: item, .. } = item {
                text.push_str(&item);
            }
        }
        Ok(text)
    }
}

impl View for StatusLineView {
    fn draw(
        &self,
        area: Rect,
        context: &dyn UIContext,
        renderer: &mut dyn Renderer,
    ) -> std::io::Result<()> {
        let mut bg = Color::Grey;
        let mut fg = Color::Black;

        if let Some(cs) = context.get_colorscheme() {
            if let Some(style) = cs.get_style("StatusLine") {
                if let Some(style_bg) = style.bg {
                    bg = style_bg;
                }
                if let Some(style_fg) = style.fg {
                    fg = style_fg;
                }
            }
        }

        renderer.set_bg(bg)?;
        renderer.set_fg(fg)?;

        let text = self.render_text(area.width as usize)?;
        renderer.move_to(area.x, area.y)?;
        renderer.print(&text)?;

        renderer.reset_colors()?;
        Ok(())
    }
}
