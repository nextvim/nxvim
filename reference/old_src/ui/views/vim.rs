use crate::ui::colorscheme::ColorScheme as LegacyColorScheme;
use std::io::Write;
use vim_ui::Rect;
use vim_ui::{BufferId, BufferViewModel, Color, ColorScheme, Metadata, Style, UIContext, View};

pub struct ViewContext {
    colorscheme: ColorScheme,
    text_models: std::collections::HashMap<vim_ui::WindowId, vim_ui::TextViewModel>,
}

impl ViewContext {
    pub fn new(source: &LegacyColorScheme) -> Self {
        let mut colorscheme = ColorScheme::new(Metadata {
            name: source.metadata.name.clone(),
            description: source.metadata.description.clone(),
            author: source.metadata.author.clone(),
            r#type: source.metadata.r#type.clone(),
            github: None,
        });

        colorscheme.foreground = source.ui.get("foreground").map(|style| color(style.color));
        colorscheme.background = source.ui.get("background").map(|style| color(style.color));
        colorscheme.cursor = source.ui.get("caret").map(|style| color(style.color));
        colorscheme.selection = source.ui.get("selection").map(|style| color(style.color));

        copy_style(
            &mut colorscheme,
            source,
            "TabLine",
            "tabline_foreground",
            "tabline_background",
        );
        copy_style(
            &mut colorscheme,
            source,
            "TabLineSel",
            "tabline_sel_foreground",
            "tabline_sel_background",
        );
        copy_style(
            &mut colorscheme,
            source,
            "TabLineFill",
            "tabline_foreground",
            "tabline_fill",
        );
        copy_style(
            &mut colorscheme,
            source,
            "StatusLine",
            "statusline_foreground",
            "statusline_background",
        );
        copy_style(
            &mut colorscheme,
            source,
            "StatusLineNC",
            "statusline_nc_foreground",
            "statusline_nc_background",
        );

        Self {
            colorscheme,
            text_models: std::collections::HashMap::new(),
        }
    }
}

impl ViewContext {
    pub fn with_text_model(
        mut self,
        window_id: vim_ui::WindowId,
        model: vim_ui::TextViewModel,
    ) -> Self {
        self.text_models.insert(window_id, model);
        self
    }
}

impl UIContext for ViewContext {
    fn get_buffer_model(&self, _id: BufferId) -> Option<BufferViewModel<'_>> {
        None
    }

    fn get_active_buffer_id(&self) -> Option<BufferId> {
        None
    }

    fn get_text_model(&self, window_id: vim_ui::WindowId) -> Option<&vim_ui::TextViewModel> {
        self.text_models.get(&window_id)
    }

    fn get_colorscheme(&self) -> Option<&ColorScheme> {
        Some(&self.colorscheme)
    }
}

pub fn draw(
    view: &dyn View,
    writer: &mut dyn Write,
    area: Rect,
    context: &ViewContext,
) -> std::io::Result<()> {
    let mut renderer = vim_ui::CrosstermRenderer::new(writer);
    view.draw(
        vim_ui::Rect::new(area.x, area.y, area.width, area.height),
        context,
        &mut renderer,
    )
}

fn copy_style(
    target: &mut ColorScheme,
    source: &LegacyColorScheme,
    target_name: &str,
    foreground_name: &str,
    background_name: &str,
) {
    let foreground = source.ui.get(foreground_name);
    let background = source.ui.get(background_name);
    if foreground.is_none() && background.is_none() {
        return;
    }
    let attributes = foreground.or(background).unwrap();
    target.insert_style(
        target_name,
        Style {
            fg: foreground.map(|style| color(style.color)),
            bg: background.map(|style| color(style.color)),
            bold: attributes.bold,
            italic: attributes.italic,
            underline: attributes.underline,
            strikethrough: attributes.strikethrough,
        },
    );
}

pub fn color(source: crossterm::style::Color) -> Color {
    use crossterm::style::Color as CrosstermColor;

    match source {
        CrosstermColor::Reset => Color::Reset,
        CrosstermColor::Black => Color::Black,
        CrosstermColor::DarkGrey => Color::DarkGrey,
        CrosstermColor::Red | CrosstermColor::DarkRed => Color::Red,
        CrosstermColor::Green | CrosstermColor::DarkGreen => Color::Green,
        CrosstermColor::Yellow | CrosstermColor::DarkYellow => Color::Yellow,
        CrosstermColor::Blue | CrosstermColor::DarkBlue => Color::Blue,
        CrosstermColor::Magenta | CrosstermColor::DarkMagenta => Color::Magenta,
        CrosstermColor::Cyan | CrosstermColor::DarkCyan => Color::Cyan,
        CrosstermColor::White => Color::White,
        CrosstermColor::Grey => Color::Grey,
        CrosstermColor::Rgb { r, g, b } => Color::Rgb(r, g, b),
        CrosstermColor::AnsiValue(value) => ansi_color(value),
    }
}

fn ansi_color(value: u8) -> Color {
    if value < 16 {
        return match value {
            0 => Color::Black,
            1 | 9 => Color::Red,
            2 | 10 => Color::Green,
            3 | 11 => Color::Yellow,
            4 | 12 => Color::Blue,
            5 | 13 => Color::Magenta,
            6 | 14 => Color::Cyan,
            7 | 15 => Color::White,
            8 => Color::DarkGrey,
            _ => Color::Reset,
        };
    }
    if value <= 231 {
        let index = value - 16;
        let component = |value: u8| if value == 0 { 0 } else { 55 + value * 40 };
        return Color::Rgb(
            component(index / 36),
            component((index % 36) / 6),
            component(index % 6),
        );
    }
    let grey = 8 + (value - 232) * 10;
    Color::Rgb(grey, grey, grey)
}
