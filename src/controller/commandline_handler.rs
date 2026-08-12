use text::{Point, ToOffset, ToPoint};
use vim_input::{Action, Mode};
use vim_ui::WindowId;

use crate::app::{script::ScriptRuntime, ui::ViewIds};
use crate::controller::input::InputController;
use crate::model::EditorModel;

use super::command::{CommandOutcome, ViewEffect};

pub struct CommandlineHandler;

impl CommandlineHandler {
    pub fn handles(action: &Action) -> bool {
        matches!(
            action,
            Action::SetToCommand | Action::Clear | Action::InsertNewLine { .. }
        )
    }

    pub fn execute(
        model: &mut EditorModel,
        input: &mut InputController,
        script: &mut ScriptRuntime,
        view_ids: ViewIds,
        active_window: WindowId,
        action: &Action,
    ) -> CommandOutcome {
        match action {
            Action::SetToCommand => {
                input.set_mode(Mode::Insert);
                CommandOutcome::with_effect(ViewEffect::Focus(view_ids.commandline))
            }
            Action::Clear if active_window == view_ids.commandline => {
                CommandOutcome::with_effect(ViewEffect::Focus(Self::editor_focus(model, view_ids)))
            }
            Action::InsertNewLine { .. }
                if active_window == view_ids.commandline
                    && model.window_buffer(active_window) == Some(model.commandline_buffer()) =>
            {
                input.set_mode(Mode::Normal);
                if let Some(command) = Self::current_command(model, active_window) {
                    if let Err(error) = script.execute(&command) {
                        model.status = Some(error);
                    }
                }
                CommandOutcome::with_effect(ViewEffect::Focus(Self::editor_focus(model, view_ids)))
            }
            _ => CommandOutcome::default(),
        }
    }

    fn editor_focus(model: &EditorModel, view_ids: ViewIds) -> WindowId {
        model
            .previous_window()
            .filter(|&id| id != view_ids.commandline && model.window_state(id).is_some())
            .unwrap_or(view_ids.main)
    }

    fn current_command(model: &EditorModel, commandline_id: WindowId) -> Option<String> {
        let buffer_id = model.commandline_buffer();
        let window = model.window_state(commandline_id)?;
        let buffer = model.get_buffer(buffer_id).ok()?;
        let current_row = window
            .selections
            .first()?
            .head()
            .to_point(buffer.as_text_buffer())
            .row;
        let target_row = current_row.checked_sub(1)?;
        let text_buffer = buffer.as_text_buffer();
        let start = Point::new(target_row, 0).to_offset(text_buffer);
        let end = Point::new(target_row, text_buffer.line_len(target_row)).to_offset(text_buffer);
        Some(text_buffer.as_rope().chunks_in_range(start..end).collect())
    }
}
