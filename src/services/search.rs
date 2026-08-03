use onig::Regex;

pub fn compile(pattern: &str) -> Option<Regex> {
    match Regex::new(pattern) {
        Ok(regex) => Some(regex),
        Err(err) => {
            eprintln!("Regex compile error: {}", err);
            None
        }
    }
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
        while offset <= self.len() {
            if let Some(caps) = regex.captures(&self[offset..]) {
                if let Some((start, end)) = caps.pos(0) {
                    let abs_start = offset + start;
                    let abs_end = offset + end;
                    let len = abs_end - abs_start;
                    let slice = &self[abs_start..abs_end];
                    out.push((abs_start, len, slice));
                    if abs_end == offset {
                        if let Some(ch) = self[offset..].chars().next() {
                            offset += ch.len_utf8();
                        } else {
                            break;
                        }
                    } else {
                        offset = abs_end;
                    }
                } else {
                    break;
                }
            } else {
                break;
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
            .find(|(start, len, _)| *start <= position && position < *start + *len)
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

pub struct Search {}
impl Search {
    pub fn new() -> Self {
        Self {}
    }
}
