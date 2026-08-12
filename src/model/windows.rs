use std::collections::HashMap;
use vim_buffer::BufferId;
use vim_ui::WindowId;

use super::window_state::{Viewport, WindowState};

pub struct Windows {
    windows: HashMap<WindowId, WindowState>,
    focused: WindowId,
    previous: Option<WindowId>,
}

impl Windows {
    pub fn new(focused: WindowId) -> Self {
        Self {
            windows: HashMap::new(),
            focused,
            previous: None,
        }
    }

    pub fn register(&mut self, id: WindowId, buffer: &vim_buffer::Buffer, viewport: Viewport) {
        self.windows.insert(id, WindowState::new(buffer, viewport));
    }

    pub fn register_placeholder(&mut self, id: WindowId, buffer: &vim_buffer::Buffer) {
        self.windows.insert(id, WindowState::placeholder(buffer));
    }

    pub fn remove(&mut self, id: WindowId) -> Option<WindowState> {
        let removed = self.windows.remove(&id);
        if id == self.focused {
            if let Some(previous) = self.previous.filter(|id| self.windows.contains_key(id)) {
                self.focused = previous;
            } else if let Some(next) = self.windows.keys().copied().next() {
                self.focused = next;
            }
            self.previous = None;
        }
        removed
    }

    pub fn focused(&self) -> WindowId {
        self.focused
    }

    pub fn previous(&self) -> Option<WindowId> {
        self.previous
    }

    pub fn focus(&mut self, id: WindowId) -> bool {
        if !self.windows.contains_key(&id) {
            return false;
        }
        if self.focused != id {
            self.previous = Some(self.focused);
            self.focused = id;
        }
        true
    }

    pub fn state(&self, id: WindowId) -> Option<&WindowState> {
        self.windows.get(&id)
    }

    pub fn state_mut(&mut self, id: WindowId) -> Option<&mut WindowState> {
        self.windows.get_mut(&id)
    }

    pub fn iter(&self) -> impl Iterator<Item = (WindowId, &WindowState)> {
        self.windows.iter().map(|(&id, state)| (id, state))
    }

    pub fn buffer_id(&self, id: WindowId) -> Option<BufferId> {
        self.state(id).map(|state| state.buffer_id)
    }

    pub fn switch_to(&mut self, id: WindowId, buffer: &vim_buffer::Buffer) -> bool {
        let Some(state) = self.state_mut(id) else {
            return false;
        };
        state.switch_buffer(buffer);
        true
    }

    pub fn switch_next_buffer(
        &mut self,
        id: WindowId,
        buffers: &[BufferId],
        storage: &super::Buffers,
    ) -> bool {
        self.switch_buffer_by(id, buffers, storage, |position, len| (position + 1) % len)
    }

    pub fn switch_previous_buffer(
        &mut self,
        id: WindowId,
        buffers: &[BufferId],
        storage: &super::Buffers,
    ) -> bool {
        self.switch_buffer_by(id, buffers, storage, |position, len| {
            if position == 0 { len - 1 } else { position - 1 }
        })
    }

    pub fn split_from(
        &mut self,
        source_id: WindowId,
        new_id: WindowId,
        buffer: &vim_buffer::Buffer,
    ) -> bool {
        let Some(source) = self.state(source_id) else {
            return false;
        };
        let viewport = source.viewport;
        self.register(new_id, buffer, viewport);
        self.focus(new_id)
    }

    pub fn remove_buffer(&mut self, id: BufferId, fallback: Option<&vim_buffer::Buffer>) {
        let affected: Vec<WindowId> = self
            .iter()
            .filter_map(|(window_id, state)| (state.buffer_id == id).then_some(window_id))
            .collect();
        for window_id in affected {
            if let Some(buffer) = fallback {
                self.switch_to(window_id, buffer);
            } else {
                self.remove(window_id);
            }
        }
    }

    pub fn validate(&self, buffers: &super::Buffers) -> Result<(), String> {
        if !self.windows.contains_key(&self.focused) {
            return Err("focused window is not registered".to_string());
        }
        for (window_id, state) in self.iter() {
            let Ok(buffer) = buffers.get(state.buffer_id) else {
                return Err(format!(
                    "window {} references missing buffer {}",
                    window_id.get(),
                    state.buffer_id.get()
                ));
            };
            if state.display_map.snapshot().buffer_snapshot().version
                != buffer.snapshot().as_inner().version
                && state.last_version.as_ref() == Some(&buffer.snapshot().as_inner().version)
            {
                return Err(format!(
                    "window {} display map is stale for buffer {}",
                    window_id.get(),
                    state.buffer_id.get()
                ));
            }
        }
        Ok(())
    }

    fn switch_buffer_by(
        &mut self,
        id: WindowId,
        buffers: &[BufferId],
        storage: &super::Buffers,
        next_position: impl FnOnce(usize, usize) -> usize,
    ) -> bool {
        if buffers.is_empty() {
            return false;
        }
        let Some(current) = self.buffer_id(id) else {
            return false;
        };
        let position = buffers
            .iter()
            .position(|&buffer_id| buffer_id == current)
            .unwrap_or(0);
        let target = buffers[next_position(position, buffers.len())];
        let Ok(buffer) = storage.get(target) else {
            return false;
        };
        self.switch_to(id, buffer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (super::super::Buffers, Windows, WindowId, WindowId) {
        let mut buffers = super::super::Buffers::new();
        let first = buffers.current();
        let second = buffers.create("second");
        let first_window = WindowId::new(1);
        let second_window = WindowId::new(2);
        let mut windows = Windows::new(first_window);
        windows.register_placeholder(first_window, buffers.get(first).unwrap());
        windows.register_placeholder(second_window, buffers.get(second).unwrap());
        (buffers, windows, first_window, second_window)
    }

    #[test]
    fn focus_tracks_previous_window() {
        let (_buffers, mut windows, first, second) = fixture();
        assert!(windows.focus(second));
        assert_eq!(windows.focused(), second);
        assert_eq!(windows.previous(), Some(first));
    }

    #[test]
    fn split_inherits_buffer_and_keeps_independent_state() {
        let (buffers, mut windows, first, _) = fixture();
        let split = WindowId::new(3);
        let buffer_id = windows.buffer_id(first).unwrap();
        assert!(windows.split_from(first, split, buffers.get(buffer_id).unwrap()));
        assert_eq!(windows.buffer_id(split), Some(buffer_id));
        assert_eq!(windows.focused(), split);

        windows.state_mut(split).unwrap().viewport.width = 12;
        assert_ne!(
            windows.state(first).unwrap().viewport.width,
            windows.state(split).unwrap().viewport.width
        );
    }

    #[test]
    fn buffer_switching_wraps() {
        let (buffers, mut windows, first, _) = fixture();
        let listed = buffers.listed();
        let original = windows.buffer_id(first).unwrap();
        assert!(windows.switch_previous_buffer(first, &listed, &buffers));
        assert_ne!(windows.buffer_id(first), Some(original));
        assert!(windows.switch_next_buffer(first, &listed, &buffers));
        assert_eq!(windows.buffer_id(first), Some(original));
    }
}
