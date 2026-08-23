use std::any::Any;

use vim_formatter::{CompiledFormat, FormatDialect, FormatResolver, RenderItem, StyleId, parse};
use vim_ui::{Color, Rect, Renderer, View};

use crate::view::globals::RenderGlobals;

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

#[derive(Clone, Copy)]
struct TabLineColors {
    fill_bg: Color,
    fill_fg: Color,
    active_bg: Color,
    active_fg: Color,
    inactive_bg: Color,
    inactive_fg: Color,
}

impl Default for TabLineColors {
    fn default() -> Self {
        Self {
            fill_bg: Color::DarkGrey,
            fill_fg: Color::White,
            active_bg: Color::Grey,
            active_fg: Color::Black,
            inactive_bg: Color::DarkGrey,
            inactive_fg: Color::White,
        }
    }
}

/// Tab strip, built with `vim_formatter` for the same reason as the
/// statusline: real width/truncation handling instead of ad hoc string math.
/// The format source embeds one `%NT ... ` segment per tab, so it is rebuilt
/// (parsed and compiled) whenever the tab list changes, in `refresh`.
pub struct TabLineView {
    format: CompiledFormat,
    colors: TabLineColors,
}

impl TabLineView {
    pub fn new() -> Self {
        Self {
            format: compile("%T%="),
            colors: TabLineColors::default(),
        }
    }

    pub fn refresh(&mut self, tabs: &[String], active_index: usize, globals: &RenderGlobals) {
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
        self.format = compile(&source);

        let mut colors = TabLineColors::default();
        if let Some(cs) = globals.colorscheme {
            if let Some(style) = cs
                .get_style("TabLineFill")
                .or_else(|| cs.get_style("TabLine"))
            {
                colors.fill_bg = style.bg.unwrap_or(colors.fill_bg);
                colors.fill_fg = style.fg.unwrap_or(colors.fill_fg);
            }
            if let Some(style) = cs.get_style("TabLineSel") {
                colors.active_bg = style.bg.unwrap_or(colors.active_bg);
                colors.active_fg = style.fg.unwrap_or(colors.active_fg);
            }
            if let Some(style) = cs.get_style("TabLine") {
                colors.inactive_bg = style.bg.unwrap_or(colors.inactive_bg);
                colors.inactive_fg = style.fg.unwrap_or(colors.inactive_fg);
            }
        }
        self.colors = colors;
    }
}

impl Default for TabLineView {
    fn default() -> Self {
        Self::new()
    }
}

impl View for TabLineView {
    fn draw(&self, area: Rect, renderer: &mut dyn Renderer) -> std::io::Result<()> {
        let items = self
            .format
            .render(&TabLineResolver, area.width as usize)
            .map_err(std::io::Error::other)?;

        renderer.move_to(area.x, area.y)?;
        for item in items {
            let RenderItem::Text { text, style } = item else {
                continue;
            };
            let (fg, bg) = match style {
                Some(ACTIVE_STYLE) => (self.colors.active_fg, self.colors.active_bg),
                Some(INACTIVE_STYLE) => (self.colors.inactive_fg, self.colors.inactive_bg),
                _ => (self.colors.fill_fg, self.colors.fill_bg),
            };
            renderer.set_fg(fg)?;
            renderer.set_bg(bg)?;
            renderer.print(&text)?;
        }

        renderer.reset_colors()?;
        Ok(())
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

fn compile(source: &str) -> CompiledFormat {
    let ast =
        parse(source, FormatDialect::TabLine).expect("generated tabline format must be valid");
    CompiledFormat::compile(&ast).expect("generated tabline format must compile")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::globals::RenderGlobals;
    use vim_ui::BufferedRenderer;

    fn row_text(renderer: &BufferedRenderer, width: u16) -> String {
        (0..width)
            .map(|x| {
                renderer
                    .current
                    .get_cell(x, 0)
                    .map(|cell| cell.symbol)
                    .unwrap_or(' ')
            })
            .collect()
    }

    fn globals() -> RenderGlobals<'static> {
        RenderGlobals {
            mode: vim_input::Mode::Normal,
            status_message: None,
            search_pattern: None,
            search_regex: None,
            search_range: None,
            substitute_text: None,
            colorscheme: None,
        }
    }

    #[test]
    fn refresh_renders_every_tab_name_in_order() {
        let mut view = TabLineView::new();
        let tabs = vec!["main.rs".to_string(), "lib.rs".to_string()];
        view.refresh(&tabs, 1, &globals());

        let mut renderer = BufferedRenderer::new(40, 1);
        view.draw(Rect::new(0, 0, 40, 1), &mut renderer).unwrap();

        let text = row_text(&renderer, 40);
        let main_index = text.find("main.rs").expect("first tab name rendered");
        let lib_index = text.find("lib.rs").expect("second tab name rendered");
        assert!(main_index < lib_index);
    }

    #[test]
    fn active_tab_uses_a_different_style_than_inactive_tabs() {
        let mut view = TabLineView::new();
        let tabs = vec!["main.rs".to_string(), "lib.rs".to_string()];
        view.refresh(&tabs, 0, &globals());

        let mut renderer = BufferedRenderer::new(40, 1);
        view.draw(Rect::new(0, 0, 40, 1), &mut renderer).unwrap();

        let active_cell = renderer.current.get_cell(1, 0).unwrap();
        let inactive_start = "main.rs ".len() as u16;
        let inactive_cell = renderer.current.get_cell(inactive_start + 1, 0).unwrap();
        assert_ne!(active_cell.bg, inactive_cell.bg);
    }
}
