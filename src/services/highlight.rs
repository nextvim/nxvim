use clock::Global;
use rope::Point;
use std::sync::OnceLock;
use std::{collections::HashMap, path::Path};
use syntect::{
    easy::ScopeRangeIterator,
    highlighting::{HighlightState, Highlighter},
    parsing::{ParseState, ScopeStack, SyntaxReference, SyntaxSet},
};
use text::{BufferSnapshot, ToOffset, ToPoint};
use vim_ui::colorscheme::{self, Style};

const ENABLE_STATE_CACHE: bool = true;
const CACHE_INTERVAL: u32 = 32;
const START_OFFSET: u32 = 1024;

fn find_entry<T>(state_cache: &HashMap<usize, T>, target: usize) -> Option<(&usize, &T)> {
    let mut nearest_key = None;
    let mut min_diff = usize::MAX;

    for key in state_cache.keys() {
        if *key == target {
            return Some((key, state_cache.get(key).unwrap()));
        } else if *key > target && (*key - target) < min_diff {
            nearest_key = Some(key);
            min_diff = *key - target;
        }
    }

    nearest_key.map(|key| (key, state_cache.get(key).unwrap()))
}

#[derive(Clone)]
pub struct StateCache {
    pub line_number: u32,
    pub parser_state: ParseState,
    pub highlight_state: Option<syntect::highlighting::HighlightState>,
    pub scope_stack: Option<ScopeStack>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StyledSpan {
    pub style: Style,
    pub start: u32,
    pub end: u32,
}

#[derive(Clone, Debug)]
pub struct StyleCache {
    pub styles: Vec<StyledSpan>,
}

pub struct Highlights {
    syntax_set: SyntaxSet,
    syntax: SyntaxReference,
    state_cache: HashMap<usize, StateCache>,
    pub textmate_style_cache: HashMap<u32, StyleCache>,
    pub treesitter_style_cache: HashMap<u32, Vec<StyledSpan>>,
    style_cache: HashMap<u32, StyleCache>,
    highlight_start: u32,
    pub last_snapshot_version: Option<Global>,
}

fn row_text(buffer: &BufferSnapshot, row: u32) -> String {
    let start = Point::new(row, 0).to_offset(buffer);
    let end = Point::new(row, buffer.line_len(row)).to_offset(buffer);
    buffer.as_rope().chunks_in_range(start..end).collect()
}

const SCOPE_MAPPINGS: &[(&str, &str)] = &[
    // Invalid / Errors
    ("invalid.deprecated", "error"),
    ("invalid.illegal", "error"),
    ("invalid", "error"),
    // Comments
    ("punctuation.definition.comment", "comment"),
    ("comment.block.documentation", "comment"),
    ("comment.block", "comment"),
    ("comment.line", "comment"),
    ("comment", "comment"),
    // Regex & Strings
    ("string.regexp.escaped", "special"),
    ("string.regexp.escape", "special"),
    ("string.regexp", "special"),
    ("punctuation.definition.string", "string"),
    ("string.quoted.double", "string"),
    ("string.quoted.single", "string"),
    ("string.quoted", "string"),
    ("string.interpolated", "special"),
    ("string", "string"),
    // Booleans, Numbers, Constants
    ("constant.language.boolean", "boolean"),
    ("constant.language.null", "constant"),
    ("constant.language", "constant"),
    ("constant.numeric.float", "float"),
    ("constant.numeric", "number"),
    ("constant.character.escape", "special"),
    ("constant.character", "character"),
    ("constant.other.symbol", "special"),
    ("constant.other.key", "special"),
    ("constant.other", "constant"),
    ("constant", "constant"),
    // Functions & Constructors
    ("entity.name.function.constructor", "constructor"),
    ("meta.constructor", "constructor"),
    ("entity.name.function.method", "function"),
    ("entity.name.function", "function"),
    ("support.function", "function"),
    ("variable.function", "function"),
    // Properties & Members
    ("variable.other.member", "property"),
    ("variable.other.property", "property"),
    ("meta.object-key", "property"),
    ("support.type.property-name", "property"),
    // Variables & Parameters
    ("variable.parameter", "variable"),
    ("variable.language.this", "special"),
    ("variable.language.self", "special"),
    ("variable.language", "special"),
    ("variable.other.constant", "constant"),
    ("variable.other.readwrite", "variable"),
    ("variable.other", "variable"),
    ("variable", "variable"),
    ("parameter", "variable"),
    ("support.variable", "variable"),
    // Namespaces & Modules
    ("entity.name.namespace", "module"),
    ("entity.name.module", "module"),
    ("support.other.namespace", "module"),
    ("namespace", "module"),
    ("meta.path", "module"),
    ("meta.block", "module"),
    // Types & Classes
    ("entity.other.inherited-class", "type"),
    ("entity.name.class", "type"),
    ("entity.name.type", "type"),
    ("support.type", "type"),
    ("support.class", "type"),
    ("storage.type", "type"),
    ("storage.modifier", "keyword"),
    ("storage", "keyword"),
    // Keywords & Operators
    ("keyword.operator.logical", "operator"),
    ("keyword.operator.comparison", "operator"),
    ("keyword.operator.assignment", "operator"),
    ("keyword.operator", "operator"),
    ("keyword.control.flow", "keyword"),
    ("keyword.control.import", "keyword"),
    ("keyword.control", "keyword"),
    ("keyword.declaration", "keyword"),
    ("keyword.other", "keyword"),
    ("keyword", "keyword"),
    // HTML / XML / Tag Mappings
    ("entity.name.tag", "tag"),
    ("entity.other.attribute-name", "tag_attribute"),
    ("meta.tag", "tag"),
    ("punctuation.definition.tag", "tag_delimiter"),
    // Punctuation, Brackets, Delimiters
    ("punctuation.definition.parameters", "delimiter"),
    ("punctuation.definition.arguments", "delimiter"),
    ("punctuation.section.embedded", "special"),
    ("punctuation.terminator", "delimiter"),
    ("punctuation.separator", "delimiter"),
    ("meta.brace", "delimiter"),
    ("punctuation", "delimiter"),
    // Markup (Markdown etc)
    ("markup.heading", "heading"),
    ("markup.underline.link", "link"),
    ("markup.bold", "special"),
    ("markup.italic", "special"),
    ("markup.list", "special"),
    ("markup.quote", "comment"),
    ("markup", "special"),
];

fn map_node_kind_to_syntax_key(kind: &str) -> Option<&'static str> {
    if kind.contains("comment") {
        Some("comment")
    } else if kind.contains("keyword")
        || kind == "use_declaration"
        || kind == "let_declaration"
        || kind == "const_declaration"
        || kind == "type_declaration"
        || kind == "struct"
        || kind == "enum"
        || kind == "union"
        || kind == "fn"
        || kind == "impl"
        || kind == "trait"
        || kind == "mod"
        || kind == "as"
        || kind == "where"
        || kind == "pub"
        || kind == "use"
        || kind == "unsafe"
        || kind == "extern"
        || kind == "return"
        || kind == "if"
        || kind == "else"
        || kind == "match"
        || kind == "while"
        || kind == "loop"
        || kind == "for"
        || kind == "in"
        || kind == "break"
        || kind == "continue"
    {
        Some("keyword")
    } else if kind.contains("string") || kind == "char_literal" || kind == "character" {
        Some("string")
    } else if kind.contains("function")
        || kind == "call_expression"
        || kind == "field_identifier"
        || kind == "method_declaration"
        || kind == "function_item"
        || kind == "function_signature_item"
    {
        Some("function")
    } else if kind.contains("type")
        || kind == "struct_item"
        || kind == "enum_item"
        || kind == "type_identifier"
        || kind == "primitive_type"
        || kind == "generic_type"
    {
        Some("type")
    } else if kind.contains("integer")
        || kind.contains("float")
        || kind == "number"
        || kind == "integer_literal"
        || kind == "float_literal"
    {
        Some("number")
    } else if kind.contains("boolean")
        || kind == "boolean_literal"
        || kind == "true"
        || kind == "false"
    {
        Some("boolean")
    } else if kind == "const" || kind == "constant" || kind == "static" {
        Some("constant")
    } else if kind.contains("operator")
        || kind == "binary_expression"
        || kind == "unary_expression"
        || kind == "assignment_expression"
        || kind == "compound_assignment_expr"
        || kind == "="
        || kind == "=="
        || kind == "!="
        || kind == "<"
        || kind == ">"
        || kind == "<="
        || kind == ">="
        || kind == "+"
        || kind == "-"
        || kind == "*"
        || kind == "/"
        || kind == "%"
        || kind == "&"
        || kind == "|"
        || kind == "^"
        || kind == "!"
        || kind == "&&"
        || kind == "||"
        || kind == "<<"
        || kind == ">>"
    {
        Some("operator")
    } else if kind == "{"
        || kind == "}"
        || kind == "["
        || kind == "]"
        || kind == "("
        || kind == ")"
        || kind == ","
        || kind == ";"
        || kind == "."
        || kind == ":"
        || kind == "::"
        || kind == "->"
        || kind == "=>"
    {
        Some("delimiter")
    } else {
        None
    }
}

fn override_row_style(
    styles: &mut Vec<StyledSpan>,
    target_start: u32,
    target_end: u32,
    override_style: Style,
) {
    let mut new_styles = Vec::new();
    for span in styles.drain(..) {
        if span.end <= target_start || span.start >= target_end {
            new_styles.push(span);
        } else {
            if span.start < target_start {
                new_styles.push(StyledSpan {
                    style: span.style.clone(),
                    start: span.start,
                    end: target_start,
                });
            }
            let overlap_start = span.start.max(target_start);
            let overlap_end = span.end.min(target_end);
            new_styles.push(StyledSpan {
                style: override_style.clone(),
                start: overlap_start,
                end: overlap_end,
            });
            if span.end > target_end {
                new_styles.push(StyledSpan {
                    style: span.style,
                    start: target_end,
                    end: span.end,
                });
            }
        }
    }
    *styles = new_styles;
}

fn get_catppuccin_theme() -> &'static syntect::highlighting::Theme {
    static THEME: OnceLock<syntect::highlighting::Theme> = OnceLock::new();
    THEME.get_or_init(|| {
        let theme_bytes = include_bytes!("catppuccin-mocha.tmTheme");
        let mut cursor = std::io::Cursor::new(theme_bytes);
        syntect::highlighting::ThemeSet::load_from_reader(&mut cursor).unwrap_or_else(|_| {
            let theme_set = syntect::highlighting::ThemeSet::load_defaults();
            theme_set.themes.values().next().cloned().unwrap()
        })
    })
}

impl Highlights {
    pub fn clear(&mut self) {
        self.state_cache.clear();
        self.textmate_style_cache.clear();
        self.treesitter_style_cache.clear();
        self.style_cache.clear();
        self.highlight_start = 0;
        self.last_snapshot_version = None;
    }

    pub fn new(file_path: &str) -> Self {
        let extension = Path::new(file_path)
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_lowercase())
            .unwrap_or_default();

        let syntax_set = SyntaxSet::load_defaults_newlines();

        let syntax = syntax_set
            .find_syntax_by_extension(&extension)
            .unwrap_or_else(|| syntax_set.find_syntax_plain_text());

        Self {
            syntax_set: syntax_set.clone(),
            syntax: syntax.clone(),
            state_cache: HashMap::new(),
            textmate_style_cache: HashMap::new(),
            treesitter_style_cache: HashMap::new(),
            style_cache: HashMap::new(),
            highlight_start: 0,
            last_snapshot_version: None,
        }
    }

    pub fn syntax_set(&self) -> &SyntaxSet {
        &self.syntax_set
    }

    pub fn syntax(&self) -> &SyntaxReference {
        &self.syntax
    }

    pub fn is_sync(&self, buffer: &BufferSnapshot) -> bool {
        self.last_snapshot_version.as_ref() == Some(&buffer.version)
    }

    pub fn highlight_lines(
        &mut self,
        buffer: &BufferSnapshot,
        start_row: u32,
        row_count: u32,
        colorscheme: &colorscheme::ColorScheme,
        syntax_tree: Option<&crate::services::treesitter::tree_sitter::SyntaxTree>,
        textmate_highlights: bool,
        treesitter_highlights: bool,
        map_scope_to_scheme: bool,
    ) {
        self.last_snapshot_version = Some(buffer.version.clone());
        self.textmate_style_cache.clear();
        self.treesitter_style_cache.clear();
        let mut cached_state: Option<StateCache> = None;

        if row_count == 0 || start_row >= buffer.row_count() {
            return;
        }

        let mut start: u32 = if textmate_highlights {
            start_row.saturating_sub(START_OFFSET)
        } else {
            start_row
        };

        if ENABLE_STATE_CACHE {
            if let Some((_key, value)) = find_entry::<StateCache>(
                &self.state_cache,
                start_row.saturating_sub(CACHE_INTERVAL) as usize,
            ) {
                let ln: u32 = value.line_number as u32;
                if ln > start && ln < start_row {
                    start = ln;
                    self.highlight_start = ln;
                    cached_state = Some(value.clone());
                }
            }
        }

        let end_row = std::cmp::min(buffer.row_count(), start_row.saturating_add(row_count));

        let mut parser = match cached_state {
            Some(ref state) => state.parser_state.clone(),
            None => ParseState::new(&self.syntax),
        };
        let mut stack = match cached_state {
            Some(ref state) => state.scope_stack.clone().unwrap_or_else(ScopeStack::new),
            None => ScopeStack::new(),
        };

        let theme = get_catppuccin_theme();
        let highlighter = Highlighter::new(theme);
        let mut highlighter_state = match cached_state {
            Some(ref state) => state
                .highlight_state
                .clone()
                .unwrap_or_else(|| HighlightState::new(&highlighter, ScopeStack::new())),
            None => HighlightState::new(&highlighter, ScopeStack::new()),
        };

        for row in start..end_row {
            let text = row_text(buffer, row) + "\n";
            let mut styles = Vec::new();

            if textmate_highlights {
                let parsed = parser
                    .parse_line(&text, &self.syntax_set)
                    .expect("syntax parsing failed");
                let ops = parsed.ops;

                if map_scope_to_scheme {
                    // let mut column = 0_u32;
                    // for (range, op) in ScopeRangeIterator::new(&ops, &text) {
                    //     let _ = stack.apply(&op);
                    //     let start_column = column;
                    //     let len = range.end - range.start;
                    //     column += len as u32;
                    //     let style = map_scope_to_style(stack.as_slice(), colorscheme);
                    //     styles.push(StyledSpan {
                    //         style,
                    //         start: start_column,
                    //         end: column,
                    //     });
                    // }
                } else {
                    let mut column = 0_u32;
                    let highlight_iter = syntect::highlighting::HighlightIterator::new(
                        &mut highlighter_state,
                        &ops,
                        &text,
                        &highlighter,
                    );
                    for (syntect_style, token_text) in highlight_iter {
                        let start_column = column;
                        column += token_text.len() as u32;
                        // let style = convert_to_style(syntect_style);
                        // styles.push(StyledSpan {
                        //     style,
                        //     start: start_column,
                        //     end: column,
                        // });
                    }
                    for op in &ops {
                        let _ = stack.apply(&op.1);
                    }
                }
            } else {
                // let resolved_style = map_scope_to_style(&[], colorscheme);
                // styles.push(StyledSpan {
                //     style: resolved_style,
                //     start: 0,
                //     end: text.len() as u32,
                // });
            }

            if row >= start_row {
                self.textmate_style_cache.insert(row, StyleCache { styles });
            }

            if ENABLE_STATE_CACHE && row % CACHE_INTERVAL == 0 {
                self.state_cache.insert(
                    row as usize,
                    StateCache {
                        line_number: row,
                        parser_state: parser.clone(),
                        highlight_state: Some(highlighter_state.clone()),
                        scope_stack: Some(stack.clone()),
                    },
                );
            }
        }

        self.rebuild_merged_cache();
    }

    pub fn name(&self) -> String {
        self.syntax.name.clone()
    }

    pub fn render_row(&self, row: u32) -> Option<&StyleCache> {
        self.style_cache.get(&row)
    }

    pub fn contains_rows(&self, start_row: u32, end_row: u32) -> bool {
        (start_row..end_row).all(|row| self.style_cache.contains_key(&row))
    }

    pub fn invalidate_state(&mut self, start_row: u32) {
        self.state_cache.retain(|&row, _| row < start_row as usize);
        self.textmate_style_cache.retain(|&row, _| row < start_row);
        self.rebuild_merged_cache();
    }

    pub fn get_style_cache(&self) -> &HashMap<u32, StyleCache> {
        &self.style_cache
    }

    pub fn get_state_cache(&self) -> &HashMap<usize, StateCache> {
        &self.state_cache
    }

    pub fn merge_caches(
        &mut self,
        style_cache: HashMap<u32, StyleCache>,
        state_cache: HashMap<usize, StateCache>,
    ) {
        self.textmate_style_cache.extend(style_cache);
        self.state_cache.extend(state_cache);
        self.rebuild_merged_cache();
    }

    pub fn rebuild_merged_cache(&mut self) {
        self.style_cache.clear();
        for (&row, base_cache) in &self.textmate_style_cache {
            let mut styles = base_cache.styles.clone();
            if let Some(overrides) = self.treesitter_style_cache.get(&row) {
                for span in overrides {
                    override_row_style(&mut styles, span.start, span.end, span.style.clone());
                }
            }
            self.style_cache.insert(row, StyleCache { styles });
        }
    }
}
