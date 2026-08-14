use std::collections::HashMap;

use crate::controller::input::InputController;
use crate::model::EditorModel;

use super::textview;

#[derive(Default)]
pub struct LayoutSnapshot {
    windows: HashMap<vim_ui::WindowId, WindowLayout>,
}

#[derive(Debug, Clone, Copy)]
pub struct WindowLayout {
    pub rect: vim_ui::Rect,
    pub draws_border: bool,
}

impl LayoutSnapshot {
    pub fn insert(&mut self, window_id: vim_ui::WindowId, rect: vim_ui::Rect, draws_border: bool) {
        self.windows
            .insert(window_id, WindowLayout { rect, draws_border });
    }

    pub fn get(&self, window_id: vim_ui::WindowId) -> Option<WindowLayout> {
        self.windows.get(&window_id).copied()
    }
}

pub struct EditorViewModel {
    text_models: HashMap<vim_ui::WindowId, vim_ui::TextViewModel>,
    active_buffer_id: Option<vim_ui::BufferId>,
    buffer_ids: Vec<vim_ui::BufferId>,
    buffer_names: HashMap<vim_ui::BufferId, String>,
    active_cursor: Option<(u32, u32)>,
    mode_name: String,
    status_message: Option<String>,
}

impl EditorViewModel {
    pub fn build(
        model: &EditorModel,
        controller: &InputController,
        layout: &LayoutSnapshot,
    ) -> Self {
        let mode = controller.mode();
        let mode_name = format!("{mode:?}").to_uppercase();
        let mut buffer_ids = Vec::new();
        let mut buffer_names = HashMap::new();

        for id in model.list() {
            let ui_buffer_id = vim_ui::BufferId::new(id.get());
            buffer_ids.push(ui_buffer_id);
            buffer_names.insert(ui_buffer_id, buffer_name(model, id));
        }

        let active_window = model.focused_window();
        let active_buffer_id = model
            .window_buffer(active_window)
            .map(|id| vim_ui::BufferId::new(id.get()));
        let active_cursor = model.window_state(active_window).and_then(|window| {
            if window.selections.selections.is_empty() {
                return Some((1, 1));
            }
            let buffer = model
                .get_buffer(window_buffer_id(model, active_window)?)
                .ok()?;
            let point = window
                .selections
                .primary()
                .head()
                .to_point(buffer.snapshot().as_inner());
            Some((point.row + 1, point.column + 1))
        });

        let mut text_models = HashMap::new();
        for (window_id, buffer_id) in model.window_buffers() {
            let Some(window) = model.window_state(window_id) else {
                continue;
            };
            let Ok(buffer) = model.get_buffer(buffer_id) else {
                continue;
            };
            let window_layout = layout.get(window_id).unwrap_or(WindowLayout {
                rect: vim_ui::Rect::new(
                    0,
                    0,
                    window.viewport.width as u16,
                    window.viewport.height as u16,
                ),
                draws_border: window.viewport.has_border,
            });
            let inner_rect = if window_layout.draws_border {
                window_layout.rect.inner(1)
            } else {
                window_layout.rect
            };
            let highlights = model.buffer_state(buffer_id).map(|s| s.highlights.as_slice());
            text_models.insert(
                window_id,
                textview::build_text(buffer, window, inner_rect, window_id == active_window, mode, highlights),
            );
        }

        Self {
            text_models,
            active_buffer_id,
            buffer_ids,
            buffer_names,
            active_cursor,
            mode_name,
            status_message: model.status.clone(),
        }
    }
}

impl vim_ui::UIContext for EditorViewModel {
    fn get_buffer_model(&self, _id: vim_ui::BufferId) -> Option<vim_ui::BufferViewModel<'_>> {
        None
    }

    fn get_active_buffer_id(&self) -> Option<vim_ui::BufferId> {
        self.active_buffer_id
    }

    fn get_text_model(&self, window_id: vim_ui::WindowId) -> Option<&vim_ui::TextViewModel> {
        self.text_models.get(&window_id)
    }

    fn get_colorscheme(&self) -> Option<&vim_ui::ColorScheme> {
        None
    }

    fn get_buffer_ids(&self) -> Vec<vim_ui::BufferId> {
        self.buffer_ids.clone()
    }

    fn get_buffer_name(&self, id: vim_ui::BufferId) -> Option<String> {
        self.buffer_names.get(&id).cloned()
    }

    fn get_status_message(&self) -> Option<String> {
        self.status_message.clone()
    }

    fn get_mode_name(&self) -> String {
        self.mode_name.clone()
    }

    fn get_cursor_position(&self) -> Option<(u32, u32)> {
        self.active_cursor
    }
}

fn buffer_name(model: &EditorModel, id: vim_buffer::BufferId) -> String {
    model
        .get_buffer(id)
        .ok()
        .and_then(|buffer| buffer.path())
        .and_then(|path| path.file_name())
        .and_then(|name| name.to_str())
        .map(str::to_owned)
        .unwrap_or_else(|| format!("[No Name {}]", id.get()))
}

fn window_buffer_id(
    model: &EditorModel,
    window_id: vim_ui::WindowId,
) -> Option<vim_buffer::BufferId> {
    model.window_buffer(window_id)
}

use text::ToPoint;

#[cfg(test)]
mod tests {
    use super::*;
    use vim_ui::UIContext;

    fn fixture() -> (
        EditorModel,
        InputController,
        LayoutSnapshot,
        vim_ui::WindowId,
    ) {
        let main = vim_ui::WindowId::new(1);
        let commandline = vim_ui::WindowId::new(2);
        let model = EditorModel::new(Vec::new(), main, commandline);
        let controller = InputController::new(vim_input::Mode::Normal);
        let mut layout = LayoutSnapshot::default();
        layout.insert(main, vim_ui::Rect::new(0, 1, 80, 20), true);
        layout.insert(commandline, vim_ui::Rect::new(0, 22, 80, 1), false);
        (model, controller, layout, main)
    }

    #[test]
    fn build_projects_active_buffer_mode_cursor_and_text() {
        let (model, controller, layout, main) = fixture();
        let view_model = EditorViewModel::build(&model, &controller, &layout);

        assert_eq!(view_model.get_mode_name(), "NORMAL");
        assert_eq!(view_model.get_cursor_position(), Some((1, 1)));
        assert_eq!(
            view_model.get_active_buffer_id().map(|id| id.get()),
            model.window_buffer(main).map(|id| id.get())
        );
        assert!(view_model.get_text_model(main).is_some());
    }

    #[test]
    fn build_is_deterministic_for_immutable_inputs() {
        let (model, controller, layout, main) = fixture();
        let first = EditorViewModel::build(&model, &controller, &layout);
        let second = EditorViewModel::build(&model, &controller, &layout);

        assert_eq!(first.get_buffer_ids(), second.get_buffer_ids());
        assert_eq!(first.get_mode_name(), second.get_mode_name());
        assert_eq!(first.get_cursor_position(), second.get_cursor_position());
        assert_eq!(
            first.get_text_model(main).unwrap().viewport_width,
            second.get_text_model(main).unwrap().viewport_width
        );
    }
}
