use std::collections::HashMap;

/// An enum representing the standard colors supported by a terminal colorscheme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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

impl From<Color> for crossterm::style::Color {
    fn from(color: Color) -> Self {
        match color {
            Color::Reset => crossterm::style::Color::Reset,
            Color::Black => crossterm::style::Color::Black,
            Color::Red => crossterm::style::Color::Red,
            Color::Green => crossterm::style::Color::Green,
            Color::Yellow => crossterm::style::Color::Yellow,
            Color::Blue => crossterm::style::Color::Blue,
            Color::Magenta => crossterm::style::Color::Magenta,
            Color::Cyan => crossterm::style::Color::Cyan,
            Color::White => crossterm::style::Color::White,
            Color::Grey => crossterm::style::Color::Grey,
            Color::DarkGrey => crossterm::style::Color::DarkGrey,
            Color::Rgb(r, g, b) => crossterm::style::Color::Rgb { r, g, b },
        }
    }
}

/// Metadata for a color scheme.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Metadata {
    pub name: String,
    pub description: Option<String>,
    pub author: Option<String>,
    pub r#type: Option<String>, // "light" or "dark"
    pub github: Option<String>,
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_colorscheme_creation_and_access() {
        let metadata = Metadata {
            name: "tokyonight-custom".to_string(),
            description: Some("Custom TokyoNight style".to_string()),
            author: Some("Author".to_string()),
            r#type: Some("dark".to_string()),
            github: None,
        };

        let mut scheme = ColorScheme::new(metadata);

        // Populating the basic quick-access colors
        scheme.foreground = Some(Color::Rgb(192, 202, 245));
        scheme.background = Some(Color::Rgb(26, 27, 38));
        scheme.cursor = Some(Color::Rgb(255, 0, 124));
        scheme.selection = Some(Color::Rgb(47, 51, 76));

        // Populating the hash of styles
        let normal_style = Style::default()
            .fg(Color::Rgb(192, 202, 245))
            .bg(Color::Rgb(26, 27, 38));

        let keyword_style = Style::default()
            .fg(Color::Rgb(187, 154, 247))
            .bold()
            .italic();

        scheme.insert_style("Normal", normal_style);
        scheme.insert_style("Keyword", keyword_style);

        // Verification
        assert_eq!(scheme.metadata.name, "tokyonight-custom");
        assert_eq!(scheme.foreground, Some(Color::Rgb(192, 202, 245)));
        assert_eq!(scheme.background, Some(Color::Rgb(26, 27, 38)));

        let normal = scheme.get_style("Normal").unwrap();
        assert_eq!(normal.fg, Some(Color::Rgb(192, 202, 245)));
        assert_eq!(normal.bg, Some(Color::Rgb(26, 27, 38)));
        assert!(!normal.bold);

        let keyword = scheme.get_style("Keyword").unwrap();
        assert_eq!(keyword.fg, Some(Color::Rgb(187, 154, 247)));
        assert!(keyword.bold);
        assert!(keyword.italic);
    }
}
