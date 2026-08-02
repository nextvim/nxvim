use crate::{
    compiler::{CompileError, Diagnostic, DiagnosticKind, Phase},
    context::{CaseBehavior, EditorOptions},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HighCharPolicy {
    None,
    Alphabetic,
    KeywordWord,
    All,
}

/// A compiled Vim character option such as `iskeyword` or `isfname`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OptionCharSet {
    bytes: [bool; 256],
    high_char_policy: HighCharPolicy,
}

impl OptionCharSet {
    pub fn parse(value: &str, high_char_policy: HighCharPolicy) -> Result<Self, CompileError> {
        OptionSetParser::new(value, high_char_policy).parse()
    }

    pub fn keyword(value: &str) -> Result<Self, CompileError> {
        Self::parse(value, HighCharPolicy::KeywordWord)
    }

    pub fn file_name(value: &str) -> Result<Self, CompileError> {
        let mut set = Self::parse(value, HighCharPolicy::All)?;
        // Vim includes U+00A0..U+00FF for UTF-8 regardless of `isfname`.
        set.bytes[0xa0..=0xff].fill(true);
        Ok(set)
    }

    pub fn printable(value: &str) -> Result<Self, CompileError> {
        let mut set = Self::parse(value, HighCharPolicy::None)?;
        // Vim always displays printable ASCII directly.
        set.bytes[0x20..=0x7e].fill(true);
        Ok(set)
    }

    pub fn contains(&self, character: char) -> bool {
        let codepoint = character as u32;
        if codepoint <= 255 {
            return self.bytes[codepoint as usize];
        }
        match self.high_char_policy {
            HighCharPolicy::None => false,
            HighCharPolicy::Alphabetic => character.is_alphabetic(),
            // Vim also includes emoji according to its own Unicode tables. Rust
            // does not expose an emoji general category, so that extension is
            // retained as a known lowering task rather than guessed here.
            HighCharPolicy::KeywordWord => character.is_alphanumeric(),
            HighCharPolicy::All => true,
        }
    }

    pub fn byte_ranges(&self) -> Vec<(u8, u8)> {
        let mut ranges = Vec::new();
        let mut start = None;
        for index in 0..=256 {
            let included = index < 256 && self.bytes[index];
            match (start, included) {
                (None, true) => start = Some(index),
                (Some(range_start), false) => {
                    ranges.push((range_start as u8, (index - 1) as u8));
                    start = None;
                }
                _ => {}
            }
        }
        ranges
    }
}

/// Resolve Vim's `ignorecase`/`smartcase` behavior after pattern-level `\c`
/// and `\C` switches and AST literal analysis have been performed.
pub fn resolve_case(
    options: &EditorOptions,
    pattern_override: Option<CaseBehavior>,
    has_uppercase_literal: bool,
) -> CaseBehavior {
    if let Some(case_behavior) = pattern_override {
        return case_behavior;
    }
    if !options.ignore_case || (options.smart_case && has_uppercase_literal) {
        CaseBehavior::Sensitive
    } else {
        CaseBehavior::Insensitive
    }
}

struct OptionSetParser<'a> {
    source: &'a str,
    offset: usize,
    set: OptionCharSet,
}

impl<'a> OptionSetParser<'a> {
    fn new(source: &'a str, high_char_policy: HighCharPolicy) -> Self {
        Self {
            source,
            offset: 0,
            set: OptionCharSet {
                bytes: [false; 256],
                high_char_policy,
            },
        }
    }

    fn parse(mut self) -> Result<OptionCharSet, CompileError> {
        while self.offset < self.source.len() {
            let part_start = self.offset;
            let exclude = self.peek() == Some('^') && self.peek_after_current().is_some();
            if exclude {
                self.bump();
            }
            let first = self.parse_character_number(part_start)?;

            let mut last = first;
            let has_range =
                self.peek() == Some('-') && self.peek_after_current().is_some_and(|c| c != ',');
            if has_range {
                self.bump();
                last = self.parse_character_number(part_start)?;
            }

            let included = !exclude;
            if !has_range && first == CharacterNumber::Alphabetic {
                for byte in 0_u8..=255 {
                    if char::from(byte).is_alphabetic() {
                        self.set.bytes[usize::from(byte)] = included;
                    }
                }
            } else {
                let first = first.literal_value();
                let last = last.literal_value();
                if first > last {
                    return Err(self.error(part_start, self.offset, "option range is reversed"));
                }
                self.set.bytes[usize::from(first)..=usize::from(last)].fill(included);
            }

            if self.offset < self.source.len() && !self.consume(',') {
                return Err(self.error(
                    self.offset,
                    self.next_offset(),
                    "expected a comma between option parts",
                ));
            }
        }
        Ok(self.set)
    }

    fn parse_character_number(
        &mut self,
        part_start: usize,
    ) -> Result<CharacterNumber, CompileError> {
        let Some(character) = self.peek() else {
            return Err(self.error(
                part_start,
                self.offset,
                "missing character after option prefix",
            ));
        };

        if character.is_ascii_digit() {
            let start = self.offset;
            while self.peek().is_some_and(|next| next.is_ascii_digit()) {
                self.bump();
            }
            let number = self.source[start..self.offset]
                .parse::<u16>()
                .map_err(|_| self.error(start, self.offset, "invalid character number"))?;
            if number > 255 {
                return Err(self.error(
                    start,
                    self.offset,
                    "character number must be between 0 and 255",
                ));
            }
            return Ok(CharacterNumber::Byte(number as u8));
        }

        self.bump();
        if character == '@' {
            Ok(CharacterNumber::Alphabetic)
        } else if character as u32 <= 255 {
            Ok(CharacterNumber::Byte(character as u8))
        } else {
            Err(self.error(
                part_start,
                self.offset,
                "option characters must be in the range 0..=255",
            ))
        }
    }

    fn peek(&self) -> Option<char> {
        self.source[self.offset..].chars().next()
    }

    fn peek_after_current(&self) -> Option<char> {
        let mut characters = self.source[self.offset..].chars();
        characters.next()?;
        characters.next()
    }

    fn next_offset(&self) -> usize {
        self.offset + self.peek().map_or(0, char::len_utf8)
    }

    fn bump(&mut self) {
        self.offset = self.next_offset();
    }

    fn consume(&mut self, expected: char) -> bool {
        if self.peek() == Some(expected) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn error(&self, start: usize, end: usize, message: impl Into<String>) -> CompileError {
        CompileError {
            diagnostics: vec![Diagnostic {
                kind: DiagnosticKind::InvalidSyntax,
                phase: Phase::Lower,
                span: start..end,
                message: message.into(),
                help: Some("see :help 'isfname' for Vim's character-option format".into()),
            }],
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum CharacterNumber {
    Byte(u8),
    Alphabetic,
}

impl CharacterNumber {
    fn literal_value(self) -> u8 {
        match self {
            Self::Byte(value) => value,
            Self::Alphabetic => b'@',
        }
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn parses_ranges_literals_at_and_literal_comma() {
        let set = OptionCharSet::keyword("@,48-57,_,,,@-@").unwrap();
        for character in ['a', 'Z', '5', '_', ',', '@'] {
            assert!(set.contains(character), "missing {character:?}");
        }
        assert!(!set.contains('-'));
    }

    #[test]
    fn applies_exclusions_from_left_to_right() {
        let set = OptionCharSet::keyword("@,^a-z,#,^").unwrap();
        assert!(set.contains('A'));
        assert!(!set.contains('a'));
        assert!(set.contains('#'));
        assert!(set.contains('^'));
    }

    #[test]
    fn creates_compact_byte_ranges() {
        let set = OptionCharSet::parse("48-57,A-C", HighCharPolicy::None).unwrap();
        assert_eq!(set.byte_ranges(), vec![(48, 57), (65, 67)]);
    }

    #[test]
    fn rejects_out_of_range_and_reversed_values() {
        assert!(OptionCharSet::keyword("256").is_err());
        assert!(OptionCharSet::keyword("90-65").is_err());
    }

    #[test]
    fn resolves_ignorecase_smartcase_and_pattern_overrides() {
        let mut options = EditorOptions {
            ignore_case: true,
            smart_case: true,
            ..EditorOptions::default()
        };
        assert_eq!(
            resolve_case(&options, None, false),
            CaseBehavior::Insensitive
        );
        assert_eq!(resolve_case(&options, None, true), CaseBehavior::Sensitive);
        assert_eq!(
            resolve_case(&options, Some(CaseBehavior::Insensitive), true),
            CaseBehavior::Insensitive
        );
        options.ignore_case = false;
        assert_eq!(resolve_case(&options, None, false), CaseBehavior::Sensitive);
    }

    proptest! {
        #[test]
        fn arbitrary_option_values_never_panic(value in ".{0,256}") {
            if let Ok(set) = OptionCharSet::keyword(&value) {
                for (start, end) in set.byte_ranges() {
                    prop_assert!(start <= end);
                }
            }
        }
    }
}
