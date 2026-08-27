//! Semantic editor state.
//!
//! This façade exposes semantic buffer operations without depending on terminal
//! input, rendering, UI layout, controller handlers, or service implementations.
//! Buffer ownership lives in `crate::kernel::EditorState`; window state
//! (viewport, display map, selections) lives on `vim_ui::Window` — see
//! `crate::app::windows::WindowOps` for the compatibility operations that
//! combine the two stores during migration.

mod buffer_state;
mod buffers;

use vim_buffer::BufferId;

use vim_regex::Regex;

pub use buffer_state::BufferState;
pub use buffers::Buffers;

pub struct EditorModel {
    kernel: crate::kernel::EditorState,
    initial_buffer: BufferId,
    pub status: Option<String>,
    pub commandline_buffer: BufferId,

    pub search_pattern: Option<String>,
    pub search_regex: Option<Regex>,
    pub search_range: Option<vim_script::ast::CommandRange>,
    pub substitute_text: Option<String>,
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
        let kernel = crate::kernel::EditorState::new(buffers);

        Self {
            kernel,
            initial_buffer,
            status: None,
            commandline_buffer,

            search_pattern: None,
            search_regex: None,
            search_range: None,
            substitute_text: None,
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
        self.kernel.buffers()
    }

    pub fn buffers_mut(&mut self) -> &mut Buffers {
        self.kernel.buffers_mut()
    }

    pub fn kernel(&self) -> &crate::kernel::EditorState {
        &self.kernel
    }

    pub fn kernel_mut(&mut self) -> &mut crate::kernel::EditorState {
        &mut self.kernel
    }

    pub fn create(&mut self, initial_text: impl Into<String>) -> BufferId {
        self.kernel.create_buffer(initial_text)
    }

    pub fn open_path(&mut self, path: impl AsRef<std::path::Path>) -> BufferId {
        self.kernel.open_buffer(path)
    }

    pub fn wipe(
        &mut self,
        id: BufferId,
        force: bool,
    ) -> Result<vim_buffer::ManagerOutcome, vim_buffer::BufferError> {
        self.kernel.wipe_buffer(id, force)
    }

    pub fn save(
        &mut self,
        id: BufferId,
        path: Option<&std::path::Path>,
        force: bool,
    ) -> Result<vim_buffer::SaveOutcome, vim_buffer::BufferError> {
        self.kernel.save_buffer(id, path, force)
    }

    pub fn edit_buffer_with_state<R>(
        &mut self,
        id: BufferId,
        edit: impl FnOnce(&mut vim_buffer::Buffer, &mut BufferState) -> R,
    ) -> Result<R, vim_buffer::BufferError> {
        self.kernel.edit_buffer_with_state(id, edit)
    }

    pub fn complete_background_save(
        &mut self,
        id: BufferId,
        path: &std::path::Path,
    ) -> Result<(), vim_buffer::BufferError> {
        self.kernel.complete_background_save(id, path)
    }

    pub fn get_buffer(&self, id: BufferId) -> Result<&vim_buffer::Buffer, vim_buffer::BufferError> {
        self.kernel.buffers().get(id)
    }

    pub fn get_buffer_mut(
        &mut self,
        id: BufferId,
    ) -> Result<&mut vim_buffer::Buffer, vim_buffer::BufferError> {
        self.kernel.buffers_mut().get_mut(id)
    }

    /// Buffers that may be presented and selected as editor tabs.
    pub fn list(&self) -> Vec<BufferId> {
        self.kernel.buffers().listed()
    }

    pub fn buffer_state(&self, id: BufferId) -> Option<&BufferState> {
        self.kernel.buffers().state(id)
    }

    pub fn buffer_state_mut(&mut self, id: BufferId) -> Option<&mut BufferState> {
        self.get_buffer(id).ok()?;
        Some(self.kernel.buffers_mut().state_mut(id))
    }

    pub fn invalidate_all_highlights(&mut self) {
        for state in self.kernel.buffers_mut().states.values_mut() {
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
