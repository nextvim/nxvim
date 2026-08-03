use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
pub struct Metadata {
    pub name: String,
    pub description: Option<String>,
    pub author: Option<String>,
    pub r#type: Option<String>, // "light" or "dark"
}

#[derive(Debug, Clone, Deserialize)]
pub struct ColorSchemeFile {
    pub metadata: Metadata,
    pub colors: HashMap<String, String>,
    pub ui: HashMap<String, String>,
    pub syntax: HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Style {
    pub color: crossterm::style::Color,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
}

#[derive(Debug, Clone)]
pub struct ColorScheme {
    pub metadata: Metadata,
    pub colors: HashMap<String, crossterm::style::Color>,
    pub ui: HashMap<String, Style>,
    pub syntax: HashMap<String, Style>,
}

impl ColorScheme {
    pub fn load_default() -> Self {
        Self::get_by_name("catppuccin").expect("Failed to load default tokyonight colorscheme")
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

        let mut ui = HashMap::new();
        for (k, v) in &parsed.ui {
            if let Some(style) = parse_style(v, &parsed.colors, &parsed.ui) {
                ui.insert(k.clone(), style);
            }
        }

        let mut syntax = HashMap::new();
        for (k, v) in &parsed.syntax {
            if let Some(style) = parse_style(v, &parsed.colors, &parsed.syntax) {
                syntax.insert(k.clone(), style);
            }
        }

        Ok(Self {
            metadata: parsed.metadata,
            colors,
            ui,
            syntax,
        })
    }

    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self, Box<dyn std::error::Error>> {
        let contents = fs::read_to_string(path)?;
        Self::load_from_str(&contents)
    }
}

fn parse_hex_color(hex: &str) -> Option<crossterm::style::Color> {
    let hex = hex.trim().trim_start_matches('#');
    let base_hex = hex.split('+').next()?.trim();
    if base_hex.len() == 6 {
        let r = u8::from_str_radix(&base_hex[0..2], 16).ok()?;
        let g = u8::from_str_radix(&base_hex[2..4], 16).ok()?;
        let b = u8::from_str_radix(&base_hex[4..6], 16).ok()?;
        Some(crossterm::style::Color::Rgb { r, g, b })
    } else if base_hex.len() == 3 {
        let r_char = &base_hex[0..1];
        let g_char = &base_hex[1..2];
        let b_char = &base_hex[2..3];
        let r = u8::from_str_radix(&format!("{}{}", r_char, r_char), 16).ok()?;
        let g = u8::from_str_radix(&format!("{}{}", g_char, g_char), 16).ok()?;
        let b = u8::from_str_radix(&format!("{}{}", b_char, b_char), 16).ok()?;
        Some(crossterm::style::Color::Rgb { r, g, b })
    } else {
        None
    }
}

fn resolve_color(
    val: &str,
    palette: &HashMap<String, String>,
    fallback_map: &HashMap<String, String>,
) -> Option<crossterm::style::Color> {
    resolve_color_recursive(val, palette, fallback_map, 0)
}

fn resolve_color_recursive(
    val: &str,
    palette: &HashMap<String, String>,
    fallback_map: &HashMap<String, String>,
    depth: usize,
) -> Option<crossterm::style::Color> {
    if depth > 10 {
        return None;
    }
    let base_ref = val.split('+').next()?.trim();
    if let Some(resolved) = palette.get(base_ref) {
        let base_resolved = resolved.split('+').next()?.trim();
        parse_hex_color(base_resolved)
    } else if let Some(linked_val) = fallback_map.get(base_ref) {
        resolve_color_recursive(linked_val, palette, fallback_map, depth + 1)
    } else {
        parse_hex_color(base_ref)
    }
}

fn parse_style(
    val: &str,
    palette: &HashMap<String, String>,
    fallback_map: &HashMap<String, String>,
) -> Option<Style> {
    let parts: Vec<&str> = val.split('+').collect();
    if parts.is_empty() {
        return None;
    }
    let color_ref = parts[0].trim();
    let color = resolve_color(color_ref, palette, fallback_map)?;

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
        color,
        bold,
        italic,
        underline,
        strikethrough,
    })
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

        // Verify resolved style values
        let fg_style = scheme.ui.get("foreground").unwrap();
        let bg_style = scheme.ui.get("background").unwrap();
        let caret_style = scheme.ui.get("caret").unwrap();
        let comment_style = scheme.syntax.get("comment").unwrap();
        let keyword_style = scheme.syntax.get("keyword").unwrap();
        let operator_style = scheme.syntax.get("operator").unwrap();
        let function_style = scheme.syntax.get("function").unwrap();

        // Verify resolved palette colors map
        let base_palette = scheme.colors.get("base").unwrap();
        assert_eq!(
            base_palette,
            &crossterm::style::Color::Rgb {
                r: 30,
                g: 30,
                b: 46
            }
        );

        assert_eq!(
            bg_style,
            &Style {
                color: crossterm::style::Color::Rgb {
                    r: 30,
                    g: 30,
                    b: 46
                },
                bold: false,
                italic: false,
                underline: false,
                strikethrough: false,
            }
        );
        assert_eq!(
            fg_style,
            &Style {
                color: crossterm::style::Color::Rgb {
                    r: 205,
                    g: 214,
                    b: 244
                },
                bold: false,
                italic: false,
                underline: false,
                strikethrough: false,
            }
        );
        assert_eq!(
            caret_style.color,
            crossterm::style::Color::Rgb {
                r: 245,
                g: 224,
                b: 220
            }
        );
        assert!(caret_style.bold);
        assert!(!caret_style.italic);

        assert_eq!(
            comment_style,
            &Style {
                color: crossterm::style::Color::Rgb {
                    r: 108,
                    g: 112,
                    b: 134
                },
                bold: false,
                italic: true,
                underline: false,
                strikethrough: false,
            }
        );
        assert_eq!(
            keyword_style,
            &Style {
                color: crossterm::style::Color::Rgb {
                    r: 203,
                    g: 166,
                    b: 247
                },
                bold: true,
                italic: true,
                underline: false,
                strikethrough: false,
            }
        );
        assert!(operator_style.underline);
        assert!(function_style.strikethrough);

        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn test_load_default() {
        let scheme = ColorScheme::load_default();
        assert_eq!(scheme.metadata.name, "catppuccin-mocha");
        assert!(scheme.ui.contains_key("background"));
        assert!(scheme.syntax.contains_key("keyword"));

        let catppuccin = ColorScheme::get_by_name("catppuccin").unwrap();
        assert_eq!(catppuccin.metadata.name, "catppuccin-mocha");

        let kanagawa = ColorScheme::get_by_name("kanagawa").unwrap();
        assert_eq!(kanagawa.metadata.name, "kanagawa");
    }
}

pub trait ToCrossTerm {
    fn rgb(&self) -> crossterm::style::Color;
}

impl ToCrossTerm for syntect::highlighting::Color {
    fn rgb(&self) -> crossterm::style::Color {
        crossterm::style::Color::Rgb {
            r: self.r,
            g: self.g,
            b: self.b,
        }
    }
}
