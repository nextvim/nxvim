use std::ops::Range;

use unicode_width::UnicodeWidthChar;

/// Half-open byte range in either the pattern or matched text.
pub type TextRange = Range<usize>;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum MagicMode {
    VeryMagic,
    #[default]
    Magic,
    NoMagic,
    VeryNoMagic,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CaseBehavior {
    /// Resolve case sensitivity from editor options and pattern contents.
    #[default]
    Automatic,
    Sensitive,
    Insensitive,
}

/// Editor options which affect compilation, but do not depend on a particular
/// cursor position or visual selection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditorOptions {
    pub magic: bool,
    pub ignore_case: bool,
    pub smart_case: bool,
    pub is_keyword: String,
    pub is_file_name: String,
    pub is_print: String,
}

impl Default for EditorOptions {
    fn default() -> Self {
        Self {
            magic: true,
            ignore_case: false,
            smart_case: false,
            is_keyword: "@,48-57,_,192-255".into(),
            is_file_name: "@,48-57,/,.,-,_,+,,,#,$,%,~,=".into(),
            is_print: "@,161-255".into(),
        }
    }
}

/// Buffer and editor state consulted by Vim-only runtime assertions.
///
/// All offsets are UTF-8 byte offsets. Implementations derive line, character,
/// and virtual columns from these offsets according to Vim semantics.
pub trait MatchContext {
    fn text(&self) -> &str;

    fn cursor_offset(&self) -> Option<usize> {
        None
    }

    fn visual_range(&self) -> Option<TextRange> {
        None
    }

    /// Returns a one-based line number and one-based byte column.
    fn line_and_byte_column(&self, byte_offset: usize) -> Option<(usize, usize)>;

    /// Returns Vim's one-based screen/virtual column at a byte offset.
    fn virtual_column(&self, byte_offset: usize) -> Option<usize>;

    /// Determines word boundaries for `\<` and `\>` assertions.
    fn is_keyword_character(&self, character: char) -> bool {
        character == '_' || character.is_alphanumeric()
    }
}

/// Default UTF-8 buffer context used by standalone matching.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BufferContext {
    text: String,
    cursor: Option<usize>,
    visual: Option<TextRange>,
    tab_stop: usize,
    ambiguous_width_is_double: bool,
}

impl BufferContext {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            cursor: None,
            visual: None,
            tab_stop: 8,
            ambiguous_width_is_double: false,
        }
    }

    pub fn with_cursor(mut self, byte_offset: usize) -> Self {
        self.cursor = self.valid_offset(byte_offset).then_some(byte_offset);
        self
    }

    pub fn with_visual_range(mut self, range: TextRange) -> Self {
        self.visual = (range.start <= range.end
            && self.valid_offset(range.start)
            && self.valid_offset(range.end))
        .then_some(range);
        self
    }

    pub fn with_tab_stop(mut self, tab_stop: usize) -> Self {
        self.tab_stop = tab_stop.max(1);
        self
    }

    pub fn with_ambiguous_width_is_double(mut self, enabled: bool) -> Self {
        self.ambiguous_width_is_double = enabled;
        self
    }

    fn valid_offset(&self, byte_offset: usize) -> bool {
        byte_offset <= self.text.len() && self.text.is_char_boundary(byte_offset)
    }
}

impl MatchContext for BufferContext {
    fn text(&self) -> &str {
        &self.text
    }

    fn cursor_offset(&self) -> Option<usize> {
        self.cursor
    }

    fn visual_range(&self) -> Option<TextRange> {
        self.visual.clone()
    }

    fn line_and_byte_column(&self, byte_offset: usize) -> Option<(usize, usize)> {
        if !self.valid_offset(byte_offset) {
            return None;
        }
        let before = &self.text[..byte_offset];
        let line = before.bytes().filter(|byte| *byte == b'\n').count() + 1;
        let line_start = before.rfind('\n').map_or(0, |offset| offset + 1);
        Some((line, byte_offset - line_start + 1))
    }

    fn virtual_column(&self, byte_offset: usize) -> Option<usize> {
        if !self.valid_offset(byte_offset) {
            return None;
        }
        let before = &self.text[..byte_offset];
        let line_start = before.rfind('\n').map_or(0, |offset| offset + 1);
        let mut column = 1;
        for character in self.text[line_start..byte_offset].chars() {
            if character == '\t' {
                column += self.tab_stop - ((column - 1) % self.tab_stop);
            } else {
                let width = if self.ambiguous_width_is_double {
                    character.width_cjk()
                } else {
                    character.width()
                };
                column += width.unwrap_or(0);
            }
        }
        Some(column)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_one_based_lines_and_byte_columns() {
        let context = BufferContext::new("αβ\ntext");
        assert_eq!(context.line_and_byte_column(2), Some((1, 3)));
        assert_eq!(context.line_and_byte_column(5), Some((2, 1)));
        assert_eq!(context.line_and_byte_column(6), Some((2, 2)));
        assert_eq!(context.line_and_byte_column(1), None);
    }

    #[test]
    fn computes_virtual_columns_with_tabs_and_unicode_width() {
        let context = BufferContext::new("a\t界x").with_tab_stop(4);
        assert_eq!(context.virtual_column(1), Some(2));
        assert_eq!(context.virtual_column(2), Some(5));
        assert_eq!(context.virtual_column(5), Some(7));
    }

    #[test]
    fn ignores_invalid_editor_offsets() {
        let context = BufferContext::new("é")
            .with_cursor(1)
            .with_visual_range(0..1);
        assert_eq!(context.cursor_offset(), None);
        assert_eq!(context.visual_range(), None);
    }
}
