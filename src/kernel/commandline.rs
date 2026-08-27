use vim_buffer::Buffer;
use vim_ui::WindowState;

use super::{CommandLineKind, MutationOutcome, transaction};

/// Kernel-owned state for the interactive Ex/search command line.
#[derive(Debug, Clone)]
pub struct CommandLineState {
    kind: CommandLineKind,
    command_history: Vec<String>,
    search_history: Vec<String>,
    history_index: Option<usize>,
    history_temp: String,
}

impl Default for CommandLineState {
    fn default() -> Self {
        Self {
            kind: CommandLineKind::Ex,
            command_history: Vec::new(),
            search_history: Vec::new(),
            history_index: None,
            history_temp: String::new(),
        }
    }
}

impl CommandLineState {
    pub fn kind(&self) -> CommandLineKind {
        self.kind
    }

    pub fn prefix(&self) -> char {
        match self.kind {
            CommandLineKind::Ex => ':',
            CommandLineKind::SearchForward => '/',
            CommandLineKind::SearchBackward => '?',
        }
    }

    pub fn is_search(&self) -> bool {
        !matches!(self.kind, CommandLineKind::Ex)
    }

    pub fn enter(&mut self, kind: CommandLineKind) {
        self.kind = kind;
        self.history_index = None;
        self.history_temp.clear();
    }

    pub fn record(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        let history = if self.is_search() {
            &mut self.search_history
        } else {
            &mut self.command_history
        };
        if history.last().map(String::as_str) != Some(text) {
            history.push(text.to_owned());
        }
    }

    pub fn previous(&mut self, current_text: &str) -> Option<String> {
        let history = if self.is_search() {
            &self.search_history
        } else {
            &self.command_history
        };
        if history.is_empty() {
            return None;
        }
        let index = match self.history_index {
            None => {
                self.history_temp = current_text.to_owned();
                history.len() - 1
            }
            Some(index) => index.saturating_sub(1),
        };
        self.history_index = Some(index);
        Some(history[index].clone())
    }

    pub fn next(&mut self) -> Option<String> {
        let history = if self.is_search() {
            &self.search_history
        } else {
            &self.command_history
        };
        let index = self.history_index?;
        if index + 1 < history.len() {
            self.history_index = Some(index + 1);
            Some(history[index + 1].clone())
        } else {
            self.history_index = None;
            Some(self.history_temp.clone())
        }
    }
}

pub fn text(buffer: &text::Buffer) -> String {
    buffer
        .as_rope()
        .chunks_in_range(0..buffer.len())
        .collect::<String>()
        .replace('\n', "")
}

pub fn first_line(buffer: &text::Buffer) -> String {
    use text::ToOffset;

    if buffer.row_count() == 0 {
        return String::new();
    }
    let start = text::Point::new(0, 0).to_offset(buffer);
    let end = text::Point::new(0, buffer.line_len(0)).to_offset(buffer);
    buffer.as_rope().chunks_in_range(start..end).collect()
}

pub fn replace_text(
    buffer: &mut Buffer,
    window: &mut WindowState,
    text: &str,
) -> Result<MutationOutcome, String> {
    let range = vim_buffer::TextRange::new(
        vim_buffer::ByteOffset(0),
        vim_buffer::ByteOffset(buffer.as_text_buffer().len()),
    )
    .expect("a complete buffer range is ordered");
    let outcome = transaction(
        buffer,
        vim_buffer::EditOrigin::VimScript,
        None,
        |transaction| {
            transaction.replace(None, range, text);
        },
    )?;

    window.selections.selections.clear();
    window
        .selections
        .add(buffer.as_text_buffer(), buffer.as_text_buffer().len());
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn histories_are_distinct_and_restore_temporary_input() {
        let mut state = CommandLineState::default();
        state.record("write");
        assert_eq!(state.previous("wip"), Some("write".to_owned()));
        assert_eq!(state.next(), Some("wip".to_owned()));

        state.enter(CommandLineKind::SearchForward);
        state.record("needle");
        assert_eq!(state.previous("partial"), Some("needle".to_owned()));
        assert_eq!(state.next(), Some("partial".to_owned()));
    }
}
