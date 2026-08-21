//! Semantic editor state.
//!
//! This layer owns buffers and must not depend on terminal input, rendering,
//! UI layout, controller handlers, or service implementations. Window state
//! (viewport, display map, selections) lives on `vim_ui::Window` — see
//! `crate::app::windows::WindowOps` for the operations that combine it with
//! buffer storage.

mod buffer_state;
mod buffers;

use vim_buffer::BufferId;
use vim_regex::Regex;

pub use buffer_state::BufferState;
pub use buffers::Buffers;

pub struct EditorModel {
    buffers: Buffers,
    initial_buffer: BufferId,
    pub(crate) status: Option<String>,
    pub commandline_buffer: BufferId,
    pub commandline_mode: char,
    pub search_pattern: Option<String>,
    pub search_regex: Option<Regex>,
    pub command_history: Vec<String>,
    pub search_history: Vec<String>,
    pub history_index: Option<usize>,
    pub history_temp: String,
}

impl EditorModel {
    pub fn new(paths: Vec<std::path::PathBuf>) -> Self {
        let mut buffers = Buffers::new();
        let initial_buffer = buffers.open_paths(paths);
        let (commandline_buffer, _) = buffers
            .create_named("#commandline", "")
            .expect("Failed to create #commandline buffer");
        buffers
            .set_listed(commandline_buffer, false)
            .expect("command-line buffer must exist");

        Self {
            buffers,
            initial_buffer,
            status: None,
            commandline_buffer,
            commandline_mode: ':',
            search_pattern: None,
            search_regex: None,
            command_history: Vec::new(),
            search_history: Vec::new(),
            history_index: None,
            history_temp: String::new(),
        }
    }

    /// The buffer opened (or created) for the main window at startup.
    pub fn initial_buffer(&self) -> BufferId {
        self.initial_buffer
    }

    pub fn commandline_buffer(&self) -> BufferId {
        self.commandline_buffer
    }

    pub fn buffers(&self) -> &Buffers {
        &self.buffers
    }

    pub fn buffers_mut(&mut self) -> &mut Buffers {
        &mut self.buffers
    }

    pub fn create(&mut self, initial_text: impl Into<String>) -> BufferId {
        self.buffers.create(initial_text)
    }

    pub fn open_path(&mut self, path: impl AsRef<std::path::Path>) -> BufferId {
        self.buffers.open_path(path)
    }

    pub fn wipe(
        &mut self,
        id: BufferId,
        force: bool,
    ) -> Result<vim_buffer::ManagerOutcome, vim_buffer::BufferError> {
        self.buffers.wipe(id, force)
    }

    pub fn get_buffer(&self, id: BufferId) -> Result<&vim_buffer::Buffer, vim_buffer::BufferError> {
        self.buffers.get(id)
    }

    pub fn get_buffer_mut(
        &mut self,
        id: BufferId,
    ) -> Result<&mut vim_buffer::Buffer, vim_buffer::BufferError> {
        self.buffers.get_mut(id)
    }

    /// Buffers that may be presented and selected as editor tabs.
    pub fn list(&self) -> Vec<BufferId> {
        self.buffers.listed()
    }

    pub fn buffer_state(&self, id: BufferId) -> Option<&BufferState> {
        self.buffers.state(id)
    }

    pub fn buffer_state_mut(&mut self, id: BufferId) -> Option<&mut BufferState> {
        self.get_buffer(id).ok()?;
        Some(self.buffers.state_mut(id))
    }

    pub fn invalidate_all_highlights(&mut self) {
        for state in self.buffers.states.values_mut() {
            state.highlights.invalidate();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commandline_buffer_is_created_and_unlisted() {
        let model = EditorModel::new(Vec::new());

        assert_eq!(model.list().len(), 1);
        assert!(!model.list().contains(&model.commandline_buffer()));
        assert!(model.get_buffer(model.commandline_buffer()).is_ok());
        assert!(model.get_buffer(model.initial_buffer()).is_ok());
    }

    #[test]
    fn named_buffers_are_editable_tabs_but_commandline_is_not() {
        let base = std::env::temp_dir().join(format!("nxvim-tabs-{}", std::process::id()));
        let first = base.with_extension("first-missing");
        let second = base.with_extension("second-missing");
        let model = EditorModel::new(vec![first, second]);

        assert_eq!(model.list().len(), 2);
        assert!(!model.list().contains(&model.commandline_buffer()));
    }
}
