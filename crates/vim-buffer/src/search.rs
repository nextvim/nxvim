// use onig::Regex;
use vim_regex::{CompileOptions, Regex};

pub fn compile(pattern: &str) -> Option<Regex> {
    // Regex::new(pattern).ok()
    Regex::compile(pattern, CompileOptions::default()).ok()
}

pub trait TextSearch {
    fn find_string(&self, text: &str) -> Vec<(usize, usize, &str)>;
    fn find_pattern(&self, regex: &Regex) -> Vec<(usize, usize, &str)>;
    fn find_words(&self) -> Vec<(usize, usize, &str)>;
    fn find_word(&self, position: usize) -> Option<(usize, usize, &str)>;
    fn find_next_word(&self, position: usize) -> Option<(usize, usize, &str)>;
    fn find_previous_word(&self, position: usize) -> Option<(usize, usize, &str)>;
    fn find_next_word_end(&self, position: usize) -> Option<(usize, usize, &str)>;
    fn find_previous_word_end(&self, position: usize) -> Option<(usize, usize, &str)>;
    fn find_big_words(&self) -> Vec<(usize, usize, &str)>;
    fn find_next_big_word(&self, position: usize) -> Option<(usize, usize, &str)>;
    fn find_previous_big_word(&self, position: usize) -> Option<(usize, usize, &str)>;
    fn find_next_big_word_end(&self, position: usize) -> Option<(usize, usize, &str)>;
    fn find_previous_big_word_end(&self, position: usize) -> Option<(usize, usize, &str)>;
    fn find_next_match(&self, search: &str, position: usize) -> Option<(usize, usize, &str)>;
    fn find_previous_match(&self, search: &str, position: usize) -> Option<(usize, usize, &str)>;
    fn find_next_pattern_match(
        &self,
        regex: &Regex,
        position: usize,
    ) -> Option<(usize, usize, &str)>;
    fn find_previous_pattern_match(
        &self,
        regex: &Regex,
        position: usize,
    ) -> Option<(usize, usize, &str)>;
    fn find_next_match_end(&self, search: &str, position: usize) -> Option<(usize, usize, &str)>;
    fn find_previous_match_end(
        &self,
        search: &str,
        position: usize,
    ) -> Option<(usize, usize, &str)>;
    fn find_next_pattern_match_end(
        &self,
        regex: &Regex,
        position: usize,
    ) -> Option<(usize, usize, &str)>;
    fn find_previous_pattern_match_end(
        &self,
        regex: &Regex,
        position: usize,
    ) -> Option<(usize, usize, &str)>;
}

impl TextSearch for str {
    fn find_string(&self, text: &str) -> Vec<(usize, usize, &str)> {
        let mut matches = Vec::new();
        if text.is_empty() {
            return matches;
        }
        let mut start = 0;
        while let Some(pos) = self[start..].find(text) {
            let abs_start = start + pos;
            let len = text.len();
            let slice = &self[abs_start..abs_start + len];
            matches.push((abs_start, len, slice));
            // Allow overlapping matches: advance by 1 byte
            start = abs_start + 1;
            if start >= self.len() {
                break;
            }
        }
        matches
    }

    fn find_pattern(&self, regex: &Regex) -> Vec<(usize, usize, &str)> {
        let mut out = Vec::new();
        let mut offset = 0;

        while offset < self.len() {
            let Ok(Some(found)) = regex.find(&self[offset..]) else {
                break;
            };

            let (start, end) = (found.range.start, found.range.end);

            let abs_start = offset + start;
            let abs_end = offset + end;

            out.push((abs_start, abs_end - abs_start, &self[abs_start..abs_end]));

            if abs_end == offset {
                let Some(ch) = self[offset..].chars().next() else {
                    break;
                };
                offset += ch.len_utf8();
            } else {
                offset = abs_end;
            }
        }

        out
    }

    fn find_words(&self) -> Vec<(usize, usize, &str)> {
        let mut words = Vec::new();
        let mut current_start = None;
        let mut in_alphanumeric = false;

        for (idx, ch) in self.char_indices() {
            if ch.is_whitespace() {
                if let Some(start) = current_start {
                    words.push((start, idx, &self[start..idx]));
                    current_start = None;
                }
            } else {
                let ch_is_alphanumeric = ch.is_alphanumeric() || ch == '_';
                if let Some(start) = current_start {
                    if ch_is_alphanumeric != in_alphanumeric {
                        words.push((start, idx, &self[start..idx]));
                        current_start = Some(idx);
                        in_alphanumeric = ch_is_alphanumeric;
                    }
                } else {
                    current_start = Some(idx);
                    in_alphanumeric = ch_is_alphanumeric;
                }
            }
        }

        if let Some(start) = current_start {
            words.push((start, self.len(), &self[start..]));
        }

        words
    }

    fn find_word(&self, position: usize) -> Option<(usize, usize, &str)> {
        self.find_words()
            .into_iter()
            .find(|(start, end, _)| *start <= position && position < *end)
    }

    fn find_next_word(&self, position: usize) -> Option<(usize, usize, &str)> {
        self.find_words()
            .into_iter()
            .find(|(start, _, _)| *start > position)
    }

    fn find_previous_word(&self, position: usize) -> Option<(usize, usize, &str)> {
        self.find_words()
            .into_iter()
            .rev()
            .find(|(start, _, _)| *start < position)
    }

    fn find_next_word_end(&self, position: usize) -> Option<(usize, usize, &str)> {
        self.find_words()
            .into_iter()
            .find(|(_, end, _)| (*end - 1) > position)
    }

    fn find_previous_word_end(&self, position: usize) -> Option<(usize, usize, &str)> {
        self.find_words()
            .into_iter()
            .rev()
            .find(|(_, end, _)| (*end - 1) < position)
    }

    fn find_next_match(&self, search: &str, position: usize) -> Option<(usize, usize, &str)> {
        self.find_string(search)
            .into_iter()
            .find(|(start, _, _)| *start >= position)
    }

    fn find_previous_match(&self, search: &str, position: usize) -> Option<(usize, usize, &str)> {
        self.find_string(search)
            .into_iter()
            .rev()
            .find(|(start, _, _)| *start < position)
    }

    fn find_next_match_end(&self, search: &str, position: usize) -> Option<(usize, usize, &str)> {
        self.find_string(search)
            .into_iter()
            .find(|(_, end, _)| (*end - 1) > position)
    }

    fn find_previous_match_end(
        &self,
        search: &str,
        position: usize,
    ) -> Option<(usize, usize, &str)> {
        self.find_string(search)
            .into_iter()
            .rev()
            .find(|(_, end, _)| (*end - 1) < position)
    }

    fn find_next_pattern_match(
        &self,
        search: &Regex,
        position: usize,
    ) -> Option<(usize, usize, &str)> {
        self.find_pattern(search)
            .into_iter()
            .find(|(start, _, _)| *start >= position)
    }

    fn find_previous_pattern_match(
        &self,
        search: &Regex,
        position: usize,
    ) -> Option<(usize, usize, &str)> {
        self.find_pattern(search)
            .into_iter()
            .rev()
            .find(|(start, _, _)| *start < position)
    }

    fn find_next_pattern_match_end(
        &self,
        search: &Regex,
        position: usize,
    ) -> Option<(usize, usize, &str)> {
        self.find_pattern(search)
            .into_iter()
            .find(|(_, end, _)| (*end - 1) > position)
    }

    fn find_previous_pattern_match_end(
        &self,
        search: &Regex,
        position: usize,
    ) -> Option<(usize, usize, &str)> {
        self.find_pattern(search)
            .into_iter()
            .rev()
            .find(|(_, end, _)| (*end - 1) < position)
    }

    fn find_big_words(&self) -> Vec<(usize, usize, &str)> {
        let mut words = Vec::new();
        let mut current_start = None;

        for (idx, ch) in self.char_indices() {
            if ch.is_whitespace() {
                if let Some(start) = current_start {
                    words.push((start, idx, &self[start..idx]));
                    current_start = None;
                }
            } else {
                if current_start.is_none() {
                    current_start = Some(idx);
                }
            }
        }

        if let Some(start) = current_start {
            words.push((start, self.len(), &self[start..]));
        }

        words
    }

    fn find_next_big_word(&self, position: usize) -> Option<(usize, usize, &str)> {
        self.find_big_words()
            .into_iter()
            .find(|(start, _, _)| *start > position)
    }

    fn find_previous_big_word(&self, position: usize) -> Option<(usize, usize, &str)> {
        self.find_big_words()
            .into_iter()
            .rev()
            .find(|(start, _, _)| *start < position)
    }

    fn find_next_big_word_end(&self, position: usize) -> Option<(usize, usize, &str)> {
        self.find_big_words()
            .into_iter()
            .find(|(_, end, _)| (*end - 1) > position)
    }

    fn find_previous_big_word_end(&self, position: usize) -> Option<(usize, usize, &str)> {
        self.find_big_words()
            .into_iter()
            .rev()
            .find(|(_, end, _)| (*end - 1) < position)
    }
}

#[allow(dead_code)]
pub struct Search {}
impl Search {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_next_match_returns_first_match_at_or_after_position() {
        let text = "foo bar foo baz foo";
        // matches start at 0, 8, 16

        // Position of a match itself is inclusive.
        assert_eq!(text.find_next_match("foo", 0), Some((0, 3, "foo")));
        assert_eq!(text.find_next_match("foo", 8), Some((8, 3, "foo")));
        assert_eq!(text.find_next_match("foo", 16), Some((16, 3, "foo")));

        // Positions in between find the following match.
        assert_eq!(text.find_next_match("foo", 1), Some((8, 3, "foo")));
        assert_eq!(text.find_next_match("foo", 9), Some((16, 3, "foo")));

        // Nothing left after the last match.
        assert_eq!(text.find_next_match("foo", 17), None);
    }

    #[test]
    fn find_next_match_returns_none_when_absent() {
        assert_eq!("abc".find_next_match("z", 0), None);
    }

    #[test]
    fn find_next_match_allows_overlapping_matches() {
        let text = "aaaa";
        // "aa" overlaps itself at starts 0, 1, 2.
        assert_eq!(text.find_next_match("aa", 0), Some((0, 2, "aa")));
        assert_eq!(text.find_next_match("aa", 1), Some((1, 2, "aa")));
        assert_eq!(text.find_next_match("aa", 2), Some((2, 2, "aa")));
        assert_eq!(text.find_next_match("aa", 3), None);
    }

    #[test]
    fn find_previous_match_returns_closest_match_strictly_before_position() {
        let text = "foo bar foo baz foo";
        // matches start at 0, 8, 16 (text.len() == 19)

        assert_eq!(text.find_previous_match("foo", 19), Some((16, 3, "foo")));

        // Sitting exactly on a match's start does not return that match.
        assert_eq!(text.find_previous_match("foo", 16), Some((8, 3, "foo")));
        assert_eq!(text.find_previous_match("foo", 9), Some((8, 3, "foo")));
        assert_eq!(text.find_previous_match("foo", 8), Some((0, 3, "foo")));

        // Nothing before the first match.
        assert_eq!(text.find_previous_match("foo", 0), None);
    }

    #[test]
    fn find_next_and_previous_match_agree_with_find_string() {
        let text = "the quick brown fox jumps over the lazy dog the end";
        let matches = text.find_string("the");
        assert_eq!(matches.len(), 3);

        assert_eq!(text.find_next_match("the", 0), Some(matches[0]));
        assert_eq!(
            text.find_previous_match("the", text.len()),
            Some(matches[2])
        );
    }

    #[test]
    fn find_next_pattern_match_returns_first_match_at_or_after_position() {
        let text = "foo1 bar foo22 baz foo333";
        let re = compile(r"foo\d+").unwrap();
        // matches: (0, 4, "foo1"), (9, 5, "foo22"), (19, 6, "foo333")

        assert_eq!(text.find_next_pattern_match(&re, 0), Some((0, 4, "foo1")));
        assert_eq!(text.find_next_pattern_match(&re, 1), Some((9, 5, "foo22")));
        assert_eq!(text.find_next_pattern_match(&re, 9), Some((9, 5, "foo22")));
        assert_eq!(text.find_next_pattern_match(&re, 25), None);
    }

    #[test]
    fn find_previous_pattern_match_returns_closest_match_strictly_before_position() {
        let text = "foo1 bar foo22 baz foo333";
        let re = compile(r"foo\d+").unwrap();
        // matches: (0, 4, "foo1"), (9, 5, "foo22"), (19, 6, "foo333")

        assert_eq!(
            text.find_previous_pattern_match(&re, text.len()),
            Some((19, 6, "foo333"))
        );
        assert_eq!(
            text.find_previous_pattern_match(&re, 19),
            Some((9, 5, "foo22"))
        );
        assert_eq!(
            text.find_previous_pattern_match(&re, 9),
            Some((0, 4, "foo1"))
        );
        assert_eq!(text.find_previous_pattern_match(&re, 0), None);
    }

    #[test]
    fn test_find_words_vim_definition() {
        let text = "} )( typeof window !== \"undefined\" ? window : this, function( window, noGlobal ) {";
        let words = text.find_words();
        let word_slices: Vec<&str> = words.iter().map(|(_, _, slice)| *slice).collect();
        let expected = vec![
            "}",
            ")(",
            "typeof",
            "window",
            "!==",
            "\"",
            "undefined",
            "\"",
            "?",
            "window",
            ":",
            "this",
            ",",
            "function",
            "(",
            "window",
            ",",
            "noGlobal",
            ")",
            "{",
        ];
        assert_eq!(word_slices, expected);
    }
}
