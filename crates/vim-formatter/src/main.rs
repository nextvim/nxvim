use std::{borrow::Cow, collections::HashMap, env, error::Error, io};

use vim_formatter::{
    CompiledFormat, ExprId, FormatDialect, FormatResolver, RenderItem, StyleId, TablineTarget,
    parse,
};

const KANAGAWA: &str = include_str!("../kanagawa.toml");
const CATPPUCCIN: &str = include_str!("../catppuccin.toml");

struct EmbeddedTheme {
    key: &'static str,
    name: &'static str,
    source: &'static str,
}

const THEMES: &[EmbeddedTheme] = &[
    EmbeddedTheme {
        key: "kanagawa",
        name: "Kanagawa",
        source: KANAGAWA,
    },
    EmbeddedTheme {
        key: "catppuccin",
        name: "Catppuccin Mocha",
        source: CATPPUCCIN,
    },
];

#[derive(Clone, Copy)]
struct Rgb(u8, u8, u8);

#[derive(Clone, Copy)]
struct ColorPair {
    foreground: Rgb,
    background: Rgb,
}

struct DemoTheme {
    name: &'static str,
    statusline: ColorPair,
    statusline_nc: ColorPair,
    tabline: ColorPair,
    tabline_selected: ColorPair,
    tabline_fill: ColorPair,
    mode: ColorPair,
    file: ColorPair,
    modified: ColorPair,
    position: ColorPair,
    metadata: ColorPair,
    tab_close: ColorPair,
}

impl DemoTheme {
    fn from_embedded(theme: &EmbeddedTheme) -> Self {
        let mut section = "";
        let mut palette = HashMap::new();
        let mut ui = HashMap::new();

        for raw_line in theme.source.lines() {
            let line = raw_line.trim();
            if line.starts_with('[') && line.ends_with(']') {
                section = &line[1..line.len() - 1];
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim();
            let value = value.trim().trim_matches('"');
            match section {
                "colors" => {
                    if let Some(color) = parse_hex(value) {
                        palette.insert(key.to_owned(), color);
                    }
                }
                "ui" => {
                    ui.insert(key.to_owned(), value.to_owned());
                }
                _ => {}
            }
        }

        let color = |ui_name: &str| {
            let palette_name = ui.get(ui_name).expect("theme UI color must exist");
            *palette
                .get(palette_name)
                .expect("theme palette color must exist")
        };
        Self {
            name: theme.name,
            statusline: ColorPair {
                foreground: color("statusline_foreground"),
                background: color("statusline_background"),
            },
            statusline_nc: ColorPair {
                foreground: color("statusline_nc_foreground"),
                background: color("statusline_nc_background"),
            },
            tabline: ColorPair {
                foreground: color("tabline_foreground"),
                background: color("tabline_background"),
            },
            tabline_selected: ColorPair {
                foreground: color("tabline_sel_foreground"),
                background: color("tabline_sel_background"),
            },
            tabline_fill: ColorPair {
                foreground: color("tabline_foreground"),
                background: color("tabline_fill"),
            },
            mode: ColorPair {
                foreground: color("cursor_line_nr"),
                background: color("statusline_background"),
            },
            file: ColorPair {
                foreground: color("foreground"),
                background: color("statusline_background"),
            },
            modified: ColorPair {
                foreground: color("diagnostic_warn"),
                background: color("statusline_background"),
            },
            position: ColorPair {
                foreground: color("diagnostic_info"),
                background: color("statusline_background"),
            },
            metadata: ColorPair {
                foreground: color("special_key"),
                background: color("statusline_background"),
            },
            tab_close: ColorPair {
                foreground: color("diagnostic_error"),
                background: color("tabline_fill"),
            },
        }
    }

    fn style(&self, id: Option<StyleId>, dialect: FormatDialect) -> ColorPair {
        match id {
            Some(StyleId(1)) => self.statusline,
            Some(StyleId(2)) => self.statusline_nc,
            Some(StyleId(3)) => self.tabline,
            Some(StyleId(4)) => self.tabline_selected,
            Some(StyleId(5)) => self.tabline_fill,
            Some(StyleId(6)) => self.mode,
            Some(StyleId(7)) => self.file,
            Some(StyleId(8)) => self.modified,
            Some(StyleId(9)) => self.position,
            Some(StyleId(10)) => self.metadata,
            Some(StyleId(11)) => self.tab_close,
            _ if dialect == FormatDialect::TabLine => self.tabline_fill,
            _ => self.statusline,
        }
    }
}

struct DemoContext;

impl FormatResolver for DemoContext {
    fn file_name(&self) -> Cow<'_, str> {
        Cow::Borrowed("src/main.rs")
    }

    fn full_path(&self) -> Cow<'_, str> {
        Cow::Borrowed("/home/iceman/Developer/rust/vim-formatter/src/main.rs")
    }

    fn line(&self) -> usize {
        128
    }

    fn column(&self) -> usize {
        17
    }

    fn virtual_column(&self) -> usize {
        20
    }

    fn total_lines(&self) -> usize {
        512
    }

    fn buffer_number(&self) -> usize {
        3
    }

    fn is_modified(&self) -> bool {
        true
    }

    fn file_type(&self) -> Cow<'_, str> {
        Cow::Borrowed("rust")
    }

    fn encoding(&self) -> Cow<'_, str> {
        Cow::Borrowed("utf-8")
    }

    fn file_format(&self) -> Cow<'_, str> {
        Cow::Borrowed("unix")
    }

    fn current_character(&self) -> Option<char> {
        Some('λ')
    }

    fn tab_count(&self) -> usize {
        3
    }

    fn current_tab(&self) -> usize {
        2
    }

    fn resolve_highlight(&self, name: &str) -> Option<StyleId> {
        match name {
            "StatusLine" => Some(StyleId(1)),
            "StatusLineNC" => Some(StyleId(2)),
            "TabLine" => Some(StyleId(3)),
            "TabLineSel" => Some(StyleId(4)),
            "TabLineFill" => Some(StyleId(5)),
            "Mode" => Some(StyleId(6)),
            "File" => Some(StyleId(7)),
            "Modified" => Some(StyleId(8)),
            "Position" => Some(StyleId(9)),
            "Metadata" => Some(StyleId(10)),
            "TabClose" => Some(StyleId(11)),
            _ => None,
        }
    }

    fn eval_expression(&self, _id: ExprId, source: &str) -> Cow<'_, str> {
        match source {
            "mode()" => Cow::Borrowed("NORMAL"),
            "&spell ? '[SPELL]' : ''" => Cow::Borrowed("[SPELL]"),
            _ => Cow::Owned(format!("{{{source}}}")),
        }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let context = DemoContext;
    let selected = select_theme()?;
    let theme = DemoTheme::from_embedded(selected);

    println!("vim-formatter: statusline/tabline examples");
    println!("theme: {} (true-color ANSI rendering)\n", theme.name);
    print_theme_samples(&theme);

    show(
        "Classic statusline",
        "%#File# %f %#Modified#%m%#StatusLine#%= %#Metadata#%y%#StatusLine# | %#Position#%l:%c | %p%%%#StatusLine# ",
        FormatDialect::StatusLine,
        72,
        &context,
        &theme,
    )?;

    show(
        "Expression, metadata, and grouped filename",
        "%#Mode# %{mode()}%#File# %20(%t%#Modified#%m%#File#%)%#StatusLine#%=%#Metadata#%{&spell ? '[SPELL]' : ''} %e[%o] %#Position#%l/%L%#StatusLine# ",
        FormatDialect::StatusLine,
        88,
        &context,
        &theme,
    )?;

    show(
        "Long path with Vim's explicit truncation marker",
        "%#File# %<%F%#StatusLine#%= %#Metadata#%n:%y %#Position#%l:%c:%v %P%#StatusLine# ",
        FormatDialect::StatusLine,
        64,
        &context,
        &theme,
    )?;

    show(
        "Clickable tabline",
        "%#TabLine#%1T 1: README.md %2T%#TabLineSel# 2: main.rs %3T%#TabLine# 3: parser.rs %T%= %#TabClose#%X × ",
        FormatDialect::TabLine,
        78,
        &context,
        &theme,
    )?;

    Ok(())
}

fn show(
    title: &str,
    source: &str,
    dialect: FormatDialect,
    width: usize,
    context: &impl FormatResolver,
    theme: &DemoTheme,
) -> Result<(), Box<dyn Error>> {
    let ast = parse(source, dialect)?;
    let compiled = CompiledFormat::compile(&ast)?;
    let output = compiled.render(context, width)?;

    println!("--- {title} ---");
    println!("dialect: {dialect:?}, width: {width}");
    println!("format : {source}");
    println!("plain  : |{}|", plain_text(&output));
    println!("render : |{}\x1b[0m|", ansi_render(&output, theme, dialect));
    println!("items  : {}\n", annotated_items(&output));
    Ok(())
}

fn select_theme() -> Result<&'static EmbeddedTheme, Box<dyn Error>> {
    let requested = env::args()
        .nth(1)
        .unwrap_or_else(|| "kanagawa".to_owned())
        .to_lowercase();
    THEMES
        .iter()
        .find(|theme| theme.key == requested)
        .ok_or_else(|| {
            let choices = THEMES
                .iter()
                .map(|theme| theme.key)
                .collect::<Vec<_>>()
                .join(", ");
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("unknown theme {requested:?}; choose one of: {choices}"),
            )
            .into()
        })
}

fn parse_hex(value: &str) -> Option<Rgb> {
    let value = value.strip_prefix('#')?;
    if value.len() != 6 {
        return None;
    }
    Some(Rgb(
        u8::from_str_radix(&value[0..2], 16).ok()?,
        u8::from_str_radix(&value[2..4], 16).ok()?,
        u8::from_str_radix(&value[4..6], 16).ok()?,
    ))
}

fn ansi(text: &str, colors: ColorPair) -> String {
    let Rgb(fr, fg, fb) = colors.foreground;
    let Rgb(br, bg, bb) = colors.background;
    format!("\x1b[38;2;{fr};{fg};{fb};48;2;{br};{bg};{bb}m{text}")
}

fn ansi_render(items: &[RenderItem<'_>], theme: &DemoTheme, dialect: FormatDialect) -> String {
    let mut rendered = String::new();
    for item in items {
        if let RenderItem::Text { text, style } = item {
            rendered.push_str(&ansi(text, theme.style(*style, dialect)));
        }
    }
    rendered
}

fn print_theme_samples(theme: &DemoTheme) {
    println!("{} highlight samples:", theme.name);
    println!(
        "  base:    {} {} {} {} {}\x1b[0m",
        ansi(" StatusLine ", theme.statusline),
        ansi(" StatusLineNC ", theme.statusline_nc),
        ansi(" TabLine ", theme.tabline),
        ansi(" TabLineSel ", theme.tabline_selected),
        ansi(" TabLineFill ", theme.tabline_fill),
    );
    println!(
        "  accents: {} {} {} {} {} {}\x1b[0m\n",
        ansi(" Mode ", theme.mode),
        ansi(" File ", theme.file),
        ansi(" Modified ", theme.modified),
        ansi(" Position ", theme.position),
        ansi(" Metadata ", theme.metadata),
        ansi(" TabClose ", theme.tab_close),
    );
}

fn plain_text(items: &[RenderItem<'_>]) -> String {
    items
        .iter()
        .filter_map(|item| match item {
            RenderItem::Text { text, .. } => Some(text.as_ref()),
            _ => None,
        })
        .collect()
}

fn annotated_items(items: &[RenderItem<'_>]) -> String {
    items
        .iter()
        .map(|item| match item {
            RenderItem::Text { text, style } => match style {
                Some(style) => format!("[style:{} {:?}]", style.0, text.as_ref()),
                None => format!("[plain {:?}]", text.as_ref()),
            },
            RenderItem::ClickTarget { target } => match target {
                TablineTarget::Tab(tab) => format!("<click:tab:{tab}>"),
                TablineTarget::Reset => "<click:reset>".to_owned(),
                TablineTarget::Close(0) => "<click:close-current>".to_owned(),
                TablineTarget::Close(tab) => format!("<click:close:{tab}>"),
            },
            RenderItem::Align => "<unresolved-align>".to_owned(),
            RenderItem::Truncate => "<unresolved-truncate>".to_owned(),
            _ => "<unknown-render-item>".to_owned(),
        })
        .collect::<Vec<_>>()
        .join(" ")
}
