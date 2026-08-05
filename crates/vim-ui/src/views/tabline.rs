use crate::rect::Rect;
use crate::renderer::Renderer;
use crate::types::Color;
use crate::window::{UIContext, View};
use vim_formatter::{CompiledFormat, FormatDialect, FormatResolver, RenderItem, StyleId, parse};

const INACTIVE_STYLE: StyleId = StyleId(1);
const ACTIVE_STYLE: StyleId = StyleId(2);

#[derive(Default)]
struct TabLineResolver;

impl FormatResolver for TabLineResolver {
    fn resolve_highlight(&self, name: &str) -> Option<StyleId> {
        match name {
            "TabLine" => Some(INACTIVE_STYLE),
            "TabLineSel" => Some(ACTIVE_STYLE),
            _ => None,
        }
    }
}

pub struct TabLineView {
    format: CompiledFormat,
    resolver: TabLineResolver,
}

impl TabLineView {
    pub fn new(tabs: Vec<String>, active_index: usize) -> Self {
        let mut source = String::new();
        for (index, tab) in tabs.iter().enumerate() {
            let highlight = if index == active_index {
                "TabLineSel"
            } else {
                "TabLine"
            };
            let tab = tab.replace('%', "%%");
            source.push_str(&format!("%#{highlight}#%{}T {tab} ", index + 1));
        }
        source.push_str("%T%=");

        let ast =
            parse(&source, FormatDialect::TabLine).expect("generated tabline format must be valid");
        let format = CompiledFormat::compile(&ast).expect("generated tabline format must compile");

        Self {
            format,
            resolver: TabLineResolver,
        }
    }
}

impl View for TabLineView {
    fn draw(
        &self,
        area: Rect,
        context: &dyn UIContext,
        renderer: &mut dyn Renderer,
    ) -> std::io::Result<()> {
        let mut fill_bg = Color::DarkGrey;
        let mut fill_fg = Color::White;
        let mut active_bg = Color::Grey;
        let mut active_fg = Color::Black;
        let mut inactive_bg = Color::DarkGrey;
        let mut inactive_fg = Color::White;

        if let Some(cs) = context.get_colorscheme() {
            if let Some(style) = cs
                .get_style("TabLineFill")
                .or_else(|| cs.get_style("TabLine"))
            {
                fill_bg = style.bg.unwrap_or(fill_bg);
                fill_fg = style.fg.unwrap_or(fill_fg);
            }
            if let Some(style) = cs.get_style("TabLineSel") {
                active_bg = style.bg.unwrap_or(active_bg);
                active_fg = style.fg.unwrap_or(active_fg);
            }
            if let Some(style) = cs.get_style("TabLine") {
                inactive_bg = style.bg.unwrap_or(inactive_bg);
                inactive_fg = style.fg.unwrap_or(inactive_fg);
            }
        }

        let items = self
            .format
            .render(&self.resolver, area.width as usize)
            .map_err(std::io::Error::other)?;

        renderer.move_to(area.x, area.y)?;
        for item in items {
            let RenderItem::Text { text, style } = item else {
                continue;
            };
            let (fg, bg) = match style {
                Some(ACTIVE_STYLE) => (active_fg, active_bg),
                Some(INACTIVE_STYLE) => (inactive_fg, inactive_bg),
                _ => (fill_fg, fill_bg),
            };
            renderer.set_fg(fg)?;
            renderer.set_bg(bg)?;
            renderer.print(&text)?;
        }

        renderer.reset_colors()?;
        Ok(())
    }
}
