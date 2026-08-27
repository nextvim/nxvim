use vim_ui::WindowState;

use super::{CommandOutcome, EditorContext};

/// Kernel-owned search and substitution-preview state.
pub struct SearchState {
    pattern: Option<String>,
    regex: Option<vim_regex::Regex>,
    range: Option<vim_script::ast::CommandRange>,
    substitute_text: Option<String>,
}

impl Default for SearchState {
    fn default() -> Self {
        Self {
            pattern: None,
            regex: None,
            range: None,
            substitute_text: None,
        }
    }
}

impl SearchState {
    pub fn pattern(&self) -> Option<&str> {
        self.pattern.as_deref()
    }

    pub fn regex(&self) -> Option<&vim_regex::Regex> {
        self.regex.as_ref()
    }

    pub fn range(&self) -> Option<&vim_script::ast::CommandRange> {
        self.range.as_ref()
    }

    pub fn substitute_text(&self) -> Option<&str> {
        self.substitute_text.as_deref()
    }

    pub fn set_pattern(&mut self, pattern: impl Into<String>) {
        let pattern = pattern.into();
        self.regex = vim_regex::Regex::compile(&pattern, vim_regex::CompileOptions::default()).ok();
        self.pattern = Some(pattern);
        self.range = None;
        self.substitute_text = None;
    }

    pub fn clear(&mut self) {
        self.pattern = None;
        self.regex = None;
        self.range = None;
        self.substitute_text = None;
    }

    pub fn set_substitution(
        &mut self,
        pattern: impl Into<String>,
        range: Option<vim_script::ast::CommandRange>,
        replacement: impl Into<String>,
    ) {
        let pattern = pattern.into();
        self.regex = vim_regex::Regex::compile(&pattern, vim_regex::CompileOptions::default()).ok();
        self.pattern = Some(pattern);
        self.range = range;
        self.substitute_text = Some(replacement.into());
    }
}

pub fn move_cursor(
    context: EditorContext,
    pattern: &str,
    forward: bool,
    buffer: &text::Buffer,
    window: &mut WindowState,
) -> CommandOutcome {
    window.selections.search = pattern.to_owned();
    window.selections.regex = vim_buffer::compile(&pattern).map(std::sync::Arc::new);
    if forward {
        window.selections.move_to_next_match(&pattern, true, buffer);
    } else {
        window
            .selections
            .move_to_previous_match(&pattern, true, buffer);
    }
    CommandOutcome::cursor_moved(context.window)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setting_and_clearing_search_state_is_atomic() {
        let mut state = SearchState::default();
        state.set_substitution("needle", None, "replacement");
        assert_eq!(state.pattern(), Some("needle"));
        assert_eq!(state.substitute_text(), Some("replacement"));
        state.set_pattern("other");
        assert_eq!(state.pattern(), Some("other"));
        assert_eq!(state.substitute_text(), None);
        state.clear();
        assert_eq!(state.pattern(), None);
        assert!(state.regex().is_none());
    }
}
