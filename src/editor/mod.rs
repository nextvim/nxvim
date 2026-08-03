pub mod buffers;
pub mod display;
pub mod document;
pub mod selections;

use crate::controller::{self};
use crate::services::Services;
use crate::ui::colorscheme::ColorScheme;

use vim_input::{Action, Mode};

pub struct Editor {
    pub mode: Mode,

    // settings
    pub wrap: bool,
    pub syntax: bool,
    pub tree_sitter: bool,
    pub show_line_numbers: bool,
    pub fold: bool,
    pub fold_multiline_only: bool,
    pub textmate_highlights: bool,
    pub textmate_theme: Option<String>,
    pub treesitter_highlights: bool,
    pub map_scope_to_scheme: bool,

    // state
    pub should_redraw: bool,
    pub buffers_to_redraw: Vec<usize>,

    pub services: Services,

    pub last_action: Action,
    pub pending_keys: String,
    pub search_pattern: String,
    pub search_regex: Option<onig::Regex>,
}

impl Editor {
    pub fn set_tree_sitter_enabled(
        &mut self,
        ui: &mut crate::ui::Ui,
        buffers: &mut crate::editor::buffers::VimBuffers,
        enabled: bool,
    ) {
        self.tree_sitter = enabled;
        if !enabled {
            ui.cancel_document_parse_tasks();
            for id in buffers.list() {
                if let Some(entry) = buffers.entry_mut(id) {
                    entry.syntax_tree = None;
                }
            }
        }
    }

    /// Dispatches an action to the active document. Document behavior belongs
    /// to `VimDocument`; the editor only resolves the active window and buffer.
    pub fn apply_active_vim_action(
        &mut self,
        ui: &mut crate::ui::Ui,
        buffers: &mut crate::editor::buffers::VimBuffers,
        action: &Action,
    ) -> bool {
        let Some(active_win_id) = ui.focused_window_id() else {
            return false;
        };
        let Some(window) = ui.window_mut(active_win_id) else {
            return false;
        };
        let Some(document) = window.doc.as_mut() else {
            return false;
        };
        let Some(buffer_id) = vim_buffer::BufferId::new(document.id as u64) else {
            return false;
        };
        let Ok(buffer) = buffers.get_mut(buffer_id) else {
            return false;
        };

        if document.apply_action(buffer, action).is_err() {
            return false;
        }
        document.should_sync = true;
        self.buffers_to_redraw.push(document.id);
        self.mode = document.mode();
        true
    }

    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let services = Services::new();

        Ok(Self {
            mode: Mode::Normal,
            wrap: false,
            syntax: true,
            tree_sitter: true,
            show_line_numbers: true,
            fold: true,
            fold_multiline_only: false,
            should_redraw: true,
            buffers_to_redraw: Vec::new(),
            services,
            textmate_highlights: true,
            textmate_theme: None,
            treesitter_highlights: false,
            map_scope_to_scheme: true,
            last_action: Action::NoOp,
            pending_keys: String::new(),
            search_pattern: String::new(),
            search_regex: None,
        })
    }

    pub fn set_pattern(&mut self, pattern: String) {
        if self.search_pattern == pattern && self.search_regex.is_some() {
            return;
        }
        self.search_regex = if pattern.is_empty() {
            None
        } else {
            onig::Regex::new(&pattern).ok()
        };
        self.search_pattern = pattern;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::WindowId;

    #[test]
    fn vim_action_dispatch_handles_insert_undo_and_redo() {
        let mut editor = Editor::new().unwrap();
        let mut ui = crate::ui::Ui::new();
        let mut buffers = crate::editor::buffers::VimBuffers::new();
        let buffer_id = buffers.create("one");
        assert!(ui.set_vim_window_buffer(WindowId::MainWindow as usize, buffer_id, &buffers,));

        editor.apply_active_vim_action(&mut ui, &mut buffers, &Action::SetToInsert);
        editor.apply_active_vim_action(&mut ui, &mut buffers, &Action::InsertText("X".into()));
        assert_eq!(
            buffers
                .get(buffer_id)
                .unwrap()
                .snapshot()
                .chunks()
                .collect::<String>(),
            "Xone"
        );

        editor.apply_active_vim_action(&mut ui, &mut buffers, &Action::Undo { count: 1 });
        assert_eq!(
            buffers
                .get(buffer_id)
                .unwrap()
                .snapshot()
                .chunks()
                .collect::<String>(),
            "one"
        );

        editor.apply_active_vim_action(&mut ui, &mut buffers, &Action::Redo { count: 1 });
        assert_eq!(
            buffers
                .get(buffer_id)
                .unwrap()
                .snapshot()
                .chunks()
                .collect::<String>(),
            "Xone"
        );
    }
}
