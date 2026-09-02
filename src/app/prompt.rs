//! Application-owned, single-line command-line buffer.

use vim_buffer::{Buffer, BufferId, ByteOffset, EditOrigin, TextRange};
pub struct CommandPrompt {
    buffer: Buffer,
    text: String,
    cursor: usize,
    anchor: usize,
}

impl Default for CommandPrompt {
    fn default() -> Self {
        Self {
            buffer: Buffer::new(
                BufferId::new(u64::MAX).expect("command-line buffer id is non-zero"),
                clock::ReplicaId::LOCAL,
                "",
            ),
            text: String::new(),
            cursor: 0,
            anchor: 0,
        }
    }
}

impl CommandPrompt {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn push(&mut self, ch: char) {
        self.insert(&ch.to_string());
    }

    pub fn insert(&mut self, value: &str) {
        let (start, end) = self.selection();
        self.replace(start, end, value);
        self.cursor = start + value.len();
        self.anchor = self.cursor;
    }

    pub fn backspace(&mut self) -> bool {
        let (start, end) = self.selection();
        if start != end {
            self.replace(start, end, "");
            self.cursor = start;
        } else if let Some(previous) = self.text[..self.cursor]
            .char_indices()
            .next_back()
            .map(|(i, _)| i)
        {
            self.replace(previous, self.cursor, "");
            self.cursor = previous;
        } else {
            return false;
        }
        self.anchor = self.cursor;
        true
    }

    pub fn delete(&mut self) -> bool {
        let (start, end) = self.selection();
        let end = if start != end {
            end
        } else if let Some(ch) = self.text[self.cursor..].chars().next() {
            self.cursor + ch.len_utf8()
        } else {
            return false;
        };
        self.replace(start, end, "");
        self.cursor = start;
        self.anchor = start;
        true
    }

    pub fn move_left(&mut self, select: bool) {
        if !select && self.cursor != self.anchor {
            self.cursor = self.selection().0;
        } else if let Some(previous) = self.text[..self.cursor]
            .char_indices()
            .next_back()
            .map(|(i, _)| i)
        {
            self.cursor = previous;
        }
        if !select {
            self.anchor = self.cursor;
        }
    }

    pub fn move_right(&mut self, select: bool) {
        if !select && self.cursor != self.anchor {
            self.cursor = self.selection().1;
        } else if let Some(ch) = self.text[self.cursor..].chars().next() {
            self.cursor += ch.len_utf8();
        }
        if !select {
            self.anchor = self.cursor;
        }
    }

    pub fn move_home(&mut self, select: bool) {
        self.cursor = 0;
        if !select {
            self.anchor = 0;
        }
    }
    pub fn move_end(&mut self, select: bool) {
        self.cursor = self.text.len();
        if !select {
            self.anchor = self.cursor;
        }
    }

    pub fn clear(&mut self) {
        self.set_text(String::new());
    }
    pub fn take(&mut self) -> String {
        let text = self.text.clone();
        self.clear();
        text
    }

    pub fn set_text(&mut self, text: String) {
        let len = self.text.len();
        self.replace(0, len, &text);
        self.cursor = text.len();
        self.anchor = self.cursor;
    }

    fn selection(&self) -> (usize, usize) {
        (self.cursor.min(self.anchor), self.cursor.max(self.anchor))
    }

    fn replace(&mut self, start: usize, end: usize, replacement: &str) {
        debug_assert!(
            start <= end && self.text.is_char_boundary(start) && self.text.is_char_boundary(end)
        );
        let range =
            TextRange::new(ByteOffset(start), ByteOffset(end)).expect("valid command-line range");
        let mut transaction = self.buffer.transaction(EditOrigin::InsertMode);
        transaction.replace(None, range, replacement);
        transaction
            .commit(None)
            .expect("command-line edit must be valid");
        self.text.replace_range(start..end, replacement);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edits_at_cursor_and_replaces_selection() {
        let mut prompt = CommandPrompt::new();
        prompt.set_text("aéz".into());
        prompt.move_left(false);
        prompt.backspace();
        assert_eq!(prompt.text(), "az");
        prompt.move_home(false);
        prompt.move_right(true);
        prompt.push('x');
        assert_eq!(prompt.text(), "xz");
    }
}
