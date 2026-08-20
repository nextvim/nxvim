use std::collections::HashMap;
use std::fs;
use std::path::Path;
use serde::Deserialize;
use crossterm::style::Color as CrossColor;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    Reset,
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    Grey,
    DarkGrey,
    Rgb(u8, u8, u8),
}

impl From<Color> for CrossColor {
    fn from(color: Color) -> Self {
        match color {
            Color::Reset => CrossColor::Reset,
            Color::Black => CrossColor::Black,
            Color::Red => CrossColor::Red,
            Color::Green => CrossColor::Green,
            Color::Yellow => CrossColor::Yellow,
            Color::Blue => CrossColor::Blue,
            Color::Magenta => CrossColor::Magenta,
            Color::Cyan => CrossColor::Cyan,
            Color::White => CrossColor::White,
            Color::Grey => CrossColor::Grey,
            Color::DarkGrey => CrossColor::DarkGrey,
            Color::Rgb(r, g, b) => CrossColor::Rgb { r, g, b },
        }
    }
}

/// Metadata for a color scheme.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
pub struct Metadata {
    pub name: String,
    pub description: Option<String>,
    pub author: Option<String>,
    pub r#type: Option<String>, // "light" or "dark"
    pub github: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ColorSchemeFile {
    pub metadata: Metadata,
    pub colors: HashMap<String, String>,
    pub ui: HashMap<String, String>,
    pub syntax: HashMap<String, String>,
}

/// A text/UI style containing colors and text attributes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Style {
    pub fg: Option<Color>,
    pub bg: Option<Color>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
}

impl Style {
    /// Creates a style with only a foreground color.
    pub fn with_fg(fg: Color) -> Self {
        Self {
            fg: Some(fg),
            ..Default::default()
        }
    }

    /// Creates a style with only a background color.
    pub fn with_bg(bg: Color) -> Self {
        Self {
            bg: Some(bg),
            ..Default::default()
        }
    }

    /// Builder method to set the foreground color.
    pub fn fg(mut self, fg: Color) -> Self {
        self.fg = Some(fg);
        self
    }

    /// Builder method to set the background color.
    pub fn bg(mut self, bg: Color) -> Self {
        self.bg = Some(bg);
        self
    }

    /// Builder method to set bold attribute.
    pub fn bold(mut self) -> Self {
        self.bold = true;
        self
    }

    /// Builder method to set italic attribute.
    pub fn italic(mut self) -> Self {
        self.italic = true;
        self
    }

    /// Builder method to set underline attribute.
    pub fn underline(mut self) -> Self {
        self.underline = true;
        self
    }

    /// Builder method to set strikethrough attribute.
    pub fn strikethrough(mut self) -> Self {
        self.strikethrough = true;
        self
    }
}

/// A Vim-compatible color scheme struct populated externally.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColorScheme {
    pub metadata: Metadata,

    // Basic colors extracted as members for quick access
    pub foreground: Option<Color>,
    pub background: Option<Color>,
    pub cursor: Option<Color>,
    pub selection: Option<Color>,

    // A hash of styles (with color)
    pub styles: HashMap<String, Style>,
}

impl ColorScheme {
    /// Creates a new empty color scheme with the given metadata.
    pub fn new(metadata: Metadata) -> Self {
        Self {
            metadata,
            foreground: None,
            background: None,
            cursor: None,
            selection: None,
            styles: HashMap::new(),
        }
    }

    /// Retrieves a style from the styles hash map.
    pub fn get_style(&self, name: &str) -> Option<&Style> {
        self.styles.get(name)
    }

    /// Inserts or updates a style in the styles hash map.
    pub fn insert_style(&mut self, name: impl Into<String>, style: Style) {
        self.styles.insert(name.into(), style);
    }

    /// Checks whether the color scheme is dark by analyzing its background color.
    pub fn is_dark(&self) -> bool {
        match self.background {
            Some(Color::Rgb(r, g, b)) => {
                let luminance = 0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32;
                luminance < 128.0
            }
            Some(Color::Black) | Some(Color::DarkGrey) => true,
            Some(Color::White) => false,
            _ => self.metadata.r#type.as_deref() != Some("light"),
        }
    }

    pub fn load_default() -> Self {
        Self::get_by_name("catppuccin").expect("Failed to load default colorscheme")
    }

    pub fn get_by_name(name: &str) -> Option<Self> {
        let contents = match name.to_lowercase().as_str() {
            "catppuccin" | "catppuccin-mocha" => Some(include_str!("./schemes/catppuccin.toml")),
            "tokyonight" => Some(include_str!("./schemes/tokyonight.toml")),
            "kanagawa" => Some(include_str!("./schemes/kanagawa.toml")),
            _ => None,
        };
        contents.and_then(|c| Self::load_from_str(c).ok())
    }

    pub fn load_from_str(contents: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let parsed: ColorSchemeFile = toml::from_str(contents)?;

        let mut colors = HashMap::new();
        for (k, v) in &parsed.colors {
            if let Some(color) = parse_hex_color(v) {
                colors.insert(k.clone(), color);
            }
        }

        let resolve = |val: &str| -> Option<Color> {
            let base_ref = val.split('+').next()?.trim();
            if let Some(resolved) = parsed.colors.get(base_ref) {
                let base_resolved = resolved.split('+').next()?.trim();
                parse_hex_color(base_resolved)
            } else {
                parse_hex_color(base_ref)
            }
        };

        let parse_style = |val: &str| -> Option<Style> {
            let parts: Vec<&str> = val.split('+').collect();
            if parts.is_empty() {
                return None;
            }
            let color_ref = parts[0].trim();
            let color = resolve(color_ref)?;

            let mut bold = false;
            let mut italic = false;
            let mut underline = false;
            let mut strikethrough = false;

            for &attr in &parts[1..] {
                match attr.trim() {
                    "bold" => bold = true,
                    "italic" => italic = true,
                    "underline" => underline = true,
                    "strikethrough" | "strike" => strikethrough = true,
                    _ => {}
                }
            }

            Some(Style {
                fg: Some(color),
                bg: None,
                bold,
                italic,
                underline,
                strikethrough,
            })
        };

        let mut styles = HashMap::new();

        for (k, v) in &parsed.syntax {
            if let Some(style) = parse_style(v) {
                let mut name = k.clone();
                if !name.is_empty() {
                    let first = name.chars().next().unwrap().to_uppercase().to_string();
                    name = format!("{}{}", first, &name[1..]);
                }
                styles.insert(name.clone(), style);
                styles.insert(k.clone(), style);
            }
        }

        let foreground = parsed.ui.get("foreground").and_then(|v| resolve(v));
        let background = parsed.ui.get("background").and_then(|v| resolve(v));
        let cursor = parsed.ui.get("caret").and_then(|v| resolve(v));
        let selection = parsed.ui.get("selection").and_then(|v| resolve(v));

        if let (Some(fg), Some(bg)) = (foreground, background) {
            styles.insert("Normal".to_string(), Style {
                fg: Some(fg),
                bg: Some(bg),
                ..Default::default()
            });
        }

        let statusline_fg = parsed.ui.get("statusline_foreground").and_then(|v| resolve(v));
        let statusline_bg = parsed.ui.get("statusline_background").and_then(|v| resolve(v));
        if statusline_fg.is_some() || statusline_bg.is_some() {
            styles.insert("StatusLine".to_string(), Style {
                fg: statusline_fg,
                bg: statusline_bg,
                ..Default::default()
            });
        }

        let tabline_fg = parsed.ui.get("tabline_foreground").and_then(|v| resolve(v));
        let tabline_bg = parsed.ui.get("tabline_background").and_then(|v| resolve(v));
        if tabline_fg.is_some() || tabline_bg.is_some() {
            styles.insert("TabLine".to_string(), Style {
                fg: tabline_fg,
                bg: tabline_bg,
                ..Default::default()
            });
        }

        let tabline_fill = parsed.ui.get("tabline_fill").and_then(|v| resolve(v));
        if let Some(bg) = tabline_fill {
            styles.insert("TabLineFill".to_string(), Style {
                bg: Some(bg),
                ..Default::default()
            });
        }

        let tabline_sel_fg = parsed.ui.get("tabline_sel_foreground").and_then(|v| resolve(v));
        let tabline_sel_bg = parsed.ui.get("tabline_sel_background").and_then(|v| resolve(v));
        if tabline_sel_fg.is_some() || tabline_sel_bg.is_some() {
            styles.insert("TabLineSel".to_string(), Style {
                fg: tabline_sel_fg,
                bg: tabline_sel_bg,
                ..Default::default()
            });
        }

        let find_highlight = parsed.ui.get("find_highlight").and_then(|v| resolve(v));
        let find_highlight_fg = parsed.ui.get("find_highlight_foreground").and_then(|v| resolve(v));
        if find_highlight.is_some() || find_highlight_fg.is_some() {
            styles.insert("Search".to_string(), Style {
                fg: find_highlight_fg,
                bg: find_highlight,
                ..Default::default()
            });
        }

        let line_nr_fg = parsed.ui.get("gutter_foreground").and_then(|v| resolve(v));
        if let Some(fg) = line_nr_fg {
            styles.insert("LineNr".to_string(), Style {
                fg: Some(fg),
                ..Default::default()
            });
        }

        let cursor_line_nr = parsed.ui.get("cursor_line_nr").and_then(|v| resolve(v));
        if let Some(fg) = cursor_line_nr {
            styles.insert("CursorLineNr".to_string(), Style {
                fg: Some(fg),
                ..Default::default()
            });
        }

        let border_fg = parsed.ui.get("border_foreground").and_then(|v| resolve(v));
        let border_bg = parsed.ui.get("border_background").and_then(|v| resolve(v));
        if border_fg.is_some() || border_bg.is_some() {
            styles.insert("WinSeparator".to_string(), Style {
                fg: border_fg,
                bg: border_bg,
                ..Default::default()
            });
        }

        Ok(Self {
            metadata: parsed.metadata,
            foreground,
            background,
            cursor,
            selection,
            styles,
        })
    }

    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self, Box<dyn std::error::Error>> {
        let contents = fs::read_to_string(path)?;
        Self::load_from_str(&contents)
    }
}

fn parse_hex_color(hex: &str) -> Option<Color> {
    let hex = hex.trim().trim_start_matches('#');
    let base_hex = hex.split('+').next()?.trim();
    if base_hex.len() == 6 {
        let r = u8::from_str_radix(&base_hex[0..2], 16).ok()?;
        let g = u8::from_str_radix(&base_hex[2..4], 16).ok()?;
        let b = u8::from_str_radix(&base_hex[4..6], 16).ok()?;
        Some(Color::Rgb(r, g, b))
    } else if base_hex.len() == 3 {
        let r_char = &base_hex[0..1];
        let g_char = &base_hex[1..2];
        let b_char = &base_hex[2..3];
        let r = u8::from_str_radix(&format!("{}{}", r_char, r_char), 16).ok()?;
        let g = u8::from_str_radix(&format!("{}{}", g_char, g_char), 16).ok()?;
        let b = u8::from_str_radix(&format!("{}{}", b_char, b_char), 16).ok()?;
        Some(Color::Rgb(r, g, b))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_colorscheme_parsing() {
        let toml_content = r##"
            [metadata]
            name = "catppuccin-mocha"
            description = "Soothing pastel theme for the high-spirited!"
            author = "Catppuccin Community"
            type = "dark"

            [colors]
            base = "#1e1e2e"
            text = "#cdd6f4"
            rosewater = "#f5e0dc"
            mauve = "#cba6f7"
            sky = "#89dceb"

            [ui]
            foreground = "text"
            background = "base"
            caret = "rosewater+bold"
            selection = "foreground"

            [syntax]
            comment = "#6c7086+italic"
            keyword = "mauve+bold+italic"
            operator = "sky+underline"
            function = "keyword+strikethrough"
        "##;

        let path = "temp_colorscheme_test.toml";
        std::fs::write(path, toml_content).unwrap();

        let scheme = ColorScheme::load_from_file(path).unwrap();

        assert_eq!(scheme.metadata.name, "catppuccin-mocha");
        assert_eq!(scheme.metadata.r#type.as_deref(), Some("dark"));

        let bg_style = scheme.styles.get("Normal").unwrap();
        assert_eq!(
            bg_style.bg,
            Some(Color::Rgb(30, 30, 46))
        );

        let keyword_style = scheme.styles.get("keyword").unwrap();
        assert_eq!(
            keyword_style.fg,
            Some(Color::Rgb(203, 166, 247))
        );
        assert!(keyword_style.bold);
        assert!(keyword_style.italic);

        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn test_load_default() {
        let scheme = ColorScheme::load_default();
        assert_eq!(scheme.metadata.name, "catppuccin-mocha");
        assert!(scheme.styles.contains_key("Normal"));
        assert!(scheme.styles.contains_key("keyword"));

        let catppuccin = ColorScheme::get_by_name("catppuccin").unwrap();
        assert_eq!(catppuccin.metadata.name, "catppuccin-mocha");

        let kanagawa = ColorScheme::get_by_name("kanagawa").unwrap();
        assert_eq!(kanagawa.metadata.name, "kanagawa");
    }

    #[test]
    fn test_is_dark() {
        let mut scheme = ColorScheme::new(Metadata::default());
        
        // Default with fallback
        assert!(scheme.is_dark());

        // Dark background (black)
        scheme.background = Some(Color::Black);
        assert!(scheme.is_dark());

        // Light background (white)
        scheme.background = Some(Color::White);
        assert!(!scheme.is_dark());

        // Custom RGB dark background (0, 0, 0)
        scheme.background = Some(Color::Rgb(0, 0, 0));
        assert!(scheme.is_dark());

        // Custom RGB light background (255, 255, 255)
        scheme.background = Some(Color::Rgb(255, 255, 255));
        assert!(!scheme.is_dark());
    }
}
