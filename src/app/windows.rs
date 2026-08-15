//! Operations that need both a window (`vim_ui::Ui`) and buffer storage
//! (`crate::model::EditorModel`). These exist because a window only knows a
//! buffer by id; looking up, switching, or editing the buffer it names
//! requires both stores at once. Keeping this here (instead of on
//! `EditorModel`) avoids hiding a second, window-owning store behind model
//! methods now that windows are owned solely by `vim_ui::Ui`.

use vim_buffer::{Buffer, BufferError, BufferId};
use vim_ui::{Ui, Viewport, Window, WindowId, WindowState};

use crate::model::{BufferState, EditorModel};

pub struct WindowOps;

impl WindowOps {
    /// Attaches `buffer` to `id` with a concrete viewport.
    pub fn register(ui: &mut Ui, id: WindowId, buffer: &Buffer, viewport: Viewport) {
        if let Some(window) = ui.window_mut(id) {
            window.attach(buffer, viewport);
        }
    }

    /// Attaches `buffer` to `id` with a placeholder viewport, used before the
    /// real layout is known.
    pub fn register_placeholder(ui: &mut Ui, id: WindowId, buffer: &Buffer) {
        if let Some(window) = ui.window_mut(id) {
            window.attach_placeholder(buffer);
        }
    }

    pub fn window_buffer(ui: &Ui, id: WindowId) -> Option<BufferId> {
        ui.window(id).and_then(Window::buffer_id)
    }

    /// All windows that currently host buffer content, i.e. not chrome
    /// (tabline/statusline/panel) windows.
    pub fn window_buffers(ui: &Ui) -> Vec<(WindowId, BufferId)> {
        ui.window_store()
            .iter()
            .filter_map(|(&id, window)| window.buffer_id().map(|buffer_id| (id, buffer_id)))
            .collect()
    }

    pub fn switch_to(
        ui: &mut Ui,
        model: &EditorModel,
        window_id: WindowId,
        buffer_id: BufferId,
    ) -> bool {
        let Ok(buffer) = model.get_buffer(buffer_id) else {
            return false;
        };
        ui.window_mut(window_id)
            .is_some_and(|window| window.switch_to(buffer))
    }

    pub fn switch_next_buffer(ui: &mut Ui, model: &EditorModel, window_id: WindowId) -> bool {
        Self::switch_buffer_by(ui, model, window_id, |position, len| (position + 1) % len)
    }

    pub fn switch_previous_buffer(ui: &mut Ui, model: &EditorModel, window_id: WindowId) -> bool {
        Self::switch_buffer_by(ui, model, window_id, |position, len| {
            if position == 0 { len - 1 } else { position - 1 }
        })
    }

    /// Registers `new_id` (already created in the layout by the caller) as a
    /// split of `source`, inheriting its buffer and viewport.
    pub fn split(ui: &mut Ui, model: &EditorModel, source: WindowId, new_id: WindowId) -> bool {
        let Some(buffer_id) = Self::window_buffer(ui, source) else {
            return false;
        };
        let Ok(buffer) = model.get_buffer(buffer_id) else {
            return false;
        };
        let Some(viewport) = ui
            .window(source)
            .and_then(Window::window_state)
            .map(|state| state.viewport)
        else {
            return false;
        };
        Self::register(ui, new_id, buffer, viewport);
        true
    }

    /// Reassigns windows displaying `removed` to `fallback` (or closes them
    /// if there is none), and drops any state windows retained for it.
    pub fn remove_buffer(ui: &mut Ui, removed: BufferId, fallback: Option<&Buffer>) {
        let affected: Vec<WindowId> = ui
            .window_store()
            .iter()
            .filter_map(|(&id, window)| (window.buffer_id() == Some(removed)).then_some(id))
            .collect();
        for window_id in affected {
            match fallback {
                Some(buffer) => {
                    if let Some(window) = ui.window_mut(window_id) {
                        window.switch_to(buffer);
                    }
                }
                None => {
                    let _ = ui.close_window(window_id);
                }
            }
        }
        for (_, window) in ui.window_store_mut().iter_mut() {
            window.forget_buffer(removed);
        }
    }

    /// Borrows the buffer, its analysis state, and its window's content
    /// together for one edit, bumping the buffer's revision.
    pub fn edit_window<R>(
        ui: &mut Ui,
        model: &mut EditorModel,
        window_id: WindowId,
        edit: impl FnOnce(&mut Buffer, &mut BufferState, &mut WindowState) -> R,
    ) -> Result<R, BufferError> {
        let Some(buffer_id) = Self::window_buffer(ui, window_id) else {
            return Err(BufferError::NotImplemented(
                "editing an unregistered window",
            ));
        };
        let (buffer, state) = model.buffers_mut().get_mut_with_state(buffer_id)?;
        state.revision = state.revision.wrapping_add(1);
        let window = ui
            .window_mut(window_id)
            .and_then(Window::window_state_mut)
            .expect("window buffer came from registered window");
        Ok(edit(buffer, state, window))
    }

    pub fn validate(ui: &Ui, model: &EditorModel) -> Result<(), String> {
        let focused = ui.focused_window_id();
        if !ui.window(focused).is_some_and(Window::has_content) {
            return Err("focused window is not registered".to_string());
        }
        for (window_id, window) in ui.window_store().iter() {
            let Some(state) = window.window_state() else {
                continue;
            };
            let Ok(buffer) = model.get_buffer(state.buffer_id) else {
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
        ui: &mut Ui,
        model: &EditorModel,
        window_id: WindowId,
        next_position: impl FnOnce(usize, usize) -> usize,
    ) -> bool {
        let listed = model.list();
        if listed.is_empty() {
            return false;
        }
        let Some(current) = Self::window_buffer(ui, window_id) else {
            return false;
        };
        let position = listed.iter().position(|&id| id == current).unwrap_or(0);
        let target = listed[next_position(position, listed.len())];
        Self::switch_to(ui, model, window_id, target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vim_ui::{Rect, SplitAxis};

    fn fixture() -> (Ui, EditorModel, WindowId, WindowId) {
        let mut ui = Ui::new(Rect::new(0, 0, 80, 24));
        let main = ui.focused_window_id();
        let model = EditorModel::new(Vec::new());
        WindowOps::register_placeholder(
            &mut ui,
            main,
            model.get_buffer(model.initial_buffer()).unwrap(),
        );
        let commandline = ui.create_window("COMMAND LINE".to_string());
        WindowOps::register_placeholder(
            &mut ui,
            commandline,
            model.get_buffer(model.commandline_buffer()).unwrap(),
        );
        (ui, model, main, commandline)
    }

    #[test]
    fn split_inherits_buffer_and_keeps_independent_state() {
        let (mut ui, model, main, _) = fixture();
        let buffer_id = WindowOps::window_buffer(&ui, main).unwrap();
        let split = ui.split_focused(SplitAxis::Columns).unwrap();

        assert!(WindowOps::split(&mut ui, &model, main, split));
        assert_eq!(WindowOps::window_buffer(&ui, split), Some(buffer_id));

        ui.window_mut(split)
            .unwrap()
            .window_state_mut()
            .unwrap()
            .viewport
            .width = 12;
        assert_ne!(
            ui.window(main)
                .unwrap()
                .window_state()
                .unwrap()
                .viewport
                .width,
            ui.window(split)
                .unwrap()
                .window_state()
                .unwrap()
                .viewport
                .width
        );
    }

    #[test]
    fn buffer_switching_wraps() {
        let (mut ui, mut model, main, _) = fixture();
        let second = model.create("second");
        let _ = second;
        let original = WindowOps::window_buffer(&ui, main).unwrap();

        assert!(WindowOps::switch_previous_buffer(&mut ui, &model, main));
        assert_ne!(WindowOps::window_buffer(&ui, main), Some(original));
        assert!(WindowOps::switch_next_buffer(&mut ui, &model, main));
        assert_eq!(WindowOps::window_buffer(&ui, main), Some(original));
    }

    #[test]
    fn wiping_displayed_buffer_reassigns_window_and_preserves_invariants() {
        let (mut ui, mut model, main, _) = fixture();
        let removed = WindowOps::window_buffer(&ui, main).unwrap();
        let fallback_id = model.create("fallback");

        model.wipe(removed, true).unwrap();
        let fallback = model.get_buffer(fallback_id).ok();
        WindowOps::remove_buffer(&mut ui, removed, fallback);

        assert_eq!(WindowOps::window_buffer(&ui, main), Some(fallback_id));
        assert!(WindowOps::validate(&ui, &model).is_ok());
    }

    #[test]
    fn switching_back_restores_window_state_for_buffer() {
        let (mut ui, mut model, main, _) = fixture();
        let original = WindowOps::window_buffer(&ui, main).unwrap();
        let _second = model.create("second");

        ui.window_mut(main)
            .unwrap()
            .window_state_mut()
            .unwrap()
            .viewport
            .width = 37;
        assert!(WindowOps::switch_next_buffer(&mut ui, &model, main));
        ui.window_mut(main)
            .unwrap()
            .window_state_mut()
            .unwrap()
            .viewport
            .width = 91;
        assert!(WindowOps::switch_previous_buffer(&mut ui, &model, main));

        let restored = ui.window(main).unwrap().window_state().unwrap();
        assert_eq!(restored.buffer_id, original);
        assert_eq!(restored.viewport.width, 37);
    }
}
