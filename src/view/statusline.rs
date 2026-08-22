use std::any::Any;
use std::borrow::Cow;

use vim_formatter::{CompiledFormat, ExprId, FormatDialect, FormatResolver, RenderItem, parse};
use vim_ui::{Color, Rect, Renderer, Style, View};

use crate::view::globals::RenderGlobals;

/// Statusline text, built with `vim_formatter` so width handling (alignment,
/// truncation) is the same real Vim-statusline machinery the format language
/// gives every escape code, instead of ad hoc string padding.
const LINE1: &str = " %{mode} [%f%m] %=%l:%c | utf-8 ";
const LINE2: &str = " %{scope}%=";

/// Data resolved fresh each frame; the two compiled formats are static.
struct StatusLineData {
    mode_name: String,
    buffer_name: String,
    modified: bool,
    cursor: Option<(u32, u32)>,
    scope_path: Vec<String>,
    inspect_label: String,
    style: Style,
}

impl Default for StatusLineData {
    fn default() -> Self {
        Self {
            mode_name: String::new(),
            buffer_name: String::new(),
            modified: false,
            cursor: None,
            scope_path: Vec::new(),
            inspect_label: "Scope".to_string(),
            style: Style::default(),
        }
    }
}

impl FormatResolver for StatusLineData {
    fn file_name(&self) -> Cow<'_, str> {
        Cow::Borrowed(&self.buffer_name)
    }

    fn line(&self) -> usize {
        self.cursor.map(|(row, _)| row as usize).unwrap_or(0)
    }

    fn column(&self) -> usize {
        self.cursor.map(|(_, column)| column as usize).unwrap_or(0)
    }

    fn is_modified(&self) -> bool {
        self.modified
    }

    fn eval_expression(&self, _id: ExprId, source: &str) -> Cow<'_, str> {
        match source {
            "mode" => Cow::Borrowed(self.mode_name.as_str()),
            "status" => Cow::Borrowed(""),
            "scope" => Cow::Owned(if self.scope_path.is_empty() {
                format!("{}: [None]", self.inspect_label)
            } else {
                format!("{}: {}", self.inspect_label, self.scope_path.join(" > "))
            }),
            _ => Cow::Borrowed(""),
        }
    }
}

pub struct StatusLineView {
    line1: CompiledFormat,
    line2: CompiledFormat,
    data: StatusLineData,
}

impl StatusLineView {
    pub fn new() -> Self {
        let line1 = compile(LINE1);
        let line2 = compile(LINE2);
        Self {
            line1,
            line2,
            data: StatusLineData::default(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn refresh(
        &mut self,
        globals: &RenderGlobals,
        buffer_name: String,
        modified: bool,
        cursor: Option<(u32, u32)>,
        scope_path: Vec<String>,
        inspect_label: String,
    ) {
        let mut style = Style::default().bg(Color::Grey).fg(Color::Black);
        if let Some(cs) = globals.colorscheme {
            if let Some(cs_style) = cs.get_style("StatusLine") {
                style = *cs_style;
            }
        }
        self.data = StatusLineData {
            mode_name: format!("{:?}", globals.mode).to_uppercase(),
            buffer_name,
            modified,
            cursor,
            scope_path,
            inspect_label,
            style,
        };
    }
}

impl Default for StatusLineView {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vim_ui::BufferedRenderer;

    fn row_text(renderer: &BufferedRenderer, y: u16, width: u16) -> String {
        (0..width)
            .map(|x| {
                renderer
                    .current
                    .get_cell(x, y)
                    .map(|cell| cell.symbol)
                    .unwrap_or(' ')
            })
            .collect()
    }

    #[test]
    fn refresh_composes_mode_filename_and_cursor_position() {
        let mut view = StatusLineView::new();
        let globals = RenderGlobals {
            mode: vim_input::Mode::Insert,
            status_message: Some("hello"),
            search_pattern: None,
            search_regex: None,
            colorscheme: None,
        };
        view.refresh(
            &globals,
            "main.rs".to_string(),
            true,
            Some((3, 8)),
            vec!["function_item".to_string(), "block".to_string()],
            "Scope".to_string(),
        );

        let mut renderer = BufferedRenderer::new(80, 2);
        view.draw(Rect::new(0, 0, 80, 2), &mut renderer).unwrap();

        let line1 = row_text(&renderer, 0, 80);
        assert!(line1.contains("INSERT"));
        assert!(line1.contains("main.rs"));
        assert!(line1.contains("[+]"));
        assert!(!line1.contains("hello"));
        assert!(line1.contains("3:8"));

        let line2 = row_text(&renderer, 1, 80);
        assert!(line2.contains("function_item > block"));
    }

    #[test]
    fn accepts_focus_is_false() {
        assert!(!StatusLineView::new().accepts_focus());
    }
}

impl View for StatusLineView {
    fn draw(&self, area: Rect, renderer: &mut dyn Renderer) -> std::io::Result<()> {
        draw_line(
            &self.line1,
            &self.data,
            area.x,
            area.y,
            area.width,
            renderer,
        )?;
        if area.height > 1 {
            draw_line(
                &self.line2,
                &self.data,
                area.x,
                area.y + 1,
                area.width,
                renderer,
            )?;
        }
        Ok(())
    }

    fn accepts_focus(&self) -> bool {
        false
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

fn compile(source: &str) -> CompiledFormat {
    let ast =
        parse(source, FormatDialect::StatusLine).expect("built-in statusline format is valid");
    CompiledFormat::compile(&ast).expect("built-in statusline format compiles")
}

fn draw_line(
    format: &CompiledFormat,
    data: &StatusLineData,
    x: u16,
    y: u16,
    width: u16,
    renderer: &mut dyn Renderer,
) -> std::io::Result<()> {
    renderer.set_style(data.style)?;
    renderer.move_to(x, y)?;
    let items = format
        .render(data, width as usize)
        .map_err(std::io::Error::other)?;
    for item in items {
        if let RenderItem::Text { text, .. } = item {
            renderer.print(&text)?;
        }
    }
    renderer.reset_colors()
}
