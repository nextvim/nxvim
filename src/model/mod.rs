//! Semantic editor state.
//!
//! This layer owns buffers and windows and must not depend on terminal input,
//! rendering, UI layout, controller handlers, or service implementations.

mod buffer_state;
mod buffers;
mod window_state;
mod windows;

use std::path::PathBuf;
use vim_buffer::BufferId;
use vim_ui::WindowId;

pub use buffer_state::BufferState;
pub use buffers::Buffers;
pub use window_state::WindowState;
pub use windows::Windows;

pub struct EditorModel {
    buffers: Buffers,
    windows: Windows,
    pub(crate) status: Option<String>,
    commandline_buffer: BufferId,
}

impl EditorModel {
    pub fn new(paths: Vec<PathBuf>, main_window: WindowId, commandline_window: WindowId) -> Self {
        let mut buffers = Buffers::new();
        let first_buffer = buffers.open_paths(paths);
        let (commandline_buffer, _) = buffers
            .create_named("#commandline", "")
            .expect("Failed to create #commandline buffer");
        buffers
            .set_listed(commandline_buffer, false)
            .expect("command-line buffer must exist");

        let mut windows = Windows::new(main_window);
        windows.register_placeholder(
            main_window,
            buffers
                .get(first_buffer)
                .expect("initial editor buffer must exist"),
        );
        windows.register_placeholder(
            commandline_window,
            buffers
                .get(commandline_buffer)
                .expect("command-line buffer must exist"),
        );

        Self {
            buffers,
            windows,
            status: None,
            commandline_buffer,
        }
    }

    pub fn commandline_buffer(&self) -> BufferId {
        self.commandline_buffer
    }

    pub fn create(&mut self, initial_text: impl Into<String>) -> BufferId {
        self.buffers.create(initial_text)
    }

    pub fn save_window(
        &mut self,
        window_id: WindowId,
        path: Option<&std::path::Path>,
        force: bool,
    ) -> Result<vim_buffer::SaveOutcome, vim_buffer::BufferError> {
        let buffer_id =
            self.windows
                .buffer_id(window_id)
                .ok_or(vim_buffer::BufferError::NotImplemented(
                    "saving an unregistered window",
                ))?;
        self.buffers.save(buffer_id, path, force)
    }

    pub fn wipe(
        &mut self,
        id: BufferId,
        force: bool,
    ) -> Result<vim_buffer::ManagerOutcome, vim_buffer::BufferError> {
        let result = self.buffers.wipe(id, force);
        if result.is_ok() {
            self.cleanup_windows(id);
        }
        result
    }

    pub fn get_buffer(&self, id: BufferId) -> Result<&vim_buffer::Buffer, vim_buffer::BufferError> {
        self.buffers.get(id)
    }

    /// Buffers that may be presented and selected as editor tabs.
    pub fn list(&self) -> Vec<BufferId> {
        self.editable_buffers()
    }

    pub fn buffer_state(&self, id: BufferId) -> Option<&BufferState> {
        self.buffers.state(id)
    }

    pub fn buffer_state_mut(&mut self, id: BufferId) -> Option<&mut BufferState> {
        self.get_buffer(id).ok()?;
        Some(self.buffers.state_mut(id))
    }

    pub fn window_buffer(&self, window_id: WindowId) -> Option<BufferId> {
        self.windows.buffer_id(window_id)
    }

    pub fn window_state(&self, window_id: WindowId) -> Option<&WindowState> {
        self.windows.state(window_id)
    }

    pub fn window_state_mut(&mut self, window_id: WindowId) -> Option<&mut WindowState> {
        self.windows.state_mut(window_id)
    }

    pub fn window_buffers(&self) -> impl Iterator<Item = (WindowId, BufferId)> + '_ {
        self.windows
            .iter()
            .map(|(window_id, state)| (window_id, state.buffer_id))
    }

    pub fn focused_window(&self) -> WindowId {
        self.windows.focused()
    }

    pub fn previous_window(&self) -> Option<WindowId> {
        self.windows.previous()
    }

    pub fn focus_window(&mut self, window_id: WindowId) -> bool {
        self.windows.focus(window_id)
    }

    pub fn switch_next_buffer(&mut self, window_id: WindowId) -> bool {
        let listed = self.editable_buffers();
        self.windows
            .switch_next_buffer(window_id, &listed, &self.buffers)
    }

    pub fn switch_previous_buffer(&mut self, window_id: WindowId) -> bool {
        let listed = self.editable_buffers();
        self.windows
            .switch_previous_buffer(window_id, &listed, &self.buffers)
    }

    pub fn split_window(&mut self, source: WindowId, new_id: WindowId) -> bool {
        let Some(buffer_id) = self.windows.buffer_id(source) else {
            return false;
        };
        let Ok(buffer) = self.buffers.get(buffer_id) else {
            return false;
        };
        self.windows.split_from(source, new_id, buffer)
    }

    pub fn remove_window(&mut self, window_id: WindowId) -> bool {
        self.windows.remove(window_id).is_some()
    }

    pub fn edit_window<R>(
        &mut self,
        window_id: WindowId,
        edit: impl FnOnce(&mut vim_buffer::Buffer, &mut BufferState, &mut WindowState) -> R,
    ) -> Result<R, vim_buffer::BufferError> {
        let Some(buffer_id) = self.windows.buffer_id(window_id) else {
            return Err(vim_buffer::BufferError::NotImplemented(
                "editing an unregistered window",
            ));
        };
        self.buffers.state_mut(buffer_id);

        let Buffers { inner, states } = &mut self.buffers;
        let buffer = inner.get_mut(buffer_id)?;
        let state = states
            .get_mut(&buffer_id)
            .expect("buffer state was initialized");
        state.revision = state.revision.wrapping_add(1);
        let window = self
            .windows
            .state_mut(window_id)
            .expect("window buffer came from registered window");
        Ok(edit(buffer, state, window))
    }

    pub fn validate(&self) -> Result<(), String> {
        self.windows.validate(&self.buffers)
    }

    fn cleanup_windows(&mut self, removed: BufferId) {
        let fallback_id = self
            .editable_buffers()
            .into_iter()
            .find(|&id| id != removed);
        let fallback = fallback_id.and_then(|id| self.buffers.get(id).ok());
        self.windows.remove_buffer(removed, fallback);
    }

    fn editable_buffers(&self) -> Vec<BufferId> {
        self.buffers.listed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commandline_buffer_and_window_are_registered_once() {
        let main = WindowId::new(10);
        let commandline = WindowId::new(11);
        let model = EditorModel::new(Vec::new(), main, commandline);

        assert_eq!(
            model.window_buffer(commandline),
            Some(model.commandline_buffer())
        );
        assert_eq!(model.list().len(), 1);
        assert!(!model.list().contains(&model.commandline_buffer()));
        assert!(model.get_buffer(model.commandline_buffer()).is_ok());
        assert!(model.validate().is_ok());
    }

    #[test]
    fn named_buffers_are_editable_tabs_but_commandline_is_not() {
        let main = WindowId::new(10);
        let commandline = WindowId::new(11);
        let base = std::env::temp_dir().join(format!("nxvim-tabs-{}", std::process::id()));
        let first = base.with_extension("first-missing");
        let second = base.with_extension("second-missing");
        let model = EditorModel::new(vec![first, second], main, commandline);

        assert_eq!(model.list().len(), 2);
        assert!(!model.list().contains(&model.commandline_buffer()));
    }

    #[test]
    fn buffer_switching_skips_commandline_buffer() {
        let main = WindowId::new(10);
        let commandline = WindowId::new(11);
        let mut model = EditorModel::new(Vec::new(), main, commandline);
        let first = model.window_buffer(main).unwrap();
        let second = model.create("second");

        assert!(model.switch_next_buffer(main));
        assert_eq!(model.window_buffer(main), Some(second));
        assert!(model.switch_next_buffer(main));
        assert_eq!(model.window_buffer(main), Some(first));
        assert_ne!(model.window_buffer(main), Some(model.commandline_buffer()));
    }

    #[test]
    fn wiping_displayed_buffer_reassigns_window_and_preserves_invariants() {
        let main = WindowId::new(10);
        let commandline = WindowId::new(11);
        let mut model = EditorModel::new(Vec::new(), main, commandline);
        let removed = model.window_buffer(main).unwrap();
        let fallback = model.create("fallback");

        model.wipe(removed, true).unwrap();

        assert_eq!(model.window_buffer(main), Some(fallback));
        assert!(model.validate().is_ok());
    }
}
