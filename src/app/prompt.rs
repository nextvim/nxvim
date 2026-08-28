//! Application-owned command-line prompt.

#[derive(Clone, Debug, Default)]
pub struct CommandPrompt {
    text: String,
}

impl CommandPrompt {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn push(&mut self, ch: char) {
        self.text.push(ch);
    }

    pub fn backspace(&mut self) -> bool {
        self.text.pop().is_some()
    }

    pub fn clear(&mut self) {
        self.text.clear();
    }

    pub fn take(&mut self) -> String {
        std::mem::take(&mut self.text)
    }
}
