pub mod buffer_state;
pub mod buffers;
pub mod window_state;
pub mod windows;

use std::path::PathBuf;
use vim_buffer::BufferId;
use vim_ui::WindowId;

pub use buffer_state::BufferState;
pub use buffers::Buffers;
pub use window_state::{Viewport, WindowState};
pub use windows::Windows;

pub struct EditorModel {
    pub buffers: Buffers,
    pub windows: Windows,
    pub status: Option<String>,
    commandline_buffer: BufferId,
}

impl EditorModel {
    pub fn new(paths: Vec<PathBuf>, main_window: WindowId, commandline_window: WindowId) -> Self {
        let mut buffers = Buffers::new();
        let first_buffer = buffers.open_paths(paths);
        let (commandline_buffer, _) = buffers
            .create_named("#commandline", "")
            .expect("Failed to create #commandline buffer");

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

    pub fn get_current(&self) -> Option<BufferId> {
        Some(self.buffers.current())
    }

    pub fn create(&mut self, initial_text: impl Into<String>) -> BufferId {
        self.buffers.create(initial_text)
    }

    pub fn create_named(
        &mut self,
        name: impl AsRef<std::path::Path>,
        initial_text: impl Into<String>,
    ) -> Result<(BufferId, vim_buffer::ManagerOutcome), vim_buffer::BufferError> {
        self.buffers.create_named(name, initial_text)
    }

    pub fn load(
        &mut self,
        path: impl AsRef<std::path::Path>,
    ) -> Result<(BufferId, vim_buffer::ManagerOutcome), vim_buffer::BufferError> {
        self.buffers.load(path)
    }

    pub fn unload(
        &mut self,
        id: BufferId,
        force: bool,
    ) -> Result<vim_buffer::ManagerOutcome, vim_buffer::BufferError> {
        let result = self.buffers.unload(id, force);
        if result.is_ok() {
            self.cleanup_windows(id);
        }
        result
    }

    pub fn delete(
        &mut self,
        id: BufferId,
        force: bool,
    ) -> Result<vim_buffer::ManagerOutcome, vim_buffer::BufferError> {
        let result = self.buffers.delete(id, force);
        if result.is_ok() {
            self.cleanup_windows(id);
        }
        result
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

    pub fn get_buffer_mut(
        &mut self,
        id: BufferId,
    ) -> Result<&mut vim_buffer::Buffer, vim_buffer::BufferError> {
        self.buffers.get_mut(id)
    }

    pub fn list(&self) -> Vec<BufferId> {
        self.buffers.list()
    }

    pub fn listed(&self) -> Vec<BufferId> {
        self.buffers.listed()
    }

    pub fn get_buffer_context(&self, id: BufferId) -> Option<&BufferState> {
        self.buffers.state(id)
    }

    pub fn get_buffer_context_mut(&mut self, id: BufferId) -> Option<&mut BufferState> {
        Some(self.buffers.state_mut(id))
    }

    pub fn buffer_revision(&self, id: BufferId) -> Option<u64> {
        self.get_buffer(id).ok()?;
        Some(
            self.get_buffer_context(id)
                .map_or(0, |state| state.revision),
        )
    }

    pub fn task_owner(
        &self,
        buffer_id: BufferId,
        window_id: Option<WindowId>,
    ) -> Option<crate::app::services::OwnerId> {
        Some(crate::app::services::OwnerId {
            buffer_id: Some(buffer_id),
            window_id,
            revision: self.buffer_revision(buffer_id)?,
        })
    }

    pub fn set_buffer_context(&mut self, id: BufferId, context: BufferState) {
        *self.buffers.state_mut(id) = context;
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

    pub fn synchronize_viewports(&mut self, layout: &crate::view::LayoutSnapshot) {
        let updates: Vec<_> = self
            .window_buffers()
            .filter_map(|(window_id, buffer_id)| {
                let window_layout = layout.get(window_id)?;
                let inner_rect = if window_layout.draws_border {
                    window_layout.rect.inner(1)
                } else {
                    window_layout.rect
                };
                let snapshot = self
                    .get_buffer(buffer_id)
                    .ok()?
                    .snapshot()
                    .as_inner()
                    .clone();
                Some((
                    window_id,
                    snapshot,
                    window_layout.rect.width as u32,
                    inner_rect.height as u32,
                    window_layout.draws_border,
                ))
            })
            .collect();

        for (window_id, snapshot, width, height, has_border) in updates {
            if let Some(window) = self.window_state_mut(window_id) {
                window.update(snapshot, width, height, has_border);
            }
        }
    }

    pub fn register_window(
        &mut self,
        window_id: WindowId,
        buffer_id: BufferId,
        viewport: Viewport,
    ) -> bool {
        let Ok(buffer) = self.buffers.get(buffer_id) else {
            return false;
        };
        self.windows.register(window_id, buffer, viewport);
        true
    }

    pub fn focus_window(&mut self, window_id: WindowId) -> bool {
        self.windows.focus(window_id)
    }

    pub fn switch_next_buffer(&mut self, window_id: WindowId) -> bool {
        let listed = self.buffers.listed();
        self.windows
            .switch_next_buffer(window_id, &listed, &self.buffers)
    }

    pub fn switch_previous_buffer(&mut self, window_id: WindowId) -> bool {
        let listed = self.buffers.listed();
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

    pub fn with_mut<F, R>(
        &mut self,
        id: BufferId,
        window_id: WindowId,
        f: F,
    ) -> Result<R, vim_buffer::BufferError>
    where
        F: FnOnce(&mut vim_buffer::Buffer, &mut BufferState, &mut WindowState) -> R,
    {
        self.buffers.state_mut(id);
        if self.windows.state(window_id).is_none() {
            let buffer = self.buffers.get(id)?;
            self.windows.register_placeholder(window_id, buffer);
        }
        if self.windows.buffer_id(window_id) != Some(id) {
            let buffer = self.buffers.get(id)?;
            self.windows.switch_to(window_id, buffer);
        }

        let Buffers { inner, states } = &mut self.buffers;
        let buffer = inner.get_mut(id)?;
        let context = states.get_mut(&id).unwrap();
        context.revision = context.revision.wrapping_add(1);
        let window = self.windows.state_mut(window_id).unwrap();
        Ok(f(buffer, context, window))
    }

    pub fn validate(&self) -> Result<(), String> {
        self.windows.validate(&self.buffers)
    }

    fn cleanup_windows(&mut self, removed: BufferId) {
        let fallback_id = self
            .buffers
            .listed()
            .into_iter()
            .find(|&id| id != removed && id != self.commandline_buffer);
        let fallback = fallback_id.and_then(|id| self.buffers.get(id).ok());
        self.windows.remove_buffer(removed, fallback);
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
        assert_eq!(model.list().len(), 2);
        assert!(model.validate().is_ok());
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
